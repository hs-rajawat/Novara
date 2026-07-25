//! Artwork asset repository: per-(game, kind) provenance/refresh ledger.
//! `games.cover_path`/`hero_path`/`logo_path`/`icon_path` remain the render
//! source of truth (frontend reads them directly); this table is the
//! bookkeeping that lets `crate::artwork::ArtworkService` decide what still
//! needs fetching and never clobber a user-set asset.

use crate::error::AppResult;
use crate::models::{now_rfc3339, ArtworkAsset};

use super::Db;

impl Db {
    pub async fn get_artwork_asset(
        &self,
        game_id: &str,
        kind: &str,
    ) -> AppResult<Option<ArtworkAsset>> {
        Ok(sqlx::query_as::<_, ArtworkAsset>(
            "SELECT * FROM artwork_assets WHERE game_id = ?1 AND kind = ?2",
        )
        .bind(game_id)
        .bind(kind)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_artwork_assets(&self, game_id: &str) -> AppResult<Vec<ArtworkAsset>> {
        Ok(
            sqlx::query_as::<_, ArtworkAsset>("SELECT * FROM artwork_assets WHERE game_id = ?1")
                .bind(game_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Record a successful fetch/copy for `(game_id, kind)` — but only if
    /// this write is actually allowed to land. The `WHERE` clause on the
    /// `DO UPDATE` is the enforcement point for "a provider may never
    /// overwrite existing artwork except its own, and never a user-locked
    /// asset": conflicting against a row that is `user_locked`, or `ready`
    /// from a *different* `source`, makes the whole statement a no-op — no
    /// insert, no update, same as SQLite's `DO NOTHING`. A `pending`/
    /// `failed` row (nothing successfully fetched yet) or a `ready` row
    /// from this same provider (a refresh) both pass through normally.
    ///
    /// This is enforced here rather than trusted to the caller so the
    /// guarantee holds regardless of what `ArtworkService`'s fill loop
    /// does — a provider calling this with a stale or wrong-priority view
    /// of the table still can't clobber another provider's or the user's
    /// asset. Returns whether the write actually applied, so
    /// `ArtworkService` can tell a real update from a silently-skipped one
    /// (e.g. to avoid emitting `GameUpdated` for a no-op).
    pub async fn upsert_artwork_ready(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
        remote_url: Option<&str>,
        local_path: &str,
        etag: Option<&str>,
    ) -> AppResult<bool> {
        let now = now_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO artwork_assets
              (game_id, kind, source, remote_url, local_path, state, etag, fetched_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7, ?7)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = excluded.source,
              remote_url = excluded.remote_url,
              local_path = excluded.local_path,
              state = 'ready',
              etag = excluded.etag,
              fetched_at = excluded.fetched_at,
              updated_at = excluded.updated_at
            WHERE artwork_assets.user_locked = 0
              AND (artwork_assets.state != 'ready' OR artwork_assets.source = excluded.source)
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .bind(remote_url)
        .bind(local_path)
        .bind(etag)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Record a failed fetch attempt so the next `resolve_missing` pass
    /// knows to retry it, without pretending it succeeded. Guarded by the
    /// same `WHERE` as `upsert_artwork_ready`: a provider failing on a kind
    /// another provider already has `ready` (or the user has locked) must
    /// not blank that row to `failed` — it neither owns it nor gets to
    /// speak for it. Returns whether the row actually changed.
    pub async fn mark_artwork_failed(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
    ) -> AppResult<bool> {
        let now = now_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO artwork_assets (game_id, kind, source, state, updated_at)
            VALUES (?1, ?2, ?3, 'failed', ?4)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = excluded.source,
              state = 'failed',
              updated_at = excluded.updated_at
            WHERE artwork_assets.user_locked = 0
              AND (artwork_assets.state != 'ready' OR artwork_assets.source = excluded.source)
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark `(game_id, kind)` as user-set: locks it against the auto-fetcher
    /// and records provenance as 'manual'. Called by `set_cover_path` /
    /// `set_hero_path` / `set_logo_path` / `set_icon_path`.
    pub async fn lock_artwork_asset(
        &self,
        game_id: &str,
        kind: &str,
        local_path: &str,
    ) -> AppResult<()> {
        let now = now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO artwork_assets
              (game_id, kind, source, local_path, state, user_locked, fetched_at, updated_at)
            VALUES (?1, ?2, 'manual', ?3, 'ready', 1, ?4, ?4)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = 'manual',
              local_path = excluded.local_path,
              state = 'ready',
              user_locked = 1,
              fetched_at = excluded.fetched_at,
              updated_at = excluded.updated_at
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(local_path)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
