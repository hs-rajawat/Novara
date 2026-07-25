//! Regression tests for the Batch 7 save-manager repairs.
//!
//! Every test here corresponds to a way the previous implementation could lose,
//! corrupt, or misplace a user's save data.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::archive::{compile_glob, read_archive, sibling_with_suffix, write_archive};
use super::SaveManager;
use crate::error::AppError;
use crate::events::EventBus;
use crate::test_support::{seed_game, test_db};

/// A temporary directory removed when the test ends.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("novara-b7-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
    fn child(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

/// Build a save folder with a nested layout.
fn sample_saves(root: &Path) {
    write_file(&root.join("profile.sav"), b"slot one");
    write_file(&root.join("settings.cfg"), b"volume=11");
    write_file(&root.join("slots/slot1.sav"), b"deep save");
    write_file(&root.join("logs/debug.log"), b"noise");
}

// ── 7.1 sibling paths for dotted folder names ───────────────────────────

/// `Path::with_extension` replaces everything after the last dot, so for a save
/// folder containing dots the "sibling" was a different path entirely and the
/// restore wrote somewhere unintended.
#[test]
fn sibling_paths_are_correct_for_dotted_folder_names() {
    for name in [
        "S.T.A.L.K.E.R.",
        "Company.of.Heroes",
        "plain",
        "trailing.",
        ".hidden",
    ] {
        let target = Path::new("D:/saves").join(name);
        let sibling = sibling_with_suffix(&target, "gvprev.123").unwrap();

        assert_eq!(
            sibling.parent(),
            target.parent(),
            "{name}: must remain a sibling"
        );
        let sibling_name = sibling.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(
            sibling_name,
            format!("{name}.gvprev.123"),
            "{name}: the whole original name must be preserved"
        );
        assert_ne!(sibling, target, "{name}: must not collide with the target");
    }
}

/// The precise failure the old code had, stated as a test so it cannot come back.
///
/// Note which names break: `with_extension` replaces everything after the *last*
/// dot, so `Company.of.Heroes` became `Company.of.<suffix>` — a path that is not
/// the intended sibling and can collide with an unrelated folder. A name ending
/// in a dot, like `S.T.A.L.K.E.R.`, happens to survive because there is no
/// extension to replace, which is exactly why this bug is easy to miss: it
/// depends on the shape of the user's folder name.
#[test]
fn with_extension_would_have_corrupted_a_dotted_name() {
    let target = Path::new("D:/saves/Company.of.Heroes");
    let wrong = target.with_extension("gvprev.123");
    let right = sibling_with_suffix(target, "gvprev.123").unwrap();

    assert_eq!(
        wrong.file_name().unwrap().to_string_lossy(),
        "Company.of.gvprev.123",
        "documents the old behaviour: the final name component was truncated"
    );
    assert_eq!(
        right.file_name().unwrap().to_string_lossy(),
        "Company.of.Heroes.gvprev.123"
    );
    assert_ne!(wrong, right);

    // The trailing-dot case is where the old code accidentally behaved: there is
    // no extension to replace, so it appended. Appending to the whole name is
    // still correct (it yields a distinct sibling), just spelled differently.
    let stalker = Path::new("D:/saves/S.T.A.L.K.E.R.");
    let appended = sibling_with_suffix(stalker, "gvprev.123").unwrap();
    assert_eq!(appended.parent(), stalker.parent());
    assert_ne!(appended, stalker);
    assert_eq!(
        appended.file_name().unwrap().to_string_lossy(),
        "S.T.A.L.K.E.R..gvprev.123",
        "the original name is preserved in full, dot included"
    );
}

// ── archive round trip ──────────────────────────────────────────────────

#[test]
fn archive_round_trips_a_nested_folder() {
    let dir = TempDir::new("roundtrip");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    let restored = dir.child("restored");

    let stats = write_archive(&source, &archive, None).unwrap();
    assert_eq!(stats.file_count, 4);

    let read = read_archive(&archive, &restored).unwrap();
    assert_eq!(read.file_count, 4);
    assert_eq!(read.total_bytes, stats.total_bytes);
    assert_eq!(fs::read(restored.join("profile.sav")).unwrap(), b"slot one");
    assert_eq!(
        fs::read(restored.join("slots/slot1.sav")).unwrap(),
        b"deep save"
    );
}

/// Deterministic output is what makes the checksum meaningful across runs.
#[test]
fn archiving_the_same_folder_twice_produces_identical_bytes() {
    let dir = TempDir::new("determinism");
    let source = dir.child("saves");
    sample_saves(&source);
    let a = dir.child("a.gvbk");
    let b = dir.child("b.gvbk");
    write_archive(&source, &a, None).unwrap();
    write_archive(&source, &b, None).unwrap();
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
}

#[test]
fn an_empty_source_folder_produces_a_valid_empty_archive() {
    let dir = TempDir::new("empty");
    let source = dir.child("saves");
    fs::create_dir_all(&source).unwrap();
    let archive = dir.child("backup.gvbk");
    let restored = dir.child("restored");

    let stats = write_archive(&source, &archive, None).unwrap();
    assert_eq!(stats.file_count, 0);
    assert_eq!(read_archive(&archive, &restored).unwrap().file_count, 0);
}

// ── 7.6 the glob filter ─────────────────────────────────────────────────

#[test]
fn glob_compilation_treats_absent_and_blank_as_no_filter() {
    assert!(compile_glob(None).unwrap().is_none());
    assert!(compile_glob(Some("")).unwrap().is_none());
    assert!(compile_glob(Some("   ")).unwrap().is_none());
    assert!(compile_glob(Some(";;")).unwrap().is_none());
}

#[test]
fn glob_compilation_rejects_an_invalid_pattern() {
    let err = compile_glob(Some("[unclosed")).unwrap_err();
    assert!(matches!(err, AppError::Invalid(_)), "got {err:?}");
}

/// The filter was accepted through the whole stack and then ignored, so a user
/// could set one and silently get everything.
#[test]
fn the_glob_filter_selects_which_files_are_archived() {
    let dir = TempDir::new("glob");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    let restored = dir.child("restored");

    let filter = compile_glob(Some("*.sav;slots/**")).unwrap();
    assert!(filter.is_some());
    let stats = write_archive(&source, &archive, filter.as_ref()).unwrap();
    assert_eq!(stats.file_count, 2, "profile.sav and slots/slot1.sav");

    read_archive(&archive, &restored).unwrap();
    assert!(restored.join("profile.sav").is_file());
    assert!(restored.join("slots/slot1.sav").is_file());
    assert!(
        !restored.join("settings.cfg").exists(),
        "an excluded file must not be archived"
    );
    assert!(!restored.join("logs/debug.log").exists());
}

/// `*` must not cross a directory boundary, or a filter meant for the top level
/// would silently pull in everything nested.
#[test]
fn a_single_star_does_not_cross_directory_boundaries() {
    let dir = TempDir::new("star");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");

    let filter = compile_glob(Some("*.sav")).unwrap();
    let stats = write_archive(&source, &archive, filter.as_ref()).unwrap();
    assert_eq!(stats.file_count, 1, "only the top-level profile.sav");
}

#[test]
fn the_glob_filter_is_case_insensitive() {
    let dir = TempDir::new("case");
    let source = dir.child("saves");
    write_file(&source.join("PROFILE.SAV"), b"x");
    let archive = dir.child("backup.gvbk");
    let filter = compile_glob(Some("*.sav")).unwrap();
    assert_eq!(
        write_archive(&source, &archive, filter.as_ref())
            .unwrap()
            .file_count,
        1
    );
}

// ── 7.4 / 7.5 malformed archives ────────────────────────────────────────

/// Craft an archive with one entry, so individual fields can be corrupted.
fn crafted_archive(path: &Path, version: u8, rel: &str, declared_size: u64, payload: &[u8]) {
    let mut f = fs::File::create(path).unwrap();
    f.write_all(b"GVBK").unwrap();
    f.write_all(&[version]).unwrap();
    f.write_all(&(rel.len() as u32).to_le_bytes()).unwrap();
    f.write_all(rel.as_bytes()).unwrap();
    f.write_all(&declared_size.to_le_bytes()).unwrap();
    f.write_all(payload).unwrap();
    f.write_all(&0u32.to_le_bytes()).unwrap();
    // No footer: written as version 1 unless the caller says otherwise.
}

#[test]
fn a_wrong_magic_is_rejected() {
    let dir = TempDir::new("magic");
    let archive = dir.child("bad.gvbk");
    fs::write(&archive, b"NOPE\x01").unwrap();
    let err = read_archive(&archive, &dir.child("out")).unwrap_err();
    assert!(format!("{err}").contains("not a gvbk archive"), "{err}");
}

#[test]
fn an_unknown_version_is_rejected_rather_than_guessed_at() {
    let dir = TempDir::new("version");
    let archive = dir.child("bad.gvbk");
    crafted_archive(&archive, 99, "a.sav", 1, b"x");
    let err = read_archive(&archive, &dir.child("out")).unwrap_err();
    assert!(format!("{err}").contains("unsupported gvbk version"), "{err}");
}

/// The allocation bug: `vec![0u8; size as usize]` straight from the archive's own
/// size field. A declared size can never exceed the bytes left in the file, and
/// that is now checked before anything is read.
#[test]
fn a_declared_size_larger_than_the_file_is_rejected_without_allocating() {
    let dir = TempDir::new("hugesize");
    let archive = dir.child("bad.gvbk");
    // Claims 16 exabytes while carrying one byte.
    crafted_archive(&archive, 1, "a.sav", u64::MAX, b"x");

    let err = read_archive(&archive, &dir.child("out")).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("declares") && msg.contains("remain in the file"),
        "{msg}"
    );
}

#[test]
fn an_over_long_entry_path_is_rejected() {
    let dir = TempDir::new("longpath");
    let archive = dir.child("bad.gvbk");
    let mut f = fs::File::create(&archive).unwrap();
    f.write_all(b"GVBK\x01").unwrap();
    f.write_all(&(50_000u32).to_le_bytes()).unwrap();
    drop(f);
    let err = read_archive(&archive, &dir.child("out")).unwrap_err();
    assert!(format!("{err}").contains("exceeds the"), "{err}");
}

#[test]
fn a_truncated_archive_is_rejected_and_extracts_nothing() {
    let dir = TempDir::new("truncated");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    write_archive(&source, &archive, None).unwrap();

    // Lop off the tail.
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();

    let out = dir.child("out");
    let err = read_archive(&archive, &out).unwrap_err();
    assert!(format!("{err}").contains("truncated"), "{err}");
    assert!(
        !out.exists() || fs::read_dir(&out).unwrap().next().is_none(),
        "a failed restore must not leave files behind"
    );
}

/// The checksum's whole purpose: silent corruption must not be restored.
#[test]
fn a_corrupted_payload_is_detected_by_the_checksum() {
    let dir = TempDir::new("corrupt");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    write_archive(&source, &archive, None).unwrap();

    // Flip one bit somewhere in the middle of the payload region.
    let mut bytes = fs::read(&archive).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0b0000_0001;
    fs::write(&archive, &bytes).unwrap();

    let out = dir.child("out");
    let err = read_archive(&archive, &out).unwrap_err();
    assert!(format!("{err}").contains("checksum mismatch"), "{err}");
    assert!(
        !out.exists() || fs::read_dir(&out).unwrap().next().is_none(),
        "nothing may be extracted from a corrupt archive"
    );
}

#[test]
fn a_footer_declaring_the_wrong_file_count_is_rejected() {
    let dir = TempDir::new("count");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    write_archive(&source, &archive, None).unwrap();

    // Rewrite just the declared count in the footer (last 12 bytes).
    let mut bytes = fs::read(&archive).unwrap();
    let footer = bytes.len() - 12;
    bytes[footer..footer + 8].copy_from_slice(&99u64.to_le_bytes());
    fs::write(&archive, &bytes).unwrap();

    let err = read_archive(&archive, &dir.child("out")).unwrap_err();
    assert!(format!("{err}").contains("declares 99 files"), "{err}");
}

/// A user's existing version 1 backups must stay restorable.
#[test]
fn legacy_version_1_archives_are_still_readable() {
    let dir = TempDir::new("legacy");
    let archive = dir.child("old.gvbk");
    crafted_archive(&archive, 1, "profile.sav", 8, b"slot one");

    let out = dir.child("out");
    let stats = read_archive(&archive, &out).unwrap();
    assert_eq!(stats.file_count, 1);
    assert_eq!(fs::read(out.join("profile.sav")).unwrap(), b"slot one");
}

// ── path traversal ──────────────────────────────────────────────────────

/// Entry paths come from the archive, and the old code joined them straight onto
/// the destination — so `../../..` in an entry wrote wherever it liked. This is
/// the security fix, and each rejected shape gets its own case.
#[test]
fn entry_paths_that_escape_the_destination_are_rejected() {
    for (label, rel) in [
        ("parent traversal", "../escaped.sav"),
        ("deep traversal", "a/../../escaped.sav"),
        ("absolute unix", "/etc/passwd"),
        ("windows drive", "C:/Windows/System32/evil.dll"),
        ("backslash separator", "..\\escaped.sav"),
        ("current dir", "./a.sav"),
    ] {
        let dir = TempDir::new("traversal");
        let archive = dir.child("evil.gvbk");
        crafted_archive(&archive, 1, rel, 4, b"evil");

        let out = dir.child("out");
        let err = read_archive(&archive, &out)
            .expect_err(&format!("{label}: {rel:?} should have been rejected"));
        assert!(
            format!("{err}").contains("unsafe path in archive"),
            "{label}: {err}"
        );
        // And nothing was written anywhere.
        assert!(
            !out.exists() || fs::read_dir(&out).unwrap().next().is_none(),
            "{label}: nothing may be extracted"
        );
        assert!(
            !dir.child("escaped.sav").exists(),
            "{label}: a file escaped the destination"
        );
    }
}

/// The architectural invariant, asserted: validation must not touch the
/// filesystem, so a rejected archive leaves the destination entirely absent.
///
/// If someone later merges the validate and extract phases for performance, the
/// first valid entries would be written before the bad one was found and this
/// test fails — which is the point. A partially applied restore is the worst
/// outcome for save data: neither the old state nor the new one.
#[test]
fn validation_creates_nothing_even_when_early_entries_are_valid() {
    let dir = TempDir::new("invariant");
    let archive = dir.child("mixed.gvbk");

    // A benign entry first, then one that escapes the destination. A streaming
    // extractor would already have written `good.sav` before objecting.
    let mut f = fs::File::create(&archive).unwrap();
    f.write_all(b"GVBK\x01").unwrap();
    for (rel, payload) in [("good.sav", &b"fine"[..]), ("../escaped.sav", &b"evil"[..])] {
        f.write_all(&(rel.len() as u32).to_le_bytes()).unwrap();
        f.write_all(rel.as_bytes()).unwrap();
        f.write_all(&(payload.len() as u64).to_le_bytes()).unwrap();
        f.write_all(payload).unwrap();
    }
    f.write_all(&0u32.to_le_bytes()).unwrap();
    drop(f);

    let out = dir.child("out");
    let err = read_archive(&archive, &out).expect_err("the hostile entry must be rejected");
    assert!(format!("{err}").contains("unsafe path in archive"), "{err}");

    assert!(
        !out.exists(),
        "validation must not create the destination directory at all"
    );
    assert!(
        !dir.child("good.sav").exists() && !out.join("good.sav").exists(),
        "the valid entry preceding the hostile one must not have been written"
    );
    assert!(
        !dir.child("escaped.sav").exists(),
        "nothing may escape the destination"
    );
}

/// The same property for corruption rather than hostility: a checksum failure is
/// detected before any entry is written.
#[test]
fn validation_creates_nothing_when_the_checksum_fails() {
    let dir = TempDir::new("invariant-crc");
    let source = dir.child("saves");
    sample_saves(&source);
    let archive = dir.child("backup.gvbk");
    write_archive(&source, &archive, None).unwrap();

    // Corrupt the final payload byte, so every preceding entry is valid. Layout
    // is: ... payload | end marker (4) | footer (12), so the last payload byte
    // sits at len - 17.
    let mut bytes = fs::read(&archive).unwrap();
    let last_payload = bytes.len() - 17;
    bytes[last_payload] ^= 0xFF;
    fs::write(&archive, &bytes).unwrap();

    let out = dir.child("out");
    let err = read_archive(&archive, &out).expect_err("corruption must be rejected");
    assert!(format!("{err}").contains("checksum mismatch"), "{err}");
    assert!(
        !out.exists(),
        "no entry may be written when a later one fails validation"
    );
}

#[test]
fn ordinary_nested_paths_are_still_accepted() {
    let dir = TempDir::new("normal");
    let archive = dir.child("ok.gvbk");
    crafted_archive(&archive, 1, "deep/nested/slot.sav", 2, b"ok");
    let out = dir.child("out");
    read_archive(&archive, &out).unwrap();
    assert!(out.join("deep/nested/slot.sav").is_file());
}

// ── the manager: backup and restore ─────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    manager: SaveManager,
    saves: PathBuf,
    profile_id: String,
    db: crate::db::Db,
}

async fn fixture(save_folder_name: &str, glob: Option<&str>) -> Fixture {
    let db = test_db().await;
    let game = seed_game(&db, "Test Game").await;
    let dir = TempDir::new("mgr");
    let saves = dir.child(save_folder_name);
    sample_saves(&saves);

    let profile = db
        .create_save_profile(
            &game,
            "Default",
            saves.to_string_lossy().as_ref(),
            glob,
            false,
            false,
        )
        .await
        .unwrap();

    let manager = SaveManager::new(db.clone(), EventBus::new(256), dir.path()).unwrap();
    Fixture {
        _dir: dir,
        manager,
        saves,
        profile_id: profile.id,
        db,
    }
}

#[tokio::test]
async fn backup_then_restore_reproduces_the_save_folder() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
    assert_eq!(backup.file_count, 4);

    // Mutate the live folder, then restore over it.
    fs::write(f.saves.join("profile.sav"), b"ruined").unwrap();
    fs::remove_file(f.saves.join("slots/slot1.sav")).unwrap();

    f.manager.restore(backup.backup_id).await.unwrap();

    assert_eq!(fs::read(f.saves.join("profile.sav")).unwrap(), b"slot one");
    assert_eq!(
        fs::read(f.saves.join("slots/slot1.sav")).unwrap(),
        b"deep save"
    );
}

/// 7.1 end to end: a save folder whose name contains dots must restore in place.
#[tokio::test]
async fn restore_works_for_a_save_folder_with_dots_in_its_name() {
    let f = fixture("S.T.A.L.K.E.R.", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
    fs::write(f.saves.join("profile.sav"), b"ruined").unwrap();

    f.manager.restore(backup.backup_id).await.unwrap();

    assert!(f.saves.is_dir(), "the original folder must still be the target");
    assert_eq!(fs::read(f.saves.join("profile.sav")).unwrap(), b"slot one");
}

/// 7.2: the displaced copy used to be kept forever under `.gvprev.<timestamp>`,
/// so every restore permanently grew the app-data directory.
#[tokio::test]
async fn restore_does_not_leave_displaced_or_staging_directories_behind() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();

    for _ in 0..3 {
        f.manager.restore(backup.backup_id).await.unwrap();
    }

    let siblings: Vec<String> = fs::read_dir(f.saves.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("gvprev") || n.contains("gvrestore") || n.contains("gvbk.part"))
        .collect();
    assert!(
        siblings.is_empty(),
        "restore left temporary directories behind: {siblings:?}"
    );
}

/// Every restore still leaves a recoverable copy — as an archive, which is
/// checksummed, rather than a loose directory.
#[tokio::test]
async fn restore_records_a_safety_backup_first() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
    f.manager.restore(backup.backup_id).await.unwrap();

    let backups = f.db.list_backups(&f.profile_id).await.unwrap();
    assert_eq!(backups.len(), 2, "the original plus the pre-restore snapshot");
    assert!(
        backups
            .iter()
            .any(|b| b.note.as_deref() == Some("pre-restore auto-backup")),
        "the safety backup must be recorded: {backups:?}"
    );
}

/// 7.3: the safety backup's result was discarded with `let _ =`, so a restore
/// proceeded even when its own undo path had failed.
/// Archive filenames used one second of resolution, so two backups of the same
/// profile within the same second shared a filename: the second overwrote the
/// first, and two database rows pointed at one file.
///
/// This is how a restore could report success having restored the wrong data —
/// `restore` takes a safety backup immediately before extracting, so it
/// overwrote the very archive it was about to read.
#[tokio::test]
async fn rapid_backups_never_share_an_archive_file() {
    let f = fixture("saves", None).await;

    let mut paths = Vec::new();
    for i in 0..5 {
        fs::write(f.saves.join("profile.sav"), format!("state {i}")).unwrap();
        let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
        assert!(
            backup.archive_path.is_file(),
            "archive {i} should exist on disk"
        );
        paths.push(backup.archive_path);
    }

    let unique: std::collections::HashSet<_> = paths.iter().collect();
    assert_eq!(
        unique.len(),
        paths.len(),
        "each backup must have its own archive: {paths:?}"
    );

    // Every archive must still hold the state it captured, not a later one.
    for (i, path) in paths.iter().enumerate() {
        let out = f._dir.child(&format!("check{i}"));
        read_archive(path, &out).unwrap();
        assert_eq!(
            fs::read(out.join("profile.sav")).unwrap(),
            format!("state {i}").into_bytes(),
            "archive {i} was overwritten by a later backup"
        );
    }
}

#[tokio::test]
async fn restore_refuses_when_the_save_location_is_not_a_directory() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();

    // A misconfigured profile pointing at a file. Replacing it with a directory
    // would be silent, irreversible data loss.
    let saves = f.saves.clone();
    fs::remove_dir_all(&saves).unwrap();
    fs::write(&saves, b"not a directory").unwrap();

    let err = f.manager.restore(backup.backup_id).await.unwrap_err();
    assert!(
        format!("{err}").contains("not a directory"),
        "{err}"
    );
    assert_eq!(fs::read(&saves).unwrap(), b"not a directory");
    assert_eq!(
        f.db.list_backups(&f.profile_id).await.unwrap().len(),
        1,
        "no safety backup should have been claimed"
    );
}

#[tokio::test]
async fn restore_refuses_when_the_safety_backup_cannot_be_taken() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();

    // Make the safety backup fail while the target is still a real directory:
    // an unsatisfiable filter is rejected before any archiving happens, which is
    // a backup failure the restore must treat as fatal.
    sqlx::query("UPDATE save_profiles SET glob = '[unclosed' WHERE id = ?1")
        .bind(&f.profile_id)
        .execute(&f.db.pool)
        .await
        .unwrap();

    let err = f.manager.restore(backup.backup_id).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("refusing to restore"), "{msg}");
    assert!(
        msg.contains("has not been touched"),
        "the message must reassure the user their data is intact: {msg}"
    );

    // The live saves are exactly as they were, and no restore happened.
    assert_eq!(fs::read(f.saves.join("profile.sav")).unwrap(), b"slot one");
    assert_eq!(
        f.db.list_backups(&f.profile_id).await.unwrap().len(),
        1,
        "a failed safety backup must not be recorded"
    );
    let siblings: Vec<String> = fs::read_dir(f.saves.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("gvprev") || n.contains("gvrestore"))
        .collect();
    assert!(siblings.is_empty(), "nothing should have been staged: {siblings:?}");
}

#[tokio::test]
async fn restore_reports_a_missing_archive_without_touching_the_saves() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
    fs::remove_file(&backup.archive_path).unwrap();

    let err = f.manager.restore(backup.backup_id).await.unwrap_err();
    assert!(format!("{err}").contains("archive is missing"), "{err}");
    // No safety backup was taken, because nothing was attempted.
    assert_eq!(f.db.list_backups(&f.profile_id).await.unwrap().len(), 1);
    assert_eq!(fs::read(f.saves.join("profile.sav")).unwrap(), b"slot one");
}

/// A corrupt archive must fail before the live folder is touched at all.
#[tokio::test]
async fn a_corrupt_archive_leaves_the_live_saves_intact() {
    let f = fixture("saves", None).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();

    let mut bytes = fs::read(&backup.archive_path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    fs::write(&backup.archive_path, &bytes).unwrap();

    fs::write(f.saves.join("profile.sav"), b"current state").unwrap();
    let err = f.manager.restore(backup.backup_id).await.unwrap_err();
    // Which check fires depends on where the damaged byte lands — a corrupt
    // entry path is caught as invalid UTF-8, a corrupt payload by the checksum.
    // The property under test is that *some* validation refuses it and the live
    // folder is untouched, not which one.
    assert!(
        matches!(err, AppError::SaveMgr(_)),
        "a corrupt archive must be refused: {err:?}"
    );

    assert_eq!(
        fs::read(f.saves.join("profile.sav")).unwrap(),
        b"current state",
        "the live saves must be exactly as they were"
    );
    let siblings: Vec<String> = fs::read_dir(f.saves.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("gvprev") || n.contains("gvrestore"))
        .collect();
    assert!(siblings.is_empty(), "a refused restore must stage nothing: {siblings:?}");
}

#[tokio::test]
async fn backup_reports_a_missing_source_directory() {
    let f = fixture("saves", None).await;
    fs::remove_dir_all(&f.saves).unwrap();
    let err = f.manager.backup(&f.profile_id, None).await.unwrap_err();
    assert!(format!("{err}").contains("does not exist"), "{err}");
}

#[tokio::test]
async fn backup_rejects_an_invalid_glob_before_doing_any_work() {
    let f = fixture("saves", Some("[unclosed")).await;
    let err = f.manager.backup(&f.profile_id, None).await.unwrap_err();
    assert!(matches!(err, AppError::Invalid(_)), "got {err:?}");
    assert_eq!(f.db.list_backups(&f.profile_id).await.unwrap().len(), 0);
}

#[tokio::test]
async fn a_profile_glob_is_applied_to_the_backup() {
    let f = fixture("saves", Some("*.sav")).await;
    let backup = f.manager.backup(&f.profile_id, None).await.unwrap();
    assert_eq!(backup.file_count, 1, "only the top-level .sav file");
}
