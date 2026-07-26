//! The title-resolution pass.
//!
//! Runs *before* the metadata and artwork fills, because it is what gives those
//! fills something to work with: an Epic or manual game has no Steam app-id, so
//! every Steam-backed provider returns `Unsupported` and the game gets nothing.
//! Once its title is resolved, the existing pipeline serves it unchanged.
//!
//! Kept as a separate pass rather than folded into either fill service, for two
//! reasons. Both fills need the result, so doing it inside one would leave the
//! other depending on the first having run. And `identity_for` must stay a pure
//! offline read — a provider that lazily performed a network lookup while building
//! an identity would put a network call somewhere the privacy gate is not applied.

use std::sync::Arc;

use tracing::{info, warn};

use crate::db::Db;
use crate::error::AppResult;
use crate::metadata::providers::steam_title_search::{
    self, SteamTitleSearch, TitleSearchOutcome,
};
use crate::metadata::throttle::Throttle;

pub struct TitleResolver {
    db: Db,
    search: SteamTitleSearch,
}

/// What one pass did. `unavailable` is reported rather than hidden: it is the
/// difference between "these games are not on Steam" and "Steam could not be
/// reached", which is exactly what a user seeing missing artwork needs to know.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ResolveReport {
    pub checked: u32,
    pub matched: u32,
    pub unmatched: u32,
    pub unavailable: u32,
}

impl TitleResolver {
    pub fn new(db: Db, client: reqwest::Client, throttle: Arc<Throttle>) -> Self {
        Self {
            db,
            search: SteamTitleSearch::new(client, throttle),
        }
    }

    /// Resolve every game that needs it and records the outcomes.
    ///
    /// A no-op when the network is not permitted: the whole pass requires it, so
    /// there is nothing to do rather than something to skip per game.
    pub async fn resolve_missing(&self, allow_network: bool) -> AppResult<ResolveReport> {
        let mut report = ResolveReport::default();
        if !allow_network {
            return Ok(report);
        }

        let fingerprint = steam_title_search::fingerprint();
        let pending = self.db.games_needing_title_search(&fingerprint).await?;

        for (game_id, title) in pending {
            report.checked += 1;
            match self.search.resolve(&title, allow_network).await {
                TitleSearchOutcome::Matched {
                    app_id,
                    matched_title,
                } => {
                    self.db
                        .record_steam_title_match(
                            &game_id,
                            Some(&app_id),
                            Some(&matched_title),
                            &fingerprint,
                        )
                        .await?;
                    report.matched += 1;
                }
                TitleSearchOutcome::NoMatch => {
                    // Recorded so the same question is not asked again every
                    // sweep. This is the negative cache the ledger exists for.
                    self.db
                        .record_steam_title_match(&game_id, None, None, &fingerprint)
                        .await?;
                    report.unmatched += 1;
                }
                TitleSearchOutcome::Unavailable => {
                    // Deliberately not recorded. A timeout is not evidence about
                    // this game, and caching it would turn one bad request into a
                    // permanent verdict.
                    report.unavailable += 1;
                }
            }
        }

        if report.checked > 0 {
            info!(
                checked = report.checked,
                matched = report.matched,
                unmatched = report.unmatched,
                unavailable = report.unavailable,
                "steam title resolution complete"
            );
        }
        if report.unavailable > 0 {
            warn!(
                unavailable = report.unavailable,
                "some titles could not be resolved because Steam was unreachable; \
                 they will be retried on the next sweep"
            );
        }
        Ok(report)
    }

    /// Resolve one game on demand, for the explicit "Refresh Metadata" action.
    ///
    /// Unlike `resolve_missing` this ignores the cache, because the user asking
    /// for a refresh is asking for the question to be put again.
    pub async fn resolve_one(&self, game_id: &str, allow_network: bool) -> AppResult<bool> {
        if !allow_network {
            return Ok(false);
        }
        let Some(game) = self.db.get_game(game_id).await? else {
            return Ok(false);
        };
        // A game Steam already identifies must never be re-keyed by title: the
        // search could only contradict a fact already established.
        if self
            .db
            .list_source_app_ids(game_id)
            .await?
            .iter()
            .any(|(source, _)| source == "steam")
        {
            return Ok(false);
        }

        let fingerprint = steam_title_search::fingerprint();
        match self.search.resolve(&game.title, allow_network).await {
            TitleSearchOutcome::Matched {
                app_id,
                matched_title,
            } => {
                self.db
                    .record_steam_title_match(
                        game_id,
                        Some(&app_id),
                        Some(&matched_title),
                        &fingerprint,
                    )
                    .await?;
                Ok(true)
            }
            TitleSearchOutcome::NoMatch => {
                self.db
                    .record_steam_title_match(game_id, None, None, &fingerprint)
                    .await?;
                Ok(false)
            }
            TitleSearchOutcome::Unavailable => Ok(false),
        }
    }
}
