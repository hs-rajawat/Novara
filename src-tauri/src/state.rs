//! AppState — the single object that every command handler receives.
//! Holds the DB pool, event bus, and long-lived services. Cloning is
//! cheap because each field is internally Arc'd.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tracing::{info, warn};
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
    /// Handles for the long-lived background loops (playtime watcher, periodic
    /// integrity sweep) so shutdown can stop them deliberately instead of
    /// relying on process exit to tear them down mid-operation.
    background: Mutex<Vec<JoinHandle<()>>>,
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
        // One throttle for the whole application, so the concurrency cap and
        // minimum request spacing bound NOVARA's *total* outbound rate rather
        // than each call site's. Both fill services and asset downloads share
        // it deliberately.
        let throttle = Arc::new(crate::metadata::throttle::Throttle::default());
        let metadata = Arc::new(MetadataService::new(
            db.clone(),
            bus.clone(),
            http_client.clone(),
            throttle.clone(),
        ));
        let artwork = Arc::new(ArtworkService::new(
            db.clone(),
            bus.clone(),
            app_data_dir.clone(),
            http_client,
            throttle,
        ));

        // Reconcile sessions left open by a previous run before the watcher
        // starts. Nothing else will ever close them: the tracker's in-memory
        // map begins empty, so those rows have no owner. Done before the
        // watcher spawns so a stale row cannot be mistaken for a live session.
        match db.close_orphaned_sessions().await {
            Ok(0) => {}
            Ok(n) => info!(closed = n, "closed play sessions orphaned by a previous run"),
            // Best-effort: a failure here must not prevent startup.
            Err(e) => warn!(error = %e, "failed to close orphaned play sessions"),
        }

        // Kick off passive process watcher with a generous poll interval —
        // 5s is enough resolution for playtime tracking and gentle on CPU.
        let watcher = playtime.clone().spawn_watcher(Duration::from_secs(5));

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
            background: Mutex::new(vec![watcher]),
        })
    }

    /// Register a long-lived background task for cancellation at shutdown.
    pub async fn register_background(&self, handle: JoinHandle<()>) {
        self.background.lock().await.push(handle);
    }

    /// Stop every long-lived background loop.
    ///
    /// These loops never return on their own — they sleep and poll forever — so
    /// without this they only stopped because the process died, potentially
    /// part-way through a filesystem sweep or a database write. Aborting them
    /// explicitly at shutdown means the remaining shutdown work (closing open
    /// play sessions) is not racing a sweep that is still mutating rows.
    pub async fn shutdown_background(&self) -> usize {
        let handles: Vec<JoinHandle<()>> = self.background.lock().await.drain(..).collect();
        let count = handles.len();
        for handle in handles {
            handle.abort();
        }
        count
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
