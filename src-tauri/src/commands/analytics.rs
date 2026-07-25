//! Aggregations for the Dashboard and Analytics pages.
//!
//! # Convention: calendar-based analytics use the user's local day
//!
//! **Every calendar-based aggregation in NOVARA buckets by the user's local
//! calendar day, unless a specific feature explicitly documents otherwise.**
//! This is a project-wide convention, not a detail of the heatmap below —
//! `heatmap_rows` is simply where it was first enforced, after a UTC/local
//! mismatch between this command and the frontend grid shifted every cell (and
//! the longest-streak figure derived from it) by a day for any timezone away
//! from UTC.
//!
//! Concretely, for anything grouping timestamps into days:
//!   * bucket with SQLite's `date(col, 'localtime')`, not `substr(col, 1, 10)`
//!     or any other UTC-based slice;
//!   * on the frontend, derive day keys from local `Date` components
//!     (`getFullYear()`/`getMonth()`/`getDate()`, as in `lib/heatmap.ts`'s
//!     `localDayKey`), never from `toISOString()`, which converts to UTC;
//!   * compare cutoffs through `datetime(...)` on both sides of a SQL query,
//!     not as raw strings — `now_rfc3339()` produces a `T` separator while
//!     SQLite's date functions produce a space, and `'T' > ' '` silently
//!     degrades a string-compared boundary.
//!
//! A future analytics feature (a weekly summary, a "played this month" stat,
//! anything that answers "which day") should follow this by default. If a
//! feature genuinely needs UTC bucketing — cross-timezone comparison, a
//! server-side rollup — that is a deliberate exception and must say so in its
//! own docs, precisely because the default reader's assumption will be local.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;

#[derive(Serialize)]
pub struct DashboardStats {
    pub total_games: i64,
    pub completed_games: i64,
    pub total_playtime_seconds: i64,
    pub favorite_count: i64,
    pub recently_played: Vec<RecentGame>,
    pub top_genres: Vec<GenreCount>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RecentGame {
    pub id: String,
    pub title: String,
    pub cover_path: Option<String>,
    pub last_played_at: Option<String>,
    pub total_playtime_seconds: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct GenreCount {
    pub name: String,
    pub count: i64,
}

/// The four scalar library totals, extracted from `dashboard_stats` so the
/// aggregate can be tested against an empty database — the exact case that
/// used to fail.
///
/// Every aggregate is COALESCE'd. On an empty `games` table SQLite's SUM()
/// returns NULL, which cannot decode into i64, so this command failed outright
/// on every fresh install and the Dashboard stayed blank until the first game
/// existed. COUNT(*) is the only aggregate here that is already NULL-safe.
pub(crate) async fn library_totals(
    pool: &sqlx::SqlitePool,
) -> AppResult<(i64, i64, i64, i64)> {
    Ok(sqlx::query_as(
        r#"
        SELECT
          COUNT(*),
          COALESCE(SUM(CASE WHEN completion_state = 'completed' THEN 1 ELSE 0 END), 0),
          COALESCE(SUM(total_playtime_seconds), 0),
          COALESCE(SUM(is_favorite), 0)
        FROM games
        "#,
    )
    .fetch_one(pool)
    .await?)
}

#[tauri::command]
pub async fn dashboard_stats(state: State<'_, Arc<AppState>>) -> AppResult<DashboardStats> {
    let db = &state.db.pool;

    let (total_games, completed_games, total_playtime_seconds, favorite_count) =
        library_totals(db).await?;

    let recently_played: Vec<RecentGame> = sqlx::query_as(
        r#"
        SELECT id, title, cover_path, last_played_at, total_playtime_seconds
        FROM games
        WHERE last_played_at IS NOT NULL
        ORDER BY last_played_at DESC
        LIMIT 8
        "#,
    )
    .fetch_all(db)
    .await?;

    let top_genres: Vec<GenreCount> = sqlx::query_as(
        r#"
        SELECT g.name, COUNT(*) as count
        FROM genres g
        JOIN game_genres gg ON gg.genre_id = g.id
        GROUP BY g.id
        ORDER BY count DESC
        LIMIT 6
        "#,
    )
    // Propagated rather than `.unwrap_or_default()`. Swallowing the error
    // here rendered an empty "Top genres" panel that is indistinguishable
    // from a library with no genres assigned, and left no trace anywhere.
    .fetch_all(db)
    .await?;

    Ok(DashboardStats {
        total_games,
        completed_games,
        total_playtime_seconds,
        favorite_count,
        recently_played,
        top_genres,
    })
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct HeatmapCell {
    pub day: String,         // YYYY-MM-DD
    pub seconds: i64,
}

/// Bucket play sessions into days, using `day_modifier` as SQLite's date
/// modifier.
///
/// Production passes `"localtime"`, which resolves each row against the OS
/// timezone database and so handles historical DST transitions correctly. Tests
/// pass an explicit offset such as `"+330 minutes"`, because `'localtime'`
/// depends on the machine's timezone and cannot be varied reliably from a test
/// process on Windows. The bucketing being verified — `date(started_at, <mod>)`
/// and the grouping around it — is identical either way; only the source of the
/// offset differs.
pub(crate) async fn heatmap_rows(
    pool: &sqlx::SqlitePool,
    days: i64,
    day_modifier: &str,
) -> AppResult<Vec<HeatmapCell>> {
    Ok(sqlx::query_as(
        r#"
        SELECT
          date(started_at, ?2) as day,
          SUM(MAX(duration_seconds - idle_seconds, 0)) as seconds
        FROM play_sessions
        WHERE datetime(started_at) >= datetime('now', printf('-%d days', ?1))
        GROUP BY day
        ORDER BY day
        "#,
    )
    .bind(days)
    .bind(day_modifier)
    // Propagated rather than `.unwrap_or_default()`: an empty heatmap and a
    // failed heatmap query looked identical to the user and to the logs.
    .fetch_all(pool)
    .await?)
}

/// Daily active playtime for the heatmap.
///
/// Days are **local calendar days**, not UTC ones. `started_at` is stored in UTC,
/// and grouping by `substr(started_at, 1, 10)` bucketed sessions by UTC date
/// while the frontend grid is built from local midnights — so for any timezone
/// away from UTC the entire heatmap, and the longest-streak figure derived from
/// it, was shifted by a day. A session played at 1am local in Asia/Kolkata
/// (+05:30) belongs to that local day, which is what the user means by it.
///
/// The cutoff is compared through `datetime()` on both sides rather than as
/// strings. `now_rfc3339` produces `2026-07-25T12:00:00+00:00` while SQLite's
/// date functions produce `2026-07-25 12:00:00` with a space, and `'T' > ' '`,
/// so a lexicographic comparison silently degraded the boundary to "start of the
/// cutoff day".
#[tauri::command]
pub async fn heatmap(
    days: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<HeatmapCell>> {
    heatmap_rows(&state.db.pool, days.unwrap_or(365), "localtime").await
}
