//! Epic Games Launcher scanner — reads Epic's local install manifests to
//! find installed games. Purely *local discovery* of legally-installed Epic
//! titles; no network calls, no API key, no DRM circumvention.
//!
//! Epic writes one JSON manifest per installed game to
//!   <ProgramData>\Epic\EpicGamesLauncher\Data\Manifests\*.item
//! Each `.item` is the launcher's own record of an install — exactly what
//! Epic reads back — so it is the authoritative local source.

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::error::AppResult;

use super::{DetectedGame, Scanner};

/// Epic install manifest (`.item`). `#[serde(default)]` at the struct level
/// makes every field optional: a missing or newly-added Epic field never
/// fails the parse, it just yields a default. Only the fields NOVARA
/// needs are modeled; everything else in the JSON is ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct EpicManifest {
    display_name: String,
    install_location: String,
    launch_executable: String,
    app_name: String,
    install_size: i64,
    #[serde(rename = "bIsIncompleteInstall")]
    b_is_incomplete_install: bool,
}

/// The single "is this Epic game installed" predicate, shared by the
/// scan-time hint and `EpicContext::is_installed` so there is exactly one
/// definition of Epic install-completeness. A manifest is only trusted as
/// installed when it points at a concrete location + executable and Epic
/// does not itself flag the install as incomplete (mid-download / paused).
fn epic_manifest_installed(m: &EpicManifest) -> bool {
    !m.b_is_incomplete_install && !m.install_location.is_empty() && !m.launch_executable.is_empty()
}

#[derive(Default)]
pub struct EpicScanner;

#[async_trait]
impl Scanner for EpicScanner {
    fn code(&self) -> &'static str {
        "epic"
    }

    async fn scan(&self, _roots: &[PathBuf]) -> AppResult<Vec<DetectedGame>> {
        let Some(dir) = find_epic_manifests_dir() else {
            debug!("epic not installed");
            return Ok(vec![]);
        };
        Ok(scan_manifests_dir(&dir))
    }
}

/// Read every `.item` manifest in `dir` and map to `DetectedGame`. Split out
/// of `scan` so it can be exercised directly against a fixture directory
/// without touching the process-wide env override. A missing/unreadable
/// directory yields an empty list; one malformed `.item` is skipped, never
/// aborting the whole scan.
fn scan_manifests_dir(dir: &std::path::Path) -> Vec<DetectedGame> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return vec![];
    };

    let mut games = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("item") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "epic manifest unreadable, skipping");
                continue;
            }
        };
        // One malformed `.item` must never abort the whole Epic scan.
        let manifest: EpicManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                warn!(path = %path.display(), error = %e, "epic manifest parse failed, skipping");
                continue;
            }
        };
        if manifest.install_location.is_empty() || manifest.app_name.is_empty() {
            continue;
        }
        games.push(to_detected(manifest));
    }
    games
}

/// Map an Epic manifest to a `DetectedGame`. The `LaunchExecutable` is
/// persisted (integrity/diagnostics/future use) but Epic games launch via
/// their `com.epicgames.launcher://` URI keyed on `AppName`, not by spawning
/// this binary — see `crate::sources::launch_uri`.
fn to_detected(m: EpicManifest) -> DetectedGame {
    let install_state_hint = Some(epic_manifest_installed(&m));
    let executable = if m.launch_executable.is_empty() {
        None
    } else {
        Some(PathBuf::from(&m.launch_executable))
    };
    let size = if m.install_size > 0 {
        Some(m.install_size)
    } else {
        None
    };
    DetectedGame {
        source_code: "epic".into(),
        title: m.display_name,
        install_dir: PathBuf::from(m.install_location),
        executable,
        source_app_id: Some(m.app_name),
        install_size_bytes: size,
        install_state_hint,
    }
}

#[cfg(target_os = "windows")]
fn find_epic_manifests_dir() -> Option<PathBuf> {
    // Override via $GAMEVAULT_EPIC_MANIFESTS_DIR (used by tests). Otherwise
    // the launcher's fixed ProgramData location.
    if let Ok(dir) = std::env::var("GAMEVAULT_EPIC_MANIFESTS_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    let program_data =
        std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_string());
    let p = PathBuf::from(program_data)
        .join("Epic")
        .join("EpicGamesLauncher")
        .join("Data")
        .join("Manifests");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
fn find_epic_manifests_dir() -> Option<PathBuf> {
    // Epic Games Launcher is Windows/macOS; a native Linux client doesn't
    // exist. Keep the env override so cross-platform tests can exercise the
    // scanner, but otherwise report "not installed".
    if let Ok(dir) = std::env::var("GAMEVAULT_EPIC_MANIFESTS_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// A one-time snapshot of Epic's installed games, reused across many
/// `is_installed` checks in a single verification sweep — the Epic analogue
/// of `steam::SteamContext`. Discovering means reading the Manifests
/// directory once (not once per row), keeping the Library Integrity System
/// cheap. Safe to build even when Epic isn't installed: the map is simply
/// empty and every `is_installed` check then reports not installed.
pub struct EpicContext {
    installed: HashMap<String, bool>,
}

impl EpicContext {
    pub fn discover() -> Self {
        match find_epic_manifests_dir() {
            Some(dir) => Self::from_dir(&dir),
            None => Self {
                installed: HashMap::new(),
            },
        }
    }

    /// Build a context from a specific manifests directory. Split out of
    /// `discover` so tests can exercise it against a fixture directory
    /// without touching the process-wide env override.
    fn from_dir(dir: &std::path::Path) -> Self {
        let mut installed = HashMap::new();
        if let Ok(read) = std::fs::read_dir(dir) {
            for entry in read.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("item") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(manifest) = serde_json::from_str::<EpicManifest>(&text) else {
                    continue;
                };
                if manifest.app_name.is_empty() {
                    continue;
                }
                installed.insert(
                    manifest.app_name.clone(),
                    epic_manifest_installed(&manifest),
                );
            }
        }
        Self { installed }
    }

    /// Whether Epic currently reports `app_id` (the manifest `AppName`) as a
    /// complete install. A manifest absent from the directory is the
    /// unambiguous "not installed" signal — Epic deletes the `.item` on a
    /// real uninstall.
    pub fn is_installed(&self, app_id: &str) -> bool {
        self.installed.get(app_id).copied().unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    fn complete_manifest(app: &str, loc: &str) -> String {
        format!(
            r#"{{"DisplayName":"{app} Game","InstallLocation":"{loc}","LaunchExecutable":"{app}.exe","AppName":"{app}","InstallSize":123456,"bIsIncompleteInstall":false}}"#
        )
    }

    #[test]
    fn scan_maps_fields_and_skips_malformed() {
        let tmp = std::env::temp_dir().join(format!("gv_epic_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        write(
            &tmp,
            "good.item",
            &complete_manifest("Fortnite", "C:/Games/Fortnite"),
        );
        write(
            &tmp,
            "incomplete.item",
            r#"{"DisplayName":"Paused","InstallLocation":"C:/Games/Paused","LaunchExecutable":"p.exe","AppName":"Paused","InstallSize":10,"bIsIncompleteInstall":true}"#,
        );
        write(&tmp, "broken.item", "{ this is not json ");
        // Missing fields must default, not panic; empty app_name/location is skipped.
        write(&tmp, "empty.item", r#"{"DisplayName":"NoLoc"}"#);
        // Non-.item files ignored.
        write(&tmp, "notes.txt", "ignore me");

        let games = scan_manifests_dir(&tmp);

        // good + incomplete both surface as DetectedGame (status is decided by
        // the integrity system, not the scanner); empty/broken/txt are skipped.
        assert_eq!(games.len(), 2, "got: {games:?}");

        let fortnite = games.iter().find(|g| g.title == "Fortnite Game").unwrap();
        assert_eq!(fortnite.source_code, "epic");
        assert_eq!(fortnite.source_app_id.as_deref(), Some("Fortnite"));
        assert_eq!(
            fortnite.executable.as_deref(),
            Some(std::path::Path::new("Fortnite.exe"))
        );
        assert_eq!(fortnite.install_size_bytes, Some(123456));
        assert_eq!(fortnite.install_state_hint, Some(true));

        let paused = games.iter().find(|g| g.title == "Paused").unwrap();
        assert_eq!(paused.install_state_hint, Some(false));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn context_reports_installed_state() {
        let tmp = std::env::temp_dir().join(format!("gv_epic_ctx_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        write(
            &tmp,
            "a.item",
            &complete_manifest("Alpha", "C:/Games/Alpha"),
        );
        write(
            &tmp,
            "b.item",
            r#"{"DisplayName":"Beta","InstallLocation":"C:/Games/Beta","LaunchExecutable":"b.exe","AppName":"Beta","bIsIncompleteInstall":true}"#,
        );

        let ctx = EpicContext::from_dir(&tmp);
        assert!(ctx.is_installed("Alpha"));
        assert!(
            !ctx.is_installed("Beta"),
            "incomplete install is not installed"
        );
        assert!(
            !ctx.is_installed("Missing"),
            "absent manifest is not installed"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn missing_dir_yields_empty() {
        let nope = std::env::temp_dir().join("gv_epic_does_not_exist_xyz");
        let _ = std::fs::remove_dir_all(&nope);
        assert!(scan_manifests_dir(&nope).is_empty());
        assert!(!EpicContext::from_dir(&nope).is_installed("anything"));
    }
}
