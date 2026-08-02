//! Evidence model tests.

use super::*;

fn name(alias: &str, similarity: f32) -> Evidence {
    Evidence::NameMatch {
        alias: alias.into(),
        similarity,
    }
}

fn kb(id: &str, layer: KbLayer, keyed: bool) -> Evidence {
    Evidence::KbMatch {
        entry_id: id.into(),
        layer,
        priority: 10,
        keyed,
        layout: if keyed {
            crate::saves::kb::layout::OFFICIAL.into()
        } else {
            crate::saves::kb::layout::OS.into()
        },
    }
}

fn shape(save_like: u32, total: u32) -> Evidence {
    Evidence::ContentShape {
        save_like,
        total,
        max_depth: 2,
        newest_mtime: None,
    }
}

// ── Round trip and version tolerance ─────────────────────────────────────

#[test]
fn a_set_round_trips_through_json() {
    let set = EvidenceSet::new(vec![
        name("Hollow Knight", 1.0),
        kb("builtin:hollow-knight", KbLayer::Builtin, true),
        shape(3, 5),
    ]);
    let parsed = EvidenceSet::parse(&set.to_json());
    assert_eq!(parsed, set);
    assert_eq!(parsed.schema, SCHEMA_VERSION);
}

/// The migration requires this: "An unknown variant from a newer build must
/// deserialise without error so a downgrade is survivable."
#[test]
fn an_unknown_variant_from_a_newer_build_is_preserved_not_rejected() {
    let raw = r#"{"schema":1,"items":[
        {"kind":"name_match","alias":"X","similarity":1.0},
        {"kind":"quantum_witness","qubits":7}
    ]}"#;
    let parsed = EvidenceSet::parse(raw);
    assert_eq!(parsed.items.len(), 2, "the unknown item must not be dropped");
    assert_eq!(parsed.items[1], Evidence::Unknown);
}

/// An unknown variant must contribute nothing to any judgement — it is uninterpretable
/// by definition.
#[test]
fn an_unknown_variant_carries_no_weight() {
    assert_eq!(Evidence::Unknown.ordering_weight(), 0.0);
}

/// A corrupt column degrades one candidate's explanation, never a library scan.
#[test]
fn malformed_json_parses_to_an_empty_set() {
    for bad in ["", "not json", "{}", "[]", r#"{"schema":1}"#] {
        let parsed = EvidenceSet::parse(bad);
        assert!(parsed.items.is_empty(), "`{bad}` should yield an empty set");
        assert_eq!(parsed.schema, SCHEMA_VERSION);
    }
}

// ── Provenance ───────────────────────────────────────────────────────────

/// Each item must name its own source, or a decision cannot be traced back.
#[test]
fn every_item_names_its_source() {
    let set = EvidenceSet::new(vec![
        name("Witcher 3", 0.9),
        kb("builtin:the-witcher-3", KbLayer::Builtin, true),
        Evidence::InstallLocal { subdir: "saves".into() },
        shape(2, 4),
        Evidence::ContentMismatch { reason: "images".into() },
        Evidence::WriteWitness { session_id: 7, file_count: 2, bytes: 100 },
        Evidence::UserConfirmed { at: "2026-01-01T00:00:00+00:00".into() },
    ]);
    let sources = set.explain();
    assert_eq!(sources.len(), 7);
    assert!(sources.contains(&"kb:builtin:the-witcher-3".to_string()));
    assert!(sources.contains(&"name:Witcher 3".to_string()));
    assert!(sources.contains(&"install:saves".to_string()));
    for s in &sources {
        assert!(!s.is_empty(), "an unattributable observation: {sources:?}");
    }
}

// ── Append-only merge ────────────────────────────────────────────────────

#[test]
fn merging_the_same_observation_twice_does_not_duplicate_it() {
    let mut set = EvidenceSet::new(vec![name("X", 1.0)]);
    set.merge(vec![name("X", 1.0)]);
    assert_eq!(set.items.len(), 1);
}

/// **The point of append-only.** A changed observation is a *different* observation. If
/// merging replaced it, the provenance of a decision already made would silently
/// disappear.
#[test]
fn a_changed_observation_is_added_not_replaced() {
    let mut set = EvidenceSet::new(vec![name("X", 0.8)]);
    set.merge(vec![name("X", 0.95)]);
    assert_eq!(set.items.len(), 2, "history must be kept: {:?}", set.items);
    assert!(set.items.contains(&name("X", 0.8)));
    assert!(set.items.contains(&name("X", 0.95)));
}

#[test]
fn merging_preserves_existing_items() {
    let mut set = EvidenceSet::new(vec![
        Evidence::WriteWitness { session_id: 1, file_count: 3, bytes: 900 },
    ]);
    set.merge(vec![name("X", 1.0), shape(2, 3)]);
    assert!(
        set.has(|e| matches!(e, Evidence::WriteWitness { session_id: 1, .. })),
        "an old witness is still evidence: {:?}",
        set.items
    );
    assert_eq!(set.items.len(), 3);
}

/// Eviction must be by strength, not age. Chronological eviction would lose a
/// `UserConfirmed` to a flood of name matches, which is exactly backwards.
#[test]
fn eviction_drops_the_weakest_not_the_oldest() {
    let confirmed = Evidence::UserConfirmed {
        at: "2020-01-01T00:00:00+00:00".into(),
    };
    let mut set = EvidenceSet::new(vec![confirmed.clone()]);

    // Flood with distinct weak observations, well past the cap.
    let flood: Vec<Evidence> = (0..MAX_ITEMS + 40)
        .map(|i| name(&format!("alias{i}"), 0.75))
        .collect();
    set.merge(flood);

    assert_eq!(set.items.len(), MAX_ITEMS, "the cap must be applied");
    assert!(
        set.items.contains(&confirmed),
        "the strongest item was evicted"
    );
}

#[test]
fn eviction_is_deterministic() {
    let build = || {
        let mut s = EvidenceSet::default();
        s.merge(
            (0..MAX_ITEMS + 10)
                .map(|i| name(&format!("a{i}"), 0.8))
                .collect(),
        );
        s
    };
    assert_eq!(build(), build());
}

// ── Ordering weight is not a verdict ─────────────────────────────────────

/// §5.3's ranking, asserted as an ordering rather than as specific numbers. Asserting
/// the numbers would make this a change-detector; asserting the order captures the
/// claim.
#[test]
fn ordering_weights_follow_the_documented_signal_strength() {
    let ranked = [
        Evidence::UserConfirmed { at: "x".into() },
        Evidence::WriteWitness { session_id: 1, file_count: 1, bytes: 1 },
        kb("k", KbLayer::Builtin, true),
        kb("k", KbLayer::Community, true),
        Evidence::InstallLocal { subdir: "saves".into() },
        name("a", 1.0),
        Evidence::Unknown,
    ];
    for pair in ranked.windows(2) {
        assert!(
            pair[0].ordering_weight() > pair[1].ordering_weight(),
            "{:?} should outweigh {:?}",
            pair[0],
            pair[1]
        );
    }
}

/// The finding that made `keyed` necessary: a convention rule is not a curated claim,
/// and must not carry a curated claim's weight.
#[test]
fn a_convention_rule_weighs_far_less_than_a_curated_entry() {
    let curated = kb("builtin:celeste", KbLayer::Builtin, true);
    let convention = kb("builtin:convention-my-games", KbLayer::Builtin, false);
    assert!(
        convention.ordering_weight() < curated.ordering_weight() / 3.0,
        "convention {} vs curated {}",
        convention.ordering_weight(),
        curated.ordering_weight()
    );
    // ...and barely more than a name match, which is what it actually is.
    assert!(convention.ordering_weight() < name("a", 1.0).ordering_weight() * 2.0);
}

#[test]
fn content_shape_weight_saturates() {
    let two = shape(2, 2).ordering_weight();
    let twenty = shape(20, 20).ordering_weight();
    let none = shape(0, 5).ordering_weight();
    assert!(none < two, "some save files should beat none");
    assert!(twenty > two, "more should not be worth less");
    assert!(
        twenty - two < two - none,
        "the twentieth save file should add less than the second"
    );
}

#[test]
fn an_empty_set_scores_zero() {
    assert_eq!(EvidenceSet::default().ordering_score(), 0.0);
}
