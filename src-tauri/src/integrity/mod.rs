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
//! Extending states: add a variant + `as_str`/`FromStr` arm. If the new
//! state is disk-derived (like Installed/Missing), also extend
//! `resolve_status`'s logic. If it's user-asserted (Ignored, Archived,
//! ExternalDriveOffline, ...), do NOT touch `resolve_status` — instead add
//! it to `is_auto_managed`'s exclusion (i.e. don't add it there), so
//! automatic checks (upsert self-heal, background verifier) never
//! overwrite a manually-set state.

use std::path::Path;
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
}

impl InstallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
        }
    }
}

impl FromStr for InstallStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "installed" => Self::Installed,
            "missing" => Self::Missing,
            other => {
                warn!(status = other, "unrecognized install status, treating as installed");
                Self::Installed
            }
        })
    }
}

/// Whether the automatic verifier / upsert self-heal is allowed to
/// overwrite this stored value. Any future manual state (ignored,
/// archived, ...) must NOT satisfy this, so it survives every automatic
/// pass untouched.
pub fn is_auto_managed(status: &str) -> bool {
    matches!(status, "installed" | "missing")
}

/// Pure filesystem check — the ONE place that decides "does this
/// installation still exist." No DB/async, so it's callable synchronously
/// from `upsert_game`, the background verifier, and `launch_game`.
pub fn resolve_status(install_dir: &str, executable: Option<&str>) -> InstallStatus {
    match executable {
        Some(exe) => {
            // Path::join discards the base when `exe` is itself absolute,
            // so this is correct for both relative (the common case) and
            // absolute stored executable paths — same join semantics
            // `launch_game` already relies on.
            if Path::new(install_dir).join(exe).is_file() {
                InstallStatus::Installed
            } else {
                InstallStatus::Missing
            }
        }
        // Launcher-URI-only installs (e.g. Steam's steam://run/<id>, no
        // local exe) — best-effort: verify the install directory is still
        // there. We don't have a signal for "still owned by the launcher"
        // without deeper per-source integration.
        None => {
            if Path::new(install_dir).is_dir() {
                InstallStatus::Installed
            } else {
                InstallStatus::Missing
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
) -> InstallStatus {
    if source_code == "steam" {
        if let (Some(app_id), Some(ctx)) = (source_app_id, steam) {
            return if ctx.is_installed(app_id) {
                InstallStatus::Installed
            } else {
                InstallStatus::Missing
            };
        }
    }
    if source_code == "epic" {
        if let (Some(app_id), Some(ctx)) = (source_app_id, epic) {
            return if ctx.is_installed(app_id) {
                InstallStatus::Installed
            } else {
                InstallStatus::Missing
            };
        }
    }
    resolve_status(install_dir, executable)
}
