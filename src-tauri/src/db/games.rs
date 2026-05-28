//! Game repository: list, get, upsert, merge.

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{now_rfc3339, Game, Installation};

use super::Db;

#[derive(Debug, Clone)]
pub struct UpsertGame<'a> {
    pub title: &'a str,
    pub source_code: &'a str,        // sources.code
    pub source_app_id: Option<&'a str>,
    pub install_dir: &'a str,
    pub executable: Option<&'a str>,
    pub install_size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
pub struct UpsertResult {
    pub game_id_owned: Uuid,
    pub created: bool,
}

impl Db {
    /// Insert or look up a game by (source, source_app_id) or (install_dir).
    /// Returns the canonical game id and whether it was newly created.
    pub async fn upsert_game(&self, input: UpsertGame<'_>) -> AppResult<UpsertResult> {
        let mut tx = self.pool.begin().await?;

        // 1. Find source id.
        let source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE code = ?1")
            .bind(input.source_code)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| AppError::Invalid(format!("unknown source: {}", input.source_code)))?;

        // 2. Look for an existing installation that matches.
        let existing: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT game_id FROM game_installations
            WHERE install_dir = ?1
               OR (source_id = ?2 AND source_app_id IS NOT NULL AND source_app_id = ?3)
            LIMIT 1
            "#,
        )
        .bind(input.install_dir)
        .bind(source_id)
        .bind(input.source_app_id)
        .fetch_optional(&mut *tx)
        .await?;

        let (game_id, created) = if let Some((id,)) = existing {
            (id, false)
        } else {
            // 3. Create the game row.
            let id = Uuid::new_v4().to_string();
            let now = now_rfc3339();
            let sort = normalize_sort_title(input.title);
            sqlx::query(
                r#"
                INSERT INTO games (id, title, sort_title, added_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?4)
                "#,
            )
            .bind(&id)
            .bind(input.title)
            .bind(sort)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            (id, true)
        };

        // 4. Upsert the installation row (idempotent on install_dir).
        let install_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO game_installations
              (id, game_id, source_id, install_dir, executable, source_app_id,
               install_size_bytes, is_primary, detected_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)
            ON CONFLICT(install_dir) DO UPDATE SET
              executable = excluded.executable,
              source_app_id = excluded.source_app_id,
              install_size_bytes = excluded.install_size_bytes,
              detected_at = excluded.detected_at
            "#,
        )
        .bind(install_id)
        .bind(&game_id)
        .bind(source_id)
        .bind(input.install_dir)
        .bind(input.executable)
        .bind(input.source_app_id)
        .bind(input.install_size_bytes)
        .bind(now_rfc3339())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(UpsertResult {
            game_id_owned: Uuid::parse_str(&game_id)
                .map_err(|e| AppError::Other(e.to_string()))?,
            created,
        })
    }

    pub async fn list_games(&self, include_hidden: bool) -> AppResult<Vec<Game>> {
        let rows = if include_hidden {
            sqlx::query_as::<_, Game>(
                "SELECT * FROM games ORDER BY is_favorite DESC, sort_title ASC",
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Game>(
                "SELECT * FROM games WHERE is_hidden = 0 ORDER BY is_favorite DESC, sort_title ASC",
            )
            .fetch_all(&self.pool)
            .await?
        };
        Ok(rows)
    }

    pub async fn get_game(&self, id: &str) -> AppResult<Option<Game>> {
        Ok(sqlx::query_as::<_, Game>("SELECT * FROM games WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_installations(&self, game_id: &str) -> AppResult<Vec<Installation>> {
        Ok(sqlx::query_as::<_, Installation>(
            "SELECT * FROM game_installations WHERE game_id = ?1 ORDER BY is_primary DESC",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn set_favorite(&self, id: &str, fav: bool) -> AppResult<()> {
        sqlx::query("UPDATE games SET is_favorite = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(fav as i64)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_completion(&self, id: &str, pct: f64, state: &str) -> AppResult<()> {
        sqlx::query(
            "UPDATE games SET completion_pct = ?1, completion_state = ?2, updated_at = ?3 WHERE id = ?4",
        )
        .bind(pct.clamp(0.0, 100.0))
        .bind(state)
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_notes(&self, id: &str, notes: Option<&str>) -> AppResult<()> {
        sqlx::query("UPDATE games SET user_notes = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(notes)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Merge `from_id` into `to_id`: reparent installations, achievements,
    /// sessions, saves, mods, media; then delete the source row.
    pub async fn merge_games(&self, from_id: &str, to_id: &str) -> AppResult<()> {
        if from_id == to_id {
            return Err(AppError::Invalid("cannot merge a game into itself".into()));
        }
        let mut tx = self.pool.begin().await?;
        for table in [
            "game_installations",
            "achievements",
            "play_sessions",
            "save_profiles",
            "mods",
            "media",
            "game_genres",
        ] {
            let sql = format!("UPDATE {table} SET game_id = ?1 WHERE game_id = ?2");
            sqlx::query(&sql)
                .bind(to_id)
                .bind(from_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM games WHERE id = ?1")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Strip leading articles for sorting. "The Witcher 3" → "Witcher 3".
pub fn normalize_sort_title(title: &str) -> String {
    let t = title.trim();
    for prefix in ["The ", "A ", "An "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    t.to_string()
}
