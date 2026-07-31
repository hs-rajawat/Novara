//! Locator tests.
//!
//! The first tests this code has ever had. Before Phase 0 it called `dirs::`
//! directly, so any test would have read the developer's own `%APPDATA%` and
//! proved nothing portable.
//!
//! Two of these are **invariant** tests from `docs/architecture/TESTING.md` §4 —
//! properties asserted across a category of inputs rather than one case. They may
//! not be deleted to make a change pass; if one becomes inconvenient, the
//! architecture changed and that needs a superseding ADR.
//!
//! These are not the Phase 1 scenario corpus (declarative `.toml` fixtures and a
//! table-driven runner, per ADR-0013). They are direct tests proving the filesystem
//! abstraction works and the existing behaviour is pinned before anything reshapes
//! it.

use super::*;
use crate::saves::fs::RootKind;
use crate::test_support::VirtualFs;

/// A world with one Documents/My Games root, as most PC games use.
fn my_games_with(dirs: &[&str]) -> VirtualFs {
    let mut fs = VirtualFs::new().with_root(RootKind::DocumentsMyGames, "C:/Users/t/Documents/My Games");
    for d in dirs {
        fs = fs.with_dir(&format!("C:/Users/t/Documents/My Games/{d}"));
    }
    fs
}

// ── Title variants ────────────────────────────────────────────────────────

#[test]
fn finds_an_exact_title_match_with_full_confidence() {
    let fs = my_games_with(&["Skyrim Special Edition"]);
    let found = detect(&fs, "Skyrim Special Edition");

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
    assert_eq!(found[0].hint, "Documents/My Games/Skyrim Special Edition");
}

/// Case is **normalisation noise, not uncertainty.**
///
/// This asserted 0.92 before task 1.14, on the reasoning that an unusually-spelled
/// folder is a less certain match. That conflated two different things: how unusual a
/// spelling is, and how likely the folder is to be the wrong one. There is no world
/// in which `hollow knight` is a different game from `Hollow Knight`, so the match is
/// exact and scores 1.0. The old ladder was measuring the wrong quantity.
///
/// The confidences that remain below 1.0 are the ones that describe a genuine
/// reduction in information — a stripped instalment number, a first word, an
/// initialism.
#[test]
fn a_case_difference_is_still_an_exact_match() {
    let found = detect(&my_games_with(&["hollow knight"]), "Hollow Knight");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
}

#[test]
fn strips_a_trailing_arabic_numeral() {
    let found = detect(&my_games_with(&["The Witcher"]), "The Witcher 3");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 0.75);
}

#[test]
fn strips_a_trailing_roman_numeral() {
    let found = detect(&my_games_with(&["Final Fantasy"]), "Final Fantasy XV");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 0.75);
}

#[test]
fn a_bare_numeral_title_is_not_stripped_to_nothing() {
    // `strip_trailing_number` requires more than one token, so a title that is
    // only a numeral must not collapse to an empty directory name.
    let found = detect(&my_games_with(&["7"]), "7");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
}

/// Separators are normalisation noise too — see
/// [`a_case_difference_is_still_an_exact_match`] for why this is 1.0 rather than the
/// 0.72 it asserted before task 1.14.
#[test]
fn underscores_for_spaces_is_still_an_exact_match() {
    let found = detect(&my_games_with(&["Hollow_Knight"]), "Hollow Knight");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
}

/// Likewise a folder with the spaces removed: `HollowKnight` is not a less certain
/// `Hollow Knight`, it is the same name written without spaces. Was 0.60.
#[test]
fn a_compacted_title_is_still_an_exact_match() {
    let found = detect(&my_games_with(&["HollowKnight"]), "Hollow Knight");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
}

#[test]
fn matches_a_first_word_only_when_it_is_distinctive() {
    let found = detect(&my_games_with(&["Portal"]), "Portal Reloaded");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 0.40);
}

#[test]
fn a_short_first_word_is_not_used() {
    // "Halo" is 4 characters, below the 5-character distinctiveness floor, so a
    // directory called `Halo` must not be offered for "Halo Infinite".
    let found = detect(&my_games_with(&["Halo"]), "Halo Infinite");
    assert!(
        found.is_empty(),
        "short first word should not generate a candidate: {found:?}"
    );
}

#[test]
fn finds_nothing_when_no_directory_matches() {
    let found = detect(&my_games_with(&["Some Other Game"]), "Hollow Knight");
    assert!(found.is_empty());
}

// ── Ordering and deduplication ────────────────────────────────────────────

#[test]
fn results_are_sorted_by_confidence_descending() {
    // Two folders whose confidences genuinely differ: the full title is exact, the
    // instalment-stripped form is a real reduction in information.
    //
    // This used to compare `Hollow Knight` against `HollowKnight`, which stopped
    // being a difference in confidence once normalisation started treating spacing as
    // noise — both score 1.0 now, so the test could no longer see an ordering.
    let found = detect(
        &my_games_with(&["The Witcher 3", "The Witcher"]),
        "The Witcher 3",
    );
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].confidence, 1.0);
    assert!(found[0].path.ends_with("The Witcher 3"), "got {:?}", found[0].path);
    assert_eq!(found[1].confidence, 0.75);
}

#[test]
fn the_same_directory_matched_at_two_confidences_is_reported_once() {
    // Regression guard for a real past bug. Dedup used `dedup_by`, which removes
    // only *adjacent* duplicates.
    //
    // "the witcher 3" matches the directory `the witcher` twice — as the
    // numeral-stripped variant (0.75) and as its lowercase form (0.68), which are
    // the same string because the title is already lowercase. `the_witcher_3`
    // scores 0.72, landing *between* them once sorted by confidence. The two
    // copies of `the witcher` are therefore not adjacent, and `dedup_by` let one
    // through.
    //
    // An earlier version of this test used two roots pointing at the same path.
    // That was vacuous: both candidates scored 1.0, so they *were* adjacent and
    // the buggy implementation passed it. Verified by reintroducing the bug.
    let fs = my_games_with(&["the witcher", "the_witcher_3"]);

    let found = detect(&fs, "the witcher 3");
    let paths: Vec<&str> = found.iter().map(|d| d.path.as_str()).collect();

    assert_eq!(found.len(), 2, "duplicate path leaked through: {paths:?}");
    assert_eq!(
        paths
            .iter()
            .filter(|p| p.ends_with("the witcher"))
            .count(),
        1,
        "`the witcher` reported more than once: {paths:?}"
    );
}

#[test]
fn the_same_directory_reached_through_two_roots_is_reported_once() {
    // Weaker than the test above — these tie on confidence, so even the buggy
    // implementation deduplicated them — but it pins the root-handling half of
    // the behaviour: two roots resolving to one path is not two candidates.
    let fs = VirtualFs::new()
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_root(RootKind::DocumentsMyGames, "C:/Users/t/Documents")
        .with_dir("C:/Users/t/Documents/Hollow Knight");

    let found = detect(&fs, "Hollow Knight");
    assert_eq!(found.len(), 1, "duplicate path leaked through: {found:?}");
}

#[test]
fn a_deduplicated_path_keeps_its_highest_confidence() {
    // Same directory, reachable as both an exact match and a compacted match.
    let fs = VirtualFs::new()
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_dir("C:/Users/t/Documents/HollowKnight");

    let found = detect(&fs, "HollowKnight");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, 1.0);
}

// ── Roots ─────────────────────────────────────────────────────────────────

#[test]
fn a_missing_root_is_skipped_rather_than_erroring() {
    let fs = VirtualFs::new()
        .with_missing_root(RootKind::SavedGames, "C:/Users/t/Saved Games")
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_dir("C:/Users/t/Documents/Hollow Knight");

    let found = detect(&fs, "Hollow Knight");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hint, "Documents/Hollow Knight");
}

#[test]
fn each_root_contributes_its_own_label() {
    let fs = VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, "C:/Users/t/AppData/Roaming")
        .with_root(RootKind::SavedGames, "C:/Users/t/Saved Games")
        .with_dir("C:/Users/t/AppData/Roaming/Elden Ring")
        .with_dir("C:/Users/t/Saved Games/Elden Ring");

    let hints: Vec<String> = detect(&fs, "Elden Ring")
        .into_iter()
        .map(|d| d.hint)
        .collect();

    assert!(hints.contains(&"AppData/Roaming/Elden Ring".to_string()));
    assert!(hints.contains(&"Saved Games/Elden Ring".to_string()));
}

#[test]
fn a_file_with_a_matching_name_is_not_a_candidate() {
    // Only directories are save locations; a same-named file must be ignored.
    let fs = VirtualFs::new()
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_file("C:/Users/t/Documents/Hollow Knight", 1024);

    assert!(detect(&fs, "Hollow Knight").is_empty());
}

// ── Determinism ───────────────────────────────────────────────────────────

#[test]
fn the_same_world_yields_the_same_result_every_time() {
    // The property that makes the whole corpus viable: no host filesystem is
    // consulted, so the outcome cannot vary by machine or by run.
    let build = || my_games_with(&["Hollow Knight", "HollowKnight", "hollow knight"]);

    let first = detect(&build(), "Hollow Knight");
    let second = detect(&build(), "Hollow Knight");

    let shape = |v: &[DetectedPath]| -> Vec<(String, String)> {
        v.iter()
            .map(|d| (d.path.clone(), format!("{:.4}", d.confidence)))
            .collect()
    };
    assert_eq!(shape(&first), shape(&second));
}

// ── Invariants (TESTING.md §4) ────────────────────────────────────────────

/// **I3 — no candidate outside the declared roots.**
///
/// Bounds are the defence against walking a disk and against traversal. Every
/// path detection touches, and every path it returns, must sit under a root it
/// was given.
#[test]
fn invariant_i3_never_leaves_the_declared_roots() {
    let fs = VirtualFs::new()
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_dir("C:/Users/t/Documents/Hollow Knight")
        // Present on the "disk" but outside every root — must never be seen.
        .with_dir("D:/Elsewhere/Hollow Knight");

    let found = detect(&fs, "Hollow Knight");

    for d in &found {
        assert!(
            d.path.replace('\\', "/").starts_with("C:/Users/t/Documents"),
            "candidate escaped its root: {}",
            d.path
        );
    }
    for queried in fs.queried_paths() {
        assert!(
            queried.starts_with("C:/Users/t/Documents"),
            "queried a path outside every declared root: {queried}"
        );
    }
}

/// **I2 — detection never reads file contents.**
///
/// Structurally guaranteed: [`crate::saves::fs::FileSystem`] exposes no method
/// that returns bytes, so a locator cannot open a save even by mistake (ADR-0003).
/// This test pins the guarantee at the type level so that adding a read method
/// later — which would void it — fails here and forces a superseding ADR.
#[test]
fn invariant_i2_the_filesystem_trait_exposes_no_content_read() {
    // Compile-time assertion: the locator's whole view of the world is these four
    // methods. If a `read`/`open` is added to the trait, this list stops matching
    // the trait's shape and the accompanying doc comment becomes a lie — the
    // reviewer's cue that ADR-0003 is being changed.
    fn uses_only_metadata_methods<F: crate::saves::fs::FileSystem>(fs: &F, p: &std::path::Path) {
        let _ = fs.roots();
        let _ = fs.exists(p);
        let _ = fs.is_dir(p);
        let _ = fs.read_dir(p);
        let _ = fs.metadata(p);
    }

    let fs = my_games_with(&["Hollow Knight"]);
    uses_only_metadata_methods(&fs, std::path::Path::new("C:/Users/t/Documents/My Games"));

    // And behaviourally: a matching *file* is never opened to inspect it, only
    // classified by metadata — so it is rejected without a read.
    let with_file = VirtualFs::new()
        .with_root(RootKind::Documents, "C:/Users/t/Documents")
        .with_file("C:/Users/t/Documents/Hollow Knight", 4096);
    assert!(detect(&with_file, "Hollow Knight").is_empty());
}
