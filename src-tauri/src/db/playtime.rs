use crate::error::AppResult;
use crate::models::{now_rfc3339, PlaySession};

use super::Db;

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

        Ok((game_id, active))
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
