use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::models::{Game, Installation};
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct GameWithInstalls {
    #[serde(flatten)]
    pub game: Game,
    pub installations: Vec<Installation>,
}

#[tauri::command]
pub async fn list_games(
    include_hidden: Option<bool>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<Game>> {
    state.db.list_games(include_hidden.unwrap_or(false)).await
}

#[tauri::command]
pub async fn get_game(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Option<GameWithInstalls>> {
    let Some(game) = state.db.get_game(&id).await? else {
        return Ok(None);
    };
    let installations = state.db.list_installations(&id).await?;
    Ok(Some(GameWithInstalls { game, installations }))
}

#[tauri::command]
pub async fn set_favorite(
    id: String,
    favorite: bool,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.set_favorite(&id, favorite).await
}

#[tauri::command]
pub async fn set_completion(
    id: String,
    pct: f64,
    completion_state: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.set_completion(&id, pct, &completion_state).await
}

#[tauri::command]
pub async fn update_notes(
    id: String,
    notes: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.set_notes(&id, notes.as_deref()).await
}

#[tauri::command]
pub async fn merge_duplicates(
    from_id: String,
    to_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.merge_games(&from_id, &to_id).await
}
