use std::sync::Arc;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::models::{SaveBackup, SaveProfile};
use crate::saves::locator::DetectedPath;
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

/// Search common OS locations for a save folder matching the game's title.
/// Paths already tracked by an existing save profile are excluded from results.
#[tauri::command]
pub async fn detect_save_paths(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<DetectedPath>> {
    let game = state
        .db
        .get_game(&game_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("game {game_id}")))?;

    let existing: std::collections::HashSet<String> = state
        .db
        .list_save_profiles(&game_id)
        .await?
        .into_iter()
        .map(|p| p.source_dir)
        .collect();

    let found = crate::saves::locator::detect(&crate::saves::fs::RealFs, &game.title);

    Ok(found
        .into_iter()
        .filter(|d| !existing.contains(&d.path))
        .collect())
}
