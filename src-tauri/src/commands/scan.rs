use std::path::PathBuf;
use std::sync::Arc;

use serde_json::json;
use tauri::State;

use tracing::warn;

use crate::error::AppResult;
use crate::scanner::ScanReport;
use crate::state::AppState;

/// Scan every configured path, reconcile install health, then hand the
/// metadata/artwork fill off to a background task.
///
/// The fill used to run inline, so "Scan now" stayed in its scanning state
/// until every provider lookup and artwork download had finished — for a large
/// library, minutes — in direct contradiction of the project's own constraint
/// that background work must never block a user action. Since Batch 5 the fill
/// is also deliberately rate-limited, which made waiting on it worse still.
///
/// The integrity sweep stays inline on purpose: it is local filesystem work,
/// and the install statuses it resolves are part of what makes the returned
/// reports truthful. Only the network-bound fill is deferred.
///
/// Progress reaches the UI the same way it always did, through `GameUpdated`
/// events as assets land, so nothing is lost by returning early.
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

    spawn_post_scan_fill(Arc::clone(&state));

    Ok(reports)
}

/// Run the metadata and artwork fill in the background.
///
/// `tauri::async_runtime::spawn` rather than `tokio::spawn` for the same reason
/// the integrity sweeps use it: it attaches to the runtime Tauri actually owns.
/// Every failure is logged and swallowed — this task exists to enrich a library
/// the user can already use, so it must never surface as a failed scan.
fn spawn_post_scan_fill(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        // Metadata/artwork fill is opt-in (`metadata_enabled`) and always
        // respects the `offline_mode` kill-switch.
        let allow_network = match state.allow_metadata_network().await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "failed to read metadata settings; skipping post-scan fill");
                return;
            }
        };
        // Resolve titles first: an Epic or manual game has no Steam app-id, so
        // without this both fills below find no provider able to answer for it.
        if let Err(e) = state.titles.resolve_missing(allow_network).await {
            warn!(error = %e, "post-scan title resolution failed");
        }
        if let Err(e) = state.metadata.fill_missing(allow_network).await {
            warn!(error = %e, "post-scan metadata fill failed");
        }
        if let Err(e) = state.artwork.fill_missing(allow_network).await {
            warn!(error = %e, "post-scan artwork fill failed");
        }
    });
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
