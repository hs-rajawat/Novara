use std::path::Path;
use std::sync::Arc;

use tauri::State;
use crate::db::games::UpsertGame;
use crate::error::{AppError, AppResult};
use crate::events::AppEvent;
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

/// Launch a game by its primary installation.
///
/// Resolution order:
///   1. `executable` column present → spawn the binary directly.
///   2. `source_app_id` present → open a `steam://run/<id>` URI via the OS
///      default protocol handler (Steam registers this on install).
///   3. Neither → NotFound error.
///
/// A playtime session is opened in both cases; the passive watcher closes it
/// automatically when the process disappears from the process list.
#[tauri::command]
pub async fn launch_game(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    let installs = state.db.list_installations(&game_id).await?;
    let install = installs
        .into_iter()
        .max_by_key(|i| i.is_primary)
        .ok_or_else(|| AppError::NotFound("no installation found for this game".into()))?;

    if let Some(exe_rel) = &install.executable {
        let exe_path = std::path::Path::new(&install.install_dir).join(exe_rel);
        let _child = std::process::Command::new(&exe_path)
            .current_dir(&install.install_dir)
            .spawn()
            .map_err(|e| AppError::Other(format!("failed to launch executable: {e}")))?;
        state.playtime.start(&game_id, Some(exe_rel.as_str())).await?;
    } else if let Some(app_id) = &install.source_app_id {
        open_uri(&format!("steam://run/{app_id}"))
            .map_err(|e| AppError::Other(format!("failed to open steam URI: {e}")))?;
        state.playtime.start(&game_id, None).await?;
    } else {
        return Err(AppError::NotFound(
            "no executable or launch URI for this installation".into(),
        ));
    }

    Ok(())
}

/// Open a URI with the OS default handler. Uses platform-specific commands
/// rather than a plugin to keep the dependency surface minimal.
fn open_uri(uri: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", uri])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(uri).spawn()?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(uri).spawn()?;
    }
    Ok(())
}

#[tauri::command]
pub async fn merge_duplicates(
    from_id: String,
    to_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.merge_games(&from_id, &to_id).await
}

/// Import a game directly from a specific executable, bypassing the folder
/// scanner heuristics. The parent directory becomes `install_dir`; the
/// executable is stored by filename only (relative to that directory).
///
/// Returns the canonical game UUID so the caller can navigate to GameDetails.
#[tauri::command]
pub async fn import_executable(
    exe_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    let exe = Path::new(&exe_path);
    if !exe.is_file() {
        return Err(AppError::Invalid(format!("not a file: {exe_path}")));
    }
    let install_dir = exe
        .parent()
        .ok_or_else(|| AppError::Invalid("cannot determine install directory from path".into()))?;
    let title = install_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| exe.file_stem().and_then(|s| s.to_str()).unwrap_or("Imported Game"));
    let executable = exe
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::Invalid("invalid executable filename".into()))?;
    let install_dir_str = install_dir.to_string_lossy();

    let result = state
        .db
        .upsert_game(UpsertGame {
            title,
            source_code: "manual",
            source_app_id: None,
            install_dir: &install_dir_str,
            executable: Some(executable),
            install_size_bytes: None,
            executable_override: true,
        })
        .await?;

    if result.created {
        state.bus.emit(AppEvent::GameAdded {
            game_id: result.game_id_owned.to_string(),
            title: title.to_string(),
        });
    }

    Ok(result.game_id_owned.to_string())
}

/// Override the executable for an existing installation and lock it so
/// subsequent rescans will not overwrite the user's choice.
#[tauri::command]
pub async fn set_installation_executable(
    installation_id: String,
    exe_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state
        .db
        .set_installation_executable(&installation_id, &exe_path)
        .await
}
