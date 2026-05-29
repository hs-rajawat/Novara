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
    /// Pass `true` when the caller (not the scanner) chose this executable.
    /// Prevents future rescans from overwriting the user's choice.
    pub executable_override: bool,
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
        //
        // On conflict the executable is preserved when the user has manually
        // overridden it (executable_override = 1); scanner-detected values
        // are accepted only when the override flag is 0.
        // The override flag itself is set to MAX(existing, incoming) so a
        // manual import on an already-scanned game latches the flag to 1.
        let install_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO game_installations
              (id, game_id, source_id, install_dir, executable, source_app_id,
               install_size_bytes, is_primary, detected_at, executable_override)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)
            ON CONFLICT(install_dir) DO UPDATE SET
              executable = CASE WHEN game_installations.executable_override = 1
                                THEN game_installations.executable
                                ELSE excluded.executable END,
              executable_override = MAX(game_installations.executable_override,
                                        excluded.executable_override),
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
        .bind(input.executable_override as i64)
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

    /// Override the executable for a single installation, locking it so
    /// subsequent scans leave it untouched.
    ///
    /// `exe_path` is an absolute path; the method stores a path relative to
    /// `install_dir` when the file is inside the install directory, otherwise
    /// it stores the absolute path (works because `Path::join` on an absolute
    /// component discards the base on all platforms).
    pub async fn set_installation_executable(
        &self,
        id: &str,
        exe_path: &str,
    ) -> AppResult<()> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT install_dir FROM game_installations WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((install_dir,)) = row else {
            return Err(AppError::NotFound(format!("installation not found: {id}")));
        };

        let exe = std::path::Path::new(exe_path);
        let stored = exe
            .strip_prefix(&install_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| exe_path.to_string());

        sqlx::query(
            "UPDATE game_installations \
             SET executable = ?1, executable_override = 1 \
             WHERE id = ?2",
        )
        .bind(stored)
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
