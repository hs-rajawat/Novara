use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::models::PlaySession;
use crate::state::AppState;

#[tauri::command]
pub async fn start_session(
    game_id: String,
    process_name: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<i64> {
    state.playtime.start(&game_id, process_name.as_deref()).await
}

#[tauri::command]
pub async fn stop_session(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Option<i64>> {
    state.playtime.stop(&game_id).await
}

#[tauri::command]
pub async fn list_sessions(
    game_id: Option<String>,
    limit: Option<i64>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<PlaySession>> {
    state
        .db
        .list_sessions(game_id.as_deref(), limit.unwrap_or(100))
        .await
}
