//! Save manager.
//!
//! Backups are archives of a watched folder, stored under
//! `<app_data>/backups/<profile_id>/<timestamp>.gvbk`. Versioned by timestamp.
//! The archive format, and everything about treating an archive as untrusted
//! input, lives in [`archive`].
//!
//! # Restore is the dangerous operation
//!
//! Restore replaces a directory of the user's save data. The ordering here is
//! deliberate and each step exists because of a specific way the previous
//! implementation could lose data:
//!
//! 1. Take a safety backup **and require it to succeed**. Its result used to be
//!    discarded with `let _ =`, so a restore would proceed even when its own undo
//!    path had failed.
//! 2. Validate and extract the archive to a temporary sibling directory. Nothing
//!    touches the live folder until this has fully succeeded.
//! 3. Move the live folder aside, move the extraction into place, then remove the
//!    displaced copy — it is redundant now that step 1 is guaranteed. Previously
//!    it was kept forever under a `.gvprev.<timestamp>` name, so every restore
//!    permanently grew the user's app-data directory.
//! 4. If any of step 3 fails, put the original folder back.

pub mod archive;

use std::fs;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::events::{AppEvent, EventBus};

use archive::{compile_glob, read_archive, sibling_with_suffix, write_archive};

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
        Ok(Self {
            db,
            bus,
            backups_root,
        })
    }

    pub async fn backup(&self, profile_id: &str, note: Option<&str>) -> AppResult<BackupResult> {
        let profile = self
            .db
            .get_save_profile(profile_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("save profile {profile_id}")))?;
        let source = PathBuf::from(&profile.source_dir);
        if !source.is_dir() {
            return Err(AppError::SaveMgr(format!(
                "source dir does not exist: {}",
                source.display()
            )));
        }

        // `save_profiles.glob` was accepted through the whole stack and then
        // ignored by the archiver, so a user could set a filter that did
        // nothing. Compiled here so an invalid pattern is reported before any
        // work happens.
        let filter = compile_glob(profile.glob.as_deref())?;

        let dest_dir = self.backups_root.join(profile_id);
        fs::create_dir_all(&dest_dir)?;
        let archive_path = unique_archive_path(&dest_dir)?;

        // Archiving is unbounded filesystem I/O — potentially gigabytes — so it
        // runs on the blocking pool rather than stalling a tokio worker.
        let stats = {
            let (source, archive_path) = (source.clone(), archive_path.clone());
            tauri::async_runtime::spawn_blocking(move || {
                write_archive(&source, &archive_path, filter.as_ref())
            })
            .await
            .map_err(|e| AppError::SaveMgr(format!("backup task failed: {e}")))??
        };

        let id = self
            .db
            .record_backup(
                profile_id,
                archive_path.to_string_lossy().as_ref(),
                stats.total_bytes,
                stats.file_count,
                note,
            )
            .await?;

        self.bus.emit(AppEvent::SaveBackupCreated {
            profile_id: profile_id.into(),
            backup_id: id,
        });

        Ok(BackupResult {
            backup_id: id,
            archive_path,
            size_bytes: stats.total_bytes,
            file_count: stats.file_count,
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

        if !archive.is_file() {
            return Err(AppError::SaveMgr(format!(
                "backup archive is missing: {}",
                archive.display()
            )));
        }

        // A save location that exists but is not a directory means the profile
        // is misconfigured. Refuse rather than replace it: the safety-backup step
        // below cannot archive a file, so continuing would silently swap the
        // user's file for a directory with no way back.
        if target.exists() && !target.is_dir() {
            return Err(AppError::SaveMgr(format!(
                "the save location is not a directory: {}. \
                 Nothing has been changed; check this save profile's folder.",
                target.display()
            )));
        }

        // Safety net: a fresh backup of the current state, so the restore can be
        // undone. Its failure is fatal — proceeding without a working undo path
        // is how a bad restore becomes unrecoverable. Only skipped when there is
        // nothing to preserve.
        if target.is_dir() {
            self.backup(&profile.id, Some("pre-restore auto-backup"))
                .await
                .map_err(|e| {
                    AppError::SaveMgr(format!(
                        "refusing to restore: the safety backup of your current saves failed \
                         ({e}). Your existing save data has not been touched."
                    ))
                })?;
        }

        // Sibling paths built by appending to the *whole* file name. Using
        // `Path::with_extension` here replaced everything after the last dot, so
        // for a folder like `S.T.A.L.K.E.R.` the "sibling" was a different path
        // entirely and the restore wrote somewhere unintended.
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f").to_string();
        let staging = sibling_with_suffix(&target, &format!("gvrestore.{stamp}"))?;
        let displaced = sibling_with_suffix(&target, &format!("gvprev.{stamp}"))?;

        // Extract to staging first: the live folder is untouched until the
        // archive has been fully validated and written out.
        let extraction = {
            let (archive, staging) = (archive.clone(), staging.clone());
            tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
                fs::create_dir_all(&staging).map_err(|e| {
                    AppError::SaveMgr(format!("create {}: {e}", staging.display()))
                })?;
                read_archive(&archive, &staging).map(|_| ())
            })
            .await
            .map_err(|e| AppError::SaveMgr(format!("restore task failed: {e}")))?
        };
        if let Err(e) = extraction {
            // Nothing has been moved yet, so cleaning up staging restores the
            // world exactly as it was.
            remove_dir_best_effort(&staging);
            return Err(e);
        }

        let target_for_swap = target.clone();
        let staging_for_swap = staging.clone();
        let swap = tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
            let had_target = target_for_swap.is_dir();
            if had_target {
                fs::rename(&target_for_swap, &displaced).map_err(|e| {
                    AppError::SaveMgr(format!(
                        "move existing saves aside ({} -> {}): {e}",
                        target_for_swap.display(),
                        displaced.display()
                    ))
                })?;
            }
            if let Err(e) = fs::rename(&staging_for_swap, &target_for_swap) {
                // Put the user's saves back before reporting failure; leaving
                // them parked under a temporary name would look like data loss.
                if had_target {
                    if let Err(rollback) = fs::rename(&displaced, &target_for_swap) {
                        return Err(AppError::SaveMgr(format!(
                            "restore failed ({e}) AND rolling back failed ({rollback}). \
                             Your previous saves are at {}",
                            displaced.display()
                        )));
                    }
                }
                return Err(AppError::SaveMgr(format!(
                    "restore failed while installing the backup: {e}. \
                     Your previous saves have been left in place."
                )));
            }
            // The displaced copy is redundant: the guaranteed safety backup
            // above holds exactly this state as a verifiable archive. Keeping it
            // is what made every restore grow the app-data directory without
            // bound.
            if had_target {
                if let Err(e) = fs::remove_dir_all(&displaced) {
                    warn!(
                        path = %displaced.display(),
                        error = %e,
                        "restore succeeded but the displaced save folder could not be removed"
                    );
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| AppError::SaveMgr(format!("restore swap task failed: {e}")))?;

        if swap.is_err() {
            remove_dir_best_effort(&staging);
        }
        swap
    }
}

/// A backup archive path that does not already exist.
///
/// Names were `%Y%m%dT%H%M%S.gvbk` — one second of resolution. Two backups of
/// the same profile within the same second therefore produced the *same
/// filename*, and the second silently overwrote the first while leaving two
/// database rows pointing at one file.
///
/// That is not a theoretical race: `restore` takes a pre-restore safety backup
/// immediately before extracting, so restoring a backup taken moments earlier
/// overwrote the very archive being restored — and then restored the state it had
/// just captured, reporting success. Milliseconds plus an explicit existence
/// check close it; the loop covers the remaining case of two backups inside the
/// same millisecond.
fn unique_archive_path(dest_dir: &Path) -> AppResult<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f").to_string();
    let base = stamp.replace('.', "-");
    for attempt in 0..100 {
        let name = if attempt == 0 {
            format!("{base}.gvbk")
        } else {
            format!("{base}-{attempt}.gvbk")
        };
        let candidate = dest_dir.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::SaveMgr(
        "could not find an unused backup filename".into(),
    ))
}

fn remove_dir_best_effort(path: &Path) {    if path.exists() {
        if let Err(e) = fs::remove_dir_all(path) {
            warn!(
                path = %path.display(),
                error = %e,
                "failed to clean up temporary restore directory"
            );
        }
    }
}

#[cfg(test)]
mod tests;
