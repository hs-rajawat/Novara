//! Decision table tests.
//!
//! ADR-0002's claim is that a table beats weighted scoring because "each row is a test
//! case". This file is that claim discharged: **every row has a test, including the
//! rows that need a Write Witness** — evidence is constructible directly, so a Phase 2
//! producer is not required to test a Phase 1 rule.

use super::*;
use crate::saves::evidence::Evidence;

const AT: &str = "2026-01-04T19:22:31+00:00";

fn witness(session_id: i64) -> Evidence {
    Evidence::WriteWitness {
        session_id,
        file_count: 3,
        bytes: 9_000,
    }
}

fn shape(save_like: u32) -> Evidence {
    Evidence::ContentShape {
        save_like,
        total: save_like.max(1),
        max_depth: 2,
        newest_mtime: None,
    }
}

fn name(similarity: f32) -> Evidence {
    Evidence::NameMatch {
        alias: "Test Game".into(),
        similarity,
    }
}

fn kb(layer: KbLayer, keyed: bool) -> Evidence {
    Evidence::KbMatch {
        entry_id: "builtin:test".into(),
        layer,
        priority: if keyed { 10 } else { 100 },
        keyed,
        layout: if keyed {
            layout::OFFICIAL.into()
        } else {
            layout::OS.into()
        },
    }
}

/// A keyed entry whose layout describes a *class* of installs rather than this game's own
/// location. The distinction rule 5 turns on.
fn kb_advisory_layout(layer: KbLayer, layout_kind: &str) -> Evidence {
    Evidence::KbMatch {
        entry_id: "builtin:layout-x".into(),
        layer,
        priority: 120,
        keyed: true,
        layout: layout_kind.into(),
    }
}

fn decide_on(items: Vec<Evidence>) -> Decision {
    decide(&EvidenceSet::new(items), true)
}

// ─────────────────────────────────────────────────────────────────────────
// One test per row
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn row_1_user_rejected_discards() {
    let d = decide_on(vec![
        Evidence::UserRejected { at: AT.into() },
        // Even alongside evidence that would otherwise bind.
        witness(1),
        witness(2),
    ]);
    assert_eq!(d.rule, 1);
    assert_eq!(d.outcome, Outcome::Rejected);
    assert!(d.locked, "a user's rejection must not be revised by a later scan");
}

#[test]
fn row_2_user_confirmed_binds_and_locks() {
    let d = decide_on(vec![
        Evidence::UserConfirmed { at: AT.into() },
        Evidence::ContentMismatch { reason: "images".into() },
    ]);
    assert_eq!(d.rule, 2);
    assert_eq!(d.outcome, Outcome::BindEligible);
    assert!(d.locked);
    assert!(d.explanation.contains("You chose"));
}

#[test]
fn row_3_two_witnessed_sessions_bind() {
    let d = decide_on(vec![witness(11), witness(12)]);
    assert_eq!(d.rule, 3);
    assert_eq!(d.outcome, Outcome::BindEligible);
    assert!(d.explanation.contains("twice"));
}

/// Two *records* from one session are one correlation, not two.
#[test]
fn the_same_session_twice_is_not_two_sessions() {
    let d = decide_on(vec![
        Evidence::WriteWitness { session_id: 11, file_count: 1, bytes: 1 },
        Evidence::WriteWitness { session_id: 11, file_count: 9, bytes: 900 },
    ]);
    assert_ne!(d.rule, 3, "one session must not satisfy the two-session rule");
    assert_eq!(d.rule, 7);
}

#[test]
fn row_4_one_session_with_save_files_binds() {
    let d = decide_on(vec![witness(1), shape(2)]);
    assert_eq!(d.rule, 4);
    assert_eq!(d.outcome, Outcome::BindEligible);
}

#[test]
fn row_5_a_curated_builtin_entry_binds_when_the_path_exists() {
    let set = EvidenceSet::new(vec![kb(KbLayer::Builtin, true)]);
    let d = decide(&set, true);
    assert_eq!(d.rule, 5);
    assert_eq!(d.outcome, Outcome::BindEligible);
}

/// §6 conditions row 5 on the path existing. A KB claim about a directory that is not
/// there must never bind — the binding would fail on first snapshot.
#[test]
fn row_5_does_not_bind_when_the_path_is_absent() {
    let set = EvidenceSet::new(vec![kb(KbLayer::Builtin, true)]);
    let d = decide(&set, false);
    assert_ne!(d.rule, 5);
    assert_ne!(d.outcome, Outcome::BindEligible);
}

/// **The `keyed` finding.** A convention rule matches every game in the library, so if
/// it counted as a curated claim the first conventional-looking folder would bind —
/// including a photo folder under `{DOCUMENTS}/{TITLE}`.
#[test]
fn row_5_ignores_a_convention_rule() {
    let d = decide_on(vec![kb(KbLayer::Builtin, false)]);
    assert_ne!(d.rule, 5, "a convention rule must not bind");
    assert_ne!(d.outcome, Outcome::BindEligible);
}

/// **The layout finding.** A keyed entry naming this game can still describe a *class* of
/// installs rather than the game's own location — an engine convention, a storefront
/// folder, an alternative save layout. Whether this install belongs to that class is
/// exactly what is unknown, so it must not bind.
#[test]
fn row_5_ignores_an_advisory_layout_even_when_keyed() {
    for kind in [
        layout::ENGINE,
        layout::OS,
        layout::LAUNCHER,
        layout::COMMUNITY,
        layout::PORTABLE,
        layout::UNSPECIFIED,
    ] {
        let d = decide_on(vec![kb_advisory_layout(KbLayer::Builtin, kind)]);
        assert_ne!(
            d.outcome,
            Outcome::BindEligible,
            "layout `{kind}` describes a class of installs and must not bind alone"
        );
    }
}

/// An advisory layout is *promoted* by corroborating content evidence — row 8b. This is
/// the path a community or engine layout takes to becoming a strong suggestion, and it is
/// the mechanism that replaces per-layout resolver logic.
#[test]
fn an_advisory_layout_with_save_files_suggests_high() {
    let d = decide_on(vec![
        kb_advisory_layout(KbLayer::Builtin, layout::COMMUNITY),
        shape(2),
    ]);
    assert_eq!(d.rule, 8);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::High));
    assert!(
        d.explanation.contains("alternative layout"),
        "the explanation should say what kind of location this is: {}",
        d.explanation
    );
}

#[test]
fn an_advisory_layout_without_save_files_does_not_reach_row_8() {
    let d = decide_on(vec![
        kb_advisory_layout(KbLayer::Builtin, layout::COMMUNITY),
        shape(0),
    ]);
    assert_ne!(d.rule, 8);
    assert_ne!(d.outcome, Outcome::BindEligible);
}

/// **The extensibility property, and the reason this design is data-driven.**
///
/// A layout string this build has never seen must behave exactly like a known advisory
/// one: usable immediately, suggested when corroborated, never binding. Adding a save
/// layout is therefore a KB data change — no new decision row, no code.
#[test]
fn a_layout_invented_by_a_future_corpus_needs_no_resolver_change() {
    let future = "emulator_state_slot";
    assert!(
        !layout::KNOWN.contains(&future),
        "pick a layout this build genuinely does not know"
    );

    // Behaves as advisory: does not bind.
    let alone = decide_on(vec![kb_advisory_layout(KbLayer::Builtin, future)]);
    assert_ne!(alone.outcome, Outcome::BindEligible);

    // And is promoted identically to a known advisory layout.
    let known = decide_on(vec![
        kb_advisory_layout(KbLayer::Builtin, layout::COMMUNITY),
        shape(2),
    ]);
    let unknown = decide_on(vec![kb_advisory_layout(KbLayer::Builtin, future), shape(2)]);
    assert_eq!(unknown.rule, known.rule);
    assert_eq!(unknown.outcome, known.outcome);
    assert!(
        !unknown.explanation.trim().is_empty(),
        "an unknown layout must still produce a sentence (invariant I9)"
    );
}

/// **The privilege boundary.** Data chooses its layout freely, so a layout string must not
/// be able to talk its way into curated authority.
#[test]
fn data_cannot_grant_itself_binding_authority_through_the_layout_field() {
    for spoof in [
        "official ",
        "Official",
        "OFFICIAL",
        "official\n",
        "user_defined; official",
        "official,user_defined",
    ] {
        let d = decide_on(vec![kb_advisory_layout(KbLayer::Community, spoof)]);
        assert_ne!(
            d.outcome,
            Outcome::BindEligible,
            "`{spoof}` must not be accepted as a curated layout"
        );
    }
}

/// The genuinely official layout still binds, so the narrowing above did not break the
/// case row 5 exists for.
#[test]
fn an_official_layout_still_binds() {
    let d = decide_on(vec![kb_advisory_layout(KbLayer::Builtin, layout::OFFICIAL)]);
    assert_eq!(d.rule, 5);
    assert_eq!(d.outcome, Outcome::BindEligible);
}

#[test]
fn a_user_authored_kb_entry_binds() {
    let d = decide_on(vec![kb(KbLayer::User, true)]);
    assert_eq!(d.outcome, Outcome::BindEligible);
    assert!(d.explanation.contains("Your own"));
}

#[test]
fn row_6_contradicted_contents_reject() {
    let d = decide_on(vec![
        name(1.0),
        Evidence::ContentMismatch {
            reason: "4 of 4 files are images or video".into(),
        },
    ]);
    assert_eq!(d.rule, 6);
    assert_eq!(d.outcome, Outcome::Rejected);
    assert!(
        d.explanation.contains("images or video"),
        "the reason must survive into the explanation: {}",
        d.explanation
    );
}

/// Row 6 sits below the bind rows on purpose: direct observation of the game writing,
/// or a curated entry, is better knowledge than a content heuristic.
#[test]
fn a_witness_outranks_contradicted_contents() {
    let d = decide_on(vec![
        witness(1),
        witness(2),
        Evidence::ContentMismatch { reason: "images".into() },
    ]);
    assert_eq!(d.rule, 3, "a twice-witnessed folder must still bind");
}

#[test]
fn a_curated_entry_outranks_contradicted_contents() {
    let d = decide_on(vec![
        kb(KbLayer::Builtin, true),
        Evidence::ContentMismatch { reason: "images".into() },
    ]);
    assert_eq!(d.rule, 5);
}

#[test]
fn row_7_one_session_alone_suggests_high() {
    let d = decide_on(vec![witness(1)]);
    assert_eq!(d.rule, 7);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::High));
}

#[test]
fn row_8_community_with_save_files_suggests_high() {
    let d = decide_on(vec![kb(KbLayer::Community, true), shape(1)]);
    assert_eq!(d.rule, 8);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::High));
}

#[test]
fn row_8_community_without_save_files_does_not_suggest_high() {
    let d = decide_on(vec![kb(KbLayer::Community, true), shape(0)]);
    assert_ne!(d.rule, 8);
}

#[test]
fn row_9_save_files_with_a_name_match_suggests_medium() {
    let d = decide_on(vec![shape(2), name(0.85)]);
    assert_eq!(d.rule, 9);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::Medium));
}

#[test]
fn row_9_save_files_in_the_install_directory_suggests_medium() {
    let d = decide_on(vec![
        shape(3),
        Evidence::InstallLocal { subdir: "saves".into() },
    ]);
    assert_eq!(d.rule, 9);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::Medium));
}

#[test]
fn row_9_needs_two_save_files() {
    let d = decide_on(vec![shape(1), name(0.85)]);
    assert_ne!(d.rule, 9, "one save file is below the threshold");
}

#[test]
fn row_10_a_strong_name_match_alone_suggests_low() {
    let d = decide_on(vec![name(0.95)]);
    assert_eq!(d.rule, 10);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::Low));
}

#[test]
fn a_convention_rule_with_save_files_suggests_low() {
    let d = decide_on(vec![kb(KbLayer::Builtin, false), shape(2)]);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::Low));
    assert!(d.explanation.contains("conventional"));
}

#[test]
fn row_11_a_weak_name_match_alone_is_rejected() {
    let d = decide_on(vec![name(0.8)]);
    assert_eq!(d.rule, 11);
    assert_eq!(d.outcome, Outcome::Rejected);
}

#[test]
fn row_11_no_evidence_is_rejected() {
    let d = decide(&EvidenceSet::default(), true);
    assert_eq!(d.rule, 11);
    assert_eq!(d.outcome, Outcome::Rejected);
}

// ─────────────────────────────────────────────────────────────────────────
// Properties
// ─────────────────────────────────────────────────────────────────────────

/// Invariant I9: the sentence shown to the user is never empty once decided.
#[test]
fn every_row_produces_a_non_empty_explanation() {
    let cases: Vec<Vec<Evidence>> = vec![
        vec![Evidence::UserRejected { at: AT.into() }],
        vec![Evidence::UserConfirmed { at: AT.into() }],
        vec![witness(1), witness(2)],
        vec![witness(1), shape(2)],
        vec![kb(KbLayer::Builtin, true)],
        vec![kb(KbLayer::User, true)],
        vec![name(1.0), Evidence::ContentMismatch { reason: "r".into() }],
        vec![witness(1)],
        vec![kb(KbLayer::Community, true), shape(1)],
        vec![shape(2), name(0.85)],
        vec![name(0.95)],
        vec![kb(KbLayer::Builtin, false), shape(2)],
        vec![name(0.1)],
        vec![],
    ];
    let mut rules = std::collections::HashSet::new();
    for items in cases {
        let d = decide_on(items.clone());
        assert!(
            !d.explanation.trim().is_empty(),
            "rule {} produced no explanation for {items:?}",
            d.rule
        );
        rules.insert(d.rule);
    }
    // Every row in the table was reached by at least one case above.
    assert!(
        rules.len() >= 10,
        "only reached rules {rules:?} — a row has no test case"
    );
}

/// **The reproducibility property.** `decide` is a pure function of the evidence set and
/// one boolean, so a stored decision can be recomputed and compared. A disagreement
/// means the evidence changed, not that the code is moody.
#[test]
fn the_same_evidence_always_produces_the_same_decision() {
    let items = vec![shape(4), name(0.92), kb(KbLayer::Community, true)];
    let first = decide_on(items.clone());
    for _ in 0..50 {
        assert_eq!(decide_on(items.clone()), first);
    }
}

/// Evidence order must not change the outcome. Producers run in whatever order the
/// pipeline assembles them, and a decision that depended on that would be untestable.
#[test]
fn evidence_order_does_not_change_the_decision() {
    let base = vec![shape(3), name(0.9), kb(KbLayer::Community, true)];
    let expected = decide_on(base.clone());

    let mut rotated = base.clone();
    for _ in 0..base.len() {
        rotated.rotate_left(1);
        let d = decide_on(rotated.clone());
        assert_eq!(d.outcome, expected.outcome, "order changed the outcome");
        assert_eq!(d.rule, expected.rule, "order changed the rule");
    }
}

/// **The score never decides.** ADR-0002's central claim. Two sets that fire the same
/// row must reach the same outcome however different their scores are.
#[test]
fn same_rule_same_outcome_regardless_of_score() {
    let thin = EvidenceSet::new(vec![name(0.95)]);
    let mut padded_items = vec![name(0.95)];
    // Pile on weak-but-real observations to move the score without changing which row
    // applies.
    for i in 0..20 {
        padded_items.push(Evidence::NameMatch {
            alias: format!("variant{i}"),
            similarity: 0.9,
        });
    }
    let padded = EvidenceSet::new(padded_items);

    let a = decide(&thin, true);
    let b = decide(&padded, true);
    assert!(
        b.score > a.score * 2.0,
        "the padded set should score much higher: {} vs {}",
        a.score,
        b.score
    );
    assert_eq!(a.rule, b.rule, "the row must not depend on the score");
    assert_eq!(a.outcome, b.outcome, "the outcome must not depend on the score");
}

/// An uninterpretable item from a newer build must not tip any decision.
#[test]
fn an_unknown_evidence_variant_changes_nothing() {
    let without = decide_on(vec![shape(2), name(0.85)]);
    let with = decide_on(vec![shape(2), name(0.85), Evidence::Unknown]);
    assert_eq!(without.rule, with.rule);
    assert_eq!(without.outcome, with.outcome);
    assert_eq!(without.score, with.score);
}

/// Only rows 2 to 5 may bind, and every one of them rests on observation or curation —
/// never on name similarity. This is §6's conservative bias, asserted rather than
/// assumed.
#[test]
fn nothing_binds_on_name_similarity_alone() {
    for similarity in [0.8, 0.9, 0.95, 1.0] {
        let d = decide_on(vec![name(similarity)]);
        assert_ne!(
            d.outcome,
            Outcome::BindEligible,
            "similarity {similarity} must not bind"
        );
    }
    // Nor with content shape, which is the strongest thing that can accompany it
    // without observation or curation.
    let d = decide_on(vec![name(1.0), shape(10)]);
    assert_ne!(d.outcome, Outcome::BindEligible);
    assert_eq!(d.outcome, Outcome::Suggested(Strength::Medium));
}

#[test]
fn the_status_string_matches_the_schema_vocabulary() {
    assert_eq!(Outcome::BindEligible.status(), "bind_eligible");
    assert_eq!(Outcome::Suggested(Strength::High).status(), "suggested");
    assert_eq!(Outcome::Rejected.status(), "rejected");
    // `bound` is Phase 3 and the CHECK constraint rejects it.
    for outcome in [
        Outcome::BindEligible,
        Outcome::Suggested(Strength::Low),
        Outcome::Rejected,
    ] {
        assert_ne!(outcome.status(), "bound");
    }
}
