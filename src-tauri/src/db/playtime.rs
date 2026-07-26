use crate::error::AppResult;
use crate::models::{now_rfc3339, PlaySession};

use super::Db;

/// Active seconds a session must credit before a game is considered "playing".
///
/// A launch that is closed again within a few seconds — a mistake, a wrong game,
/// a launcher that failed — should not reclassify the title as in progress. A
/// minute is comfortably past that and still well inside any real session, so the
/// Dashboard populates on the first genuine play without the user managing state.
const MIN_PLAYING_SECONDS: i64 = 60;

impl Db {
    pub async fn start_session(&self, game_id: &str, process_name: Option<&str>) -> AppResult<i64> {
        let now = now_rfc3339();
        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO play_sessions (game_id, started_at, process_name)
            VALUES (?1, ?2, ?3)
            RETURNING id
            "#,
        )
        .bind(game_id)
        .bind(&now)
        .bind(process_name)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query("UPDATE games SET last_played_at = ?1, updated_at = ?1 WHERE id = ?2")
            .bind(&now)
            .bind(game_id)
            .execute(&self.pool)
            .await?;

        Ok(row.0)
    }

    pub async fn stop_session(
        &self,
        session_id: i64,
        duration_seconds: i64,
        idle_seconds: i64,
    ) -> AppResult<(String, i64)> {
        let game_id: String =
            sqlx::query_scalar("SELECT game_id FROM play_sessions WHERE id = ?1")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;

        sqlx::query(
            r#"
            UPDATE play_sessions
            SET ended_at = ?1, duration_seconds = ?2, idle_seconds = ?3
            WHERE id = ?4
            "#,
        )
        .bind(now_rfc3339())
        .bind(duration_seconds)
        .bind(idle_seconds)
        .bind(session_id)
        .execute(&self.pool)
        .await?;

        // Aggregate active playtime (duration - idle) onto the game row.
        let active = (duration_seconds - idle_seconds).max(0);
        sqlx::query(
            "UPDATE games SET total_playtime_seconds = total_playtime_seconds + ?1 WHERE id = ?2",
        )
        .bind(active)
        .bind(&game_id)
        .execute(&self.pool)
        .await?;

        // Derive `playing` from actually playing.
        //
        // `completion_state` was only ever written by the user through the
        // GameDetails tabs, so the Dashboard's "Continue Playing" shelf — which
        // filters on it — could never populate unless someone curated it by hand.
        // Playing a game is the clearest possible signal that it is in progress.
        //
        // Deliberately narrow:
        //   * only promotes from `unplayed`, so a user's own `completed`,
        //     `abandoned` or `backlog` is never overwritten — manual progression
        //     stays entirely under their control;
        //   * requires a session long enough to be a real play rather than a
        //     launch that was closed again, so an accidental start does not
        //     reclassify the game.
        if active >= MIN_PLAYING_SECONDS {
            sqlx::query(
                "UPDATE games SET completion_state = 'playing', updated_at = ?1 \
                 WHERE id = ?2 AND completion_state = 'unplayed'",
            )
            .bind(now_rfc3339())
            .bind(&game_id)
            .execute(&self.pool)
            .await?;
        }

        Ok((game_id, active))
    }

    /// Every installation the passive watcher should look for, as
    /// `(game_id, install_dir, executable)`.
    ///
    /// Deliberately *not* filtered to rows with a non-NULL `executable`, which
    /// is what the watcher used to do. Steam installations carry no
    /// executable — Steam is launched by URI and resolves the binary itself —
    /// so filtering them out meant a Steam session opened by `launch_game` was
    /// never matched to a running process and was stopped on the very next
    /// watcher tick. Steam is the primary source, so effectively all launcher
    /// playtime was recorded as 0–5 seconds.
    ///
    /// Only auto-managed, present installations are watched; a `deleted` row
    /// cannot have a running process worth attributing.
    pub async fn list_watch_targets(&self) -> AppResult<Vec<(String, String, Option<String>)>> {
        Ok(sqlx::query_as(
            r#"
            SELECT gi.game_id, gi.install_dir, gi.executable
            FROM game_installations gi
            WHERE gi.status IN ('installed', 'offline', 'missing')
            "#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Close sessions left open by a previous run, returning how many.
    ///
    /// A session row is only closed by `stop_session`, so quitting NOVARA (or
    /// crashing) while a game was running left `ended_at IS NULL` and
    /// `duration_seconds = 0` forever. Those rows are never reconciled: the
    /// in-memory `active` map starts empty on the next launch, so nothing owns
    /// them any more.
    ///
    /// The real duration is unknowable after the fact, so nothing is credited
    /// to `games.total_playtime_seconds` — inventing a figure would corrupt
    /// analytics more quietly than leaving it at zero. `ended_at` is set to
    /// `started_at` so the row stops claiming to be in progress.
    pub async fn close_orphaned_sessions(&self) -> AppResult<u64> {
        let result = sqlx::query(
            "UPDATE play_sessions \
             SET ended_at = started_at, duration_seconds = 0 \
             WHERE ended_at IS NULL",
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete a session that never actually began, crediting no playtime.
    ///
    /// Used when a launch is observed to have failed: `launch_game` opens a
    /// session optimistically, and if no matching process ever appears there
    /// is nothing to record. Recording a zero-length session instead would put
    /// a phantom entry in the Timeline for a game that never ran.
    pub async fn discard_session(&self, session_id: i64) -> AppResult<()> {
        let game_id: Option<String> =
            sqlx::query_scalar("SELECT game_id FROM play_sessions WHERE id = ?1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        sqlx::query("DELETE FROM play_sessions WHERE id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        // `start_session` optimistically stamps `games.last_played_at`, so
        // discarding the session must also roll that back or the game claims a
        // play it never had — and "last played" drives sort order and the
        // Dashboard's featured pick. Recomputed from the sessions that remain
        // rather than nulled, so earlier real plays survive.
        if let Some(game_id) = game_id {
            sqlx::query(
                "UPDATE games SET last_played_at = \
                   (SELECT MAX(started_at) FROM play_sessions WHERE game_id = ?1) \
                 WHERE id = ?1",
            )
            .bind(&game_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_sessions(&self, game_id: Option<&str>, limit: i64) -> AppResult<Vec<PlaySession>> {
        let rows = if let Some(g) = game_id {
            sqlx::query_as::<_, PlaySession>(
                "SELECT * FROM play_sessions WHERE game_id = ?1 ORDER BY started_at DESC LIMIT ?2",
            )
            .bind(g)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, PlaySession>(
                "SELECT * FROM play_sessions ORDER BY started_at DESC LIMIT ?1",
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }
}
