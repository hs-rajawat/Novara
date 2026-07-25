use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tauri::State;

use tracing::warn;

use crate::error::AppResult;
use crate::scanner::ScanReport;
use crate::state::AppState;

#[tauri::command]
pub async fn scan_paths_now(state: State<'_, Arc<AppState>>) -> AppResult<Vec<ScanReport>> {
    let paths = list_scan_paths_internal(&state).await?;
    let reports = state.scanner.run(paths).await?;

    // Best-effort reconciliation sweep: a full scan pass only touches
    // installations its scanners currently found, so this catches anything
    // that quietly vanished (and, symmetrically, confirms a reinstall) for
    // rows the scan itself didn't revisit. Failure here shouldn't fail the
    // scan the user actually asked for.
    if let Err(e) = state.integrity.verify_all().await {
        warn!(error = %e, "post-scan integrity sweep failed");
    }

    // Metadata/artwork fill is opt-in (`metadata_enabled`) and always
    // respects the `offline_mode` kill-switch — best-effort here too, since
    // a provider hiccup shouldn't fail the scan the user actually asked for.
    match state.allow_metadata_network().await {
        Ok(allow_network) => {
            if let Err(e) = state.metadata.fill_missing(allow_network).await {
                warn!(error = %e, "post-scan metadata fill failed");
            }
            if let Err(e) = state.artwork.fill_missing(allow_network).await {
                warn!(error = %e, "post-scan artwork fill failed");
            }
        }
        Err(e) => warn!(error = %e, "failed to read metadata settings; skipping post-scan fill"),
    }

    Ok(reports)
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
