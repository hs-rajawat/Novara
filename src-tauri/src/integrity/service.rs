//! Background verification task. See `crate::integrity` (parent module)
//! for the pure status-resolution logic this wraps.

use std::sync::Arc;
use std::time::Duration;

use tauri::async_runtime::JoinHandle;
use tracing::{info, warn};

use crate::db::Db;
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus, NoticeLevel};
use crate::scanner::epic::EpicContext;
use crate::scanner::steam::SteamContext;

use super::{is_auto_managed, resolve_installation_status, InstallStatus};

#[derive(Clone)]
pub struct IntegrityService {
    db: Db,
    bus: EventBus,
}

impl IntegrityService {
    pub fn new(db: Db, bus: EventBus) -> Self {
        Self { db, bus }
    }

    /// One verification pass over every installation currently in the DB.
    /// Only writes a row when its status actually changed, and only
    /// touches rows currently in an auto-managed state — a future manual
    /// state (ignored, archived, ...) is left alone regardless of what the
    /// disk check resolves to.
    ///
    /// Steam library discovery happens exactly once here, up front, and is
    /// reused for every Steam-sourced row — not rediscovered per row — so
    /// this stays cheap at 500-1000+ game libraries.
    pub async fn verify_all(&self) -> AppResult<VerifyReport> {
        let rows = self.db.list_installation_paths().await?;
        let steam_ctx = SteamContext::discover();
        let epic_ctx = EpicContext::discover();
        let mut checked = 0u32;
        let mut changed = 0u32;
        let mut newly_missing = 0u32;
        let mut newly_deleted = 0u32;
        let mut relinked = 0u32;

        for row in rows {
            checked += 1;
            if !is_auto_managed(&row.status) {
                continue;
            }
            let resolution = resolve_installation_status(
                &row.source_code,
                row.source_app_id.as_deref(),
                &row.install_dir,
                row.executable.as_deref(),
                Some(&steam_ctx),
                Some(&epic_ctx),
            );

            // A launcher reported the app installed at a new directory — the
            // game moved. Relink the existing row in place (all history
            // preserved) rather than flipping it Missing/Deleted; this both
            // fixes the row and removes any ghost already at the destination.
            if let Some(new_dir) = resolution.relink_to.as_deref() {
                let new_dir = new_dir.to_string_lossy();
                self.db.relink_installation(&row.id, &new_dir).await?;
                relinked += 1;
                changed += 1;
                self.bus.emit(AppEvent::GameUpdated { game_id: row.game_id.clone() });
                continue;
            }

            let resolved = resolution.status;
            if resolved.as_str() != row.status {
                self.db.set_installation_status(&row.id, resolved.as_str()).await?;
                changed += 1;
                match resolved {
                    InstallStatus::Missing => newly_missing += 1,
                    InstallStatus::Deleted => newly_deleted += 1,
                    _ => {}
                }
                self.bus.emit(AppEvent::GameUpdated { game_id: row.game_id.clone() });
            }
        }

        // Missing (repairable in place) and Deleted (uninstalled) both mean a
        // game the user may expect to play is unavailable, so a single
        // combined warning is enough — Offline transitions are deliberately
        // silent (a temporary unplugged drive is not a problem to alarm on).
        let newly_unavailable = newly_missing + newly_deleted;
        if newly_unavailable > 0 {
            self.bus.emit(AppEvent::Notice {
                level: NoticeLevel::Warning,
                message: format!(
                    "{newly_unavailable} game{} {} no longer available — {} may have been moved, uninstalled, or deleted",
                    if newly_unavailable == 1 { "" } else { "s" },
                    if newly_unavailable == 1 { "is" } else { "are" },
                    if newly_unavailable == 1 { "it" } else { "they" }
                ),
            });
        }

        info!(
            checked,
            changed,
            newly_missing,
            newly_deleted,
            relinked,
            "library integrity check complete"
        );
        Ok(VerifyReport { checked, changed })
    }

    /// Spawn the one-shot startup verification pass. Must be called AFTER
    /// the event forwarder has subscribed to the bus (see lib.rs) —
    /// emitting before anything subscribes silently drops the events.
    ///
    /// Uses `tauri::async_runtime::spawn` (not raw `tokio::spawn`) so it can
    /// be launched from Tauri's synchronous `setup` closure, where no Tokio
    /// runtime is entered — same pattern as `start_event_forwarder`.
    pub fn spawn_startup_check(self: Arc<Self>) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = self.verify_all().await {
                warn!(error = %e, "startup integrity check failed");
            }
        })
    }

    /// Recurring re-verification while the app stays open, so a Steam or
    /// manual uninstall happening mid-session is caught without the user
    /// having to restart NOVARA or click Play first. `verify_all` is
    /// cheap (one Steam discovery plus small stat/file reads per
    /// installation), so this cadence is not "expensive polling."
    ///
    /// Spawned via `tauri::async_runtime::spawn` for the same reason as
    /// `spawn_startup_check` — the `setup` closure is synchronous.
    pub fn spawn_periodic_check(self: Arc<Self>, interval: Duration) -> JoinHandle<()> {
        tauri::async_runtime::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // fires immediately — spawn_startup_check already covers t=0
            loop {
                ticker.tick().await;
                if let Err(e) = self.verify_all().await {
                    warn!(error = %e, "periodic integrity check failed");
                }
            }
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VerifyReport {
    pub checked: u32,
    pub changed: u32,
}
