//! Pipeline tests.
//!
//! The pipeline assembles observations and calls the decision table. These tests hold
//! the two properties that layering depends on: the producers below can only *narrow*
//! the candidate set, and every outcome is attributable to a decision-table row rather
//! than to something the pipeline decided on its own.

use super::*;
use crate::models::SaveKbEntry;
use crate::saves::fs::RootKind;
use crate::saves::resolver::Strength;
use crate::test_support::VirtualFs;

const HOME: &str = "C:/Users/test";
const T0: u64 = 1_770_000_000;

fn world() -> VirtualFs {
    VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
        .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
        .with_root(RootKind::AppDataLocalLow, &format!("{HOME}/AppData/LocalLow"))
        .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
        .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
        .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
}

fn norm(paths: &[DetectedPath]) -> Vec<String> {
    paths.iter().map(|c| c.path.replace('\\', "/")).collect()
}

/// Detection with no knowledge base, which is the locator-only path.
fn detect_bare(fs: &VirtualFs, title: &str) -> DetectionOutcome {
    detect(fs, &GameContext::new(title), &[])
}

fn kb_entry(id: &str, layer: &str, match_kind: &str, value: &str, template: &str) -> SaveKbEntry {
    SaveKbEntry {
        id: id.into(),
        layer: layer.into(),
        match_kind: match_kind.into(),
        match_value: value.into(),
        platform: "windows".into(),
        role: "saves".into(),
        path_template: template.into(),
        glob: None,
        priority: if match_kind == "any" { 100 } else { 10 },
        note: None,
        source_ref: Some("test".into()),
        kb_version: "test".into(),
        created_at: "2026-01-01T00:00:00+00:00".into(),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The verifier is additive
// ─────────────────────────────────────────────────────────────────────────

/// **The structural property of task 1.17, still true after task 1.22.** Whatever the
/// verifier concludes, the candidate set is a subset of what the producers found — the
/// verifier can contribute evidence that leads to a rejection, never a new path.
#[test]
fn verification_never_adds_a_candidate() {
    let good = format!("{HOME}/Documents/My Games/Test Game");
    let bad = format!("{HOME}/Documents/Test Game");
    let fs = world()
        .with_dir_tree(&good)
        .with_file_at(&format!("{good}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{good}/slot1.sav"), 118_000, T0 + 10)
        .with_dir_tree(&bad)
        .with_file_at(&format!("{bad}/shot.jpg"), 4_000_000, T0)
        .with_file_at(&format!("{bad}/shot2.jpg"), 4_000_000, T0);

    let from_locator = norm(&crate::saves::locator::detect(&fs, "Test Game"));
    let outcome = detect_bare(&fs, "Test Game");
    let after = norm(&outcome.candidates);

    assert!(!from_locator.is_empty(), "the locator should have found something");
    for path in &after {
        assert!(
            from_locator.contains(path),
            "`{path}` was not produced by the locator: verification invented a candidate"
        );
    }
    assert!(
        after.len() < from_locator.len(),
        "expected the media folder to be dropped; before {from_locator:?} after {after:?}"
    );
}

/// A rejection is attributable: it names the row that produced it and carries the
/// reason. Silent drops are the hardest detection bug to diagnose, because a missing
/// candidate looks identical to one the locator never found.
#[test]
fn a_rejection_names_its_rule_and_reason() {
    let dir = format!("{HOME}/Documents/Riverbound");
    let mut fs = world().with_dir_tree(&dir);
    for i in 0..5 {
        fs = fs.with_file_at(&format!("{dir}/shot_{i}.jpg"), 4_000_000, T0 + i);
    }

    let outcome = detect_bare(&fs, "Riverbound");
    assert!(outcome.candidates.is_empty(), "got {:?}", norm(&outcome.candidates));
    assert_eq!(outcome.rejected.len(), 1);

    let rejection = &outcome.rejected[0];
    assert_eq!(rejection.rule, 6, "the content-mismatch row should have fired");
    assert!(
        rejection.reason.contains("images or video"),
        "the verifier's reason must survive into the explanation: {}",
        rejection.reason
    );
}

/// The verifier no longer decides. The same world must produce a rejection whose
/// authority is a table row, and the evidence must record the observation separately
/// from the decision.
#[test]
fn a_content_contradiction_is_stored_as_evidence_not_a_verdict() {
    let dir = format!("{HOME}/Documents/Riverbound");
    let mut fs = world().with_dir_tree(&dir);
    for i in 0..5 {
        fs = fs.with_file_at(&format!("{dir}/shot_{i}.jpg"), 4_000_000, T0 + i);
    }

    let outcome = detect_bare(&fs, "Riverbound");
    let assessed = outcome.assessed.first().expect("one candidate considered");
    assert!(
        assessed
            .evidence
            .has(|e| matches!(e, Evidence::ContentMismatch { .. })),
        "the observation must be in the evidence set: {:?}",
        assessed.evidence.items
    );
    assert!(
        assessed
            .evidence
            .has(|e| matches!(e, Evidence::ContentShape { .. })),
        "the descriptive shape must be recorded too"
    );
    assert_eq!(assessed.decision.outcome, Outcome::Rejected);
}

#[test]
fn a_plausible_save_folder_survives_verification() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{dir}/slot1.sav"), 118_000, T0 + 20);

    let outcome = detect_bare(&fs, "Test Game");
    assert_eq!(norm(&outcome.candidates), vec![dir]);
    assert!(outcome.rejected.is_empty());
    assert_eq!(
        outcome.assessed[0].decision.outcome,
        Outcome::Suggested(Strength::Medium),
        "save files plus a name match is row 9"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Knowledge-base integration (task 1.22)
// ─────────────────────────────────────────────────────────────────────────

/// A KB entry finds a folder the locator cannot: the directory is named nothing like
/// the game.
#[test]
fn a_curated_kb_entry_contributes_a_candidate_the_locator_would_miss() {
    let dir = format!("{HOME}/AppData/Roaming/ObscureStudios/SOR");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/profile.sav"), 84_000, T0);

    let ctx = GameContext {
        title: "Some Obscure RPG".into(),
        steam_appid: Some("424242".into()),
        ..Default::default()
    };
    let entries = [kb_entry(
        "builtin:sor",
        "builtin",
        "steam_appid",
        "424242",
        "{APPDATA}/ObscureStudios/SOR",
    )];

    // The locator alone finds nothing.
    assert!(
        crate::saves::locator::detect(&fs, &ctx.title).is_empty(),
        "the folder name should be unreachable by the locator"
    );

    let outcome = detect(&fs, &ctx, &entries);
    assert_eq!(norm(&outcome.candidates), vec![dir]);
    assert_eq!(outcome.assessed[0].decision.rule, 5);
    assert_eq!(outcome.assessed[0].decision.outcome, Outcome::BindEligible);
}

/// Evidence from two producers about one path merges into a single candidate rather
/// than appearing twice.
#[test]
fn a_path_found_by_both_producers_is_one_candidate_with_both_observations() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0);

    let entries = [kb_entry(
        "builtin:convention",
        "builtin",
        "any",
        "",
        "{MYGAMES}/{TITLE}",
    )];
    let outcome = detect(&fs, &GameContext::new("Test Game"), &entries);

    assert_eq!(outcome.assessed.len(), 1, "one path, one candidate");
    let evidence = &outcome.assessed[0].evidence;
    assert!(evidence.has(|e| matches!(e, Evidence::NameMatch { .. })));
    assert!(evidence.has(|e| matches!(e, Evidence::KbMatch { .. })));
}

/// A convention rule must not bind. It matches every game in the library, so treating
/// it as a curated claim would bind the first conventional-looking folder that existed.
#[test]
fn a_convention_rule_does_not_bind() {
    let dir = format!("{HOME}/Documents/My Games/Wanderlight");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 96_000, T0)
        .with_file_at(&format!("{dir}/slot1.sav"), 94_000, T0 + 5);

    let entries = [kb_entry(
        "builtin:convention",
        "builtin",
        "any",
        "",
        "{MYGAMES}/{TITLE}",
    )];
    let outcome = detect(&fs, &GameContext::new("Wanderlight"), &entries);

    assert_eq!(outcome.bind_eligible().count(), 0, "a convention rule must not bind");
    assert_eq!(norm(&outcome.candidates), vec![dir]);
}

// ─────────────────────────────────────────────────────────────────────────
// Determinism and ordering
// ─────────────────────────────────────────────────────────────────────────

/// `VirtualFs` iterates a `HashSet`, so listing order varies between runs. The pipeline
/// must produce the same ordered result regardless — the scenario corpus depends on it.
#[test]
fn the_result_is_deterministic() {
    let a = format!("{HOME}/Documents/My Games/Test Game");
    let b = format!("{HOME}/AppData/Roaming/Test Game");
    let c = format!("{HOME}/Documents/Test Game");
    let mut fs = world();
    for dir in [&a, &b, &c] {
        fs = fs
            .with_dir_tree(dir)
            .with_file_at(&format!("{dir}/slot0.sav"), 90_000, T0)
            .with_file_at(&format!("{dir}/slot1.sav"), 90_000, T0 + 3);
    }

    let first = norm(&detect_bare(&fs, "Test Game").candidates);
    assert_eq!(first.len(), 3);
    for _ in 0..20 {
        assert_eq!(norm(&detect_bare(&fs, "Test Game").candidates), first);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Verification ceiling
// ─────────────────────────────────────────────────────────────────────────

/// Directories that all match the title closely enough to be suggested on name alone.
///
/// The title is deliberately long: similarity is `1 - distance/length`, so a two-letter
/// suffix on a 28-character name still scores 0.93 and clears row 10's 0.9 threshold.
/// A short title would drop these below it and the test would measure the wrong thing.
const LONG_TITLE: &str = "Chronicles of the Wandering Star";

fn many_similar_dirs(count: usize) -> Vec<String> {
    let letters = [
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
        't', 'u', 'w', 'y', 'z',
    ];
    let mut names = Vec::with_capacity(count);
    for x in letters {
        for y in letters {
            if names.len() == count {
                return names;
            }
            names.push(format!("{LONG_TITLE} {x}{y}"));
        }
    }
    names
}

/// **Not having looked is not an observation.**
///
/// Every directory here is empty. Those the verifier examined produce a
/// `ContentMismatch` and are rejected by row 6. Those past the ceiling produce no
/// content evidence at all, so the table judges them on name similarity and row 10
/// suggests them.
///
/// The point is that the two groups are decided by *different rules for different
/// reasons* — the pipeline does not extend a content judgement to directories it never
/// inspected, and it does not silently drop them either.
#[test]
fn candidates_beyond_the_verification_ceiling_are_judged_on_what_is_known() {
    let ceiling = bounds::VERIFIER_MAX_CANDIDATES_PER_GAME;
    let names = many_similar_dirs(ceiling + 8);
    let mut fs = world();
    for name in &names {
        fs = fs.with_dir(&format!("{HOME}/Documents/{name}"));
    }

    let outcome = detect_bare(&fs, LONG_TITLE);
    assert_eq!(
        outcome.assessed.len(),
        names.len(),
        "every directory should have been considered"
    );

    let verified_rejections = outcome
        .assessed
        .iter()
        .filter(|a| a.decision.rule == 6)
        .count();
    let unverified_suggestions = outcome
        .assessed
        .iter()
        .filter(|a| a.decision.rule == 10)
        .count();

    assert_eq!(
        verified_rejections, ceiling,
        "exactly the verified prefix should be rejected for its contents"
    );
    assert_eq!(
        unverified_suggestions,
        names.len() - ceiling,
        "the unverified remainder should be judged on name evidence alone"
    );

    // And none of the unverified ones carry content evidence they never earned.
    for a in outcome.assessed.iter().filter(|a| a.decision.rule == 10) {
        assert!(
            !a.evidence
                .has(|e| matches!(e, Evidence::ContentShape { .. })),
            "`{}` was never inspected but carries content evidence",
            a.path
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Timestamp handling
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_unparseable_last_played_at_does_not_break_detection() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{dir}/slot1.sav"), 119_000, T0 + 2);

    let ctx = GameContext {
        title: "Test Game".into(),
        last_played_at: Some("not a timestamp".into()),
        ..Default::default()
    };
    assert_eq!(norm(&detect(&fs, &ctx, &[]).candidates), vec![dir]);
}

#[test]
fn timestamps_are_parsed_defensively() {
    assert!(system_time_from_rfc3339("2026-01-04T19:22:31+00:00").is_some());
    assert!(system_time_from_rfc3339("").is_none());
    // Before the Unix epoch: representable as a timestamp, not as our `SystemTime`.
    assert!(system_time_from_rfc3339("1960-01-01T00:00:00+00:00").is_none());
}
