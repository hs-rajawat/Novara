//! Shared on-disk artwork storage. Generalizes the manual-upload path
//! (`commands::games::copy_artwork`) to also handle provider-supplied
//! assets — local-file copy or network download — using the same
//! `<app_data>/artwork/<game_id>/<kind>.<ext>` convention established
//! before this milestone.

use std::path::{Path, PathBuf};

use crate::db::artwork::Validators;
use crate::error::{AppError, AppResult};

use super::throttle::Throttle;
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

/// The outcome of storing one asset.
#[derive(Debug, Clone)]
pub struct StoredAsset {
    /// Absolute path the bytes now live at.
    pub path: String,
    /// Validator the origin returned, for a later conditional refresh.
    pub etag: Option<String>,
    /// `Last-Modified` the origin returned.
    pub last_modified: Option<String>,
    /// True when the origin answered 304 and nothing was re-downloaded.
    pub unchanged: bool,
}

/// Download `url` and store it the same way. Writes to a temp file in the
/// same directory first, then renames into place, so a half-downloaded
/// file is never the path a caller persists.
///
/// Both stored validators are offered (`If-None-Match` and
/// `If-Modified-Since`), because origins disagree about which they honour —
/// measured against the Steam CDN this project uses, `If-None-Match` is ignored
/// and answered with a full 200, while `If-Modified-Since` returns 304. A 304
/// means the bytes on disk are still current, so nothing is transferred.
pub async fn store_remote_asset(
    app_data_dir: &Path,
    game_id: &str,
    kind: ArtworkKind,
    url: &str,
    client: &reqwest::Client,
    throttle: &Throttle,
    validators: Validators<'_>,
) -> AppResult<StoredAsset> {
    let dest_dir = artwork_dir(app_data_dir, game_id)?;

    let mut request = client.get(url);
    if let Some(etag) = validators.etag {
        request = request.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = validators.last_modified {
        request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }

    // Held only for the request itself; writing to disk does not need a slot.
    let resp = {
        let _slot = throttle.acquire().await;
        request
            .send()
            .await
            .map_err(|e| AppError::Metadata(format!("download {url}: {e}")))?
    };

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        // Report the path already on disk so the caller can keep it.
        let ext = extension_from_url(url).unwrap_or_else(|| "jpg".to_string());
        return Ok(StoredAsset {
            path: dest_dir.join(format!("{}.{}", kind.as_str(), ext)).display().to_string(),
            etag: validators.etag.map(str::to_string),
            last_modified: validators.last_modified.map(str::to_string),
            unchanged: true,
        });
    }

    let resp = resp
        .error_for_status()
        .map_err(|e| AppError::Metadata(format!("download {url}: {e}")))?;

    let header = |name: reqwest::header::HeaderName| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let etag = header(reqwest::header::ETAG);
    let last_modified = header(reqwest::header::LAST_MODIFIED);

    let ext = extension_from_url(url).unwrap_or_else(|| "jpg".to_string());
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Metadata(format!("read body {url}: {e}")))?;

    let dest = clean_and_dest(&dest_dir, kind, &ext);
    let tmp = dest_dir.join(format!("{}.{}.tmp", kind.as_str(), std::process::id()));
    std::fs::write(&tmp, &bytes)
        .map_err(|e| AppError::Other(format!("write artwork temp: {e}")))?;
    std::fs::rename(&tmp, &dest)
        .map_err(|e| AppError::Other(format!("finalize artwork: {e}")))?;
    Ok(StoredAsset {
        path: dest.display().to_string(),
        etag,
        last_modified,
        unchanged: false,
    })
}

/// Dispatch on `AssetSource` — the single entry point `ArtworkService` uses
/// so it never has to match on the source kind itself.
pub async fn store_asset(
    app_data_dir: &Path,
    game_id: &str,
    kind: ArtworkKind,
    source: &AssetSource,
    client: &reqwest::Client,
    throttle: &Throttle,
    validators: Validators<'_>,
) -> AppResult<StoredAsset> {
    match source {
        AssetSource::LocalFile(src) => {
            // No network, so no throttling and no validators to speak of.
            store_local_asset(app_data_dir, game_id, kind, src).map(|path| StoredAsset {
                path,
                etag: None,
                last_modified: None,
                unchanged: false,
            })
        }
        AssetSource::RemoteUrl(url) => {
            store_remote_asset(
                app_data_dir,
                game_id,
                kind,
                url,
                client,
                throttle,
                validators,
            )
            .await
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
