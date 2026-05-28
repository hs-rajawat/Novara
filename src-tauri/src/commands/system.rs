use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct AppInfo {
    pub version: &'static str,
    pub data_dir: String,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, Arc<AppState>>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.app_data_dir.display().to_string(),
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> AppResult<serde_json::Map<String, Value>> {
    state.db.all_settings().await
}

#[tauri::command]
pub async fn set_setting(
    key: String,
    value: Value,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.set_setting(&key, &value).await
}
