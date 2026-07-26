//! Game repository: list, get, upsert, merge.

use std::collections::HashMap;

use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::integrity::{resolve_status, InstallStatus};
use crate::models::{now_rfc3339, Game, Installation};

use super::Db;

/// SQL `ORDER BY` fragment ranking an installation's health, lowest = best:
/// installed > offline > missing > deleted. An offline install is healthier
/// than a missing one — its files may be perfectly intact behind an
/// unplugged drive — so it must outrank missing when picking a game's
/// primary installation. Shared by every "which installation represents
/// this game" query so a stale ghost never wins over a live install.
const STATUS_PRIORITY_ORDER: &str = "CASE gi.status \
     WHEN 'installed' THEN 0 \
     WHEN 'offline' THEN 1 \
     WHEN 'missing' THEN 2 \
     WHEN 'deleted' THEN 3 \
     ELSE 4 END";

/// Rank an install status the same way [`STATUS_PRIORITY_ORDER`] does in SQL.
///
/// The two must agree. They previously did not: every "which installation
/// represents this game" *query* used the health-first CASE below, while
/// `launch_game` picked with `max_by_key(|i| i.is_primary)` — so Play could
/// act on a different installation than the one whose status the UI was
/// showing, which is precisely the confusion the ghost-row handling in
/// `upsert_game` exists to prevent.
fn status_rank(status: &str) -> u8 {
    match status {
        "installed" => 0,
        "offline" => 1,
        "missing" => 2,
        "deleted" => 3,
        _ => 4,
    }
}

/// Pick the installation that represents a game, in Rust, matching
/// [`STATUS_PRIORITY_ORDER`] exactly: healthiest status first, then the
/// `is_primary` flag as the tie-break.
///
/// Health outranks the flag deliberately — a stale `is_primary` ghost must
/// never win over a live install.
pub fn primary_installation(installs: &[Installation]) -> Option<&Installation> {
    installs
        .iter()
        .min_by_key(|i| (status_rank(&i.status), if i.is_primary != 0 { 0 } else { 1 }))
}

/// Who chose an installation's executable, which decides whose value wins.
///
/// This is *caller intent*, deliberately separate from the persisted
/// `game_installations.executable_override` flag that records "a user chose the
/// current value". Conflating the two is what broke manual imports: the upsert's
/// conflict clause kept the stored executable whenever `executable_override = 1`,
/// so once a user had chosen an executable, **their own later choice was silently
/// discarded too** — the guard could not tell a scanner overwriting a user's
/// choice (must be blocked) from the user making a new one (must win). Passing
/// `executable_override: true` only latched the flag; it granted no precedence.
///
/// Editing an installation directly (`set_installation_executable`) always
/// worked, because it bypasses the upsert entirely — which is exactly why the
/// import path looked inconsistent to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutableSource {
    /// A scanner heuristic picked it. Never overwrites a user's choice, and never
    /// sets the override flag.
    Scanner,
    /// The user picked it explicitly (Import Executable, or a manual import).
    /// Authoritative: it overwrites whatever is stored, including a previous user
    /// choice, and latches the override flag so later scans leave it alone.
    User,
}

impl ExecutableSource {
    fn is_user_choice(self) -> bool {
        matches!(self, Self::User)
    }
}

#[derive(Debug, Clone)]
pub struct UpsertGame<'a> {
    pub title: &'a str,
    pub source_code: &'a str,        // sources.code
    pub source_app_id: Option<&'a str>,
    pub install_dir: &'a str,
    pub executable: Option<&'a str>,
    pub install_size_bytes: Option<i64>,
    /// Who chose this executable — see [`ExecutableSource`].
    pub executable_source: ExecutableSource,
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
        //
        // The launcher's app-id is authoritative and must outrank an
        // `install_dir` coincidence. Without the explicit ORDER BY, this was
        // `... WHERE install_dir = ? OR (source + app_id) LIMIT 1` with no
        // ordering, so when a *different* game's row already occupied the
        // directory now being scanned, SQLite could return that game — and
        // this scan would then be attributed to it. That contradicts the
        // documented rule ("keyed off (source, source_app_id) first, falling
        // back to install_dir") and was the mechanism by which a launcher
        // move could damage an unrelated game's installation record.
        let existing: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT game_id FROM game_installations
            WHERE (source_id = ?2 AND source_app_id IS NOT NULL AND source_app_id = ?3)
               OR install_dir = ?1
            ORDER BY
              CASE WHEN source_id = ?2 AND source_app_id IS NOT NULL AND source_app_id = ?3
                   THEN 0 ELSE 1 END
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

        // Set when the directory being scanned is already registered to a
        // different game, which makes both the in-place relink and the
        // installation upsert unsafe.
        let mut destination_claimed_by_other = false;

        // 3a. Move detection (launcher-managed sources). When this game
        // already has an installation row matched by (source, source_app_id)
        // but at a *different* directory than the one now being scanned, the
        // game moved. Relink that existing row to the new directory in place
        // — preserving its id, history links, and any manual executable
        // override — instead of letting step 4 insert a second row and leave
        // the old one behind as a stale ghost (the ghost then corrupts
        // primary-installation selection). Any row already sitting at the
        // destination dir is removed first so the unique index on
        // `install_dir` can't collide. Only applies to app_id-bearing sources
        // (Steam, Epic); manual moves are a separate, more conservative Phase.
        if let Some(app_id) = input.source_app_id {
            let moved_row: Option<(String, String)> = sqlx::query_as(
                r#"
                SELECT id, install_dir FROM game_installations
                WHERE source_id = ?1 AND source_app_id = ?2 AND install_dir <> ?3
                LIMIT 1
                "#,
            )
            .bind(source_id)
            .bind(app_id)
            .bind(input.install_dir)
            .fetch_optional(&mut *tx)
            .await?;

            if let Some((moved_id, _old_dir)) = moved_row {
                // Clear the destination first, but only of rows this move is
                // entitled to remove: a ghost of this same launcher identity,
                // or a row already belonging to this same game.
                //
                // `idx_install_dir` is UNIQUE across the whole table, so the
                // row sitting at the destination may belong to a *different*
                // game. Deleting it unconditionally — as this did — silently
                // destroyed that game's installation record and its manual
                // executable override.
                //
                // The decision is made in Rust rather than in the `WHERE`
                // clause on purpose. Expressing it in SQL requires comparing
                // `source_app_id`, which is NULL for every manual
                // installation, and in SQL `NULL = '123'` is NULL rather than
                // false — so `NOT (source_id = ? AND source_app_id = ?)`
                // evaluates to NULL and silently matches nothing. That is
                // precisely the trap that made the first version of this fix
                // look correct while still deleting the other game's row.
                let occupants: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
                    r#"
                    SELECT id, game_id, source_id, source_app_id
                    FROM game_installations
                    WHERE install_dir = ?1 AND id <> ?2
                    "#,
                )
                .bind(input.install_dir)
                .bind(&moved_id)
                .fetch_all(&mut *tx)
                .await?;

                // A row may be cleared only when it is this same game's, or a
                // ghost carrying this exact launcher identity.
                let may_clear = |game: &str, src: i64, app: Option<&str>| {
                    game == game_id || (src == source_id && app == Some(app_id))
                };
                let foreign = occupants
                    .iter()
                    .find(|(_, g, src, app)| !may_clear(g, *src, app.as_deref()));

                if let Some((_, other_game, _, _)) = foreign {
                    // Two different games claiming one folder is a
                    // duplicate-detection question, not a move — and
                    // resolving it automatically is exactly what Integrity
                    // Phase 2 defers as too risky. Skip the relink *and* the
                    // installation upsert below, and leave both rows intact.
                    //
                    // Skipping step 4 matters: its `ON CONFLICT(install_dir)
                    // DO UPDATE` does not reassign `game_id`, so letting it
                    // run would stamp this game's `source_app_id`, size and
                    // status onto the other game's row while leaving it
                    // parented to that other game — corrupting it in place
                    // instead of deleting it. This game keeps its existing
                    // row at the old directory; the periodic integrity sweep
                    // resolves whichever install is genuinely gone.
                    tracing::warn!(
                        install_dir = %input.install_dir,
                        other_game = %other_game,
                        "skipping move relink: destination is claimed by a different game"
                    );
                    destination_claimed_by_other = true;
                } else {
                    for (occupant_id, _, _, _) in &occupants {
                        sqlx::query("DELETE FROM game_installations WHERE id = ?1")
                            .bind(occupant_id)
                            .execute(&mut *tx)
                            .await?;
                    }
                    sqlx::query("UPDATE game_installations SET install_dir = ?1 WHERE id = ?2")
                        .bind(input.install_dir)
                        .bind(&moved_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
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
        // Skipped entirely when the directory is registered to a different
        // game (see 3a): `ON CONFLICT(install_dir) DO UPDATE` does not
        // reassign `game_id`, so running it would overwrite that game's
        // `source_app_id`, size and status while leaving the row parented to
        // it. Doing nothing keeps both games' records truthful.
        //
        // Executable precedence is decided by *who is asking* (`?12`, from
        // `UpsertGame::executable_source`), not by the stored flag alone:
        //   * a user's explicit choice always wins, including over a previous
        //     user choice — this is the manual-import fix;
        //   * otherwise a stored user choice is preserved against scanners;
        //   * otherwise the scanner's detection is accepted.
        // The override flag is still MAX'd, so it latches once a user has
        // chosen and a later scan cannot clear it.
        //
        // `status` only takes the freshly-resolved value when the existing
        // row is in an auto-managed state (installed/missing); a future
        // manual state (ignored, archived, ...) is left untouched by scans.
        if !destination_claimed_by_other {
            let install_id = Uuid::new_v4().to_string();
            let user_choice = input.executable_source.is_user_choice();
            sqlx::query(
                r#"
            INSERT INTO game_installations
              (id, game_id, source_id, install_dir, executable, source_app_id,
               install_size_bytes, is_primary, detected_at, executable_override,
               status, last_verified_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10, ?11)
            ON CONFLICT(install_dir) DO UPDATE SET
              executable = CASE
                             WHEN ?12 = 1 THEN excluded.executable
                             WHEN game_installations.executable_override = 1
                               THEN game_installations.executable
                             ELSE excluded.executable
                           END,
              executable_override = MAX(game_installations.executable_override,
                                        excluded.executable_override),
              source_app_id = excluded.source_app_id,
              install_size_bytes = excluded.install_size_bytes,
              detected_at = excluded.detected_at,
              status = CASE WHEN game_installations.status IN ('installed', 'missing', 'deleted', 'offline')
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
            .bind(i64::from(user_choice))
            .bind(resolved.as_str())
            .bind(&now)
            .bind(i64::from(user_choice))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        // Mirrors the SQL guard above: only "changed" if the row was (or is
        // now) in an auto-managed state and the resolved value differs. A
        // skipped upsert changed nothing, so it cannot report a change.
        let prev_auto_managed = prev_status
            .as_deref()
            .map_or(true, crate::integrity::is_auto_managed);
        let status_changed = !destination_claimed_by_other
            && prev_auto_managed
            && prev_status.as_deref() != Some(resolved.as_str());

        Ok(UpsertResult {
            game_id_owned: Uuid::parse_str(&game_id)
                .map_err(|e| AppError::Other(e.to_string()))?,
            created,
            status_changed,
        })
    }

    /// The executable recorded for the installation at `install_dir`, if any.
    ///
    /// Used by `import_executable` to confirm the user's choice actually landed,
    /// rather than trusting that the upsert applied it.
    pub async fn installation_executable(
        &self,
        install_dir: &str,
    ) -> AppResult<Option<Option<String>>> {
        Ok(
            sqlx::query_scalar("SELECT executable FROM game_installations WHERE install_dir = ?1")
                .bind(install_dir)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// The size already recorded for an installation at `install_dir`, if any.
    ///
    /// Used to avoid re-measuring a folder the scanner has seen before —
    /// walking every file of every game on every scan was the single most
    /// expensive operation in a scan pass.
    pub async fn known_install_size(&self, install_dir: &str) -> AppResult<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT install_size_bytes FROM game_installations WHERE install_dir = ?1",
        )
        .bind(install_dir)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
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
    /// by install health first — an `installed` row must always outrank a
    /// stale `offline`/`missing`/`deleted` one, or a leftover ghost would
    /// mislabel a perfectly-installed game as Missing — then by earliest
    /// `detected_at`. The first match per game wins.
    pub async fn list_primary_sources(&self) -> AppResult<HashMap<String, PrimaryInstallation>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            &r#"
            SELECT gi.game_id, s.code, s.display_name, gi.status
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            ORDER BY gi.game_id, gi.is_primary DESC, {STATUS_PRIORITY}, gi.detected_at ASC
            "#
            .replace("{STATUS_PRIORITY}", STATUS_PRIORITY_ORDER),
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
    /// `get_game` detail path. Same health-first tiebreak so an installed
    /// row always wins over a stale ghost.
    pub async fn get_primary_source(&self, game_id: &str) -> AppResult<Option<PrimaryInstallation>> {
        let row: Option<(String, String, String)> = sqlx::query_as(
            &r#"
            SELECT s.code, s.display_name, gi.status
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            WHERE gi.game_id = ?1
            ORDER BY gi.is_primary DESC, {STATUS_PRIORITY}, gi.detected_at ASC
            LIMIT 1
            "#
            .replace("{STATUS_PRIORITY}", STATUS_PRIORITY_ORDER),
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

    /// Relink a moved installation to `new_dir` in place, preserving the
    /// row's id and every history link that hangs off its `game_id`
    /// (sessions, achievements, saves, mods, media). Used by the background
    /// verifier when a launcher reports an app installed at a directory
    /// different from the stored one — the alternative (insert-new +
    /// leave-old) would strand a stale ghost row and split primary
    /// selection. Sets the row back to `installed` and stamps
    /// `last_verified_at`; the relink is only ever triggered by a confirmed
    /// present install.
    ///
    /// Any pre-existing row already sitting at `new_dir` (a leftover ghost)
    /// is deleted first, inside a transaction, so the unique index on
    /// `install_dir` cannot collide. Returns the owning `game_id` so the
    /// caller can emit `GameUpdated`.
    pub async fn relink_installation(&self, id: &str, new_dir: &str) -> AppResult<String> {
        let mut tx = self.pool.begin().await?;
        let game_id: Option<String> =
            sqlx::query_scalar("SELECT game_id FROM game_installations WHERE id = ?1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(game_id) = game_id else {
            return Err(AppError::NotFound(format!("installation not found: {id}")));
        };

        // Clear the destination, but only of rows belonging to this same
        // game. `idx_install_dir` is UNIQUE table-wide, so an unscoped delete
        // here would silently destroy a different game's installation.
        //
        // This path is user-initiated (Locate Executable / launch-time
        // relink), so a genuine conflict is reported rather than resolved
        // silently: the user is the only one who can say which game really
        // lives in that folder.
        let foreign: Option<String> = sqlx::query_scalar(
            "SELECT game_id FROM game_installations \
             WHERE install_dir = ?1 AND id <> ?2 AND game_id <> ?3 LIMIT 1",
        )
        .bind(new_dir)
        .bind(id)
        .bind(&game_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(other_game) = foreign {
            let title: Option<String> =
                sqlx::query_scalar("SELECT title FROM games WHERE id = ?1")
                    .bind(&other_game)
                    .fetch_optional(&mut *tx)
                    .await?;
            return Err(AppError::Invalid(format!(
                "that folder is already registered to another game{}",
                title.map(|t| format!(" ({t})")).unwrap_or_default()
            )));
        }
        sqlx::query(
            "DELETE FROM game_installations \
             WHERE install_dir = ?1 AND id <> ?2 AND game_id = ?3",
        )
        .bind(new_dir)
        .bind(id)
        .bind(&game_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE game_installations \
             SET install_dir = ?1, status = 'installed', last_verified_at = ?2 \
             WHERE id = ?3",
        )
        .bind(new_dir)
        .bind(now_rfc3339())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(game_id)
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

    pub async fn set_logo_path(&self, id: &str, path: &str) -> AppResult<()> {
        sqlx::query("UPDATE games SET logo_path = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(path)
            .bind(now_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_icon_path(&self, id: &str, path: &str) -> AppResult<()> {
        sqlx::query("UPDATE games SET icon_path = ?1, updated_at = ?2 WHERE id = ?3")
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

    /// `(source code, source_app_id)` for every installation of `game_id`
    /// that carries one — what `metadata::identity::identity_for` turns
    /// into `GameIdentifier::SourceAppId` entries for provider lookup. One
    /// query per game (same cost shape as `get_primary_source`), not a
    /// full-table join, since text/artwork fills iterate games one at a
    /// time already.
    pub async fn list_source_app_ids(&self, game_id: &str) -> AppResult<Vec<(String, String)>> {
        Ok(sqlx::query_as::<_, (String, String)>(
            r#"
            SELECT s.code, gi.source_app_id
            FROM game_installations gi
            JOIN sources s ON s.id = gi.source_id
            WHERE gi.game_id = ?1 AND gi.source_app_id IS NOT NULL
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Persist a `MetadataTextProvider` result. Only the fields the provider
    /// actually supplied are written — `COALESCE` against the existing
    /// column means a provider that left e.g. `publisher` empty never blanks
    /// out a value a different (or earlier) provider already set.
    /// `metadata_source` is stamped unconditionally to the provider's code
    /// so a subsequent manual edit (which sets it to `'manual'` — see
    /// `set_game_metadata_manual`, once the frontend edit path exists) is
    /// the only thing `MetadataService::fill_missing` treats as "never
    /// touch this again."
    ///
    /// Genres replace the game's existing `game_genres` rows outright
    /// (rather than merging) — a provider's genre list is authoritative for
    /// that provider's result, and re-running the same provider should not
    /// accumulate stale tags from a prior, different response.
    pub async fn set_game_metadata(
        &self,
        id: &str,
        meta: &crate::metadata::GameMetadata,
        source: &str,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;
        let now = now_rfc3339();
        sqlx::query(
            r#"
            UPDATE games SET
              description = COALESCE(?1, description),
              release_year = COALESCE(?2, release_year),
              developer = COALESCE(?3, developer),
              publisher = COALESCE(?4, publisher),
              metadata_json = COALESCE(?5, metadata_json),
              metadata_source = ?6,
              updated_at = ?7
            WHERE id = ?8
            "#,
        )
        .bind(&meta.description)
        .bind(meta.release_year)
        .bind(&meta.developer)
        .bind(&meta.publisher)
        .bind(&meta.raw_json)
        .bind(source)
        .bind(&now)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        if !meta.genres.is_empty() {
            sqlx::query("DELETE FROM game_genres WHERE game_id = ?1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            for genre in &meta.genres {
                sqlx::query("INSERT OR IGNORE INTO genres (name) VALUES (?1)")
                    .bind(genre)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    r#"
                    INSERT OR IGNORE INTO game_genres (game_id, genre_id)
                    SELECT ?1, id FROM genres WHERE name = ?2
                    "#,
                )
                .bind(id)
                .bind(genre)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// Merge `from_id` into `to_id`, then delete the source row.
    ///
    /// Every table that references `games(id)` is handled here, and the list
    /// is exhaustive as of migration 0006: `game_installations`,
    /// `achievements`, `play_sessions`, `save_profiles`, `mods`, `media`,
    /// `game_genres`, `artwork_assets`.
    ///
    /// Three classes of table, handled differently because a blanket
    /// `UPDATE ... SET game_id` is only correct for the first:
    ///
    /// 1. **Surrogate primary key, no per-game uniqueness** — reparented with
    ///    a plain `UPDATE`. Nothing can collide.
    /// 2. **Per-game uniqueness constraint** — `game_genres`
    ///    (`PRIMARY KEY (game_id, genre_id)`) and `artwork_assets`
    ///    (`UNIQUE(game_id, kind)`). An `UPDATE` here violates the constraint
    ///    whenever both games share a genre or an artwork kind, which aborts
    ///    the whole transaction and makes the merge fail outright. These are
    ///    reparented as "insert what does not collide, then drop the rest",
    ///    so the survivor's own row always wins.
    /// 3. **Values cached on `games` rather than derived on read** —
    ///    `total_playtime_seconds`, `last_played_at` and `completion_pct`.
    ///    Moving `play_sessions` and `achievements` rows does not update
    ///    these, so without the explicit fix-up below a merge silently loses
    ///    the absorbed game's playtime from every screen that reads the
    ///    cached column.
    ///
    /// `artwork_assets` was previously missing from the reparent list
    /// entirely, so `ON DELETE CASCADE` destroyed the absorbed game's whole
    /// artwork ledger — including `user_locked` flags recording deliberate
    /// user choices.
    pub async fn merge_games(&self, from_id: &str, to_id: &str) -> AppResult<()> {
        if from_id == to_id {
            return Err(AppError::Invalid("cannot merge a game into itself".into()));
        }
        let mut tx = self.pool.begin().await?;

        // Both games must exist. Without this the merge is a silent no-op on
        // unknown ids: every UPDATE matches zero rows, the DELETE matches
        // zero rows, and the command reports success for work it never did.
        for id in [from_id, to_id] {
            let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM games WHERE id = ?1")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound(format!("game not found: {id}")));
            }
        }

        // Class 1: safe to reparent wholesale.
        for table in [
            "game_installations",
            "achievements",
            "play_sessions",
            "save_profiles",
            "mods",
            "media",
        ] {
            let sql = format!("UPDATE {table} SET game_id = ?1 WHERE game_id = ?2");
            sqlx::query(&sql)
                .bind(to_id)
                .bind(from_id)
                .execute(&mut *tx)
                .await?;
        }

        // Class 2a: genres. `INSERT OR IGNORE` skips genres the survivor
        // already has; the follow-up delete clears what remains.
        sqlx::query(
            "INSERT OR IGNORE INTO game_genres (game_id, genre_id) \
             SELECT ?1, genre_id FROM game_genres WHERE game_id = ?2",
        )
        .bind(to_id)
        .bind(from_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM game_genres WHERE game_id = ?1")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;

        // Class 2b: artwork ledger. The survivor's existing row for a kind
        // wins — a merge is not a licence to replace artwork the survivor
        // already has, including anything the user locked. The absorbed
        // game's rows only fill kinds the survivor has no row for at all.
        sqlx::query(
            "INSERT OR IGNORE INTO artwork_assets \
               (game_id, kind, source, remote_url, local_path, state, etag, \
                user_locked, fetched_at, updated_at) \
             SELECT ?1, kind, source, remote_url, local_path, state, etag, \
                    user_locked, fetched_at, updated_at \
             FROM artwork_assets WHERE game_id = ?2",
        )
        .bind(to_id)
        .bind(from_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM artwork_assets WHERE game_id = ?1")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;

        // Keep the render columns consistent with the ledger just moved.
        // `games.*_path` is what the UI actually reads, so a ledger row
        // adopted for a kind the survivor has no path for would otherwise be
        // recorded as `ready` while nothing rendered.
        for (column, kind) in [
            ("cover_path", "cover"),
            ("hero_path", "hero"),
            ("logo_path", "logo"),
            ("icon_path", "icon"),
        ] {
            let sql = format!(
                "UPDATE games SET {column} = COALESCE({column}, \
                   (SELECT local_path FROM artwork_assets \
                    WHERE game_id = ?1 AND kind = ?2 AND state = 'ready')) \
                 WHERE id = ?1"
            );
            sqlx::query(&sql)
                .bind(to_id)
                .bind(kind)
                .execute(&mut *tx)
                .await?;
        }

        // Class 3: cached aggregates. Playtime is summed and the more recent
        // `last_played_at` wins; NULL-safe because either side may never have
        // been played.
        sqlx::query(
            "UPDATE games SET \
               total_playtime_seconds = total_playtime_seconds \
                 + COALESCE((SELECT total_playtime_seconds FROM games WHERE id = ?2), 0), \
               last_played_at = MAX(COALESCE(last_played_at, ''), \
                 COALESCE((SELECT last_played_at FROM games WHERE id = ?2), '')), \
               updated_at = ?3 \
             WHERE id = ?1",
        )
        .bind(to_id)
        .bind(from_id)
        .bind(now_rfc3339())
        .execute(&mut *tx)
        .await?;
        // MAX over '' leaves an empty string when neither game was played;
        // normalize it back to NULL so "never played" stays representable.
        sqlx::query("UPDATE games SET last_played_at = NULL WHERE id = ?1 AND last_played_at = ''")
            .bind(to_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM games WHERE id = ?1")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;

        // Recompute completion from the survivor's now-combined achievement
        // set, using the same formula as `recompute_completion`.
        let (total, unlocked): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(SUM(is_unlocked), 0) \
             FROM achievements WHERE game_id = ?1",
        )
        .bind(to_id)
        .fetch_one(&mut *tx)
        .await?;
        let pct = if total == 0 {
            0.0
        } else {
            (unlocked as f64 / total as f64) * 100.0
        };
        sqlx::query("UPDATE games SET completion_pct = ?1 WHERE id = ?2")
            .bind(pct)
            .bind(to_id)
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
