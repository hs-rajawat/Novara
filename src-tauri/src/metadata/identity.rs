//! Builds a `GameIdentity` from persisted game + installation rows — the
//! one place `text_service`/`artwork_service` translate a DB row into what a
//! provider actually consumes. Kept separate from both services so neither
//! has to duplicate the installation → identifier translation.

use crate::db::Db;
use crate::error::AppResult;
use crate::models::Game;

use super::{GameIdentifier, GameIdentity};

pub async fn identity_for(db: &Db, game: &Game) -> AppResult<GameIdentity> {
    let mut identifiers = Vec::new();
    for (source, app_id) in db.list_source_app_ids(&game.id).await? {
        identifiers.push(GameIdentifier::SourceAppId { source, id: app_id });
    }
    Ok(GameIdentity {
        title: game.title.clone(),
        identifiers,
    })
}
