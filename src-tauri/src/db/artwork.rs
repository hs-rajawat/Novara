//! Artwork asset repository: per-(game, kind) provenance/refresh ledger.
//! `games.cover_path`/`hero_path`/`logo_path`/`icon_path` remain the render
//! source of truth (frontend reads them directly); this table is the
//! bookkeeping that lets `crate::artwork::ArtworkService` decide what still
//! needs fetching and never clobber a user-set asset.

use crate::error::AppResult;
use crate::models::{now_rfc3339, ArtworkAsset};

use super::Db;

/// Backoff schedule for repeated artwork failures, by consecutive attempt.
///
/// Grows quickly and then caps. The point is not to eventually give up — a
/// transient CDN outage should heal — but to stop a permanently unavailable
/// slot from costing a network request on every scan. Capping rather than
/// growing without bound means a slot still recovers within a week of the
/// provider coming back.
const RETRY_SCHEDULE: [i64; 5] = [
    60 * 60,          // 1 hour
    6 * 60 * 60,      // 6 hours
    24 * 60 * 60,     // 1 day
    3 * 24 * 60 * 60, // 3 days
    7 * 24 * 60 * 60, // 1 week, then held
];

/// Seconds to wait before retrying after `attempts` consecutive failures.
pub fn retry_delay_for(attempts: i64) -> i64 {
    if attempts <= 0 {
        return 0;
    }
    let idx = (attempts as usize - 1).min(RETRY_SCHEDULE.len() - 1);
    RETRY_SCHEDULE[idx]
}

/// The RFC3339 instant at which a slot with `attempts` failures may be retried.
fn retry_at(attempts: i64) -> String {
    let delay = chrono::Duration::seconds(retry_delay_for(attempts));
    (chrono::Utc::now() + delay).to_rfc3339()
}

/// Whether an artwork slot in this state may be attempted now.
///
/// `ready` and `skipped` are terminal for the fill loop. `failed` waits for its
/// backoff to expire. Anything else — `pending`, or no row at all — is eligible.
///
/// Comparison is done in Rust rather than SQL because `next_retry_at` is
/// RFC3339 (`…T…+00:00`) while SQLite's date functions produce a space
/// separator; comparing the two lexicographically is the latent format
/// mismatch already tracked for the heatmap.
pub fn is_retry_due(
    state: &str,
    next_retry_at: Option<&str>,
    settled_by: Option<&str>,
    capability: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    match state {
        "ready" => false,
        // Terminal only while the provider set that settled it is still the
        // current one. A capability change re-opens the slot on the next sweep,
        // which is how a future provider picks up games nothing could resolve
        // before — no retry timer, no manual repair. See `metadata::capability`.
        "skipped" => crate::metadata::capability::is_stale(settled_by, capability),
        "failed" => match next_retry_at {
            None => true,
            Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
                Ok(t) => t.with_timezone(&chrono::Utc) <= now,
                // An unparseable stamp must not strand the slot forever.
                Err(_) => true,
            },
        },
        _ => true,
    }
}

/// Cache validators for one asset, as issued by its origin.
///
/// Both are carried because origins disagree about which they honour: the Steam
/// CDN this project uses returns a strong `ETag` but ignores `If-None-Match`,
/// while honouring `If-Modified-Since`. Sending both makes a conditional
/// refresh effective against either.
#[derive(Debug, Clone, Copy, Default)]
pub struct Validators<'a> {
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
}

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
    ///
    /// Success clears the retry backoff: `attempts` returns to 0 and
    /// `next_retry_at` to NULL, so a slot that recovers is not penalised for
    /// its history.
    pub async fn upsert_artwork_ready(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
        remote_url: Option<&str>,
        local_path: &str,
        validators: Validators<'_>,
    ) -> AppResult<bool> {
        let now = now_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO artwork_assets
              (game_id, kind, source, remote_url, local_path, state, etag, fetched_at, updated_at,
               attempts, next_retry_at, last_modified)
            VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?7, ?7, 0, NULL, ?8)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = excluded.source,
              remote_url = excluded.remote_url,
              local_path = excluded.local_path,
              state = 'ready',
              etag = excluded.etag,
              last_modified = excluded.last_modified,
              fetched_at = excluded.fetched_at,
              updated_at = excluded.updated_at,
              attempts = 0,
              next_retry_at = NULL
            WHERE artwork_assets.user_locked = 0
              AND (artwork_assets.state != 'ready' OR artwork_assets.source = excluded.source)
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .bind(remote_url)
        .bind(local_path)
        .bind(validators.etag)
        .bind(&now)
        .bind(validators.last_modified)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Refresh the bookkeeping for an asset the provider confirmed is
    /// unchanged (an HTTP 304 against the stored validators).
    ///
    /// The bytes on disk are already correct, so nothing is re-downloaded and
    /// `local_path` is left alone; only the freshness stamp and the cleared
    /// backoff are recorded. Without this an unchanged asset would either be
    /// re-downloaded in full or look like a failure.
    pub async fn touch_artwork_unchanged(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
        validators: Validators<'_>,
    ) -> AppResult<()> {
        let now = now_rfc3339();
        sqlx::query(
            r#"
            UPDATE artwork_assets
            SET state = 'ready', fetched_at = ?1, updated_at = ?1,
                attempts = 0, next_retry_at = NULL,
                etag = COALESCE(?2, etag),
                last_modified = COALESCE(?3, last_modified)
            WHERE game_id = ?4 AND kind = ?5 AND user_locked = 0 AND source = ?6
            "#,
        )
        .bind(&now)
        .bind(validators.etag)
        .bind(validators.last_modified)
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The stored validators for `(game_id, kind)`, if this source owns it.
    ///
    /// Scoped to `source` on purpose: a validator is only meaningful against
    /// the origin that issued it, so offering one provider's to another would
    /// invite a bogus 304.
    pub async fn artwork_validators(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
    ) -> AppResult<(Option<String>, Option<String>)> {
        Ok(sqlx::query_as(
            "SELECT etag, last_modified FROM artwork_assets \
             WHERE game_id = ?1 AND kind = ?2 AND source = ?3",
        )
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or((None, None)))
    }

    /// Mark `(game_id, kind)` as unfillable by the current provider set.
    ///
    /// `reason` records *why*, and is the architecturally important distinction:
    /// `"unsupported"` means no provider was capable of resolving this game at
    /// all (nothing was looked up), while `"not_found"` means a provider that
    /// could answer did, definitively. It is stored in `source` because that
    /// column is deliberately free-form provenance.
    ///
    /// `settled_by` is the fingerprint of the provider set that reached the
    /// conclusion. That is what makes this terminal *without* being permanent:
    /// eligibility is a comparison against the current fingerprint, so a
    /// capability change re-opens the slot on the next sweep with no timer and no
    /// manual repair. See `metadata::capability`.
    ///
    /// Guarded like the other writes so it can never displace a `ready` or
    /// `user_locked` asset. An explicit refresh clears it by writing `ready`.
    pub async fn mark_artwork_skipped(
        &self,
        game_id: &str,
        kind: &str,
        reason: &str,
        settled_by: &str,
    ) -> AppResult<bool> {
        let now = now_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO artwork_assets
              (game_id, kind, source, state, updated_at, next_retry_at, settled_by)
            VALUES (?1, ?2, ?3, 'skipped', ?4, NULL, ?5)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = excluded.source,
              state = 'skipped',
              updated_at = excluded.updated_at,
              next_retry_at = NULL,
              settled_by = excluded.settled_by
            WHERE artwork_assets.user_locked = 0
              AND artwork_assets.state != 'ready'
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(reason)
        .bind(&now)
        .bind(settled_by)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Record a failed fetch attempt, with exponential backoff so the next
    /// sweep does not immediately try again.
    ///
    /// Guarded by the same `WHERE` as `upsert_artwork_ready`: a provider
    /// failing on a kind another provider already has `ready` (or the user has
    /// locked) must not blank that row to `failed` — it neither owns it nor
    /// gets to speak for it. Returns whether the row actually changed.
    ///
    /// `attempts` is incremented and `next_retry_at` set from
    /// [`retry_delay_for`], so repeated failure backs off instead of being
    /// retried on every scan. This is only written for failures specific to
    /// one kind (a download or copy that did not complete) — a provider-level
    /// "this game does not exist here" is not recorded per kind, because the
    /// provider was never asked about those kinds individually.
    pub async fn mark_artwork_failed(
        &self,
        game_id: &str,
        kind: &str,
        source: &str,
    ) -> AppResult<bool> {
        let now = now_rfc3339();
        let prior: i64 = sqlx::query_scalar(
            "SELECT attempts FROM artwork_assets WHERE game_id = ?1 AND kind = ?2",
        )
        .bind(game_id)
        .bind(kind)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(0);
        let attempts = prior.saturating_add(1);
        let next_retry_at = retry_at(attempts);

        let result = sqlx::query(
            r#"
            INSERT INTO artwork_assets
              (game_id, kind, source, state, updated_at, attempts, next_retry_at)
            VALUES (?1, ?2, ?3, 'failed', ?4, ?5, ?6)
            ON CONFLICT(game_id, kind) DO UPDATE SET
              source = excluded.source,
              state = 'failed',
              updated_at = excluded.updated_at,
              attempts = excluded.attempts,
              next_retry_at = excluded.next_retry_at
            WHERE artwork_assets.user_locked = 0
              AND (artwork_assets.state != 'ready' OR artwork_assets.source = excluded.source)
            "#,
        )
        .bind(game_id)
        .bind(kind)
        .bind(source)
        .bind(&now)
        .bind(attempts)
        .bind(&next_retry_at)
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
