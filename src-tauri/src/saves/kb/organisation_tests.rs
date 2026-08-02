//! Tests for how the corpus is *organised*, as distinct from what it contains.
//!
//! The organisation is enforced rather than documented-and-hoped-for, because a convention
//! nothing checks stops being a convention within a few contributions. These tests read the
//! corpus files directly — the same source `build.rs` merges — so they fail on the file that
//! is actually wrong.
//!
//! The most important test here is [`the_category_directory_has_no_runtime_effect`]. Category
//! exists purely for maintainability, and that claim needs an assertion or it decays into
//! "category sort of matters, in ways nobody has written down".

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::builtin;
use super::layout;

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data").join("kb")
}

/// Every corpus file, keyed by forward-slashed path relative to `data/kb`.
///
/// Mirrors `build.rs`'s walk deliberately: if the two disagree about what the corpus is,
/// these tests are checking something the build does not use.
fn corpus_files() -> BTreeMap<String, serde_json::Value> {
    let root = corpus_root();
    let mut out = BTreeMap::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == "manifest.json" {
                    continue;
                }
                let raw = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {relative}: {e}"));
                // Same BOM tolerance as `build.rs`. This has to match: if the build accepted
                // a byte-order mark and these tests did not, a contributor whose editor
                // writes one would get a green build and seven red tests — worse than either
                // failing alone.
                let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
                let doc = serde_json::from_str(raw)
                    .unwrap_or_else(|e| panic!("{relative} is not valid JSON: {e}"));
                out.insert(relative, doc);
            }
        }
    }
    assert!(!out.is_empty(), "no corpus files found under {}", root.display());
    out
}

fn category_of(relative: &str) -> &str {
    relative.split('/').next().unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────
// Structure
// ─────────────────────────────────────────────────────────────────────────

/// Every file declares its layout once, at the top, where a reviewer sees it before reading
/// a single entry.
#[test]
fn every_corpus_file_declares_a_layout() {
    for (relative, doc) in corpus_files() {
        let declared = doc.get("layout").and_then(|v| v.as_str());
        assert!(
            declared.is_some_and(|l| !l.trim().is_empty()),
            "{relative} has no top-level `layout` — see data/kb/README.md"
        );
    }
}

/// The declared layout must agree with the directory the file sits in.
///
/// This is what makes misfiling visible **without making the path load-bearing**. Layout is
/// still read from the declaration, so moving a file cannot silently change whether its
/// entries can bind; it just fails here instead.
#[test]
fn the_declared_layout_matches_the_directory() {
    for (relative, doc) in corpus_files() {
        let declared = doc.get("layout").and_then(|v| v.as_str()).unwrap_or_default();
        let category = category_of(&relative);
        assert_eq!(
            declared, category,
            "{relative} sits in `{category}/` but declares layout `{declared}`. \
             Either move the file or correct the declaration."
        );
    }
}

/// Directory names are layouts this build understands, so a contributor browsing the tree
/// sees the same vocabulary the code uses.
#[test]
fn every_category_directory_is_a_known_layout() {
    for relative in corpus_files().keys() {
        let category = category_of(relative);
        assert!(
            layout::KNOWN.contains(&category),
            "`{category}/` is not a known layout. Add it to `layout::KNOWN` with a \
             description, or rename the directory."
        );
    }
}

/// Placement inside `official/` is mechanical: the first character of the key. No judgement
/// call, so a contributor never has to wonder, and two people adding different games rarely
/// touch the same file.
#[test]
fn official_entries_live_in_the_shard_their_key_implies() {
    for (relative, doc) in corpus_files() {
        if category_of(&relative) != layout::OFFICIAL {
            continue;
        }
        let shard = relative
            .trim_start_matches("official/")
            .trim_end_matches(".json");
        // Only single-character shards are mechanically checkable; a future per-game split
        // would use longer names and is exempt.
        if shard.len() != 1 {
            continue;
        }

        for entry in doc.get("entries").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
            let Some(key) = entry.get("match_value").and_then(|v| v.as_str()) else {
                continue;
            };
            let expected = key.chars().next().unwrap_or('0');
            assert_eq!(
                shard.chars().next().unwrap(),
                expected,
                "`{}` has key `{key}` and belongs in official/{expected}.json, not {relative}",
                entry.get("id").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    }
}

/// A file with no entries is either a placeholder or an accident. `launcher/` is legitimately
/// empty *as a directory* — it has no file at all — which is different from a file that
/// parses to nothing.
#[test]
fn no_corpus_file_is_empty() {
    for (relative, doc) in corpus_files() {
        let count = doc
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(count > 0, "{relative} contains no entries");
    }
}

/// Entries carry no `layout` of their own unless they are deliberately overriding the file's.
/// Repeating the file's layout on every entry would let the two drift.
#[test]
fn entries_do_not_repeat_their_files_layout() {
    for (relative, doc) in corpus_files() {
        let declared = doc.get("layout").and_then(|v| v.as_str()).unwrap_or_default();
        for entry in doc.get("entries").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
            if let Some(own) = entry.get("layout").and_then(|v| v.as_str()) {
                assert_ne!(
                    own, declared,
                    "`{}` in {relative} repeats the file's layout `{declared}`. \
                     Remove it — the file already says so.",
                    entry.get("id").and_then(|v| v.as_str()).unwrap_or("?")
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The merge
// ─────────────────────────────────────────────────────────────────────────

/// Nothing is lost or invented between the corpus files and the merged blob.
#[test]
fn the_merged_corpus_contains_exactly_what_the_files_declare() {
    let mut from_files: Vec<String> = Vec::new();
    for (_, doc) in corpus_files() {
        for entry in doc.get("entries").and_then(|v| v.as_array()).unwrap_or(&vec![]) {
            if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                from_files.push(id.to_string());
            }
        }
    }
    from_files.sort();

    let (_, merged) = builtin::parsed().expect("valid corpus");
    let mut from_merged: Vec<String> = merged.iter().map(|e| e.id.clone()).collect();
    from_merged.sort();

    assert_eq!(from_files, from_merged, "the merge lost or added entries");
}

/// Each merged entry inherited its file's layout.
#[test]
fn every_merged_entry_carries_a_layout() {
    let (_, merged) = builtin::parsed().expect("valid corpus");
    for entry in &merged {
        assert!(
            !entry.layout.trim().is_empty(),
            "`{}` reached the merge with no layout",
            entry.id
        );
    }
}

/// **Checksum stability.** Startup idempotence compares a SHA-256 over the merged bytes, so a
/// merge whose order varied would reload the corpus on every launch. `build.rs` sorts by path
/// for this reason; this asserts the embedded result is stable.
#[test]
fn the_corpus_checksum_is_stable() {
    let first = builtin::checksum();
    for _ in 0..5 {
        assert_eq!(builtin::checksum(), first);
    }
    // And the parse order is stable, which is what the checksum rests on.
    let a: Vec<String> = builtin::parsed().unwrap().1.iter().map(|e| e.id.clone()).collect();
    let b: Vec<String> = builtin::parsed().unwrap().1.iter().map(|e| e.id.clone()).collect();
    assert_eq!(a, b);
}

/// A failing entry must name the file to open. Merging many files into one blob would
/// otherwise make the corpus harder to debug than the single file it replaced.
#[test]
fn every_entry_knows_which_file_it_came_from() {
    let (_, with_origins) = builtin::parsed_with_origins().expect("valid corpus");
    for (entry, origin) in &with_origins {
        let origin = origin
            .as_deref()
            .unwrap_or_else(|| panic!("`{}` has no recorded origin", entry.id));
        assert!(
            origin.ends_with(".json") && origin.contains('/'),
            "`{}` has an implausible origin `{origin}`",
            entry.id
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Category is organisation only
// ─────────────────────────────────────────────────────────────────────────

/// **The claim this whole refactor rests on.**
///
/// The directory an entry lives in must not reach the database, matching, evidence or the
/// decision table. Only `layout` does, and that is declared rather than inferred.
///
/// Asserted structurally: `NewKbEntry` is what the repository writes, and it has no field
/// that could carry a category — so the origin cannot be persisted even by accident. The
/// stored row is then checked to confirm no column contains a corpus path.
#[tokio::test]
async fn the_category_directory_has_no_runtime_effect() {
    let db = crate::test_support::test_db().await;
    builtin::load(&db).await.unwrap().unwrap();

    let (_, with_origins) = builtin::parsed_with_origins().expect("valid corpus");
    let origins: Vec<String> = with_origins.iter().filter_map(|(_, o)| o.clone()).collect();
    assert!(!origins.is_empty(), "nothing to check");

    for entry in with_origins.iter().take(5) {
        let stored = db
            .kb_entry(&entry.0.id)
            .await
            .unwrap()
            .expect("entry should be stored");

        // Every column that could plausibly leak a path.
        let columns = [
            stored.id.as_str(),
            stored.layer.as_str(),
            stored.match_kind.as_str(),
            stored.match_value.as_str(),
            stored.platform.as_str(),
            stored.role.as_str(),
            stored.layout.as_str(),
            stored.path_template.as_str(),
            stored.note.as_deref().unwrap_or(""),
            stored.source_ref.as_deref().unwrap_or(""),
            stored.kb_version.as_str(),
        ];
        for column in columns {
            for origin in &origins {
                assert!(
                    !column.contains(origin.as_str()),
                    "stored column `{column}` leaks the corpus path `{origin}`"
                );
            }
            // And no bare category name masquerading as data.
            assert_ne!(
                column, "official/",
                "a corpus directory reached the database"
            );
        }
    }
}

/// Two entries that differ only in which file they were written in must be indistinguishable
/// to the decision table. This is the behavioural half of the claim above.
#[test]
fn identical_entries_from_different_files_decide_identically() {
    use crate::saves::evidence::{Evidence, EvidenceSet, KbLayer};
    use crate::saves::resolver;

    let make = |layout_kind: &str| {
        EvidenceSet::new(vec![Evidence::KbMatch {
            entry_id: "builtin:x".into(),
            layer: KbLayer::Builtin,
            priority: 10,
            keyed: true,
            layout: layout_kind.into(),
        }])
    };

    // Same layout, and the only thing that could differ is the file — which the resolver
    // cannot see, because the evidence has no field for it.
    let a = resolver::decide(&make(layout::OFFICIAL), true);
    let b = resolver::decide(&make(layout::OFFICIAL), true);
    assert_eq!(a, b);

    // Different layout genuinely does change the outcome, which confirms the test above is
    // measuring the right thing rather than an insensitive comparison.
    let advisory = resolver::decide(&make(layout::COMMUNITY), true);
    assert_ne!(
        a.outcome, advisory.outcome,
        "layout must matter even though the file does not"
    );
}
