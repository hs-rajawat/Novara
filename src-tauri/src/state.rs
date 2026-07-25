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
use crate::integrity::IntegrityService;
use crate::metadata::artwork_service::ArtworkService;
use crate::metadata::text_service::MetadataService;
use crate::playtime::PlaytimeTracker;
use crate::save_mgr::SaveManager;
use crate::scanner::ScannerOrchestrator;

pub struct AppState {
    pub db: Db,
    pub bus: EventBus,
    pub scanner: ScannerOrchestrator,
    pub saves: SaveManager,
    pub playtime: Arc<PlaytimeTracker>,
    pub integrity: Arc<IntegrityService>,
    pub metadata: Arc<MetadataService>,
    pub artwork: Arc<ArtworkService>,
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
        let integrity = Arc::new(IntegrityService::new(db.clone(), bus.clone()));

        // Shared by every network-touching provider — a single client reuses
        // connections across the whole app rather than each provider paying
        // its own TLS/DNS setup cost per call.
        let http_client = reqwest::Client::new();
        let metadata = Arc::new(MetadataService::new(db.clone(), bus.clone(), http_client.clone()));
        let artwork = Arc::new(ArtworkService::new(
            db.clone(),
            bus.clone(),
            app_data_dir.clone(),
            http_client,
        ));

        // Kick off passive process watcher with a generous poll interval —
        // 5s is enough resolution for playtime tracking and gentle on CPU.
        playtime.clone().spawn_watcher(Duration::from_secs(5));

        // The integrity service's startup/periodic sweeps are intentionally
        // NOT spawned here — they emit bus events, and doing so before
        // `start_event_forwarder` has subscribed would silently drop them.
        // lib.rs spawns them right after the forwarder subscribes instead.

        Ok(Self {
            db,
            bus,
            scanner,
            saves,
            playtime,
            integrity,
            metadata,
            artwork,
            app_data_dir,
        })
    }

    /// `metadata_enabled && !offline_mode` — the one gate every network
    /// provider call must pass. Read fresh each call rather than cached, so
    /// flipping either setting takes effect on the very next sweep/refresh
    /// without an app restart.
    pub async fn allow_metadata_network(&self) -> AppResult<bool> {
        let enabled = self
            .db
            .get_setting("metadata_enabled")
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let offline = self
            .db
            .get_setting("offline_mode")
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(enabled && !offline)
    }

    /// Forward AppEvents to the Tauri event bus so the frontend can listen.
    /// Call once after the App is set up.
    pub fn start_event_forwarder(self: &Arc<Self>, handle: AppHandle) {
        let mut rx = self.bus.subscribe();
        tauri::async_runtime::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                let _ = handle.emit("gv://event", &ev);
            }
        });
    }
}
