use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{now_rfc3339, Achievement};

use super::Db;

pub struct NewAchievement<'a> {
    pub game_id: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub category: Option<&'a str>,
    pub points: i64,
    pub is_secret: bool,
}

impl Db {
    pub async fn list_achievements(&self, game_id: &str) -> AppResult<Vec<Achievement>> {
        Ok(sqlx::query_as::<_, Achievement>(
            "SELECT * FROM achievements WHERE game_id = ?1 ORDER BY sort_order, name",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn create_achievement(&self, input: NewAchievement<'_>) -> AppResult<Achievement> {
        let id = Uuid::new_v4().to_string();
        let next_order: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM achievements WHERE game_id = ?1",
        )
        .bind(input.game_id)
        .fetch_one(&self.pool)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO achievements (id, game_id, name, description, category, points, is_secret, sort_order)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&id)
        .bind(input.game_id)
        .bind(input.name)
        .bind(input.description)
        .bind(input.category)
        .bind(input.points)
        .bind(input.is_secret as i64)
        .bind(next_order)
        .execute(&self.pool)
        .await?;

        Ok(sqlx::query_as::<_, Achievement>(
            "SELECT * FROM achievements WHERE id = ?1",
        )
        .bind(&id)
        .fetch_one(&self.pool)
        .await?)
    }

    /// Toggle to unlocked/locked. Returns the new state and the points
    /// delta (useful for the event emitter).
    pub async fn toggle_achievement(&self, id: &str) -> AppResult<(bool, i64)> {
        let row: (i64, i64) =
            sqlx::query_as("SELECT is_unlocked, points FROM achievements WHERE id = ?1")
                .bind(id)
                .fetch_one(&self.pool)
                .await?;

        let new_state = row.0 == 0;
        let unlocked_at = new_state.then(now_rfc3339);

        sqlx::query("UPDATE achievements SET is_unlocked = ?1, unlocked_at = ?2 WHERE id = ?3")
            .bind(new_state as i64)
            .bind(unlocked_at)
            .bind(id)
            .execute(&self.pool)
            .await?;

        // Recompute completion% on the game based on unlocked / total.
        let game_id: String = sqlx::query_scalar("SELECT game_id FROM achievements WHERE id = ?1")
            .bind(id)
            .fetch_one(&self.pool)
            .await?;
        self.recompute_completion(&game_id).await?;

        Ok((new_state, row.1))
    }

    pub async fn delete_achievement(&self, id: &str) -> AppResult<()> {
        let game_id: Option<String> =
            sqlx::query_scalar("SELECT game_id FROM achievements WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        sqlx::query("DELETE FROM achievements WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if let Some(g) = game_id {
            self.recompute_completion(&g).await?;
        }
        Ok(())
    }

    async fn recompute_completion(&self, game_id: &str) -> AppResult<()> {
        let (total, unlocked): (i64, i64) = sqlx::query_as(
            r#"SELECT
                 COUNT(*),
                 COALESCE(SUM(is_unlocked), 0)
               FROM achievements WHERE game_id = ?1"#,
        )
        .bind(game_id)
        .fetch_one(&self.pool)
        .await?;

        let pct = if total == 0 { 0.0 } else { (unlocked as f64 / total as f64) * 100.0 };
        sqlx::query("UPDATE games SET completion_pct = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(pct)
            .bind(now_rfc3339())
            .bind(game_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
