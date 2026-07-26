//! Builds a `GameIdentity` from persisted game + installation rows — the
//! one place `text_service`/`artwork_service` translate a DB row into what a
//! provider actually consumes. Kept separate from both services so neither
//! has to duplicate the installation → identifier translation.
//!
//! This is a pure, offline read. The title-resolution pass that populates the
//! Steam match cache is deliberately elsewhere: a lazy network lookup here would
//! put a request somewhere the privacy gate is not applied, and would make
//! building an identity non-deterministic.

use crate::db::Db;
use crate::error::AppResult;
use crate::models::Game;

use super::{GameIdentifier, GameIdentity};

pub async fn identity_for(db: &Db, game: &Game) -> AppResult<GameIdentity> {
    let mut identifiers = Vec::new();
    for (source, app_id) in db.list_source_app_ids(&game.id).await? {
        identifiers.push(GameIdentifier::SourceAppId { source, id: app_id });
    }

    // A Steam app-id resolved from the game's title, for games that have none of
    // their own — Epic and manual imports. This is the whole integration point of
    // title-based metadata: every Steam-backed provider keys on
    // `source_app_id("steam")`, so supplying one here makes the existing text and
    // artwork pipelines work for those games with no provider changes at all.
    //
    // Only used when the game has no Steam identity already. A real app-id always
    // wins: it is a fact, where a title match is an inference.
    if !identifiers
        .iter()
        .any(|id| matches!(id, GameIdentifier::SourceAppId { source, .. } if source == "steam"))
    {
        if let Some(matched) = db.steam_title_match(&game.id).await? {
            if let Some(app_id) = matched.app_id {
                identifiers.push(GameIdentifier::SourceAppId {
                    source: "steam".to_string(),
                    id: app_id,
                });
                // A DLC match is correct but has no library artwork of its own, so
                // artwork lookups are pointed at its base game while the identity
                // — and the description that comes from it — stays the DLC.
                if let Some(artwork_app_id) = matched.artwork_app_id {
                    identifiers.push(GameIdentifier::SourceArtworkAppId {
                        source: "steam".to_string(),
                        id: artwork_app_id,
                    });
                }
            }
        }
    }

    Ok(GameIdentity {
        title: game.title.clone(),
        identifiers,
    })
}
