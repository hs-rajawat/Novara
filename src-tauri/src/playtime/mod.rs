//! Playtime tracker.
//!
//! Two modes:
//!   1. Explicit:  `launch_game`, or the frontend's start/stop commands,
//!      open and close a session directly.
//!   2. Passive:   a background watcher samples running processes via
//!      `sysinfo` and matches them to installations, starting and stopping
//!      sessions automatically.
//!
//! Processes are matched by **executable path**, not by file name:
//!   * an exact match against an installation's recorded executable, when a
//!     source provides one (Epic's `LaunchExecutable`, a manual import's
//!     user-chosen binary);
//!   * otherwise any process running from **inside the installation
//!     directory**, which is the only thing that works for Steam — Steam
//!     records no executable, because it launches by URI and resolves the
//!     binary itself;
//!   * and only when the OS will not report a path at all, the file name —
//!     restricted to names that identify exactly one installation, so two
//!     games shipping `game.exe` are never confused.
//!
//! An explicitly launched session is not closed until a matching process has
//! actually been seen, or [`LAUNCH_GRACE`] elapses. Launchers take time to
//! start a game, and treating "no process yet" as "already exited" is what
//! previously reduced every Steam session to a few seconds.
//!
//! Idle time is reported by the frontend through `report_idle` and stored
//! separately from duration, so analytics can distinguish active from total
//! time. Idle accounting is conservative: a gap is only counted as idle when
//! the frontend says so.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::db::Db;
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus};

#[cfg(test)]
mod tests;

struct ActiveSession {
    session_id: i64,
    started_at: Instant,
    idle_seconds: i64,
    /// Whether a process belonging to this game has actually been observed.
    ///
    /// This is what makes an optimistic launch safe. `launch_game` opens a
    /// session the moment it hands off to Steam or Epic, but the game's own
    /// process can take many seconds to appear — the launcher has to start,
    /// update, and spawn it. Until a process has been seen once, the absence
    /// of one means "not started yet", not "already exited".
    saw_process: bool,
}

/// How long to wait for a launched game's process to appear before concluding
/// the launch failed.
///
/// Generous on purpose: a cold Steam start on a slow disk, or a shader
/// pre-cache, can easily exceed a minute. The cost of waiting too long is a
/// session that lingers a little; the cost of waiting too briefly is losing the
/// session entirely, which is the bug being fixed.
const LAUNCH_GRACE: Duration = Duration::from_secs(180);

/// One installation the watcher should look for.
struct WatchTarget {
    game_id: String,
    /// Lowercased install directory, used as a path prefix.
    install_dir: String,
    /// Lowercased full executable path, when the source provides one.
    executable: Option<String>,
    /// Lowercased executable file name, when the source provides one.
    exe_name: Option<String>,
}

/// Normalize a path for comparison: lowercase, forward slashes.
///
/// Windows paths are case-insensitive and the same location can arrive with
/// either separator (Steam's manifests use forward slashes, the filesystem
/// reports backslashes), so raw string comparison finds nothing.
fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
}

fn file_name_of(p: &str) -> Option<String> {
    p.rsplit(['/', '\\']).next().map(|s| s.to_ascii_lowercase())
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
                // A session the watcher itself opened was, by definition,
                // opened because a process was seen.
                saw_process: process_name.is_some(),
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

    /// Close every open session, for use on application shutdown.
    ///
    /// Without this, quitting NOVARA while a game runs leaves the session row
    /// open forever: `stop` is the only thing that closes one, and the
    /// in-memory map does not survive the process. The elapsed time up to
    /// shutdown is real and is credited.
    pub async fn stop_all(&self) -> AppResult<usize> {
        let game_ids: Vec<String> = self.active.lock().await.keys().cloned().collect();
        let mut closed = 0;
        for game_id in game_ids {
            match self.stop(&game_id).await {
                Ok(Some(_)) => closed += 1,
                Ok(None) => {}
                // Shutdown must not be blocked by one failed write.
                Err(e) => warn!(error = %e, game_id, "failed to close session on shutdown"),
            }
        }
        Ok(closed)
    }

    /// Whether a session is currently open for `game_id`.
    pub async fn is_active(&self, game_id: &str) -> bool {
        self.active.lock().await.contains_key(game_id)
    }

    /// Spawn the passive process watcher. Polls every `interval`, matching
    /// running processes to installations by executable path, and starting or
    /// stopping sessions accordingly. Returns a `JoinHandle` so the caller can
    /// cancel on shutdown.
    ///
    /// Matching is by **full executable path**, in three tiers:
    ///   1. exact match against an installation's recorded executable — the
    ///      strongest signal, used when launcher metadata supplies one (Epic
    ///      records `LaunchExecutable`; a manual import records the user's
    ///      choice);
    ///   2. otherwise, any process whose executable lives **under the
    ///      installation directory**, which is what makes Steam titles
    ///      trackable at all — Steam records no executable, so there is
    ///      nothing to match by name;
    ///   3. only if the OS refuses to report a process's path, a file-name
    ///      comparison — and then only when that name is unique across all
    ///      watched installations, so two different games shipping `game.exe`
    ///      are never confused for one another.
    pub fn spawn_watcher(self: Arc<Self>, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            use sysinfo::System;
            let mut sys = System::new();
            loop {
                tokio::time::sleep(interval).await;
                if let Err(e) = self.tick(&mut sys).await {
                    warn!(error = %e, "playtime watcher tick failed");
                }
            }
        })
    }

    /// One watcher pass. Separated from the spawn loop so it is callable from
    /// a test without a running timer.
    async fn tick(&self, sys: &mut sysinfo::System) -> AppResult<()> {
        let targets = self.build_targets().await?;

        sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
        let processes: Vec<(Option<String>, Option<String>)> = sys
            .processes()
            .values()
            .map(|p| {
                (
                    p.exe().map(|e| normalize_path(&e.to_string_lossy())),
                    Some(p.name().to_string_lossy().to_ascii_lowercase()),
                )
            })
            .collect();

        // File names that identify exactly one installation. Anything
        // ambiguous is excluded rather than guessed at.
        let unique_names = unique_exe_names(&targets);

        let mut seen: HashMap<String, String> = HashMap::new(); // game_id -> proc label
        for (exe_path, proc_name) in &processes {
            if let Some((game_id, label)) =
                match_process(&targets, exe_path.as_deref(), proc_name.as_deref(), &unique_names)
            {
                seen.entry(game_id).or_insert(label);
            }
        }

        self.reconcile(seen, LAUNCH_GRACE).await
    }

    /// Bring open sessions into line with the set of games observed running.
    ///
    /// Split out from [`Self::tick`] so the session lifecycle — start, the
    /// launch grace window, genuine exit, and a failed launch — can be tested
    /// against synthetic observations instead of whatever happens to be running
    /// on the machine. `grace` is a parameter for the same reason.
    async fn reconcile(
        &self,
        seen: HashMap<String, String>,
        grace: Duration,
    ) -> AppResult<()> {
        // Start sessions for games that just began running.
        for (game_id, label) in &seen {
            if !self.is_active(game_id).await {
                if let Err(e) = self.start(game_id, Some(label)).await {
                    warn!(error = %e, "auto-start session failed");
                }
            } else if let Some(s) = self.active.lock().await.get_mut(game_id) {
                // An explicitly launched session now has its process.
                s.saw_process = true;
            }
        }

        // Close sessions whose process is gone — and only those.
        let candidates: Vec<(String, bool, Duration)> = {
            let active = self.active.lock().await;
            active
                .iter()
                .map(|(g, s)| (g.clone(), s.saw_process, s.started_at.elapsed()))
                .collect()
        };
        for (game_id, saw_process, elapsed) in candidates {
            if seen.contains_key(&game_id) {
                continue;
            }
            if saw_process {
                // Ran, and has now exited: a genuine end of session.
                let _ = self.stop(&game_id).await;
            } else if elapsed >= grace {
                // Never started. Discard rather than record a phantom
                // session as long as the grace window.
                let removed = self.active.lock().await.remove(&game_id);
                if let Some(sess) = removed {
                    warn!(
                        game_id,
                        "no process appeared after launch; discarding the session"
                    );
                    if let Err(e) = self.db.discard_session(sess.session_id).await {
                        warn!(error = %e, "failed to discard session");
                    }
                    self.bus.emit(AppEvent::SessionEnded {
                        game_id: game_id.clone(),
                        session_id: sess.session_id,
                        duration_seconds: 0,
                    });
                }
            }
            // else: still inside the grace window — keep waiting.
        }
        Ok(())
    }

    async fn build_targets(&self) -> AppResult<Vec<WatchTarget>> {
        Ok(self
            .db
            .list_watch_targets()
            .await?
            .into_iter()
            .map(|(game_id, install_dir, executable)| WatchTarget {
                game_id,
                install_dir: normalize_path(&install_dir),
                exe_name: executable.as_deref().and_then(file_name_of),
                executable: executable.as_deref().map(normalize_path),
            })
            .collect())
    }
}

/// Executable file names that map to exactly one watched game.
fn unique_exe_names(targets: &[WatchTarget]) -> HashMap<String, String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for t in targets {
        if let Some(name) = &t.exe_name {
            *counts.entry(name.as_str()).or_default() += 1;
        }
    }
    targets
        .iter()
        .filter_map(|t| {
            let name = t.exe_name.as_ref()?;
            (counts.get(name.as_str()) == Some(&1))
                .then(|| (name.clone(), t.game_id.clone()))
        })
        .collect()
}

/// Attribute one running process to a watched game, if any.
///
/// Returns the game id and a label to record as the session's `process_name`.
/// Pure so the tiering can be tested without a process list.
fn match_process(
    targets: &[WatchTarget],
    exe_path: Option<&str>,
    proc_name: Option<&str>,
    unique_names: &HashMap<String, String>,
) -> Option<(String, String)> {
    if let Some(path) = exe_path {
        // Tier 1: the exact executable a source told us about.
        if let Some(t) = targets
            .iter()
            .find(|t| t.executable.as_deref() == Some(path))
        {
            let label = file_name_of(path).unwrap_or_else(|| path.to_string());
            return Some((t.game_id.clone(), label));
        }
        // Tier 2: anything running from inside the install directory. This is
        // what covers Steam, which records no executable of its own. The
        // longest matching directory wins so nested installs cannot be
        // attributed to a parent folder.
        if let Some(t) = targets
            .iter()
            .filter(|t| !t.install_dir.is_empty() && is_under(path, &t.install_dir))
            .max_by_key(|t| t.install_dir.len())
        {
            let label = file_name_of(path).unwrap_or_else(|| path.to_string());
            return Some((t.game_id.clone(), label));
        }
        return None;
    }

    // Tier 3: no path available from the OS. Fall back to the file name, but
    // only when it is unambiguous — this is the case that used to silently
    // credit the wrong game when two titles shipped the same executable name.
    let name = proc_name?;
    unique_names
        .get(name)
        .map(|game_id| (game_id.clone(), name.to_string()))
}

/// Whether `path` sits inside `dir`, comparing whole path segments.
///
/// A plain `starts_with` would treat `D:/games/portal2` as being inside
/// `D:/games/portal`, attributing one game's process to another.
fn is_under(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}
