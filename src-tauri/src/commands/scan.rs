use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tauri::State;

use crate::error::AppResult;
use crate::scanner::ScanReport;
use crate::state::AppState;

#[tauri::command]
pub async fn scan_paths_now(state: State<'_, Arc<AppState>>) -> AppResult<Vec<ScanReport>> {
    let paths = list_scan_paths_internal(&state).await?;
    state.scanner.run(paths).await
}

#[tauri::command]
pub async fn list_scan_paths(state: State<'_, Arc<AppState>>) -> AppResult<Vec<String>> {
    Ok(list_scan_paths_internal(&state)
        .await?
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub async fn add_scan_path(path: String, state: State<'_, Arc<AppState>>) -> AppResult<()> {
    let mut paths = list_scan_paths_internal(&state).await?;
    let p = PathBuf::from(&path);
    if !paths.contains(&p) {
        paths.push(p);
    }
    save_scan_paths(&state, paths).await
}

#[tauri::command]
pub async fn remove_scan_path(path: String, state: State<'_, Arc<AppState>>) -> AppResult<()> {
    let mut paths = list_scan_paths_internal(&state).await?;
    paths.retain(|p| p != &PathBuf::from(&path));
    save_scan_paths(&state, paths).await
}

async fn list_scan_paths_internal(state: &AppState) -> AppResult<Vec<PathBuf>> {
    let v = state
        .db
        .get_setting("scan_paths")
        .await?
        .unwrap_or_else(|| json!([]));
    let arr = v.as_array().cloned().unwrap_or_default();
    Ok(arr
        .into_iter()
        .filter_map(|x| x.as_str().map(PathBuf::from))
        .collect())
}

async fn save_scan_paths(state: &AppState, paths: Vec<PathBuf>) -> AppResult<()> {
    let arr: Vec<_> = paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect();
    state.db.set_setting("scan_paths", &json!(arr)).await
}
