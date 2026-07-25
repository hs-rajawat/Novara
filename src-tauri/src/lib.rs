//! NOVARA — Rust core.
//!
//! Crate layout:
//!   error      — single error type used across services
//!   events     — tokio broadcast bus for cross-service notifications
//!   db         — sqlx pool, migrations, typed repositories
//!   integrity  — Library Integrity System (install status resolution + verifier)
//!   models     — domain structs (Game, Achievement, …) shared with frontend
//!   scanner    — pluggable game-source scanners
//!   metadata   — pluggable metadata providers
//!   save_mgr   — save folder backup / restore
//!   playtime   — process watcher + idle detection
//!   commands   — Tauri IPC handlers (the only place that touches AppHandle)
//!   state      — AppState (holds Db, EventBus, configs, services)

pub mod commands;
pub mod db;
pub mod error;
pub mod events;
pub mod integrity;
pub mod launcher;
pub mod metadata;
pub mod models;
pub mod playtime;
pub mod save_detect;
pub mod save_mgr;
pub mod scanner;
pub mod sources;
pub mod state;

/// Shared test fixtures. Compiled only for `cargo test`, so it adds nothing
/// to the shipped binary.
#[cfg(test)]
pub mod test_support;

use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Resolve app data dir and bootstrap the database before any
            // command handler can run. We block on this — startup must be
            // deterministic and we'd rather fail fast than serve a
            // half-initialized state.
            let handle = app.handle().clone();
            let data_dir = handle
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;

            let state = tauri::async_runtime::block_on(async move {
                state::AppState::initialize(data_dir).await
            })?;
            let state = Arc::new(state);

            // Forward bus events to the frontend.
            state.start_event_forwarder(handle.clone());

            // Library Integrity System: a one-shot sweep at startup (catches
            // an uninstall that happened while NOVARA was closed) plus a
            // recurring sweep while the app stays open. Both must start
            // AFTER the event forwarder subscribes above, or any
            // GameUpdated/Notice they emit is dropped before anything is
            // listening.
            state.integrity.clone().spawn_startup_check();
            state
                .integrity
                .clone()
                .spawn_periodic_check(std::time::Duration::from_secs(300));

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::get_app_info,
            commands::system::get_settings,
            commands::system::set_setting,
            commands::games::list_games,
            commands::games::get_game,
            commands::games::set_favorite,
            commands::games::set_completion,
            commands::games::update_notes,
            commands::games::merge_duplicates,
            commands::games::launch_game,
            commands::games::import_executable,
            commands::games::set_installation_executable,
            commands::games::set_cover_path,
            commands::games::set_hero_path,
            commands::games::set_logo_path,
            commands::games::set_icon_path,
            commands::games::set_hidden,
            commands::games::refresh_metadata,
            commands::scan::scan_paths_now,
            commands::scan::add_scan_path,
            commands::scan::remove_scan_path,
            commands::scan::list_scan_paths,
            commands::achievements::list_achievements,
            commands::achievements::create_achievement,
            commands::achievements::toggle_achievement,
            commands::achievements::delete_achievement,
            commands::saves::list_save_profiles,
            commands::saves::create_save_profile,
            commands::saves::backup_now,
            commands::saves::list_backups,
            commands::saves::restore_backup,
            commands::saves::delete_save_profile,
            commands::saves::detect_save_paths,
            commands::playtime::start_session,
            commands::playtime::stop_session,
            commands::playtime::report_idle,
            commands::playtime::list_sessions,
            commands::analytics::dashboard_stats,
            commands::analytics::heatmap,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|handle, event| {
            // Close any in-progress play session before the process goes away.
            //
            // A session row is only ever closed by `PlaytimeTracker::stop`, and
            // the in-memory map of open sessions does not survive the process —
            // so quitting while a game was running used to leave the row with
            // `ended_at IS NULL` and no playtime credited, permanently. The
            // elapsed time up to this point is real, so it is recorded.
            //
            // `ExitRequested` is used rather than `Exit` because state is still
            // available here; the write is small and bounded, and blocking
            // briefly on shutdown is preferable to losing the session.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                if let Some(state) = handle.try_state::<Arc<state::AppState>>() {
                    let playtime = state.playtime.clone();
                    match tauri::async_runtime::block_on(playtime.stop_all()) {
                        Ok(0) => {}
                        Ok(n) => tracing::info!(closed = n, "closed open play sessions on exit"),
                        Err(e) => tracing::warn!(error = %e, "failed to close sessions on exit"),
                    }
                }
            }
        });
}
