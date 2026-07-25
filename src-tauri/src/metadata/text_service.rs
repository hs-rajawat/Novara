//! Resolves descriptive text metadata for every eligible game, one
//! provider registry, run on demand (settings toggle) or after a scan. See
//! `crate::metadata` module docs for the provider contract and `Lookup`
//! classification this loop reacts to.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tracing::{info, warn};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::events::{AppEvent, EventBus};
use crate::models::Game;

use super::identity::identity_for;
use super::offline::OfflineProvider;
use super::providers::epic_catalog::EpicCatalogProvider;
use super::providers::steam_cdn::SteamCdnProvider;
use super::{Lookup, LookupContext, MetadataTextProvider, TemporaryReason};

pub struct MetadataService {
    db: Db,
    bus: EventBus,
    /// Registered once, sorted once, at construction time — a *stable*
    /// sort over the fixed registration order below, so two providers
    /// sharing a `priority()` always tie-break the same way every run (see
    /// `MetadataTextProvider::priority` docs).
    providers: Vec<Arc<dyn MetadataTextProvider>>,
}

impl MetadataService {
    pub fn new(db: Db, bus: EventBus, client: reqwest::Client) -> Self {
        let mut providers: Vec<Arc<dyn MetadataTextProvider>> = vec![
            Arc::new(SteamCdnProvider::new(client)),
            Arc::new(EpicCatalogProvider::new()),
            Arc::new(OfflineProvider),
        ];
        providers.sort_by_key(|p| p.priority());
        Self { db, bus, providers }
    }

    /// One resolution pass over every game that doesn't already have text
    /// metadata. `allow_network` mirrors `metadata_enabled && !offline_mode`
    /// — gated by the caller (composition root / command handler), since
    /// this service doesn't read settings itself.
    pub async fn fill_missing(&self, allow_network: bool) -> AppResult<FillReport> {
        let games = self.db.list_games(true).await?;
        let mut checked = 0u32;
        let mut updated = 0u32;
        let mut circuit_broken: HashSet<&'static str> = HashSet::new();
        let mut temporary_misses: HashMap<&'static str, u32> = HashMap::new();

        for game in &games {
            // A manual edit (once the frontend gains one) sets
            // metadata_source = 'manual' — that's the user's data now, and
            // no provider result should ever touch it again.
            if game.metadata_source.as_deref() == Some("manual") {
                continue;
            }
            // Text is resolved all-or-nothing per game (services don't
            // merge partial fields across providers — see module docs), so
            // "already has a description" is enough to call this game done.
            if game.metadata_json.is_some() {
                continue;
            }
            checked += 1;
            if self
                .resolve_one(game, allow_network, &mut circuit_broken, &mut temporary_misses)
                .await?
            {
                updated += 1;
            }
        }

        info!(checked, updated, "metadata text fill complete");
        Ok(FillReport { checked, updated })
    }

    /// Explicit, single-game refresh — the "Refresh Metadata" button. Unlike
    /// `fill_missing`, this ignores the "already has a description" skip so
    /// a user can force a re-fetch, but still respects a manual edit
    /// (`metadata_source = 'manual'`) since that's the user's data, not a
    /// gap to fill. Circuit-breaker state is scoped to just this one call —
    /// irrelevant for a single game, since each provider is only ever tried
    /// once here regardless.
    pub async fn refresh_game(&self, game_id: &str, allow_network: bool) -> AppResult<bool> {
        let game = self
            .db
            .get_game(game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("game not found: {game_id}")))?;
        if game.metadata_source.as_deref() == Some("manual") {
            return Ok(false);
        }
        self.resolve_one(&game, allow_network, &mut HashSet::new(), &mut HashMap::new())
            .await
    }

    /// Try every registered provider, in priority order, for one game.
    /// Returns whether a provider's result was actually persisted.
    async fn resolve_one(
        &self,
        game: &Game,
        allow_network: bool,
        circuit_broken: &mut HashSet<&'static str>,
        temporary_misses: &mut HashMap<&'static str, u32>,
    ) -> AppResult<bool> {
        let identity = identity_for(&self.db, game).await?;
        let ctx = LookupContext {
            identity: &identity,
            allow_network,
        };

        for provider in &self.providers {
            let code = provider.code();
            if circuit_broken.contains(code) {
                continue;
            }
            if provider.requires_network() && !allow_network {
                continue;
            }
            match provider.resolve_text(&ctx).await {
                Lookup::Found(meta) => {
                    self.db.set_game_metadata(&game.id, &meta, code).await?;
                    self.bus.emit(AppEvent::GameUpdated {
                        game_id: game.id.clone(),
                    });
                    return Ok(true);
                }
                Lookup::Unsupported => continue,
                Lookup::Permanent(_) => continue,
                Lookup::Temporary(reason) => {
                    // A rate limit is an unambiguous "stop asking this
                    // provider" signal; anything else only trips the
                    // breaker after repeated misses across the batch, since
                    // a single timeout is often just one bad request, not a
                    // broken provider.
                    let broken = matches!(reason, TemporaryReason::RateLimited) || {
                        let count = temporary_misses.entry(code).or_insert(0);
                        *count += 1;
                        *count >= 5
                    };
                    if broken {
                        warn!(provider = code, "circuit-breaking metadata provider for this sweep");
                        circuit_broken.insert(code);
                    }
                    continue;
                }
            }
        }
        Ok(false)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FillReport {
    pub checked: u32,
    pub updated: u32,
}
