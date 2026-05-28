//! AppState — the single object that every command handler receives.
//! Holds the DB pool, event bus, and long-lived services. Cloning is
//! cheap because each field is internally Arc'd.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tracing::info;

use crate::db::Db;
use crate::error::AppResult;
use crate::events::EventBus;
use crate::playtime::PlaytimeTracker;
use crate::save_mgr::SaveManager;
use crate::scanner::ScannerOrchestrator;

pub struct AppState {
    pub db: Db,
    pub bus: EventBus,
    pub scanner: ScannerOrchestrator,
    pub saves: SaveManager,
    pub playtime: Arc<PlaytimeTracker>,
    pub app_data_dir: PathBuf,
}

impl AppState {
    pub async fn initialize(app_data_dir: PathBuf) -> AppResult<Self> {
        info!(dir = %app_data_dir.display(), "initializing app state");
        let db_path = app_data_dir.join("gamevault.db");
        let db = Db::open(&db_path).await?;
        let bus = EventBus::default();
        let scanner = ScannerOrchestrator::new(db.clone(), bus.clone());
        let saves = SaveManager::new(db.clone(), bus.clone(), &app_data_dir)?;
        let playtime = Arc::new(PlaytimeTracker::new(db.clone(), bus.clone()));

        // Kick off passive process watcher with a generous poll interval —
        // 5s is enough resolution for playtime tracking and gentle on CPU.
        playtime.clone().spawn_watcher(Duration::from_secs(5));

        Ok(Self {
            db,
            bus,
            scanner,
            saves,
            playtime,
            app_data_dir,
        })
    }

    /// Forward AppEvents to the Tauri event bus so the frontend can listen.
    /// Call once after the App is set up.
    pub fn start_event_forwarder(self: &Arc<Self>, handle: AppHandle) {
        let mut rx = self.bus.subscribe();
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                let _ = handle.emit("gv://event", &ev);
            }
        });
    }
}
