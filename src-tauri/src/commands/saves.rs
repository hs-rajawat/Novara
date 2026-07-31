use std::sync::Arc;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::models::{SaveBackup, SaveKbVersion, SaveProfile};
use crate::saves::kb::import::{self, UserEntryInput};
use crate::saves::locator::DetectedPath;
use crate::state::AppState;

/// The knowledge base layers present, with the version of each.
///
/// Read-only and cheap; exposed so a user can see whether the corpus loaded at all
/// rather than having to infer it from detection quality.
#[tauri::command]
pub async fn save_kb_status(state: State<'_, Arc<AppState>>) -> AppResult<Vec<SaveKbVersion>> {
    state.db.kb_versions().await
}

/// Record a user's own save-location rule.
///
/// Validated exactly as the shipped corpus is — see `saves::kb::validate`. A
/// rejected entry returns an `invalid` error naming the specific problem.
#[tauri::command]
pub async fn add_save_kb_entry(
    entry: UserEntryInput,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    import::add_user_entry(&state.db, &entry).await
}

/// Remove a user's own rule. Refuses ids outside the `user` layer.
#[tauri::command]
pub async fn remove_save_kb_entry(
    id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    import::remove_user_entry(&state.db, &id).await
}

#[tauri::command]
pub async fn list_save_profiles(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<SaveProfile>> {
    state.db.list_save_profiles(&game_id).await
}

#[derive(serde::Deserialize)]
pub struct NewSaveProfile {
    pub game_id: String,
    pub label: String,
    pub source_dir: String,
    pub glob: Option<String>,
    pub auto_backup: Option<bool>,
    /// True when the user explicitly chose this folder (dialog picker).
    /// False (default) when created from an auto-detected path.
    pub is_manual_override: Option<bool>,
}

#[tauri::command]
pub async fn create_save_profile(
    input: NewSaveProfile,
    state: State<'_, Arc<AppState>>,
) -> AppResult<SaveProfile> {
    state
        .db
        .create_save_profile(
            &input.game_id,
            &input.label,
            &input.source_dir,
            input.glob.as_deref(),
            input.auto_backup.unwrap_or(true),
            input.is_manual_override.unwrap_or(false),
        )
        .await
}

#[derive(serde::Serialize)]
pub struct BackupSummary {
    pub backup_id: i64,
    pub archive_path: String,
    pub size_bytes: i64,
    pub file_count: i64,
}

#[tauri::command]
pub async fn backup_now(
    profile_id: String,
    note: Option<String>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<BackupSummary> {
    let res = state.saves.backup(&profile_id, note.as_deref()).await?;
    Ok(BackupSummary {
        backup_id: res.backup_id,
        archive_path: res.archive_path.display().to_string(),
        size_bytes: res.size_bytes,
        file_count: res.file_count,
    })
}

#[tauri::command]
pub async fn list_backups(
    profile_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<SaveBackup>> {
    state.db.list_backups(&profile_id).await
}

#[tauri::command]
pub async fn restore_backup(
    backup_id: i64,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.saves.restore(backup_id).await
}

#[tauri::command]
pub async fn delete_save_profile(
    profile_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.delete_save_profile(&profile_id).await
}

/// Search for save folders belonging to a game, and persist what was found.
///
/// Runs the **complete Phase 1 detection pipeline** — locator, knowledge base, verifier,
/// decision table — by way of [`crate::saves::service`]. This is the same
/// `pipeline::detect_with_kb` call the scenario corpus exercises; there is deliberately
/// no production-only detection path, because two implementations would mean the tests
/// stop describing the shipped behaviour.
///
/// Detection **never binds**. The strongest outcome recorded is `bind_eligible`, meaning
/// the decision table would bind this path if asked. Creating the save profile is still
/// a user action (`create_save_profile`), and remains so until Phase 3 has a correction
/// UI.
///
/// Paths already tracked by an existing save profile are excluded from the *returned*
/// list, since offering a folder the user has already set up is noise. They are still
/// persisted as candidates: the evidence is worth keeping either way.
#[tauri::command]
pub async fn detect_save_paths(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<DetectedPath>> {
    let existing: std::collections::HashSet<String> = state
        .db
        .list_save_profiles(&game_id)
        .await?
        .into_iter()
        .map(|p| p.source_dir.replace('\\', "/"))
        .collect();

    // A user pressing the button gets a scan now; the retry ladder is for bulk sweeps.
    let run = match crate::saves::service::detect_and_persist(
        &state.db,
        &crate::saves::fs::RealFs,
        &game_id,
        crate::saves::service::Trigger::User,
    )
    .await
    {
        Ok(Some(run)) => run,
        Ok(None) => return Err(AppError::NotFound(format!("game {game_id}"))),
        Err(e) => {
            // The error ladder is shorter than the "found nothing" one, because an error
            // says nothing about the game. Recorded on a best-effort basis: failing to
            // record a failure must not replace the real error.
            let _ = crate::saves::service::record_scan_error(&state.db, &game_id).await;
            return Err(e);
        }
    };

    Ok(run
        .outcome
        .candidates
        .into_iter()
        .filter(|d| !existing.contains(&d.path.replace('\\', "/")))
        .collect())
}
