//! GameVault — Rust core.
//!
//! Crate layout:
//!   error      — single error type used across services
//!   events     — tokio broadcast bus for cross-service notifications
//!   db         — sqlx pool, migrations, typed repositories
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
pub mod metadata;
pub mod models;
pub mod playtime;
pub mod save_mgr;
pub mod scanner;
pub mod state;

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
            commands::playtime::start_session,
            commands::playtime::stop_session,
            commands::playtime::list_sessions,
            commands::analytics::dashboard_stats,
            commands::analytics::heatmap,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
