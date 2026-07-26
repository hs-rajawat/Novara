//! Launcher pre-warm — work around external launchers that drop a launch
//! URI when they're cold-started by the URI itself.
//!
//! Epic's `com.epicgames.launcher://…?action=launch` activation is delivered
//! to an `EpicGamesLauncher.exe` that is still bootstrapping; that instance
//! force-restarts itself and the forwarded URI is rejected by its own
//! `AppLaunchUriHandler` as "already being processed … not restartable", so
//! the game never spawns (it only opens the store UI). When the launcher is
//! already running the same URI launches immediately. See the launch
//! investigation notes.
//!
//! The fix is deliberately narrow: for Epic only, if its launcher process is
//! not running, start it and wait until it is present and past its initial
//! bootstrap before the caller sends the URI exactly once. Steam and every
//! other source are untouched — `prewarm_for` returns immediately for them,
//! preserving their launch path byte-for-byte.

use std::time::Duration;

#[cfg(target_os = "windows")]
use tracing::{debug, info, warn};

/// Ensure the background launcher for `source_code` is running and ready
/// before the caller sends a launch URI. A no-op for every source that
/// doesn't need it (Steam, manual, …) — those return `Ok(())` instantly.
///
/// Best-effort: any failure to locate or start the launcher is logged and
/// returns `Ok(())` so the caller still sends the URI (which is exactly the
/// prior behavior). Never returns an error that would block a launch.
pub async fn prewarm_for(source_code: &str) -> std::io::Result<()> {
    if source_code == "epic" {
        prewarm_epic().await;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
const EPIC_LAUNCHER_PROCESS: &str = "epicgameslauncher.exe";

/// How long to wait for a cold-started Epic launcher to become ready before
/// giving up and sending the URI anyway. Bounded so a broken/never-ready
/// launcher can't hang Play indefinitely.
#[cfg(target_os = "windows")]
const EPIC_READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll cadence while waiting for readiness — short enough to feel instant
/// once ready, sparse enough to avoid busy-spinning.
#[cfg(target_os = "windows")]
const EPIC_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Grace period after the process first appears, giving the launcher's URI
/// handler time to finish bootstrapping before we hand it the launch URI.
/// This is what avoids the "already being processed / not restartable" race.
#[cfg(target_os = "windows")]
const EPIC_BOOTSTRAP_GRACE: Duration = Duration::from_secs(3);

#[cfg(target_os = "windows")]
async fn prewarm_epic() {
    if epic_launcher_running() {
        debug!("epic launcher already running; no pre-warm needed");
        return;
    }

    let Some(exe) = find_epic_launcher_exe() else {
        warn!("epic launcher exe not found; sending URI without pre-warm");
        return;
    };

    info!(exe = %exe.display(), "epic launcher not running; starting it before launch");
    if let Err(e) = std::process::Command::new(&exe).spawn() {
        warn!(error = %e, "failed to start epic launcher; sending URI without pre-warm");
        return;
    }

    // Wait until the process is present, then give it a short bootstrap grace
    // period. Bounded by EPIC_READY_TIMEOUT overall.
    let start = std::time::Instant::now();
    let mut appeared = false;
    while start.elapsed() < EPIC_READY_TIMEOUT {
        if epic_launcher_running() {
            appeared = true;
            break;
        }
        tokio::time::sleep(EPIC_POLL_INTERVAL).await;
    }

    if appeared {
        info!("epic launcher is up; waiting out bootstrap grace before URI");
        tokio::time::sleep(EPIC_BOOTSTRAP_GRACE).await;
    } else {
        warn!("epic launcher did not appear within timeout; sending URI anyway");
    }
}

#[cfg(not(target_os = "windows"))]
async fn prewarm_epic() {
    // Epic pre-warm is only implemented for Windows; on other platforms the
    // caller falls through to sending the URI unchanged.
}

/// Whether `EpicGamesLauncher.exe` is currently running. One process-table
/// refresh — matches the enumeration pattern used by the playtime watcher.
#[cfg(target_os = "windows")]
fn epic_launcher_running() -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
    sys.processes()
        .values()
        .any(|p| p.name().to_string_lossy().to_ascii_lowercase() == EPIC_LAUNCHER_PROCESS)
}

/// Locate `EpicGamesLauncher.exe`. Honors a test/CI override, then the fixed
/// install location under Program Files.
#[cfg(target_os = "windows")]
fn find_epic_launcher_exe() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    if let Ok(p) = std::env::var("NOVARA_EPIC_LAUNCHER_EXE") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }

    // Epic installs the launcher shortcut target here on every supported
    // Windows install. Probe both Program Files roots.
    for root in [
        std::env::var("ProgramFiles").ok(),
        std::env::var("ProgramFiles(x86)").ok(),
        Some(r"C:\Program Files".to_string()),
    ]
    .into_iter()
    .flatten()
    {
        let p = PathBuf::from(root)
            .join("Epic Games")
            .join("Launcher")
            .join("Portal")
            .join("Binaries")
            .join("Win64")
            .join("EpicGamesLauncher.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
