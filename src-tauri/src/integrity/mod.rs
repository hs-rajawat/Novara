//! Library Integrity System — single source of truth for whether an
//! installation is still present.
//!
//! This leaf module deliberately does not depend on `Db`/`EventBus`:
//! `db::games::upsert_game` calls into it directly while scanning, and the
//! background verifier (`integrity::service::IntegrityService`) calls it
//! too. Keeping the pure checks here — rather than alongside the stateful
//! service — avoids `db` having to depend on a "service" module that
//! itself depends on `db`. It DOES depend on `scanner::{steam,epic}`'s
//! pure, filesystem-only `SteamContext`/`EpicContext` (no `Db`, no
//! orchestrator) for source-specific evidence — that keeps the dependency
//! graph a DAG: `scanner::{steam,epic}` (leaves) -> `integrity` -> `db` ->
//! `scanner` (orchestrator), no cycle.
//!
//! States (all disk-derived / auto-managed):
//!   Installed — verified present & launchable.
//!   Missing   — install dir present, the *executable* is gone (repairable
//!               in place via "Locate Executable").
//!   Deleted   — the folder/manifest is confirmed gone while its drive is
//!               online (a real uninstall; reinstall to recover).
//!   Offline   — the install's drive/volume is unmounted, so its presence
//!               cannot be checked (temporary; auto-heals on reconnect).
//!               An offline install is "healthier" than a missing one — its
//!               files may be perfectly intact behind an unplugged drive.
//!
//! A *move* is NOT a state: when a launcher reports an install at a new
//! path, the row is relinked in place (history preserved) and returns to
//! Installed. See `Resolution::relink_to`.
//!
//! Extending states: add a variant + `as_str`/`FromStr` arm. If the new
//! state is disk-derived (like Installed/Missing/Deleted/Offline), also
//! extend `resolve_status`'s logic and `is_auto_managed`. If it's
//! user-asserted (Ignored, Archived, ...), do NOT touch `resolve_status`
//! and do NOT add it to `is_auto_managed`, so automatic checks (upsert
//! self-heal, background verifier) never overwrite a manually-set state.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use tracing::warn;

use crate::scanner::epic::EpicContext;
use crate::scanner::steam::SteamContext;

pub mod service;
pub use service::IntegrityService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Installed,
    Missing,
    Deleted,
    Offline,
}

impl InstallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::Deleted => "deleted",
            Self::Offline => "offline",
        }
    }
}

impl FromStr for InstallStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "installed" => Self::Installed,
            "missing" => Self::Missing,
            "deleted" => Self::Deleted,
            "offline" => Self::Offline,
            other => {
                warn!(status = other, "unrecognized install status, treating as installed");
                Self::Installed
            }
        })
    }
}

/// The outcome of resolving one installation: its current status plus, when
/// a launcher-managed install has *moved*, the new directory the caller
/// should relink the existing row to (preserving all history) instead of
/// leaving a stale row and minting a ghost. `relink_to` is only ever set
/// alongside `Installed`.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub status: InstallStatus,
    pub relink_to: Option<PathBuf>,
}

impl Resolution {
    fn status(status: InstallStatus) -> Self {
        Self { status, relink_to: None }
    }
}

/// Whether the automatic verifier / upsert self-heal is allowed to
/// overwrite this stored value. Every disk-derived state is auto-managed;
/// any future manual state (ignored, archived, ...) must NOT satisfy this,
/// so it survives every automatic pass untouched.
pub fn is_auto_managed(status: &str) -> bool {
    matches!(status, "installed" | "missing" | "deleted" | "offline")
}

/// Whether the volume/drive backing `path` is currently mounted. This is
/// what separates a real uninstall (`Deleted`) from a merely unplugged
/// external drive (`Offline`): the same absent folder means very different
/// things depending on whether its drive is even present.
///
/// Windows (the primary target): the drive/UNC-share root is reconstructed
/// from the path prefix and probed — an unmounted `D:\` or an unreachable
/// `\\server\share` returns `false`. Paths without a recognizable prefix
/// (relative/rootless) can't be judged and are assumed online.
///
/// Unix: best-effort only — real mount-point detection needs platform APIs,
/// so this walks to the nearest existing ancestor and treats a reachable
/// ancestor as online. In practice this means Unix rarely reports Offline;
/// a genuinely gone folder is then classified Deleted/Missing rather than
/// Offline, which is the safe default (never hide a real deletion behind a
/// temporary-looking state).
#[cfg(windows)]
pub fn volume_online(path: &str) -> bool {
    use std::path::Component;
    let mut comps = Path::new(path).components();
    match comps.next() {
        Some(Component::Prefix(prefix)) => {
            // `as_os_str()` yields "C:" for a disk and "\\server\share" for
            // a UNC prefix; appending the separator gives the volume root to
            // probe. An unmounted drive / unreachable share fails to exist.
            let mut root = prefix.as_os_str().to_os_string();
            root.push(std::path::MAIN_SEPARATOR_STR);
            Path::new(&root).exists()
        }
        // Relative or rootless path — can't determine a volume; assume online
        // so we fall through to the ordinary existence checks.
        _ => true,
    }
}

#[cfg(not(windows))]
pub fn volume_online(path: &str) -> bool {
    let mut p = Path::new(path);
    loop {
        if p.exists() {
            return true;
        }
        match p.parent() {
            Some(parent) => p = parent,
            None => return true,
        }
    }
}

/// Pure filesystem check — the generic path used by manual installs and any
/// launcher row without a live source context. No DB/async, so it's callable
/// synchronously from `upsert_game`, the background verifier, and
/// `launch_game`.
///
/// Order matters: an unmounted drive is checked first, because with the
/// volume gone we cannot distinguish "moved/deleted" from "just unplugged"
/// and must not raise a false Deleted/Missing alarm.
pub fn resolve_status(install_dir: &str, executable: Option<&str>) -> InstallStatus {
    if !volume_online(install_dir) {
        return InstallStatus::Offline;
    }
    let dir = Path::new(install_dir);
    match executable {
        Some(exe) => {
            if !dir.is_dir() {
                // Whole install directory gone while its drive is online — a
                // real deletion, not a mislaid executable.
                InstallStatus::Deleted
            } else if dir.join(exe).is_file() {
                // Path::join discards the base when `exe` is itself absolute,
                // so this is correct for both relative (the common case) and
                // absolute stored executable paths — same join semantics
                // `launch_game` relies on.
                InstallStatus::Installed
            } else {
                // Directory present, executable gone — repairable in place.
                InstallStatus::Missing
            }
        }
        // Launcher-URI-only installs (e.g. Steam's steam://run/<id>, no local
        // exe) — best-effort: the install directory being present is the only
        // signal we have without deeper per-source integration.
        None => {
            if dir.is_dir() {
                InstallStatus::Installed
            } else {
                InstallStatus::Deleted
            }
        }
    }
}

/// The shared integration point every source-specific check feeds into:
/// Steam manifest verification, Epic manifest verification, the generic
/// executable/dir check used by manual (and any future source without its
/// own evidence), all resolve through here to the one
/// `game_installations.status` column that the Library UI, Game Details,
/// and launch behavior all read.
///
/// For launcher-managed sources this also performs **move detection**: when
/// the launcher still tracks the app but at a directory different from the
/// stored one, the returned `Resolution::relink_to` carries the new path so
/// the caller can relink the row in place (preserving history) rather than
/// leaving a stale row behind.
///
/// `steam`/`epic` are already-discovered source contexts, reused by the
/// caller across every row of that source in a single verification sweep
/// rather than rediscovering library locations per row. Pass `None` when no
/// context was built for a source (falls back to the generic dir/exe check,
/// best-effort for that row).
pub fn resolve_installation_status(
    source_code: &str,
    source_app_id: Option<&str>,
    install_dir: &str,
    executable: Option<&str>,
    steam: Option<&SteamContext>,
    epic: Option<&EpicContext>,
) -> Resolution {
    match source_code {
        "steam" => {
            if let (Some(app_id), Some(ctx)) = (source_app_id, steam) {
                return resolve_launcher(ctx.locate(app_id), install_dir);
            }
        }
        "epic" => {
            if let (Some(app_id), Some(ctx)) = (source_app_id, epic) {
                return resolve_launcher(ctx.locate(app_id), install_dir);
            }
        }
        _ => {}
    }
    Resolution::status(resolve_status(install_dir, executable))
}

/// Shared launcher-managed resolution: `located` is where the launcher
/// currently reports the app installed (`None` = the launcher no longer
/// tracks it, i.e. uninstalled). `stored_dir` is the directory currently
/// persisted on the row.
fn resolve_launcher(located: Option<PathBuf>, stored_dir: &str) -> Resolution {
    match located {
        Some(path) => {
            // The launcher claims it's installed. Confirm the files are
            // actually reachable; an offline drive must not read as Deleted.
            let path_str = path.to_string_lossy();
            if !volume_online(&path_str) {
                return Resolution::status(InstallStatus::Offline);
            }
            if !path.is_dir() {
                // Manifest lingers but the folder is gone on an online drive.
                return Resolution::status(InstallStatus::Deleted);
            }
            // Installed. If the launcher's location differs from what we
            // stored, the game moved — signal a relink.
            let relink_to = if !paths_equal(&path, stored_dir) {
                Some(path)
            } else {
                None
            };
            Resolution {
                status: InstallStatus::Installed,
                relink_to,
            }
        }
        None => {
            // The launcher no longer tracks this app — a real uninstall,
            // unless the drive it lived on is simply unplugged.
            if volume_online(stored_dir) {
                Resolution::status(InstallStatus::Deleted)
            } else {
                Resolution::status(InstallStatus::Offline)
            }
        }
    }
}

/// Path equality tolerant of trailing separators and case (Windows paths are
/// case-insensitive). Avoids a spurious relink when the launcher's path and
/// the stored path are the same location spelled differently.
fn paths_equal(a: &Path, b: &str) -> bool {
    let norm = |s: &str| {
        let trimmed = s.trim_end_matches(['\\', '/']);
        if cfg!(windows) {
            trimmed.to_ascii_lowercase()
        } else {
            trimmed.to_string()
        }
    };
    norm(&a.to_string_lossy()) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("gv_integrity_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn online_dir_with_exe_is_installed() {
        let dir = tmp("installed");
        std::fs::write(dir.join("game.exe"), b"x").unwrap();
        let d = dir.to_string_lossy();
        assert_eq!(resolve_status(&d, Some("game.exe")), InstallStatus::Installed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn online_dir_missing_exe_is_missing_not_deleted() {
        let dir = tmp("missing_exe");
        let d = dir.to_string_lossy();
        // Dir present, exe absent → repairable Missing, never Deleted.
        assert_eq!(resolve_status(&d, Some("game.exe")), InstallStatus::Missing);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absent_dir_on_online_drive_is_deleted() {
        // A path on an existing volume (temp dir's drive) but whose directory
        // does not exist → Deleted (real removal), not Offline.
        let base = tmp("deleted_base");
        let gone = base.join("does_not_exist");
        let d = gone.to_string_lossy();
        assert_eq!(resolve_status(&d, Some("game.exe")), InstallStatus::Deleted);
        // Launcher-URI-only (no executable) form is likewise Deleted.
        assert_eq!(resolve_status(&d, None), InstallStatus::Deleted);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn unmounted_drive_is_offline() {
        // An almost-certainly-absent drive letter — the whole volume is gone,
        // so we must report Offline, never Missing/Deleted.
        let path = r"Q:\Games\SomeGame";
        assert!(!volume_online(path));
        assert_eq!(resolve_status(path, Some("game.exe")), InstallStatus::Offline);
    }

    #[cfg(windows)]
    #[test]
    fn system_drive_is_online() {
        assert!(volume_online(r"C:\Windows"));
    }

    #[test]
    fn launcher_move_signals_relink() {
        // Launcher reports an existing directory that differs from the stored
        // one → Installed + relink_to(new path).
        let new_dir = tmp("moved_to");
        let res = resolve_launcher(Some(new_dir.clone()), r"C:\old\location");
        assert_eq!(res.status, InstallStatus::Installed);
        assert_eq!(res.relink_to.as_deref(), Some(new_dir.as_path()));
        let _ = std::fs::remove_dir_all(&new_dir);
    }

    #[test]
    fn launcher_same_location_no_relink() {
        let dir = tmp("same_loc");
        let stored = dir.to_string_lossy().to_string();
        let res = resolve_launcher(Some(dir.clone()), &stored);
        assert_eq!(res.status, InstallStatus::Installed);
        assert!(res.relink_to.is_none(), "same location must not relink");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn launcher_untracked_on_online_drive_is_deleted() {
        // Launcher no longer tracks the app; stored dir lives on an online
        // drive (temp) → Deleted.
        let base = tmp("untracked");
        let stored = base.to_string_lossy();
        let res = resolve_launcher(None, &stored);
        assert_eq!(res.status, InstallStatus::Deleted);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn launcher_untracked_on_offline_drive_is_offline() {
        // Launcher can't see it AND the drive is unplugged → Offline, because
        // an unplugged drive can't prove a deletion.
        let res = resolve_launcher(None, r"Q:\Games\SomeGame");
        assert_eq!(res.status, InstallStatus::Offline);
    }

    #[test]
    fn deleted_and_offline_are_auto_managed() {
        for s in ["installed", "missing", "deleted", "offline"] {
            assert!(is_auto_managed(s), "{s} should be auto-managed");
        }
        assert!(!is_auto_managed("ignored"));
        assert!(!is_auto_managed("archived"));
    }

    #[test]
    fn status_roundtrips_through_str() {
        for st in [
            InstallStatus::Installed,
            InstallStatus::Missing,
            InstallStatus::Deleted,
            InstallStatus::Offline,
        ] {
            assert_eq!(st.as_str().parse::<InstallStatus>().unwrap(), st);
        }
    }
}
