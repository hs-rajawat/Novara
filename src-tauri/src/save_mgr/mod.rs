//! Save manager.
//!
//! Backups are simple zip-style archives of a watched folder, stored
//! under <app_data>/backups/<profile_id>/<timestamp>.zip. Versioned
//! by timestamp; restore extracts atomically to a sibling temp dir,
//! then swaps directory names.
//!
//! For the MVP we implement an *uncompressed tar* using only the
//! standard library so no extra dependency is required. Production
//! deployments can swap to `zip` or `zstd` behind the same trait.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::events::{AppEvent, EventBus};
use crate::models::now_rfc3339;

#[derive(Clone)]
pub struct SaveManager {
    db: Db,
    bus: EventBus,
    backups_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BackupResult {
    pub backup_id: i64,
    pub archive_path: PathBuf,
    pub size_bytes: i64,
    pub file_count: i64,
}

impl SaveManager {
    pub fn new(db: Db, bus: EventBus, app_data_dir: &Path) -> AppResult<Self> {
        let backups_root = app_data_dir.join("backups");
        fs::create_dir_all(&backups_root)?;
        Ok(Self { db, bus, backups_root })
    }

    pub async fn backup(&self, profile_id: &str, note: Option<&str>) -> AppResult<BackupResult> {
        let profile = self
            .db
            .get_save_profile(profile_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("save profile {profile_id}")))?;
        let source = PathBuf::from(&profile.source_dir);
        if !source.exists() {
            return Err(AppError::SaveMgr(format!(
                "source dir does not exist: {}",
                source.display()
            )));
        }

        let dest_dir = self.backups_root.join(profile_id);
        fs::create_dir_all(&dest_dir)?;
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
        let archive_path = dest_dir.join(format!("{timestamp}.gvbk"));

        let (size, files) = write_archive(&source, &archive_path)?;

        let id = self
            .db
            .record_backup(profile_id, archive_path.to_string_lossy().as_ref(), size, files, note)
            .await?;

        self.bus.emit(AppEvent::SaveBackupCreated {
            profile_id: profile_id.into(),
            backup_id: id,
        });

        Ok(BackupResult {
            backup_id: id,
            archive_path,
            size_bytes: size,
            file_count: files,
        })
    }

    pub async fn restore(&self, backup_id: i64) -> AppResult<()> {
        let backup = self
            .db
            .get_backup(backup_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("backup {backup_id}")))?;
        let profile = self
            .db
            .get_save_profile(&backup.profile_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("save profile {}", backup.profile_id)))?;
        let archive = PathBuf::from(&backup.archive_path);
        let target = PathBuf::from(&profile.source_dir);

        // Safety net: take a fresh backup of the current state before
        // restoring, so users can undo. The note tags it as auto.
        let _ = self.backup(&profile.id, Some("pre-restore auto-backup")).await;

        // Atomic-ish: extract to a sibling tmp dir, then rename.
        let tmp = target.with_extension(format!("gvrestore.{}", now_rfc3339().replace(':', "-")));
        fs::create_dir_all(&tmp)?;
        read_archive(&archive, &tmp)?;
        // Move existing target out of the way (don't delete user data outright).
        if target.exists() {
            let trash = target.with_extension(format!("gvprev.{}", now_rfc3339().replace(':', "-")));
            fs::rename(&target, &trash)?;
        }
        fs::rename(&tmp, &target)?;
        Ok(())
    }
}

// ───────── archive format (very small custom format) ─────────
// HEADER:  b"GVBK\x01"
// ENTRIES: repeated [u32 path_len][path][u64 file_size][bytes]
// EOF:     u32 == 0 (zero-length path = end marker)
//
// Chosen because:
//   * no extra deps
//   * deterministic file order
//   * straightforward to read sequentially
// Replace with `zip` for compression / external tooling compatibility.

fn write_archive(source: &Path, dest: &Path) -> AppResult<(i64, i64)> {
    use walkdir::WalkDir;
    let mut out = fs::File::create(dest)?;
    out.write_all(b"GVBK\x01")?;
    let mut total: i64 = 0;
    let mut files: i64 = 0;

    let entries: Vec<_> = WalkDir::new(source)
        .into_iter()
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    for entry in entries {
        let path = entry.path();
        let rel = path.strip_prefix(source).map_err(|e| AppError::Other(e.to_string()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let rel_bytes = rel_str.as_bytes();
        if rel_bytes.is_empty() {
            continue;
        }
        out.write_all(&(rel_bytes.len() as u32).to_le_bytes())?;
        out.write_all(rel_bytes)?;
        let bytes = fs::read(path)?;
        out.write_all(&(bytes.len() as u64).to_le_bytes())?;
        out.write_all(&bytes)?;
        total = total.saturating_add(bytes.len() as i64);
        files += 1;
    }
    out.write_all(&0u32.to_le_bytes())?; // EOF marker
    Ok((total, files))
}

fn read_archive(archive: &Path, dest: &Path) -> AppResult<()> {
    let mut f = fs::File::open(archive)?;
    let mut header = [0u8; 5];
    f.read_exact(&mut header)?;
    if &header != b"GVBK\x01" {
        return Err(AppError::SaveMgr("not a gvbk archive".into()));
    }
    loop {
        let mut len_buf = [0u8; 4];
        f.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf);
        if len == 0 {
            break;
        }
        let mut path_buf = vec![0u8; len as usize];
        f.read_exact(&mut path_buf)?;
        let rel = String::from_utf8(path_buf).map_err(|e| AppError::SaveMgr(e.to_string()))?;
        let mut size_buf = [0u8; 8];
        f.read_exact(&mut size_buf)?;
        let size = u64::from_le_bytes(size_buf);
        let mut data = vec![0u8; size as usize];
        f.read_exact(&mut data)?;
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(out_path, data)?;
    }
    Ok(())
}
