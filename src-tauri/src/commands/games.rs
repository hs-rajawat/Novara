use std::path::Path;
use std::sync::Arc;

use crate::db::games::{ExecutableSource, UpsertGame};
use crate::error::{AppError, AppResult};
use crate::events::{AppEvent, NoticeLevel};
use crate::integrity::{resolve_installation_status, InstallStatus};
use crate::metadata::store::store_local_asset;
use crate::metadata::ArtworkKind;
use crate::models::{Game, Installation};
use crate::scanner::epic::EpicContext;
use crate::scanner::steam::SteamContext;
use crate::state::AppState;
use tauri::State;

/// Copy a user-picked file into `<app_data>/artwork/<game_id>/<kind>.<ext>`
/// and lock it against the auto-fetcher (`ArtworkService` never overwrites
/// a `user_locked` asset — see `db::artwork::lock_artwork_asset`).
async fn set_user_artwork(
    state: &AppState,
    game_id: &str,
    image_path: &str,
    kind: ArtworkKind,
) -> AppResult<String> {
    let stored = store_local_asset(&state.app_data_dir, game_id, kind, Path::new(image_path))?;
    state
        .db
        .lock_artwork_asset(game_id, kind.as_str(), &stored)
        .await?;
    match kind {
        ArtworkKind::Cover => state.db.set_cover_path(game_id, &stored).await?,
        ArtworkKind::Hero => state.db.set_hero_path(game_id, &stored).await?,
        ArtworkKind::Logo => state.db.set_logo_path(game_id, &stored).await?,
        ArtworkKind::Icon => state.db.set_icon_path(game_id, &stored).await?,
    }
    state.bus.emit(AppEvent::GameUpdated {
        game_id: game_id.to_string(),
    });
    Ok(stored)
}

#[tauri::command]
pub async fn set_cover_path(
    game_id: String,
    image_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    set_user_artwork(&state, &game_id, &image_path, ArtworkKind::Cover).await
}

#[tauri::command]
pub async fn set_hero_path(
    game_id: String,
    image_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    set_user_artwork(&state, &game_id, &image_path, ArtworkKind::Hero).await
}

#[tauri::command]
pub async fn set_logo_path(
    game_id: String,
    image_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    set_user_artwork(&state, &game_id, &image_path, ArtworkKind::Logo).await
}

#[tauri::command]
pub async fn set_icon_path(
    game_id: String,
    image_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<String> {
    set_user_artwork(&state, &game_id, &image_path, ArtworkKind::Icon).await
}

/// `list_games` row shape: the plain `Game` row plus its primary
/// installation's platform, resolved in bulk so the Library grid can render
/// platform badges without a per-card round trip.
#[derive(serde::Serialize)]
pub struct GameSummary {
    #[serde(flatten)]
    pub game: Game,
    pub primary_source_code: Option<String>,
    pub primary_source_label: Option<String>,
    /// Library Integrity status ("installed" | "missing") of the game's
    /// primary installation, if it has one. Drives the Library grid's
    /// Missing badge/hide-when-uninstalled-Steam behavior and the Play
    /// quick-action's disabled state.
    pub primary_install_status: Option<String>,
}

#[derive(serde::Serialize)]
pub struct GameWithInstalls {
    #[serde(flatten)]
    pub game: Game,
    pub installations: Vec<Installation>,
    pub primary_source_code: Option<String>,
    pub primary_source_label: Option<String>,
    pub primary_install_status: Option<String>,
}

#[tauri::command]
pub async fn list_games(
    include_hidden: Option<bool>,
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<GameSummary>> {
    let games = state.db.list_games(include_hidden.unwrap_or(false)).await?;
    let sources = state.db.list_primary_sources().await?;
    Ok(games
        .into_iter()
        .map(|g| {
            let source = sources.get(&g.id).cloned();
            GameSummary {
                primary_source_code: source.as_ref().map(|s| s.source_code.clone()),
                primary_source_label: source.as_ref().map(|s| s.source_label.clone()),
                primary_install_status: source.map(|s| s.status),
                game: g,
            }
        })
        .collect())
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
    let source = state.db.get_primary_source(&id).await?;
    Ok(Some(GameWithInstalls {
        game,
        installations,
        primary_source_code: source.as_ref().map(|s| s.source_code.clone()),
        primary_source_label: source.as_ref().map(|s| s.source_label.clone()),
        primary_install_status: source.map(|s| s.status),
    }))
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
/// Before choosing how to launch, the installation's Library Integrity
/// status is re-resolved right here — synchronously, for just this one row
/// — so Play is blocked deterministically for a Missing installation
/// regardless of whether any background sweep has caught up yet. This is
/// what stops a stale "installed" Steam row from redirecting to Steam's
/// install/store flow, and what turns a manual game's deleted executable
/// into a clear error instead of a silent no-op.
///
/// Resolution order (once confirmed Installed):
///   1. `source_app_id` present AND the source is launcher-managed
///      (`sources::launch_uri` returns a URI) → open that deep-link via the
///      OS default protocol handler. Steam registers `steam://`, Epic
///      registers `com.epicgames.launcher://`. URI-first: launcher-managed
///      titles always go through their launcher even though an `executable`
///      may be persisted for verification/diagnostics.
///   2. `executable` column present (non-launcher sources, e.g. manual) →
///      spawn the binary directly.
///   3. Neither → NotFound error.
///
/// A playtime session is opened in both cases; the passive watcher closes it
/// automatically when the process disappears from the process list.
#[tauri::command]
pub async fn launch_game(game_id: String, state: State<'_, Arc<AppState>>) -> AppResult<()> {
    let installs = state.db.list_installations(&game_id).await?;
    // Same rule the UI's status queries use (health first, `is_primary` only
    // as a tie-break) so Play cannot target a different installation than the
    // one whose status is on screen.
    let install = crate::db::games::primary_installation(&installs)
        .cloned()
        .ok_or_else(|| AppError::NotFound("no installation found for this game".into()))?;

    let source_code = state.db.source_code_for(install.source_id).await?;
    let steam_ctx = SteamContext::discover();
    let epic_ctx = EpicContext::discover();
    let resolution = resolve_installation_status(
        &source_code,
        install.source_app_id.as_deref(),
        &install.install_dir,
        install.executable.as_deref(),
        Some(&steam_ctx),
        Some(&epic_ctx),
    );

    // The launcher reports this game moved since we last saw it — relink the
    // row to the new directory (history preserved) and launch from there,
    // rather than blocking Play on a path that's merely stale.
    let install = if let Some(new_dir) = resolution.relink_to.as_deref() {
        let new_dir = new_dir.to_string_lossy();
        state.db.relink_installation(&install.id, &new_dir).await?;
        state.bus.emit(AppEvent::GameUpdated {
            game_id: game_id.clone(),
        });
        // Re-read so the launch below uses the new directory.
        let installs = state.db.list_installations(&game_id).await?;
        crate::db::games::primary_installation(&installs)
            .cloned()
            .ok_or_else(|| AppError::NotFound("no installation found for this game".into()))?
    } else {
        install
    };

    let resolved = resolution.status;
    if resolved.as_str() != install.status && resolution.relink_to.is_none() {
        state
            .db
            .set_installation_status(&install.id, resolved.as_str())
            .await?;
        state.bus.emit(AppEvent::GameUpdated {
            game_id: game_id.clone(),
        });
    }
    if resolved != InstallStatus::Installed {
        state.bus.emit(AppEvent::Notice {
            level: NoticeLevel::Warning,
            message: "This installation could not be found — it may have been uninstalled, moved, deleted, or its drive is disconnected. Locate its executable, reconnect the drive, or reinstall it from the source launcher.".into(),
        });
        return Err(AppError::Invalid(
            "installation is not available — locate the executable, reconnect the drive, or reinstall".into(),
        ));
    }

    // Launcher-managed sources (steam, epic, …) launch through their
    // deep-link URI, even though we persist an executable for verification
    // and diagnostics. Adding a future launcher is one arm in
    // `sources::launch_uri` — no change here.
    if let Some(app_id) = &install.source_app_id {
        if let Some(uri) = crate::sources::launch_uri(&source_code, app_id) {
            // Some launchers (Epic) drop the launch URI when the URI itself
            // cold-starts them. Pre-warm ensures the launcher is up and past
            // its bootstrap before we send the URI once. No-op for launchers
            // that cold-start correctly on their own (Steam).
            if let Err(e) = crate::launcher::prewarm_for(&source_code).await {
                tracing::warn!(error = %e, source = %source_code, "launcher pre-warm failed; sending URI anyway");
            }
            tracing::info!(source = %source_code, uri = %uri, "launching via launcher URI");
            open_uri(&uri)
                .map_err(|e| AppError::Other(format!("failed to open launcher URI: {e}")))?;
            state.playtime.start(&game_id, None).await?;
            return Ok(());
        }
    }

    // Non-launcher sources (manual) spawn the persisted executable directly.
    if let Some(exe_rel) = &install.executable {
        let exe_path = std::path::Path::new(&install.install_dir).join(exe_rel);
        let _child = std::process::Command::new(&exe_path)
            .current_dir(&install.install_dir)
            .spawn()
            .map_err(|e| AppError::Other(format!("failed to launch executable: {e}")))?;
        state
            .playtime
            .start(&game_id, Some(exe_rel.as_str()))
            .await?;
        return Ok(());
    }

    Err(AppError::NotFound(
        "no launch URI or executable for this installation".into(),
    ))
}

/// Open a URI with the OS default handler.
///
/// On Windows this calls `ShellExecuteW` directly rather than shelling out to
/// `cmd /c start`. `cmd` treats `&` as a command separator, which silently
/// truncates deep-link URIs that carry a query string (e.g. Epic's
/// `...?action=launch&silent=true`) and never launches the game; passing the
/// URI as one opaque wide-string argument to the shell API avoids any shell
/// re-parsing. macOS/Linux hand the URI as a single argv entry to
/// `open`/`xdg-open`, which likewise never shell-split it.
fn open_uri(uri: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;

        // NUL-terminated UTF-16 for the Win32 API.
        let wide: Vec<u16> = std::ffi::OsStr::new(uri)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: [u16; 5] = [b'o' as u16, b'p' as u16, b'e' as u16, b'n' as u16, 0];

        // SAFETY: `verb` and `wide` are valid NUL-terminated UTF-16 buffers
        // that outlive the call; all other pointers are null as permitted by
        // the API. ShellExecuteW does not retain them past the call.
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW returns a value > 32 on success; anything <= 32 is an
        // error code.
        if (result as isize) <= 32 {
            return Err(std::io::Error::other(format!(
                "ShellExecuteW failed for {uri} (code {})",
                result as isize
            )));
        }
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
        .unwrap_or_else(|| {
            exe.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Imported Game")
        });
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
            executable_source: ExecutableSource::User,
            install_state_hint: None,
        })
        .await?;

    if result.created {
        state.bus.emit(AppEvent::GameAdded {
            game_id: result.game_id_owned.to_string(),
            title: title.to_string(),
        });
    } else {
        // An existing installation was updated in place, so anything showing it
        // needs to re-read.
        state.bus.emit(AppEvent::GameUpdated {
            game_id: result.game_id_owned.to_string(),
        });
    }

    // Read back what was actually stored and confirm it is what the user picked.
    // The previous failure mode was silent: the upsert kept a different
    // executable and the import reported success anyway, so the user had no way
    // to tell their choice had been discarded. This turns that class of bug into
    // an error instead of a mystery.
    let stored = state
        .db
        .installation_executable(&install_dir_str)
        .await?
        .flatten();
    if stored.as_deref() != Some(executable) {
        return Err(AppError::Other(format!(
            "imported {} but the stored executable is {:?}; the import did not take effect",
            executable, stored
        )));
    }

    Ok(result.game_id_owned.to_string())
}

/// Override the executable for an existing installation and lock it so
/// subsequent rescans will not overwrite the user's choice. Also the
/// "Locate Executable" recovery path for a Missing installation — see
/// `Db::set_installation_executable`, which restores it to Installed.
#[tauri::command]
pub async fn set_installation_executable(
    installation_id: String,
    exe_path: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    let game_id = state
        .db
        .set_installation_executable(&installation_id, &exe_path)
        .await?;
    state.bus.emit(AppEvent::GameUpdated { game_id });
    Ok(())
}

/// "Remove from Library" (`hidden = true`) / "Restore to Library"
/// (`hidden = false`). Non-destructive — see `Db::set_hidden`: no row is
/// deleted, so playtime, sessions, achievements, saves, mods, and artwork
/// are all preserved. Never touches files on disk.
#[tauri::command]
pub async fn set_hidden(
    id: String,
    hidden: bool,
    state: State<'_, Arc<AppState>>,
) -> AppResult<()> {
    state.db.set_hidden(&id, hidden).await?;
    state.bus.emit(AppEvent::GameUpdated { game_id: id });
    Ok(())
}

#[derive(serde::Serialize)]
pub struct RefreshMetadataResult {
    pub text_updated: bool,
    pub artwork_updated: u32,
    /// Whether network-backed providers were permitted for this refresh
    /// (`metadata_enabled && !offline_mode`).
    ///
    /// Without this the frontend cannot tell "the providers ran and found
    /// nothing new" from "the feature is switched off and almost nothing
    /// ran", which are the same empty result but need opposite advice. Note
    /// it is not simply the inverse of a successful refresh: `steam_local`
    /// copies from Steam's own on-disk cache and can still succeed with
    /// network access denied.
    pub network_allowed: bool,
}

/// The "Refresh Metadata" button — an explicit, single-game re-fetch.
/// Still gated by the same `metadata_enabled && !offline_mode` formula
/// every automatic sweep uses (see `AppState::allow_metadata_network`), so
/// this is "fetch now" rather than a way to bypass the privacy toggle.
/// `metadata_source = 'manual'` and `user_locked` artwork kinds are
/// respected exactly as they are during an automatic sweep — see
/// `MetadataService::refresh_game`/`ArtworkService::refresh_game`.
#[tauri::command]
pub async fn refresh_metadata(
    game_id: String,
    state: State<'_, Arc<AppState>>,
) -> AppResult<RefreshMetadataResult> {
    let allow_network = state.allow_metadata_network().await?;
    let text_updated = state.metadata.refresh_game(&game_id, allow_network).await?;
    let artwork_updated = state.artwork.refresh_game(&game_id, allow_network).await?;
    Ok(RefreshMetadataResult {
        text_updated,
        artwork_updated,
        network_allowed: allow_network,
    })
}
