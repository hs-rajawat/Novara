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
use crate::saves::vault::SaveManager;
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
    /// Resolves Steam app-ids from titles, so Epic and manual games can use the
    /// Steam-backed providers.
    pub titles: Arc<crate::metadata::title_resolver::TitleResolver>,
    pub app_data_dir: PathBuf,
    /// Handles for the long-lived background loops (playtime watcher, periodic
    /// integrity sweep) so shutdown can stop them deliberately instead of
    /// relying on process exit to tear them down mid-operation.
    background: Mutex<Vec<JoinHandle<()>>>,
}

impl AppState {
    pub async fn initialize(app_data_dir: PathBuf) -> AppResult<Self> {
        info!(dir = %app_data_dir.display(), "initializing app state");
        // Filename deliberately left as `gamevault.db` through this rebrand —
        // see HANDOFF §7.2. Renaming it is a separate, isolated change.
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
        let throttle = Arc::new(crate::resolve::throttle::Throttle::default());
        let metadata = Arc::new(MetadataService::new(
            db.clone(),
            bus.clone(),
            http_client.clone(),
            throttle.clone(),
        ));
        // Shares the same client and throttle as the fills: a title search is
        // outbound traffic like any other and must count against the same budget.
        let titles = Arc::new(crate::metadata::title_resolver::TitleResolver::new(
            db.clone(),
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

        // Load the embedded save-location knowledge base. Idempotent: a checksum
        // comparison short-circuits this to one query on every launch after the
        // first, and re-running it cannot duplicate entries.
        //
        // A failure here is a defect in the shipped corpus, not anything a user
        // did, so it is logged at error level and startup continues. NOVARA is a
        // game launcher first — refusing to start because a save-detection hint
        // file is malformed would trade a degraded feature for an unusable app.
        // `builtin::tests::the_embedded_kb_parses_and_every_entry_validates` is
        // what actually prevents this from shipping, and it fails the build.
        match crate::saves::kb::builtin::load(&db).await {
            Ok(Ok(crate::saves::kb::builtin::LoadOutcome::Applied { version, entries })) => {
                info!(version = %version, entries, "applied built-in save knowledge base")
            }
            Ok(Ok(crate::saves::kb::builtin::LoadOutcome::AlreadyCurrent { version })) => {
                info!(version = %version, "built-in save knowledge base already current")
            }
            Ok(Err(e)) => tracing::error!(
                error = %e,
                "built-in save knowledge base is invalid — save detection will be \
                 degraded. This is a packaging defect; please report it."
            ),
            Err(e) => warn!(error = %e, "could not load the built-in save knowledge base"),
        }

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
            titles,
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
    /// provider call must pass.
    ///
    /// Delegates to `Db::allow_metadata_network`, where the rule and its
    /// deny-by-default behaviour are defined and tested.
    pub async fn allow_metadata_network(&self) -> AppResult<bool> {
        self.db.allow_metadata_network().await
    }

    /// Forward AppEvents to the Tauri event bus so the frontend can listen.
    /// Call once after the App is set up.
    pub fn start_event_forwarder(self: &Arc<Self>, handle: AppHandle) {
        let mut rx = self.bus.subscribe();
        tauri::async_runtime::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                let _ = handle.emit("novara://event", &ev);
            }
        });
    }
}
