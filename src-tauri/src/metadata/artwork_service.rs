//! Resolves missing artwork (cover/hero/logo/icon) for every eligible game,
//! one provider registry, run on demand (settings toggle) or after a scan.
//! See `crate::metadata` module docs for the provider contract, `Lookup`
//! classification, and the ownership rules `db::artwork::upsert_artwork_ready`
//! enforces at the write layer — this service relies on that enforcement
//! rather than re-deriving it, but still skips kinds it already believes are
//! `ready` so a settled library doesn't re-download or re-HEAD assets every
//! sweep.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tracing::{info, warn};

use crate::db::artwork::Validators;
use crate::db::Db;
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus};

use super::identity::identity_for;
use super::providers::epic_catalog::EpicCatalogProvider;
use super::providers::steam_cdn::SteamCdnProvider;
use super::providers::steam_local::SteamLocalProvider;
use super::store::store_asset;
use super::throttle::Throttle;
use super::{ArtworkKind, ArtworkProvider, AssetSource, Lookup, LookupContext, TemporaryReason};
use crate::scanner::steam::SteamContext;

pub struct ArtworkService {
    db: Db,
    bus: EventBus,
    app_data_dir: std::path::PathBuf,
    client: reqwest::Client,
    throttle: Arc<Throttle>,
    /// Test-only provider registry. Production always builds its own, fresh,
    /// per call; this exists so the fill loop's termination behaviour can be
    /// verified against scriptable providers that count their invocations.
    #[cfg(test)]
    test_providers: Option<Vec<Arc<dyn ArtworkProvider>>>,
}

impl ArtworkService {
    pub fn new(
        db: Db,
        bus: EventBus,
        app_data_dir: std::path::PathBuf,
        client: reqwest::Client,
        throttle: Arc<Throttle>,
    ) -> Self {
        Self {
            db,
            bus,
            app_data_dir,
            client,
            throttle,
            #[cfg(test)]
            test_providers: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_providers(
        db: Db,
        bus: EventBus,
        app_data_dir: std::path::PathBuf,
        providers: Vec<Arc<dyn ArtworkProvider>>,
    ) -> Self {
        Self {
            db,
            bus,
            app_data_dir,
            client: reqwest::Client::new(),
            throttle: Arc::new(Throttle::default()),
            test_providers: Some(providers),
        }
    }

    /// One resolution pass over every game with at least one artwork slot that
    /// is eligible for attention. Steam library discovery happens exactly once
    /// here, up front — same "discover once per sweep" pattern as
    /// `IntegrityService::verify_all` — and the registry (built fresh each
    /// call so it always reflects the current discovery) is sorted once by
    /// priority with a stable sort over its fixed construction order, so
    /// ties resolve the same way every run.
    ///
    /// A slot is eligible unless it has reached a terminal state (`ready` or
    /// `skipped`) or is inside its retry backoff. That is what makes repeated
    /// scans of a settled library cost nothing: previously the only terminal
    /// state was `ready`, and because no provider supplies `icon`, every game
    /// re-ran the entire provider chain on every scan — three CDN HEAD requests
    /// per Steam game, indefinitely.
    pub async fn fill_missing(&self, allow_network: bool) -> AppResult<FillReport> {
        let providers = self.build_providers();

        // Hidden games are excluded: the user removed them from the library, so
        // fetching artwork for them is work nobody asked for.
        let games = self.db.list_games(false).await?;
        let mut checked = 0u32;
        let mut updated = 0u32;
        let mut settled = 0u32;
        let mut circuit_broken: HashSet<&'static str> = HashSet::new();
        let mut temporary_misses: HashMap<&'static str, u32> = HashMap::new();

        for game in &games {
            let existing = self.db.list_artwork_assets(&game.id).await?;
            let eligible = eligible_kinds(&existing, chrono::Utc::now());
            if eligible.is_empty() {
                continue;
            }
            checked += 1;

            let outcome = self
                .resolve_one(
                    game,
                    eligible,
                    &providers,
                    allow_network,
                    &mut circuit_broken,
                    &mut temporary_misses,
                )
                .await?;
            updated += outcome.filled;

            // Only settle a slot as terminally unavailable when this pass
            // actually got a definitive answer from every provider. If any
            // provider was skipped (network disabled) or circuit-broken, or
            // reported a transient failure, the absence of artwork says nothing
            // about whether it exists — marking `skipped` there would strand
            // the slot until an explicit refresh.
            if outcome.conclusive {
                for kind in outcome.unresolved {
                    if self.db.mark_artwork_skipped(&game.id, kind.as_str()).await? {
                        settled += 1;
                    }
                }
            }
        }

        info!(
            checked,
            updated, settled, "artwork fill complete"
        );
        Ok(FillReport { checked, updated })
    }

    /// Explicit, single-game refresh — the "Refresh Metadata" button.
    /// Unlike `fill_missing`, this attempts every `ArtworkKind`, not just
    /// currently-missing ones: a user asking to refresh wants a real
    /// re-fetch, not just gap-filling. Whether an already-`ready` kind
    /// actually gets overwritten is still decided by
    /// `Db::upsert_artwork_ready`'s ownership guard (same-source refresh or
    /// not `user_locked`), not by this method — attempting a kind that
    /// guard rejects is harmless.
    pub async fn refresh_game(&self, game_id: &str, allow_network: bool) -> AppResult<u32> {
        let game = self
            .db
            .get_game(game_id)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound(format!("game not found: {game_id}")))?;
        let providers = self.build_providers();
        let all: HashSet<ArtworkKind> = ArtworkKind::ALL.into_iter().collect();
        let outcome = self
            .resolve_one(
                &game,
                all,
                &providers,
                allow_network,
                &mut HashSet::new(),
                &mut HashMap::new(),
            )
            .await?;
        Ok(outcome.filled)
    }

    /// Registry construction is shared between `fill_missing` and
    /// `refresh_game` but deliberately not cached on `self` — Steam library
    /// discovery must happen fresh each call (see struct docs).
    fn build_providers(&self) -> Vec<Arc<dyn ArtworkProvider>> {
        #[cfg(test)]
        if let Some(providers) = &self.test_providers {
            let mut providers = providers.clone();
            providers.sort_by_key(|p| p.priority());
            return providers;
        }
        let mut providers: Vec<Arc<dyn ArtworkProvider>> = vec![
            Arc::new(SteamLocalProvider::new(SteamContext::discover())),
            Arc::new(SteamCdnProvider::new(
                self.client.clone(),
                self.throttle.clone(),
            )),
            Arc::new(EpicCatalogProvider::new()),
        ];
        providers.sort_by_key(|p| p.priority());
        providers
    }

    /// Try every provider, in priority order, for one game's eligible kinds.
    async fn resolve_one(
        &self,
        game: &crate::models::Game,
        mut missing: HashSet<ArtworkKind>,
        providers: &[Arc<dyn ArtworkProvider>],
        allow_network: bool,
        circuit_broken: &mut HashSet<&'static str>,
        temporary_misses: &mut HashMap<&'static str, u32>,
    ) -> AppResult<ResolveOutcome> {
        let mut updated = 0u32;
        // Every provider must give a definitive answer for the remaining kinds
        // to be settled as unavailable. Set false by anything that means "we
        // did not really ask".
        let mut conclusive = true;
        let identity = identity_for(&self.db, game).await?;
        let ctx = LookupContext {
            identity: &identity,
            allow_network,
        };

        for provider in providers {
            if missing.is_empty() {
                break;
            }
            let code = provider.code();
            if circuit_broken.contains(code) {
                conclusive = false;
                continue;
            }
            if provider.requires_network() && !allow_network {
                // The provider was never consulted, so its silence is not
                // evidence that the artwork does not exist.
                conclusive = false;
                continue;
            }

            match provider.resolve_artwork(&ctx).await {
                Lookup::Found(descriptors) => {
                    for descriptor in descriptors {
                        if !missing.contains(&descriptor.kind) {
                            // A higher-priority provider (earlier in
                            // this loop) already filled this kind this
                            // sweep — this provider doesn't get to
                            // contest it.
                            continue;
                        }
                        let remote_url = match &descriptor.source {
                            AssetSource::RemoteUrl(url) => Some(url.clone()),
                            AssetSource::LocalFile(_) => None,
                        };
                        // Offer the validators this provider previously stored
                        // for this slot, so an unchanged asset costs a 304
                        // instead of a full download.
                        let (etag, last_modified) = self
                            .db
                            .artwork_validators(&game.id, descriptor.kind.as_str(), code)
                            .await?;
                        let stored = store_asset(
                            &self.app_data_dir,
                            &game.id,
                            descriptor.kind,
                            &descriptor.source,
                            &self.client,
                            &self.throttle,
                            Validators {
                                etag: etag.as_deref(),
                                last_modified: last_modified.as_deref(),
                            },
                        )
                        .await;

                        match stored {
                            Ok(asset) if asset.unchanged => {
                                // Nothing transferred; just refresh the
                                // bookkeeping and treat the slot as resolved.
                                self.db
                                    .touch_artwork_unchanged(
                                        &game.id,
                                        descriptor.kind.as_str(),
                                        code,
                                        Validators {
                                            etag: asset.etag.as_deref(),
                                            last_modified: asset.last_modified.as_deref(),
                                        },
                                    )
                                    .await?;
                                missing.remove(&descriptor.kind);
                            }
                            Ok(asset) => {
                                let wrote = self
                                    .db
                                    .upsert_artwork_ready(
                                        &game.id,
                                        descriptor.kind.as_str(),
                                        code,
                                        remote_url.as_deref(),
                                        &asset.path,
                                        Validators {
                                            etag: asset.etag.as_deref(),
                                            last_modified: asset.last_modified.as_deref(),
                                        },
                                    )
                                    .await?;
                                if wrote {
                                    self.set_game_path(&game.id, descriptor.kind, &asset.path)
                                        .await?;
                                    missing.remove(&descriptor.kind);
                                    self.bus.emit(AppEvent::GameUpdated {
                                        game_id: game.id.clone(),
                                    });
                                    updated += 1;
                                } else {
                                    // The ownership guard refused the write:
                                    // the slot belongs to someone else, so it
                                    // is resolved as far as this pass goes.
                                    missing.remove(&descriptor.kind);
                                }
                            }
                            Err(e) => {
                                warn!(
                                    provider = code,
                                    kind = descriptor.kind.as_str(),
                                    error = %e,
                                    "failed to store artwork asset"
                                );
                                // A real, kind-specific failure: this is what
                                // the backoff exists for.
                                self.db
                                    .mark_artwork_failed(&game.id, descriptor.kind.as_str(), code)
                                    .await?;
                                missing.remove(&descriptor.kind);
                                conclusive = false;
                            }
                        }
                    }
                }
                Lookup::Unsupported => continue,
                Lookup::Permanent(_) => {
                    // A provider-level "not here" — definitive, and it says
                    // nothing per kind. It is deliberately *not* recorded as a
                    // per-kind failure: this previously looped every remaining
                    // kind and wrote `failed` against kinds the provider had
                    // never been asked about, giving the ledger false
                    // provenance. Termination now comes from `skipped`.
                    continue;
                }
                Lookup::Temporary(reason) => {
                    // Transient: the answer is unknown, not negative.
                    conclusive = false;
                    let broken = matches!(reason, TemporaryReason::RateLimited) || {
                        let count = temporary_misses.entry(code).or_insert(0);
                        *count += 1;
                        *count >= 5
                    };
                    if broken {
                        warn!(provider = code, "circuit-breaking artwork provider for this sweep");
                        circuit_broken.insert(code);
                    }
                }
            }
        }

        Ok(ResolveOutcome {
            filled: updated,
            conclusive,
            unresolved: missing,
        })
    }

    async fn set_game_path(&self, game_id: &str, kind: ArtworkKind, path: &str) -> AppResult<()> {
        match kind {
            ArtworkKind::Cover => self.db.set_cover_path(game_id, path).await,
            ArtworkKind::Hero => self.db.set_hero_path(game_id, path).await,
            ArtworkKind::Logo => self.db.set_logo_path(game_id, path).await,
            ArtworkKind::Icon => self.db.set_icon_path(game_id, path).await,
        }
    }
}

fn kind_from_str(s: &str) -> Option<ArtworkKind> {
    ArtworkKind::ALL.into_iter().find(|k| k.as_str() == s)
}

/// Which artwork slots this pass should attempt for a game.
///
/// A slot drops out when it has reached a terminal state (`ready` or
/// `skipped`) or is still inside its retry backoff. Kinds with no row at all
/// are eligible, which is how a newly scanned game gets filled.
pub(crate) fn eligible_kinds(
    existing: &[crate::models::ArtworkAsset],
    now: chrono::DateTime<chrono::Utc>,
) -> HashSet<ArtworkKind> {
    let mut out: HashSet<ArtworkKind> = ArtworkKind::ALL.into_iter().collect();
    for asset in existing {
        let Some(kind) = kind_from_str(&asset.kind) else {
            continue;
        };
        // A user-locked asset is never contested by the fetcher.
        if asset.user_locked != 0
            || !crate::db::artwork::is_retry_due(
                &asset.state,
                asset.next_retry_at.as_deref(),
                now,
            )
        {
            out.remove(&kind);
        }
    }
    out
}

/// What one game's pass concluded.
struct ResolveOutcome {
    /// How many kinds were newly written.
    filled: u32,
    /// Whether every provider gave a definitive answer, so remaining kinds can
    /// be settled as unavailable.
    conclusive: bool,
    /// Kinds still without artwork after the pass.
    unresolved: HashSet<ArtworkKind>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FillReport {
    pub checked: u32,
    pub updated: u32,
}
