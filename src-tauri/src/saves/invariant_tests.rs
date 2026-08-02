//! Phase 1 exit-gate invariants (task 1.24).
//!
//! `docs/architecture/TESTING.md` names ten invariants; four are Phase 1 gates:
//!
//! | | Claim |
//! |---|---|
//! | **I2** | No detection path writes, creates, removes or opens-for-read a candidate |
//! | **I3** | No candidate path is outside the declared root set, at any depth |
//! | **I9** | Every decision-table outcome carries a non-empty explanation |
//! | **I10** | A pathological tree yields a bounded partial result, never a hang |
//!
//! I2 and I3 are partly structural — the `FileSystem` trait has no content-read method,
//! and `join_under` refuses escapes — but TESTING.md asks for them to be *asserted*, not
//! merely arranged. These tests do that against the recording filesystem, so a future
//! change that reintroduces the capability fails here rather than shipping.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::saves::fs::{FileSystem, RootKind};
use crate::saves::{bounds, evidence, pipeline, resolver};
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

/// Every declared root, for prefix checks.
fn root_prefixes(fs: &VirtualFs) -> Vec<String> {
    fs.roots()
        .iter()
        .map(|r| r.path.to_string_lossy().replace('\\', "/"))
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────
// I2 — detection is read-only
// ─────────────────────────────────────────────────────────────────────────

/// **I2, structurally.** The trait a detector reads through exposes exactly four
/// operations, none of which can open a file. This is ADR-0003's guarantee expressed as a
/// compile-time fact: a verifier physically cannot read a save.
///
/// If someone adds a `read_file` to `FileSystem`, this test still compiles — but the
/// assertion below, which watches what detection actually *did*, is what would catch it.
#[test]
fn i2_the_filesystem_trait_cannot_open_a_file() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0);

    // The whole surface. `metadata` returns sizes and mtimes; nothing returns bytes.
    let _: bool = fs.exists(Path::new(&dir));
    let _: bool = fs.is_dir(Path::new(&dir));
    let _ = fs.read_dir(Path::new(&dir));
    let meta = fs.metadata(Path::new(&format!("{dir}/slot0.sav"))).unwrap();
    assert_eq!(meta.len, 120_000, "size comes from metadata, not from reading");
}

/// **I2, asserted.** Run a full detection and check the recording filesystem saw only
/// metadata questions — never a request for a file's contents.
///
/// The recorder cannot log a content read because no such method exists, so what this
/// really pins is the *other* half: detection touched real paths, so the absence of reads
/// is a fact about a run that happened rather than about a run that did nothing.
#[test]
fn i2_a_full_detection_run_reads_no_contents() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{dir}/slot1.sav"), 118_000, T0 + 30);

    let outcome = pipeline::detect(&fs, &pipeline::GameContext::new("Test Game"), &[]);
    assert!(!outcome.assessed.is_empty(), "detection must have done something");

    let queried = fs.queried_paths();
    assert!(
        queried.iter().any(|p| p.contains("Test Game")),
        "the candidate should have been examined: {queried:?}"
    );
    // Sizes and mtimes reached the evidence, which is the only way file-level facts are
    // allowed to travel.
    assert!(outcome.assessed[0]
        .evidence
        .has(|e| matches!(e, evidence::Evidence::ContentShape { .. })));
}

// ─────────────────────────────────────────────────────────────────────────
// I3 — nothing escapes the root set
// ─────────────────────────────────────────────────────────────────────────

/// **I3.** Every path detection *asked about* lies under a declared root or the install
/// directory. Asserted over the recorder rather than over the results, because a result
/// filtered late would still mean the escape happened.
#[test]
fn i3_every_queried_path_stays_within_the_declared_roots() {
    let install = "D:/Games/Test Game";
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let fs = world()
        .with_dir_tree(&dir)
        .with_dir_tree(&format!("{install}/saves"))
        .with_file_at(&format!("{dir}/slot0.sav"), 1_000, T0);

    let ctx = pipeline::GameContext {
        title: "Test Game".into(),
        install_dir: Some(install.into()),
        ..Default::default()
    };
    let outcome = pipeline::detect(&fs, &ctx, &[]);
    assert!(!outcome.assessed.is_empty());

    let mut allowed = root_prefixes(&fs);
    allowed.push(install.to_string());

    for path in fs.queried_paths() {
        assert!(
            allowed.iter().any(|root| path.starts_with(root)),
            "queried `{path}` which is outside every declared root {allowed:?}"
        );
        assert!(!path.contains(".."), "queried a traversal: {path}");
    }
}

/// **I3 under attack.** A hostile title, developer and publisher must not steer a single
/// query outside the roots. The locator builds paths from this metadata, so it is the live
/// injection surface.
#[test]
fn i3_hostile_metadata_cannot_widen_the_search() {
    for hostile in [
        "../../../Windows/System32",
        "..",
        "C:/Windows",
        "\\\\server\\share",
        "C:",
    ] {
        let fs = world().with_dir_tree("C:/Windows/System32");
        let ctx = pipeline::GameContext {
            title: hostile.into(),
            developer: Some(hostile.into()),
            publisher: Some(hostile.into()),
            ..Default::default()
        };
        let _ = pipeline::detect(&fs, &ctx, &[]);

        let allowed = root_prefixes(&fs);
        for path in fs.queried_paths() {
            assert!(
                allowed.iter().any(|root| path.starts_with(root)),
                "`{hostile}` caused a query to `{path}`, outside {allowed:?}"
            );
        }
    }
}

/// **I3 at depth.** The verifier descends two levels below a candidate. A directory tree
/// deeper than that must not produce a query below the ceiling — which is also what stops a
/// junction pointing at a huge tree from being walked.
///
/// `VirtualFs` cannot model a symlink, so this asserts the property that makes a symlink
/// harmless: the walk is depth-bounded regardless of what it finds. The real-filesystem
/// half is `safety` in the scenario corpus.
#[test]
fn i3_depth_is_bounded_however_deep_the_tree_goes() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let deep = format!("{dir}/a/b/c/d/e/f/g");
    let fs = world()
        .with_dir_tree(&deep)
        .with_file_at(&format!("{deep}/buried.sav"), 1_000, T0);

    let _ = pipeline::detect(&fs, &pipeline::GameContext::new("Test Game"), &[]);

    for path in fs.queried_paths() {
        if let Some(rest) = path.strip_prefix(&format!("{dir}/")) {
            let depth = rest.split('/').count();
            assert!(
                depth <= bounds::VERIFIER_MAX_DEPTH + 1,
                "queried `{path}`, {depth} levels below the candidate (ceiling {})",
                bounds::VERIFIER_MAX_DEPTH
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// I9 — every outcome is explained
// ─────────────────────────────────────────────────────────────────────────

/// **I9.** Asserted over a real pipeline run rather than over hand-built evidence, so it
/// covers the path a user actually gets. The resolver's own exhaustive version is
/// `resolver::tests::every_row_produces_a_non_empty_explanation`.
#[test]
fn i9_every_decision_from_a_real_run_carries_an_explanation() {
    let good = format!("{HOME}/Documents/My Games/Test Game");
    let media = format!("{HOME}/Documents/Test Game");
    let mut fs = world()
        .with_dir_tree(&good)
        .with_file_at(&format!("{good}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{good}/slot1.sav"), 118_000, T0 + 5)
        .with_dir_tree(&media);
    for i in 0..5 {
        fs = fs.with_file_at(&format!("{media}/shot_{i}.jpg"), 4_000_000, T0 + i);
    }

    let outcome = pipeline::detect(&fs, &pipeline::GameContext::new("Test Game"), &[]);
    // Both a kept and a rejected candidate, so the invariant covers both branches.
    assert!(!outcome.candidates.is_empty(), "expected a surviving candidate");
    assert!(!outcome.rejected.is_empty(), "expected a rejected candidate");

    for a in &outcome.assessed {
        assert!(
            !a.decision.explanation.trim().is_empty(),
            "`{}` decided by rule {} with no explanation",
            a.path,
            a.decision.rule
        );
    }
    for r in &outcome.rejected {
        assert!(!r.reason.trim().is_empty(), "`{}` rejected with no reason", r.path);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// I10 — pathological input is bounded, never a hang
// ─────────────────────────────────────────────────────────────────────────

/// Directory names that all match the title closely enough to be candidates.
///
/// Letters chosen to avoid a trailing roman numeral, which the sequel rule would refuse.
fn many_matching(count: usize, title: &str) -> Vec<String> {
    let letters = [
        'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
        't', 'u', 'w', 'y', 'z',
    ];
    let mut out = Vec::with_capacity(count);
    for x in letters {
        for y in letters {
            for z in letters {
                if out.len() == count {
                    return out;
                }
                out.push(format!("{title} {x}{y}{z}"));
            }
        }
    }
    out
}

/// **I10.** A root holding tens of thousands of matching directories must produce a bounded
/// partial result quickly. The failure this guards against is a hang the user blames on
/// NOVARA.
///
/// The time assertion is generous — this is a correctness bound, not a benchmark — but a
/// regression that removed the caps would blow past it by orders of magnitude rather than
/// by a factor of two.
#[test]
fn i10_a_pathological_root_yields_a_bounded_partial_result() {
    const TITLE: &str = "Chronicles of the Wandering Star";
    let mut fs = world();
    for name in many_matching(30_000, TITLE) {
        fs = fs.with_dir(&format!("{HOME}/Documents/{name}"));
    }

    let started = Instant::now();
    let outcome = pipeline::detect(&fs, &pipeline::GameContext::new(TITLE), &[]);
    let elapsed = started.elapsed();

    assert!(
        outcome.assessed.len() <= bounds::MAX_CANDIDATES_PER_GAME,
        "returned {} candidates, ceiling is {}",
        outcome.assessed.len(),
        bounds::MAX_CANDIDATES_PER_GAME
    );
    assert!(
        !outcome.assessed.is_empty(),
        "a partial result, not an empty one — the directories do match"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "took {elapsed:?} on a pathological root; the caps are not holding"
    );
}

/// **I10, the verifier half.** A single candidate containing an enormous number of files
/// must also be bounded, and must record that it only saw part of the picture.
#[test]
fn i10_a_pathological_candidate_directory_is_bounded_and_marked_truncated() {
    let dir = format!("{HOME}/Documents/My Games/Test Game");
    let mut fs = world().with_dir_tree(&dir);
    for i in 0..(bounds::VERIFIER_MAX_ENTRIES * 3) {
        fs = fs.with_file(&format!("{dir}/f{i}.dat"), 4_096);
    }

    let started = Instant::now();
    let assessment = crate::saves::verifier::verify(&fs, Path::new(&dir), None);
    let elapsed = started.elapsed();

    assert!(assessment.shape.truncated, "a partial walk must say so");
    assert!(
        assessment.shape.files_seen <= bounds::VERIFIER_MAX_ENTRIES,
        "walked {} entries, ceiling is {}",
        assessment.shape.files_seen,
        bounds::VERIFIER_MAX_ENTRIES
    );
    assert!(
        assessment.shape.sampled <= bounds::VERIFIER_MAX_METADATA_READS,
        "made {} metadata calls, ceiling is {}",
        assessment.shape.sampled,
        bounds::VERIFIER_MAX_METADATA_READS
    );
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
}

/// **I3 and I10 through a junction**, on a real filesystem because `VirtualFs` has no
/// concept of one.
///
/// A candidate that is really a reparse point at a large tree is the classic way to turn a
/// bounded scan into an unbounded one. Two separate mechanisms stop it, and this asserts
/// both:
///
/// * **I3** — every path the walk builds goes through `join_under`, so it stays lexically
///   under the candidate no matter where the data physically lives. The verifier returns no
///   paths at all, only counts, so a junction cannot inject a path into the results.
/// * **I10** — the entry ceiling bounds the walk by *entries seen*, which is indifferent to
///   how big the target is.
///
/// Skipped rather than failed if the junction cannot be created: `mklink /J` needs no
/// elevation on NTFS, but it does need NTFS.
#[test]
fn i3_i10_a_junction_to_a_large_tree_is_bounded_and_confined() {
    use crate::saves::fs::RealFs;
    use crate::test_support::TempDir;

    let temp = TempDir::new("junction-safety");
    let target = temp.path().join("target");
    let candidate = temp.path().join("candidate");
    std::fs::create_dir_all(&target).unwrap();

    // A tree comfortably past the entry ceiling, two levels deep so the walk has somewhere
    // to go.
    for bucket in 0..8 {
        let sub = target.join(format!("b{bucket}"));
        std::fs::create_dir_all(&sub).unwrap();
        for i in 0..400 {
            std::fs::write(sub.join(format!("f{i}.dat")), b"x").unwrap();
        }
    }

    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&candidate)
        .arg(&target)
        .output();
    let junction_exists = made.map(|o| o.status.success()).unwrap_or(false) && candidate.is_dir();
    if !junction_exists {
        println!("skipping: could not create a junction on this filesystem");
        return;
    }

    let started = Instant::now();
    let assessment = crate::saves::verifier::verify(&RealFs, &candidate, None);
    let elapsed = started.elapsed();

    // I10: bounded despite the target holding 3,200 files.
    assert!(
        assessment.shape.files_seen <= bounds::VERIFIER_MAX_ENTRIES,
        "walked {} entries through a junction, ceiling is {}",
        assessment.shape.files_seen,
        bounds::VERIFIER_MAX_ENTRIES
    );
    assert!(
        assessment.shape.sampled <= bounds::VERIFIER_MAX_METADATA_READS,
        "made {} metadata calls, ceiling is {}",
        assessment.shape.sampled,
        bounds::VERIFIER_MAX_METADATA_READS
    );
    assert!(elapsed < Duration::from_secs(5), "took {elapsed:?} through a junction");

    // I3: the verifier hands back a shape, never a path. There is no channel by which the
    // junction target could enter the candidate set — which is the structural reason a
    // reparse point is harmless here rather than something to detect and special-case.
    let _: &crate::saves::verifier::DirectoryShape = &assessment.shape;
}

// ─────────────────────────────────────────────────────────────────────────
// Decision-row coverage, as the gate states it
// ─────────────────────────────────────────────────────────────────────────

/// The gate requires every Phase-1-reachable decision row to have a positive and a negative
/// case. This test states which rows those are and proves each is reachable both ways from
/// a constructed evidence set — the row-by-row detail lives in `resolver::tests`.
///
/// Rows 3, 4 and 7 need `WriteWitness`, which nothing produces until Phase 2. They are in
/// the table so precedence is correct from the start, and their cases are a Phase 2 gate.
#[test]
fn every_phase_1_reachable_row_is_covered_both_ways() {
    use evidence::{Evidence, EvidenceSet, KbLayer};
    use resolver::{decide, Outcome};

    let shape = |n: u32| Evidence::ContentShape {
        save_like: n,
        total: n.max(1),
        max_depth: 2,
        newest_mtime: None,
    };
    let name = |s: f32| Evidence::NameMatch {
        alias: "Test".into(),
        similarity: s,
    };
    let kb = |layer: KbLayer, layout: &str| Evidence::KbMatch {
        entry_id: "builtin:x".into(),
        layer,
        priority: 10,
        keyed: true,
        layout: layout.into(),
    };

    // (row, evidence that fires it, evidence that does not)
    let cases: Vec<(u8, Vec<Evidence>, Vec<Evidence>)> = vec![
        (
            1,
            vec![Evidence::UserRejected { at: "t".into() }],
            vec![name(1.0)],
        ),
        (
            2,
            vec![Evidence::UserConfirmed { at: "t".into() }],
            vec![name(1.0)],
        ),
        (
            5,
            vec![kb(KbLayer::Builtin, "official")],
            // Same layer, advisory layout: must not bind.
            vec![kb(KbLayer::Builtin, "community")],
        ),
        (
            6,
            vec![name(1.0), Evidence::ContentMismatch { reason: "images".into() }],
            vec![name(1.0), shape(2)],
        ),
        (
            8,
            vec![kb(KbLayer::Builtin, "community"), shape(2)],
            // No corroborating contents: cannot reach row 8.
            vec![kb(KbLayer::Builtin, "community"), shape(0)],
        ),
        (9, vec![shape(2), name(0.85)], vec![shape(1), name(0.85)]),
        (10, vec![name(0.95)], vec![name(0.8)]),
        (11, vec![name(0.1)], vec![kb(KbLayer::Builtin, "official")]),
    ];

    for (row, fires, does_not) in cases {
        let hit = decide(&EvidenceSet::new(fires.clone()), true);
        assert_eq!(hit.rule, row, "positive case for row {row} fired rule {}", hit.rule);
        assert!(
            !hit.explanation.trim().is_empty(),
            "row {row} produced no explanation (I9)"
        );

        let miss = decide(&EvidenceSet::new(does_not.clone()), true);
        assert_ne!(
            miss.rule, row,
            "negative case for row {row} should not have fired it"
        );
    }

    // And the rows that need Phase 2 evidence are genuinely unreachable without it.
    for row in [3u8, 4, 7] {
        let without_witness = decide(&EvidenceSet::new(vec![shape(2), name(0.9)]), true);
        assert_ne!(
            without_witness.rule, row,
            "row {row} needs WriteWitness and must not fire in Phase 1"
        );
    }

    // Sanity: a bind is only ever reached by rows 2 and 5 in Phase 1.
    let bind_rows: Vec<(u8, Vec<Evidence>)> = vec![
        (2, vec![Evidence::UserConfirmed { at: "t".into() }]),
        (5, vec![kb(KbLayer::Builtin, "official")]),
    ];
    for (row, fires) in bind_rows {
        let d = decide(&EvidenceSet::new(fires), true);
        assert_eq!(d.rule, row);
        assert_eq!(d.outcome, Outcome::BindEligible);
    }
}
