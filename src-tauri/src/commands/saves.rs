use std::sync::Arc;

use tauri::State;

use crate::error::AppResult;
use crate::models::{SaveBackup, SaveProfile};
use crate::state::AppState;

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
