//! Steam scanner — reads Steam's local manifest files to find installed
//! games. This is purely *local discovery* of legally-installed Steam
//! titles; no network calls, no API key, no DRM circumvention.
//!
//! Steam stores each installed game's metadata in
//!   <library>/steamapps/appmanifest_<appid>.acf
//! and the list of libraries lives in
//!   <steam>/steamapps/libraryfolders.vdf
//!
//! ACF/VDF are key-value text — we parse with `keyvalues-parser`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tracing::debug;

use crate::error::{AppError, AppResult};

use super::{DetectedGame, Scanner};

#[derive(Default)]
pub struct SteamScanner;

#[async_trait]
impl Scanner for SteamScanner {
    fn code(&self) -> &'static str {
        "steam"
    }

    async fn scan(&self, _roots: &[PathBuf]) -> AppResult<Vec<DetectedGame>> {
        let Some(steam_dir) = find_steam_dir() else {
            debug!("steam not installed");
            return Ok(vec![]);
        };

        let libraries = discover_libraries(&steam_dir);
        let mut games = Vec::new();
        for lib in libraries {
            let steamapps = lib.join("steamapps");
            let Ok(read) = std::fs::read_dir(&steamapps) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                    continue;
                }
                if let Ok(g) = parse_manifest(&path, &steamapps) {
                    games.push(g);
                }
            }
        }
        Ok(games)
    }
}

#[cfg(target_os = "windows")]
fn find_steam_dir() -> Option<PathBuf> {
    // Registry would be more reliable, but a Program Files probe avoids
    // pulling a registry crate for the MVP. Override via $GAMEVAULT_STEAM_DIR.
    if let Ok(dir) = std::env::var("GAMEVAULT_STEAM_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }
    for candidate in [
        r"C:\Program Files (x86)\Steam",
        r"C:\Program Files\Steam",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn find_steam_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("GAMEVAULT_STEAM_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }
    let home = dirs::home_dir()?;
    for rel in [".steam/steam", ".local/share/Steam", "Library/Application Support/Steam"] {
        let p = home.join(rel);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn discover_libraries(steam_dir: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_dir.to_path_buf()];
    let vdf = steam_dir.join("steamapps").join("libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(&vdf) else {
        return libs;
    };
    let Ok(parsed) = keyvalues_parser::Vdf::parse(&text) else {
        return libs;
    };
    // libraryfolders.vdf shape:
    //   "libraryfolders" { "0" { "path" "D:\\SteamLibrary" ... } ... }
    let Some(obj) = parsed.value.get_obj() else { return libs };
    for (_, child_values) in obj.iter() {
        for child in child_values.iter() {
            if let Some(inner) = child.get_obj() {
                if let Some(values) = inner.get("path") {
                    if let Some(s) = values.first().and_then(|v| v.get_str()) {
                        let p = PathBuf::from(s.replace("\\\\", "\\"));
                        if p.exists() && !libs.contains(&p) {
                            libs.push(p);
                        }
                    }
                }
            }
        }
    }
    libs
}

fn parse_manifest(path: &Path, steamapps_dir: &Path) -> AppResult<DetectedGame> {
    let text = std::fs::read_to_string(path)?;
    let parsed = keyvalues_parser::Vdf::parse(&text)
        .map_err(|e| AppError::Scanner(format!("ACF parse: {e}")))?;
    let obj = parsed
        .value
        .get_obj()
        .ok_or_else(|| AppError::Scanner("ACF not an object".into()))?;

    let pick = |k: &str| -> Option<String> {
        obj.get(k)
            .and_then(|v| v.first())
            .and_then(|v| v.get_str())
            .map(|s| s.to_string())
    };

    let appid = pick("appid").ok_or_else(|| AppError::Scanner("missing appid".into()))?;
    let name = pick("name").ok_or_else(|| AppError::Scanner("missing name".into()))?;
    let install_subdir = pick("installdir").unwrap_or_else(|| name.clone());
    let install_dir = steamapps_dir.join("common").join(&install_subdir);

    let size: Option<i64> = pick("SizeOnDisk").and_then(|s| s.parse().ok());

    Ok(DetectedGame {
        source_code: "steam".into(),
        title: name,
        install_dir,
        executable: None, // Steam launches via steam:// — we'd resolve a binary later if needed
        source_app_id: Some(appid),
        install_size_bytes: size,
    })
}
