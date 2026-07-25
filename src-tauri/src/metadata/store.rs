//! Shared on-disk artwork storage. Generalizes the manual-upload path
//! (`commands::games::copy_artwork`) to also handle provider-supplied
//! assets — local-file copy or network download — using the same
//! `<app_data>/artwork/<game_id>/<kind>.<ext>` convention established
//! before this milestone.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

use super::{ArtworkKind, AssetSource};

/// Copy a local file into `<app_data>/artwork/<game_id>/<kind>.<ext>`. No
/// network involved — this is the path `set_cover_path`/`set_hero_path`/
/// `set_logo_path`/`set_icon_path` use for a user-picked file, and what a
/// `LocalFile`-sourced provider asset (e.g. Steam's own artwork cache)
/// resolves to as well.
pub fn store_local_asset(
    app_data_dir: &Path,
    game_id: &str,
    kind: ArtworkKind,
    src: &Path,
) -> AppResult<String> {
    if !src.is_file() {
        return Err(AppError::Invalid(format!("not a file: {}", src.display())));
    }
    let dest_dir = artwork_dir(app_data_dir, game_id)?;
    let ext = extension_of(src).unwrap_or_else(|| "jpg".to_string());
    let dest = clean_and_dest(&dest_dir, kind, &ext);
    if src != dest.as_path() {
        std::fs::copy(src, &dest).map_err(|e| AppError::Other(format!("copy artwork: {e}")))?;
    }
    Ok(dest.display().to_string())
}

/// Download `url` and store it the same way. Writes to a temp file in the
/// same directory first, then renames into place, so a half-downloaded
/// file is never the path a caller persists.
pub async fn store_remote_asset(
    app_data_dir: &Path,
    game_id: &str,
    kind: ArtworkKind,
    url: &str,
    client: &reqwest::Client,
) -> AppResult<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| AppError::Metadata(format!("download {url}: {e}")))?;
    let ext = extension_from_url(url).unwrap_or_else(|| "jpg".to_string());
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Metadata(format!("read body {url}: {e}")))?;

    let dest_dir = artwork_dir(app_data_dir, game_id)?;
    let dest = clean_and_dest(&dest_dir, kind, &ext);
    let tmp = dest_dir.join(format!("{}.{}.tmp", kind.as_str(), std::process::id()));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| AppError::Other(format!("write artwork temp: {e}")))?;
    std::fs::rename(&tmp, &dest)
        .map_err(|e| AppError::Other(format!("finalize artwork: {e}")))?;
    Ok(dest.display().to_string())
}

/// Dispatch on `AssetSource` — the single entry point `ArtworkService` uses
/// so it never has to match on the source kind itself.
pub async fn store_asset(
    app_data_dir: &Path,
    game_id: &str,
    kind: ArtworkKind,
    source: &AssetSource,
    client: &reqwest::Client,
) -> AppResult<String> {
    match source {
        AssetSource::LocalFile(src) => store_local_asset(app_data_dir, game_id, kind, src),
        AssetSource::RemoteUrl(url) => {
            store_remote_asset(app_data_dir, game_id, kind, url, client).await
        }
    }
}

fn artwork_dir(app_data_dir: &Path, game_id: &str) -> AppResult<PathBuf> {
    let dir = app_data_dir.join("artwork").join(game_id);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Other(format!("create artwork dir: {e}")))?;
    Ok(dir)
}

/// Remove any stale file for `kind` under a different extension, and return
/// the destination path for the new one.
fn clean_and_dest(dest_dir: &Path, kind: ArtworkKind, ext: &str) -> PathBuf {
    let dest = dest_dir.join(format!("{}.{}", kind.as_str(), ext));
    for old_ext in ["jpg", "jpeg", "png", "gif", "webp", "bmp"] {
        let old = dest_dir.join(format!("{}.{}", kind.as_str(), old_ext));
        if old.exists() && old != dest {
            let _ = std::fs::remove_file(&old);
        }
    }
    dest
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
}

fn extension_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
}
