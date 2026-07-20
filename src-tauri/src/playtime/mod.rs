//! Playtime tracker.
//!
//! Two modes:
//!   1. Explicit:  the frontend calls start_session / stop_session
//!      (used when the user clicks "Play" from inside NOVARA).
//!   2. Passive:   a background watcher samples running processes via
//!      `sysinfo` and matches them to known executables, starting and
//!      stopping sessions automatically. Idle detection is best-effort
//!      and conservative (sub-threshold input gaps are still counted
//!      as active to avoid undercounting).
//!
//! Idle detection on Windows can hook GetLastInputInfo via win32. The
//! MVP keeps it simple — exposes a `report_idle_seconds` mutation from
//! the frontend so the UI's idle-detect can feed in observations
//! (the page emits user-activity events through Tauri).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::db::Db;
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus};

struct ActiveSession {
    session_id: i64,
    started_at: Instant,
    idle_seconds: i64,
}

#[derive(Clone)]
pub struct PlaytimeTracker {
    db: Db,
    bus: EventBus,
    active: Arc<Mutex<HashMap<String, ActiveSession>>>, // keyed by game_id
}

impl PlaytimeTracker {
    pub fn new(db: Db, bus: EventBus) -> Self {
        Self {
            db,
            bus,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self, game_id: &str, process_name: Option<&str>) -> AppResult<i64> {
        let session_id = self.db.start_session(game_id, process_name).await?;
        self.active.lock().await.insert(
            game_id.to_string(),
            ActiveSession {
                session_id,
                started_at: Instant::now(),
                idle_seconds: 0,
            },
        );
        self.bus.emit(AppEvent::SessionStarted {
            game_id: game_id.into(),
            session_id,
        });
        Ok(session_id)
    }

    pub async fn report_idle(&self, game_id: &str, idle_seconds_delta: i64) {
        if let Some(s) = self.active.lock().await.get_mut(game_id) {
            s.idle_seconds = s.idle_seconds.saturating_add(idle_seconds_delta);
        }
    }

    pub async fn stop(&self, game_id: &str) -> AppResult<Option<i64>> {
        let Some(sess) = self.active.lock().await.remove(game_id) else {
            return Ok(None);
        };
        let duration = sess.started_at.elapsed().as_secs() as i64;
        let (gid, active_secs) = self
            .db
            .stop_session(sess.session_id, duration, sess.idle_seconds)
            .await?;
        self.bus.emit(AppEvent::SessionEnded {
            game_id: gid,
            session_id: sess.session_id,
            duration_seconds: active_secs,
        });
        Ok(Some(sess.session_id))
    }

    /// Spawn the passive process watcher. Polls every `interval` seconds,
    /// compares running processes against known executables, and emits
    /// sessions automatically. Returns a JoinHandle so the caller can
    /// cancel on shutdown.
    pub fn spawn_watcher(self: Arc<Self>, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            use sysinfo::System;
            let mut sys = System::new();
            let mut tracked: HashMap<String, String> = HashMap::new(); // proc_name -> game_id
            loop {
                tokio::time::sleep(interval).await;

                // Refresh the executable→game_id index occasionally.
                if let Ok(rows) = sqlx::query_as::<_, (String, Option<String>)>(
                    r#"SELECT gi.game_id, gi.executable
                       FROM game_installations gi
                       WHERE gi.executable IS NOT NULL"#,
                )
                .fetch_all(&self.db.pool)
                .await
                {
                    tracked.clear();
                    for (game_id, exe) in rows.into_iter() {
                        if let Some(exe) = exe {
                            if let Some(name) = exe.rsplit(['/', '\\']).next() {
                                tracked.insert(name.to_ascii_lowercase(), game_id);
                            }
                        }
                    }
                }

                sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
                let running: std::collections::HashSet<String> = sys
                    .processes()
                    .values()
                    .map(|p| p.name().to_string_lossy().to_ascii_lowercase())
                    .collect();

                // Start sessions for newly-running tracked processes.
                for (proc_name, game_id) in tracked.iter() {
                    if running.contains(proc_name)
                        && !self.active.lock().await.contains_key(game_id)
                    {
                        if let Err(e) = self.start(game_id, Some(proc_name)).await {
                            warn!(error = %e, "auto-start session failed");
                        }
                    }
                }

                // Stop sessions for tracked processes that have exited.
                let active_keys: Vec<String> =
                    self.active.lock().await.keys().cloned().collect();
                for game_id in active_keys {
                    let still_running = tracked.iter().any(|(proc, gid)| {
                        gid == &game_id && running.contains(proc)
                    });
                    if !still_running {
                        let _ = self.stop(&game_id).await;
                    }
                }
            }
        })
    }
}
