//! Manual / non-launcher scanner.
//!
//! Walks user-provided roots up to a small depth, treats each subfolder
//! that contains an executable as a candidate game, and uses the folder
//! name as the title (better metadata gets filled in by the metadata
//! provider afterwards).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use walkdir::WalkDir;

use crate::error::AppResult;

use super::{DetectedGame, Scanner};

const MAX_DEPTH: usize = 3;
const EXE_EXTENSIONS: &[&str] = &["exe", "bat", "sh", "AppImage", "x86_64"];

#[derive(Default)]
pub struct ManualScanner;

#[async_trait]
impl Scanner for ManualScanner {
    fn code(&self) -> &'static str {
        "manual"
    }

    async fn scan(&self, roots: &[PathBuf]) -> AppResult<Vec<DetectedGame>> {
        let mut out = Vec::new();
        for root in roots {
            if !root.exists() {
                continue;
            }
            scan_root(root, &mut out);
        }
        Ok(out)
    }
}

fn scan_root(root: &Path, out: &mut Vec<DetectedGame>) {
    // For each immediate child folder, look for an executable inside.
    let Ok(read) = std::fs::read_dir(root) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(exe) = find_executable(&path) {
            let title = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Unknown")
                .to_string();
            let size = dir_size(&path).ok();
            out.push(DetectedGame {
                source_code: "manual".into(),
                title,
                install_dir: path.clone(),
                executable: Some(exe.strip_prefix(&path).unwrap_or(&exe).to_path_buf()),
                source_app_id: None,
                install_size_bytes: size,
            });
        }
    }
}

fn find_executable(dir: &Path) -> Option<PathBuf> {
    // Prefer an exe whose stem matches the folder name; otherwise the
    // largest exe (skipping obvious uninstallers / setup binaries).
    let folder_name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let mut best: Option<(PathBuf, u64)> = None;
    let mut named_match: Option<PathBuf> = None;
    for entry in WalkDir::new(dir).max_depth(MAX_DEPTH).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !EXE_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let lowered = stem.to_ascii_lowercase();
        if lowered.contains("unins") || lowered.contains("setup") || lowered.contains("crash") || lowered.contains("redist") {
            continue;
        }
        if named_match.is_none() && stem.eq_ignore_ascii_case(folder_name) {
            named_match = Some(path.to_path_buf());
        }
        if let Ok(meta) = entry.metadata() {
            let size = meta.len();
            if best.as_ref().map(|(_, s)| size > *s).unwrap_or(true) {
                best = Some((path.to_path_buf(), size));
            }
        }
    }
    named_match.or_else(|| best.map(|(p, _)| p))
}

fn dir_size(dir: &Path) -> std::io::Result<i64> {
    let mut total: i64 = 0;
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len() as i64);
            }
        }
    }
    Ok(total)
}
