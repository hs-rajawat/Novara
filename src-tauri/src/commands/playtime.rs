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

/// Report observed idle time against an in-progress session.
///
/// `PlaytimeTracker::report_idle` has existed since the MVP and
/// `idle_threshold_seconds` has been seeded in settings since migration 0001,
/// but this command was never registered — so nothing could reach it,
/// `play_sessions.idle_seconds` was always 0, and the schema's stated goal of
/// separating active from total time did not exist in practice.
///
/// Deliberately additive and best-effort: the frontend reports elapsed idle
/// deltas, and a report for a game with no open session is a no-op rather than
/// an error, because a session can end between observation and report.
#[tauri::command]
pub async fn report_idle(
    game_id: String,
    idle_seconds: i64,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    if idle_seconds <= 0 {
        return Ok(());
    }
    state.playtime.report_idle(&game_id, idle_seconds).await;
    Ok(())
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
