//! Game repository: list, get, upsert, merge.

use std::collections::HashMap;

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::integrity::{resolve_status, InstallStatus};
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
    /// Source-specific installed/not-installed evidence gathered during
    /// this scan (e.g. Steam's ACF `StateFlags`), if any — see
    /// `scanner::DetectedGame::install_state_hint`. `None` falls back to
    /// the generic `resolve_status` (install_dir/executable) check.
    pub install_state_hint: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub struct UpsertResult {
    pub game_id_owned: Uuid,
    pub created: bool,
    /// Whether this installation's Library Integrity status changed as a
    /// result of this upsert (including a brand-new row). `false` when the
    /// row wasn't auto-managed (a future manual state) and so was left
    /// alone regardless of what the disk check resolved to.
    pub status_changed: bool,
}

/// Lightweight projection used by the integrity verifier — avoids pulling
/// every `Installation` column for a full-table scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InstallationCheckRow {
    pub id: String,
    pub game_id: String,
    pub install_dir: String,
    pub executable: Option<String>,
    pub status: String,
    pub source_app_id: Option<String>,
    pub source_code: String,
}

/// A game's primary installation, as surfaced on the bulk `Game`/
/// `GameWithInstalls` views (platform badge + integrity status) without
/// requiring a full `Installation` record.
#[derive(Debug, Clone)]
pub struct PrimaryInstallation {
    pub source_code: String,
    pub source_label: String,
    pub status: String,
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

        // 3b. Library Integrity System: resolve this installation's status
        // *now*, rather than assuming a scanner re-observing this
        // install_dir means the installation is still there (it doesn't —
        // e.g. Steam's .acf manifest can outlive a manually-deleted game
        // folder). When the scanner already gathered source-specific
        // evidence this scan pass (e.g. Steam's StateFlags — see
        // `UpsertGame::install_state_hint`), trust it directly with zero
        // extra I/O; otherwise fall back to the generic executable/dir
        // check. This is the one place scanning and status resolution
        // meet; scanners themselves never need to call `resolve_status`.
        let prev_status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM game_installations WHERE install_dir = ?1",
        )
        .bind(input.install_dir)
        .fetch_optional(&mut *tx)
        .await?;
        let resolved = match input.install_state_hint {
            Some(true) => InstallStatus::Installed,
            Some(false) => InstallStatus::Missing,
            None => resolve_status(input.install_dir, input.executable),
        };
        let now = now_rfc3339();

        // 4. Upsert the installation row (idempotent on install_dir).
        //
        // On conflict the executable is preserved when the user has manually
        // overridden it (executable_override = 1); scanner-detected values
        // are accepted only when the override flag is 0.
        // The override flag itself is set to MAX(existing, incoming) so a
        // manual import on an already-scanned game latches the flag to 1.
        //
        // `status` only takes the freshly-resolved value when the existing
        // row is in an auto-managed state (installed/missing); a future
        // manual state (ignored, archived, ...) is left untouched by scans.
        let install_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO game_installations
              (id, game_id, source_id, install_dir, executable, source_app_id,
               install_size_bytes, is_primary, detected_at, executable_override,
               status, last_verified_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11)
            ON CONFLICT(install_dir) DO UPDATE SET
              executable = CASE WHEN game_installations.executable_override = 1
                                THEN game_installations.executable
                                ELSE excluded.executable END,
              executable_override = MAX(game_installations.executable_override,
                                        excluded.executable_override),
              source_app_id = excluded.source_app_id,
              install_size_bytes = excluded.install_size_bytes,
              detected_at = excluded.detected_at,
              status = CASE WHEN game_installations.status IN ('installed', 'missing')
                            THEN excluded.status
                            ELSE game_installations.status END,
              last_verified_at = excluded.last_verified_at
            "#,
        )
        .bind(install_id)
        .bind(&game_id)
        .bind(source_id)
        .bind(input.install_dir)
        .bind(input.executable)
        .bind(input.source_app_id)
        .bind(input.install_size_bytes)
        .bind(&now)
        .bind(input.executable_override as i64)
        .bind(resolved.as_str())
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Mirrors the SQL guard above: only "changed" if the row was (or is
        // now) in an auto-managed state and the resolved value differs.
        let prev_auto_managed = prev_status
            .as_deref()
            .map_or(true, crate::integrity::is_auto_managed);
        let status_changed =
            prev_auto_managed && prev_status.as_deref() != Some(resolved.as_str());

        Ok(UpsertResult {
            game_id_owned: Uuid::parse_str(&game_id)
                .map_err(|e| AppError::Other(e.to_string()))?,
            created,
            status_changed,
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

    /// (source code, source display name) for each game's primary
    /// installation, keyed by game_id. Used to render platform badges
    /// without an N+1 IPC call per card.
    ///
    /// `is_primary` isn't a real unique constraint (every scanned
    /// installation is inserted with `is_primary = 1`), so ties are broken
    /// by earliest `detected_at` and the first match per game wins.
    pub async fn list_primary_sources(&self) -> AppResult<HashMap<String, PrimaryInstallation>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            r#"
            SELECT gi.game_id, s.code, s.display_name, gi.status
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            ORDER BY gi.game_id, gi.is_primary DESC, gi.detected_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut map = HashMap::with_capacity(rows.len());
        for (game_id, source_code, source_label, status) in rows {
            map.entry(game_id).or_insert(PrimaryInstallation {
                source_code,
                source_label,
                status,
            });
        }
        Ok(map)
    }

    /// Single-game counterpart of [`Db::list_primary_sources`], for the
    /// `get_game` detail path.
    pub async fn get_primary_source(&self, game_id: &str) -> AppResult<Option<PrimaryInstallation>> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT s.code, s.display_name, gi.status
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            WHERE gi.game_id = ?1
            ORDER BY gi.is_primary DESC, gi.detected_at ASC
            LIMIT 1
            "#,
        )
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(source_code, source_label, status)| PrimaryInstallation {
            source_code,
            source_label,
            status,
        }))
    }

    pub async fn list_installations(&self, game_id: &str) -> AppResult<Vec<Installation>> {
        Ok(sqlx::query_as::<_, Installation>(
            "SELECT * FROM game_installations WHERE game_id = ?1 ORDER BY is_primary DESC",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Every installation's disk-check inputs, for the Library Integrity
    /// System's background verifier (`crate::integrity::service::IntegrityService`).
    pub async fn list_installation_paths(&self) -> AppResult<Vec<InstallationCheckRow>> {
        Ok(sqlx::query_as::<_, InstallationCheckRow>(
            r#"
            SELECT gi.id, gi.game_id, gi.install_dir, gi.executable, gi.status,
                   gi.source_app_id, s.code AS source_code
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Persist a Library Integrity System status change and stamp
    /// `last_verified_at`, so the UI can show when this was last confirmed.
    pub async fn set_installation_status(&self, id: &str, status: &str) -> AppResult<()> {
        sqlx::query(
            "UPDATE game_installations SET status = ?1, last_verified_at = ?2 WHERE id = ?3",
        )
        .bind(status)
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Look up a source's `code` (e.g. "steam", "manual") by its numeric
    /// id — `launch_game` needs this to dispatch the specific installation
    /// it's about to launch through `integrity::resolve_installation_status`.
    pub async fn source_code_for(&self, source_id: i64) -> AppResult<String> {
        sqlx::query_scalar("SELECT code FROM sources WHERE id = ?1")
            .bind(source_id)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    /// "Remove from Library": hide the game from the default Library view
    /// via the existing `is_hidden` flag (already read by `list_games`).
    /// Deliberately non-destructive — no row is deleted, so playtime,
    /// sessions, achievements, saves, mods, and artwork are all preserved
    /// untouched. Pass `hidden = false` to restore it ("Restore to
    /// Library").
    pub async fn set_hidden(&self, id: &str, hidden: bool) -> AppResult<()> {
        sqlx::query("UPDATE games SET is_hidden = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(hidden as i64)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
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

    pub async fn set_cover_path(&self, id: &str, path: &str) -> AppResult<()> {
        sqlx::query("UPDATE games SET cover_path = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(path)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_hero_path(&self, id: &str, path: &str) -> AppResult<()> {
        sqlx::query("UPDATE games SET hero_path = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(path)
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
    /// subsequent scans leave it untouched. Also restores the Library
    /// Integrity status to Installed and stamps `last_verified_at` — the
    /// picked file necessarily exists (it came from a native file-picker
    /// dialog), so this is the "Locate Executable" recovery path for a
    /// Missing installation, in addition to the general "Browse" override.
    ///
    /// `exe_path` is an absolute path; the method stores a path relative to
    /// `install_dir` when the file is inside the install directory, otherwise
    /// it stores the absolute path (works because `Path::join` on an absolute
    /// component discards the base on all platforms).
    ///
    /// Returns the owning game's id so the caller can emit `GameUpdated`.
    pub async fn set_installation_executable(
        &self,
        id: &str,
        exe_path: &str,
    ) -> AppResult<String> {
        let row: Option<(String, String)> = sqlx::query_as(
            "SELECT install_dir, game_id FROM game_installations WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let Some((install_dir, game_id)) = row else {
            return Err(AppError::NotFound(format!("installation not found: {id}")));
        };

        let exe = std::path::Path::new(exe_path);
        let stored = exe
            .strip_prefix(&install_dir)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| exe_path.to_string());

        sqlx::query(
            "UPDATE game_installations \
             SET executable = ?1, executable_override = 1, status = 'installed', last_verified_at = ?2 \
             WHERE id = ?3",
        )
        .bind(stored)
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(game_id)
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
