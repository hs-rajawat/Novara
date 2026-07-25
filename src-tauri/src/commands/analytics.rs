//! Aggregations for the Dashboard and Analytics pages.

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

#[tauri::command]
pub async fn dashboard_stats(state: State<'_, Arc<AppState>>) -> AppResult<DashboardStats> {
    let db = &state.db.pool;

    let (total_games, completed_games, total_playtime_seconds, favorite_count): (i64, i64, i64, i64) =
        sqlx::query_as(
            r#"
            SELECT
              COUNT(*),
              SUM(CASE WHEN completion_state = 'completed' THEN 1 ELSE 0 END),
              COALESCE(SUM(total_playtime_seconds), 0),
              SUM(is_favorite)
            FROM games
            "#,
        )
        .fetch_one(db)
        .await?;

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

#[derive(Serialize, sqlx::FromRow)]
pub struct HeatmapCell {
    pub day: String,         // YYYY-MM-DD
    pub seconds: i64,
}

#[tauri::command]
pub async fn heatmap(
    days: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<HeatmapCell>> {
    let cutoff_days = days.unwrap_or(365);
    let rows: Vec<HeatmapCell> = sqlx::query_as(
        r#"
        SELECT
          substr(started_at, 1, 10) as day,
          SUM(MAX(duration_seconds - idle_seconds, 0)) as seconds
        FROM play_sessions
        WHERE started_at >= datetime('now', printf('-%d days', ?1))
        GROUP BY day
        ORDER BY day
        "#,
    )
    .bind(cutoff_days)
    // Propagated rather than `.unwrap_or_default()`: an empty heatmap and a
    // failed heatmap query looked identical to the user and to the logs.
    .fetch_all(&state.db.pool)
    .await?;
    Ok(rows)
}
