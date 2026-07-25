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

use crate::db::Db;
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus};

use super::identity::identity_for;
use super::providers::epic_catalog::EpicCatalogProvider;
use super::providers::steam_cdn::SteamCdnProvider;
use super::providers::steam_local::SteamLocalProvider;
use super::store::store_asset;
use super::{ArtworkKind, ArtworkProvider, AssetSource, Lookup, LookupContext, TemporaryReason};
use crate::scanner::steam::SteamContext;

pub struct ArtworkService {
    db: Db,
    bus: EventBus,
    app_data_dir: std::path::PathBuf,
    client: reqwest::Client,
}

impl ArtworkService {
    pub fn new(
        db: Db,
        bus: EventBus,
        app_data_dir: std::path::PathBuf,
        client: reqwest::Client,
    ) -> Self {
        Self {
            db,
            bus,
            app_data_dir,
            client,
        }
    }

    /// One resolution pass over every game with at least one missing
    /// artwork kind. Steam library discovery happens exactly once here, up
    /// front — same "discover once per sweep" pattern as
    /// `IntegrityService::verify_all` — and the registry (built fresh each
    /// call so it always reflects the current discovery) is sorted once by
    /// priority with a stable sort over its fixed construction order, so
    /// ties resolve the same way every run.
    pub async fn fill_missing(&self, allow_network: bool) -> AppResult<FillReport> {
        let providers = self.build_providers();

        let games = self.db.list_games(true).await?;
        let mut checked = 0u32;
        let mut updated = 0u32;
        let mut circuit_broken: HashSet<&'static str> = HashSet::new();
        let mut temporary_misses: HashMap<&'static str, u32> = HashMap::new();

        for game in &games {
            let existing = self.db.list_artwork_assets(&game.id).await?;
            let ready: HashSet<ArtworkKind> = existing
                .iter()
                .filter(|a| a.state == "ready")
                .filter_map(|a| kind_from_str(&a.kind))
                .collect();
            let missing: HashSet<ArtworkKind> =
                ArtworkKind::ALL.into_iter().filter(|k| !ready.contains(k)).collect();
            if missing.is_empty() {
                continue;
            }
            checked += 1;

            let filled = self
                .resolve_one(
                    game,
                    missing,
                    &providers,
                    allow_network,
                    &mut circuit_broken,
                    &mut temporary_misses,
                )
                .await?;
            updated += filled;
        }

        info!(checked, updated, "artwork fill complete");
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
        self.resolve_one(
            &game,
            all,
            &providers,
            allow_network,
            &mut HashSet::new(),
            &mut HashMap::new(),
        )
        .await
    }

    /// Registry construction is shared between `fill_missing` and
    /// `refresh_game` but deliberately not cached on `self` — Steam library
    /// discovery must happen fresh each call (see struct docs).
    fn build_providers(&self) -> Vec<Arc<dyn ArtworkProvider>> {
        let mut providers: Vec<Arc<dyn ArtworkProvider>> = vec![
            Arc::new(SteamLocalProvider::new(SteamContext::discover())),
            Arc::new(SteamCdnProvider::new(self.client.clone())),
            Arc::new(EpicCatalogProvider::new()),
        ];
        providers.sort_by_key(|p| p.priority());
        providers
    }

    /// Try every provider, in priority order, for one game's `missing`
    /// kinds. Returns how many kinds were actually filled.
    async fn resolve_one(
        &self,
        game: &crate::models::Game,
        mut missing: HashSet<ArtworkKind>,
        providers: &[Arc<dyn ArtworkProvider>],
        allow_network: bool,
        circuit_broken: &mut HashSet<&'static str>,
        temporary_misses: &mut HashMap<&'static str, u32>,
    ) -> AppResult<u32> {
        let mut updated = 0u32;
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
                continue;
            }
            if provider.requires_network() && !allow_network {
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
                        let stored = store_asset(
                            &self.app_data_dir,
                            &game.id,
                            descriptor.kind,
                            &descriptor.source,
                            &self.client,
                        )
                        .await;

                        match stored {
                            Ok(local_path) => {
                                let wrote = self
                                    .db
                                    .upsert_artwork_ready(
                                        &game.id,
                                        descriptor.kind.as_str(),
                                        code,
                                        remote_url.as_deref(),
                                        &local_path,
                                        None,
                                    )
                                    .await?;
                                if wrote {
                                    self.set_game_path(&game.id, descriptor.kind, &local_path)
                                        .await?;
                                    missing.remove(&descriptor.kind);
                                    self.bus.emit(AppEvent::GameUpdated {
                                        game_id: game.id.clone(),
                                    });
                                    updated += 1;
                                }
                            }
                            Err(e) => {
                                warn!(
                                    provider = code,
                                    kind = descriptor.kind.as_str(),
                                    error = %e,
                                    "failed to store artwork asset"
                                );
                                self.db
                                    .mark_artwork_failed(&game.id, descriptor.kind.as_str(), code)
                                    .await?;
                            }
                        }
                    }
                }
                Lookup::Unsupported => continue,
                Lookup::Permanent(_) => {
                    for kind in missing.clone() {
                        self.db.mark_artwork_failed(&game.id, kind.as_str(), code).await?;
                    }
                }
                Lookup::Temporary(reason) => {
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

        Ok(updated)
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct FillReport {
    pub checked: u32,
    pub updated: u32,
}
