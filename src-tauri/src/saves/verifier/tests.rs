//! Verifier tests.
//!
//! Every heuristic gets three cases: one where it fires, one where it must *not*, and
//! one adversarial input designed to make it misfire. The adversarial cases are the
//! point — a content heuristic that has only ever seen tidy fixtures will reject real
//! save folders.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;
use crate::test_support::VirtualFs;

const DIR: &str = "C:/Users/test/Documents/Game";
/// An arbitrary fixed instant, so tests never depend on the wall clock.
const T0: u64 = 1_770_000_000;

fn at(epoch: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(epoch)
}

fn assess(fs: &VirtualFs) -> Assessment {
    verify(fs, Path::new(DIR), None)
}

fn assess_played(fs: &VirtualFs, played: u64) -> Assessment {
    verify(fs, Path::new(DIR), Some(at(played)))
}

/// A directory holding ordinary-looking saves.
fn save_folder() -> VirtualFs {
    VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/slot0.sav"), 240_000, T0)
        .with_file_at(&format!("{DIR}/slot1.sav"), 238_000, T0 + 30)
        .with_file_at(&format!("{DIR}/settings.ini"), 1_200, T0 + 31)
}

// ─────────────────────────────────────────────────────────────────────────
// Structural guarantees
// ─────────────────────────────────────────────────────────────────────────

/// The verifier cannot propose a directory, only characterise one. Enforced by the
/// signature — this test records the intent so a future change of return type has to
/// argue with it.
#[test]
fn an_assessment_carries_no_paths() {
    let a = assess(&save_folder());
    // `Assessment` is a shape plus signals. If a path ever appears in it, this test is
    // the place that should stop compiling.
    let _: &DirectoryShape = &a.shape;
    let _: &Vec<Signal> = &a.signals;
    assert!(a.shape.files_seen > 0);
}

#[test]
fn a_disqualifying_signal_is_never_also_supporting() {
    let all = [
        Signal::SaveLikeExtensions { count: 1, of: 1 },
        Signal::WriteBurst { span_secs: 1, files: 2 },
        Signal::PlayedAtCorrelation {
            closeness: Closeness::Tight,
            delta_secs: 1,
        },
        Signal::NoFilesAtAll,
        Signal::LooksLikeInstallDirectory { executables: 1 },
        Signal::LooksLikeMediaFolder { media: 4, of: 4 },
        Signal::LooksLikeCache { files: 900 },
        Signal::LooksLikeMarkerDirectory { files: 3 },
    ];
    let a = Assessment {
        shape: DirectoryShape::default(),
        signals: all.to_vec(),
    };
    let supporting = a.supporting().count();
    let disqualifying = a.signals.iter().filter(|s| s.is_contradiction()).count();
    assert_eq!(supporting + disqualifying, all.len(), "every signal must pick a side");
    assert_eq!(supporting, 3);
}

// ─────────────────────────────────────────────────────────────────────────
// Extension histogram
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn save_like_extensions_are_recognised() {
    let a = assess(&save_folder());
    assert!(
        a.supporting()
            .any(|s| matches!(s, Signal::SaveLikeExtensions { count: 3, .. })),
        "expected save-like extensions, got {:?}",
        a.signals
    );
    assert!(!a.contradicts_saves());
}

#[test]
fn a_directory_of_unknown_extensions_gets_no_save_signal() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/thing.qqq"), 5_000, T0)
        .with_file_at(&format!("{DIR}/other.zzz"), 5_000, T0);
    let a = assess(&fs);
    assert!(
        !a.signals
            .iter()
            .any(|s| matches!(s, Signal::SaveLikeExtensions { .. })),
        "got {:?}",
        a.signals
    );
    // Unknown is not disqualifying — plenty of games use bespoke extensions.
    assert!(!a.contradicts_saves(), "unknown extensions must not reject: {:?}", a.signals);
}

/// Adversarial: extension matching must be on the final component and case-folded, not
/// a substring search. A file called `notes.savage.txt` is not a save.
#[test]
fn extension_matching_is_not_a_substring_search() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/notes.savage.txt"), 900, T0)
        .with_file_at(&format!("{DIR}/readme.database"), 900, T0);
    let a = assess(&fs);
    assert!(
        !a.signals
            .iter()
            .any(|s| matches!(s, Signal::SaveLikeExtensions { .. })),
        "`savage`/`database` are not save extensions: {:?}",
        a.signals
    );
}

#[test]
fn extension_matching_is_case_insensitive() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/SLOT0.SAV"), 1_000, T0);
    let a = assess(&fs);
    assert!(a
        .signals
        .iter()
        .any(|s| matches!(s, Signal::SaveLikeExtensions { .. })));
}

// ─────────────────────────────────────────────────────────────────────────
// Install-directory rejection
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_directory_of_executables_is_rejected() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/Game.exe"), 24_000_000, T0)
        .with_file_at(&format!("{DIR}/engine.dll"), 8_000_000, T0)
        .with_file_at(&format!("{DIR}/content.pak"), 900_000_000, T0);
    let a = assess(&fs);
    assert!(matches!(
        a.contradiction(),
        Some(Signal::LooksLikeInstallDirectory { .. })
    ));
}

/// **Adversarial, and the case that matters most.** A game that keeps its saves in its
/// own install directory has both a `.exe` and real saves. Rejecting it would blank
/// out precisely the portable population task 1.16 exists to serve.
#[test]
fn an_install_directory_that_also_holds_saves_is_not_rejected() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/Game.exe"), 24_000_000, T0)
        .with_file_at(&format!("{DIR}/engine.dll"), 8_000_000, T0)
        .with_file_at(&format!("{DIR}/slot0.sav"), 140_000, T0);
    let a = assess(&fs);
    assert!(
        !a.contradicts_saves(),
        "a folder with saves alongside binaries must survive: {:?}",
        a.signals
    );
}

/// Adversarial: a single stray `.dll` beside real saves — a mod loader, an
/// anti-cheat shim — must not reject.
#[test]
fn one_stray_library_beside_saves_does_not_reject() {
    let fs = save_folder().with_file_at(&format!("{DIR}/modloader.dll"), 400_000, T0);
    let a = assess(&fs);
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
}

// ─────────────────────────────────────────────────────────────────────────
// Media-folder rejection
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_folder_of_screenshots_is_rejected() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..6 {
        fs = fs.with_file_at(&format!("{DIR}/shot_{i}.jpg"), 4_000_000, T0 + i);
    }
    let a = assess(&fs);
    assert!(
        matches!(a.contradiction(), Some(Signal::LooksLikeMediaFolder { .. })),
        "got {:?}",
        a.signals
    );
}

/// A stray junk file must not defeat the check — hence a ratio rather than purity.
#[test]
fn a_screenshot_folder_with_one_junk_file_is_still_rejected() {
    let mut fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/Thumbs.db.tmp"), 900, T0);
    for i in 0..9 {
        fs = fs.with_file_at(&format!("{DIR}/shot_{i}.png"), 3_000_000, T0 + i);
    }
    assert!(matches!(
        assess(&fs).contradiction(),
        Some(Signal::LooksLikeMediaFolder { .. })
    ));
}

/// **Adversarial.** Some games write screenshots into the save folder. Save-like files
/// present means the directory keeps its evidence and survives.
#[test]
fn screenshots_alongside_saves_do_not_reject() {
    let mut fs = save_folder();
    for i in 0..12 {
        fs = fs.with_file_at(&format!("{DIR}/shot_{i}.jpg"), 3_000_000, T0 + i);
    }
    let a = assess(&fs);
    assert!(
        !a.contradicts_saves(),
        "a save folder that also holds screenshots must survive: {:?}",
        a.signals
    );
}

/// Adversarial: a handful of media files below the dominance ratio must not reject.
#[test]
fn a_minority_of_media_files_does_not_reject() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..3 {
        fs = fs.with_file_at(&format!("{DIR}/shot_{i}.jpg"), 3_000_000, T0 + i);
    }
    for i in 0..7 {
        fs = fs.with_file_at(&format!("{DIR}/blob_{i}.qqq"), 50_000, T0 + i);
    }
    let a = assess(&fs);
    assert!(!a.contradicts_saves(), "3 of 10 media is below the ratio: {:?}", a.signals);
}

// ─────────────────────────────────────────────────────────────────────────
// Empty, cache and marker directories
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_empty_directory_is_rejected() {
    let fs = VirtualFs::new().with_dir_tree(DIR);
    assert!(matches!(assess(&fs).contradiction(), Some(Signal::NoFilesAtAll)));
}

/// A directory whose files live one level down is **not** empty. `Terraria/` holds only
/// `Players/` and `Worlds/`, and rejecting it would be a real regression.
#[test]
fn a_directory_whose_files_are_one_level_down_is_not_empty() {
    let fs = VirtualFs::new()
        .with_dir_tree(&format!("{DIR}/Players"))
        .with_file_at(&format!("{DIR}/Players/hero.plr"), 12_000, T0)
        .with_file_at(&format!("{DIR}/Players/hero.sav"), 12_000, T0);
    let a = assess(&fs);
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
    assert_eq!(a.shape.files_seen, 2);
}

#[test]
fn a_directory_of_hundreds_of_files_is_rejected_as_a_cache() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..(bounds::VERIFIER_MANY_FILES + 20) {
        fs = fs.with_file(&format!("{DIR}/entry_{i}.dat"), 4_096);
    }
    let a = assess(&fs);
    assert!(
        matches!(a.contradiction(), Some(Signal::LooksLikeCache { .. })),
        "got {:?}",
        a.contradiction()
    );
}

/// Adversarial: a large but plausible save folder — a hundred numbered slots — must
/// not be called a cache.
#[test]
fn a_hundred_save_slots_is_not_a_cache() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..100 {
        fs = fs.with_file(&format!("{DIR}/slot_{i}.sav"), 200_000);
    }
    let a = assess(&fs);
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
}

#[test]
fn a_directory_of_tiny_files_is_a_marker_directory() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/a.lock"), 0, T0)
        .with_file_at(&format!("{DIR}/b.lock"), 4, T0)
        .with_file_at(&format!("{DIR}/desktop.ini"), 12, T0);
    assert!(matches!(
        assess(&fs).contradiction(),
        Some(Signal::LooksLikeMarkerDirectory { .. })
    ));
}

/// Adversarial: one real save among the markers means it is not a marker directory.
#[test]
fn one_real_file_stops_it_being_a_marker_directory() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/a.lock"), 0, T0)
        .with_file_at(&format!("{DIR}/save.dat"), 90_000, T0);
    let a = assess(&fs);
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
}

// ─────────────────────────────────────────────────────────────────────────
// Write burst
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn files_written_together_are_a_write_burst() {
    let a = assess(&save_folder());
    assert!(
        a.supporting().any(|s| matches!(s, Signal::WriteBurst { .. })),
        "got {:?}",
        a.signals
    );
}

#[test]
fn files_written_months_apart_are_not_a_burst() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/old.sav"), 10_000, T0)
        .with_file_at(&format!("{DIR}/new.sav"), 10_000, T0 + 90 * 86_400);
    let a = assess(&fs);
    assert!(
        !a.signals.iter().any(|s| matches!(s, Signal::WriteBurst { .. })),
        "got {:?}",
        a.signals
    );
    // ...and that absence is not a rejection.
    assert!(!a.contradicts_saves());
}

/// Adversarial: a single file cannot be a burst, and asking must not panic.
#[test]
fn a_single_file_is_not_a_burst() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/only.sav"), 10_000, T0);
    let a = assess(&fs);
    assert!(!a.signals.iter().any(|s| matches!(s, Signal::WriteBurst { .. })));
}

/// Adversarial: a filesystem that reports no mtimes at all must not produce a burst
/// signal or a panic.
#[test]
fn a_filesystem_without_mtimes_produces_no_time_signals() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file(&format!("{DIR}/a.sav"), 10_000)
        .with_file(&format!("{DIR}/b.sav"), 10_000);
    let a = assess_played(&fs, T0);
    assert!(a.shape.newest.is_none());
    assert!(!a
        .signals
        .iter()
        .any(|s| matches!(s, Signal::WriteBurst { .. } | Signal::PlayedAtCorrelation { .. })));
    assert!(!a.contradicts_saves());
}

// ─────────────────────────────────────────────────────────────────────────
// 1.18 mtime correlation with last_played_at
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_write_during_the_last_session_correlates_tightly() {
    let a = assess_played(&save_folder(), T0 + 60);
    assert!(
        matches!(
            a.supporting().find(|s| matches!(s, Signal::PlayedAtCorrelation { .. })),
            Some(Signal::PlayedAtCorrelation {
                closeness: Closeness::Tight,
                ..
            })
        ),
        "got {:?}",
        a.signals
    );
}

#[test]
fn correlation_weakens_with_distance() {
    let cases = [
        (T0 + 30 * 60, Closeness::Tight),
        (T0 + 6 * 3_600, Closeness::Loose),
        (T0 + 3 * 86_400, Closeness::Distant),
    ];
    for (played, expected) in cases {
        let a = assess_played(&save_folder(), played);
        let found = a
            .signals
            .iter()
            .find_map(|s| match s {
                Signal::PlayedAtCorrelation { closeness, .. } => Some(*closeness),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no correlation signal for played={played}"));
        assert_eq!(found, expected, "for played={played}");
    }
}

#[test]
fn a_write_a_year_before_the_session_does_not_correlate() {
    let a = assess_played(&save_folder(), T0 + 365 * 86_400);
    assert!(
        !a.signals
            .iter()
            .any(|s| matches!(s, Signal::PlayedAtCorrelation { .. })),
        "got {:?}",
        a.signals
    );
    // Absence of correlation must never reject: a game may not write every launch.
    assert!(!a.contradicts_saves());
}

#[test]
fn no_play_history_produces_no_correlation_and_no_rejection() {
    let a = assess(&save_folder());
    assert!(!a
        .signals
        .iter()
        .any(|s| matches!(s, Signal::PlayedAtCorrelation { .. })));
    assert!(!a.contradicts_saves());
}

/// **Adversarial: clock skew.** A save written *after* the recorded session — a
/// background sync, a wrong system clock, a timezone bug upstream — must read as the
/// same distance rather than underflowing or panicking.
#[test]
fn a_save_newer_than_the_session_still_correlates() {
    // Session recorded an hour *before* the newest write.
    let a = assess_played(&save_folder(), T0 - 1_800);
    let found = a.signals.iter().find_map(|s| match s {
        Signal::PlayedAtCorrelation { closeness, .. } => Some(*closeness),
        _ => None,
    });
    assert_eq!(found, Some(Closeness::Tight), "got {:?}", a.signals);
}

/// Adversarial: an mtime before the Unix epoch, which `duration_since` reports as an
/// error in both directions.
#[test]
fn a_prehistoric_mtime_does_not_panic() {
    let fs = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/a.sav"), 10_000, 0);
    let a = assess_played(&fs, T0);
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
}

// ─────────────────────────────────────────────────────────────────────────
// Bounds
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn metadata_reads_are_capped() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..300 {
        fs = fs.with_file_at(&format!("{DIR}/f{i}.sav"), 100_000, T0 + i as u64);
    }
    let a = assess(&fs);
    assert_eq!(
        a.shape.sampled,
        bounds::VERIFIER_MAX_METADATA_READS,
        "metadata calls must stop at the ceiling"
    );
    // ...while classification, which is free, covered everything.
    assert_eq!(a.shape.files_seen, 300);
    assert_eq!(a.shape.save_like, 300);
}

#[test]
fn the_walk_does_not_exceed_its_depth() {
    // Three levels below the candidate: beyond VERIFIER_MAX_DEPTH of 2.
    let deep = format!("{DIR}/a/b/c");
    let fs = VirtualFs::new()
        .with_dir_tree(&deep)
        .with_file_at(&format!("{deep}/buried.sav"), 10_000, T0);
    let a = assess(&fs);
    assert_eq!(
        a.shape.files_seen, 0,
        "a file three levels down must not be counted"
    );
    assert!(a.shape.truncated, "hitting the depth limit must be recorded");
    // A truncated walk must not reject, even though it saw no files.
    assert!(
        !a.contradicts_saves(),
        "an incomplete walk must never reject: {:?}",
        a.signals
    );
}

#[test]
fn a_file_exactly_at_the_depth_limit_is_counted() {
    let ok = format!("{DIR}/a/b");
    let fs = VirtualFs::new()
        .with_dir_tree(&ok)
        .with_file_at(&format!("{ok}/reachable.sav"), 10_000, T0);
    assert_eq!(assess(&fs).shape.files_seen, 1);
}

/// Regression: a directory so large the walk truncates must **still** be rejected as a
/// cache.
///
/// The count is a floor once truncated, and a floor already past the threshold is
/// conclusive. This was originally checked behind the completeness gate, which had it
/// backwards — 420 files were rejected while 40,000 were not, because the bigger walk
/// truncated and truncation forbade rejecting. The more cache-like the directory, the
/// more it got away with.
#[test]
fn a_cache_too_large_to_walk_fully_is_still_rejected() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..(bounds::VERIFIER_MAX_ENTRIES + 500) {
        fs = fs.with_file(&format!("{DIR}/blob{i}.tmp"), 4_096);
    }
    let a = assess(&fs);
    assert!(a.shape.truncated, "this many entries must truncate the walk");
    assert!(
        matches!(a.contradiction(), Some(Signal::LooksLikeCache { .. })),
        "a truncated walk over a huge directory must still reject: {:?}",
        a.signals
    );
}

/// The other half: truncation must still block the signals that rest on *absence*.
#[test]
fn truncation_still_blocks_absence_based_rejections() {
    // Enough entries to truncate, none of them save-like, plus an executable. If the
    // completeness gate were removed, this would be rejected as an install directory on
    // the strength of a partial view.
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    fs = fs.with_file(&format!("{DIR}/Game.exe"), 24_000_000);
    for i in 0..(bounds::VERIFIER_MAX_ENTRIES + 10) {
        fs = fs.with_file(&format!("{DIR}/asset{i}.qqq"), 9_000);
    }
    let a = assess(&fs);
    assert!(a.shape.truncated);
    assert!(
        !a.signals
            .iter()
            .any(|s| matches!(s, Signal::LooksLikeInstallDirectory { .. })),
        "an absence-based rejection must not fire on a partial view: {:?}",
        a.signals
    );
}

#[test]
fn the_total_entry_count_is_capped() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..(bounds::VERIFIER_MAX_ENTRIES + 200) {
        fs = fs.with_file(&format!("{DIR}/f{i}.dat"), 5_000);
    }
    let a = assess(&fs);
    assert!(
        a.shape.files_seen <= bounds::VERIFIER_MAX_ENTRIES,
        "walked {} entries, ceiling is {}",
        a.shape.files_seen,
        bounds::VERIFIER_MAX_ENTRIES
    );
    assert!(a.shape.truncated);
}

/// **Adversarial: an unreadable directory must not be reported as empty.** Reading a
/// permission-denied folder and concluding "no saves here" would silently discard a
/// real candidate.
#[test]
fn an_unreadable_directory_is_not_treated_as_empty() {
    // The directory is never declared, so `read_dir` fails.
    let fs = VirtualFs::new();
    let a = verify(&fs, Path::new(DIR), None);
    assert!(a.shape.unreadable);
    assert!(
        !a.contradicts_saves(),
        "an unreadable directory must not be rejected: {:?}",
        a.signals
    );
}

/// A `Cache/` subfolder inside a real save folder must not drag the assessment towards
/// "cache" — engine noise is skipped, not counted.
#[test]
fn engine_noise_subdirectories_are_skipped() {
    let mut fs = save_folder().with_dir_tree(&format!("{DIR}/Cache"));
    for i in 0..50 {
        fs = fs.with_file(&format!("{DIR}/Cache/blob{i}.tmp"), 900);
    }
    let a = assess(&fs);
    assert_eq!(a.shape.files_seen, 3, "cache contents must not be counted");
    assert!(!a.contradicts_saves(), "got {:?}", a.signals);
}

// ─────────────────────────────────────────────────────────────────────────
// Cost
// ─────────────────────────────────────────────────────────────────────────

/// The claim that matters for performance: **nothing the verifier does scales with the
/// size of a save file.**
///
/// Two directories with identical structure but wildly different byte counts must cost
/// the same number of filesystem operations, because the verifier reads sizes from
/// metadata and never opens a file. Asserted through `sampled`, which counts
/// `metadata()` calls — the only per-file syscall in the walk.
#[test]
fn cost_is_independent_of_file_size() {
    let small = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/a.sav"), 1_024, T0)
        .with_file_at(&format!("{DIR}/b.sav"), 2_048, T0);
    let huge = VirtualFs::new()
        .with_dir_tree(DIR)
        .with_file_at(&format!("{DIR}/a.sav"), 8_000_000_000, T0)
        .with_file_at(&format!("{DIR}/b.sav"), 12_000_000_000, T0);

    let a = assess(&small);
    let b = assess(&huge);
    assert_eq!(a.shape.sampled, b.shape.sampled);
    assert_eq!(a.shape.files_seen, b.shape.files_seen);
    assert_eq!(a.shape.sampled, 2, "one stat per file, regardless of size");
}

/// Classification is free; only sizes and mtimes cost a syscall. This pins the split,
/// which is the whole performance argument for the design.
#[test]
fn classification_covers_more_files_than_are_sampled() {
    let mut fs = VirtualFs::new().with_dir_tree(DIR);
    for i in 0..200 {
        fs = fs.with_file(&format!("{DIR}/f{i}.sav"), 50_000);
    }
    let a = assess(&fs);
    assert_eq!(a.shape.save_like, 200, "every name was classified");
    assert_eq!(
        a.shape.sampled,
        bounds::VERIFIER_MAX_METADATA_READS,
        "only the sampled subset cost a syscall"
    );
    assert!(a.shape.sampled < a.shape.save_like);
}

/// Measured against a real filesystem rather than `VirtualFs`, because the thing worth
/// knowing is syscall cost and the virtual filesystem has none.
///
/// Ignored by default so the suite stays fast and deterministic. Run with:
/// `cargo test --lib verifier::tests::measure -- --ignored --nocapture`
#[test]
#[ignore = "measurement, not an assertion; run explicitly"]
fn measure_real_filesystem_cost() {
    use crate::saves::fs::RealFs;
    use crate::test_support::TempDir;

    let temp = TempDir::new("verifier-bench");
    let fs = RealFs;

    /// Build a directory, verify it, and report.
    fn case(fs: &RealFs, root: &std::path::Path, label: &str, files: usize, big: bool) -> Duration {
        let dir = root.join(label);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..files {
            std::fs::write(dir.join(format!("slot{i}.sav")), vec![0u8; 4_096]).unwrap();
        }
        if big {
            std::fs::write(dir.join("big.sav"), vec![0u8; 32 * 1024 * 1024]).unwrap();
        }

        // Warm the directory cache, then take the best of three: the interesting number
        // is steady-state syscall cost, not first-touch disk latency.
        let _ = verify(fs, &dir, None);
        let mut best = Duration::from_secs(9999);
        let mut shape = DirectoryShape::default();
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let a = verify(fs, &dir, None);
            best = best.min(started.elapsed());
            shape = a.shape;
        }
        println!(
            "  {label:<22} files={:<5} sampled={:<3} truncated={:<5} {best:?}",
            shape.files_seen, shape.sampled, shape.truncated
        );
        best
    }

    println!("\nverifier cost, one candidate directory:");
    let typical = case(&fs, temp.path(), "typical-12-files", 12, false);
    let large = case(&fs, temp.path(), "large-500-files", 500, false);
    let with_big = case(&fs, temp.path(), "500-plus-32MB-file", 500, true);
    let over_cap = case(&fs, temp.path(), "over-entry-cap", 2_200, false);

    println!(
        "\n  worst case per game = {} candidates x {:?} = {:?}",
        bounds::VERIFIER_MAX_CANDIDATES_PER_GAME,
        large,
        large * bounds::VERIFIER_MAX_CANDIDATES_PER_GAME as u32
    );

    // The claim under test: a 32 MB file costs no more than a 4 KB one, because no file
    // is ever opened. Allowed a wide margin — this is a timing comparison on a shared
    // machine, not a microbenchmark.
    assert!(
        with_big < large * 3,
        "a large file should not change the cost: {large:?} vs {with_big:?}"
    );
    assert!(
        typical < Duration::from_millis(50),
        "a typical save folder took {typical:?}"
    );
    assert!(
        over_cap < Duration::from_millis(500),
        "the entry cap should keep a huge directory bounded, took {over_cap:?}"
    );
}
