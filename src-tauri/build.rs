//! Build script.
//!
//! Two jobs: Tauri's own codegen, and merging the built-in knowledge base.
//!
//! ## Why the knowledge base is merged at build time
//!
//! The corpus is many small files — one per category, sharded within a category — so that
//! it stays reviewable as it grows and two contributors adding different games never touch
//! the same file. See `data/kb/README.md`.
//!
//! The *runtime* wants the opposite: one string, one parse, no directory walking at
//! startup. Merging here gives both. `saves::kb::builtin` embeds the merged output with
//! `include_str!`, exactly as it embedded the single hand-written file before, so loading
//! cost is unchanged.
//!
//! Doing it here rather than with a `mod`-style list of `include_str!` calls is what makes
//! "add a file, that's it" true: a Rust list would reintroduce the shared edit point this
//! layout exists to remove.
//!
//! ## What this script guarantees
//!
//! * **Deterministic order.** Files sorted by path, entries kept in file order. Load-bearing:
//!   startup idempotence compares a SHA-256 over the merged bytes, so unstable ordering would
//!   make the corpus reload on every launch.
//! * **No duplicate ids**, across the whole corpus rather than per file.
//! * **Every file declares a `layout`**, which every entry in it inherits.
//! * **Structural validity** — parseable JSON with the required fields present.
//!
//! Deep validation (template anchoring, traversal refusal, key normalisation) stays in
//! `saves::kb::validate`, because it needs the crate this script is building. The split is
//! deliberate: this catches "the corpus is malformed" at build time, and the test suite
//! catches "an entry is wrong" at test time.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    build_knowledge_base();
}

/// Merge `data/kb/**/*.json` into a single document in `OUT_DIR`.
fn build_knowledge_base() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let kb_dir = manifest_dir.join("data").join("kb");

    // Rebuild whenever any corpus file changes. Watching the directory alone does not
    // catch edits to existing files on every platform, so each file is registered too.
    println!("cargo:rerun-if-changed={}", kb_dir.display());

    let version = read_version(&kb_dir);

    // BTreeMap keyed by the relative path: sorted iteration for free, and the key doubles
    // as the diagnostic origin recorded on each entry.
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    collect_json_files(&kb_dir, &kb_dir, &mut files);

    let mut entries: Vec<serde_json::Value> = Vec::new();
    let mut seen_ids: BTreeMap<String, String> = BTreeMap::new();

    for (relative, path) in &files {
        println!("cargo:rerun-if-changed={}", path.display());
        if relative == "manifest.json" {
            continue;
        }

        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("knowledge base: cannot read {relative}: {e}"));
        // Strip a UTF-8 byte-order mark. Several Windows editors and PowerShell's
        // `Set-Content -Encoding utf8` write one, and `serde_json` then fails with
        // "expected value at line 1 column 1" — an error that tells a contributor nothing
        // about the actual problem. Cheap to absorb here; baffling to debug otherwise.
        let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
        let doc: serde_json::Value = serde_json::from_str(raw).unwrap_or_else(|e| {
            panic!("knowledge base: {relative} is not valid JSON: {e}")
        });

        // Every file declares its layout once. Not inferred from the directory: if the path
        // determined it, moving a file would silently change whether its entries can bind.
        let layout = doc
            .get("layout")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "knowledge base: {relative} has no top-level \"layout\". \
                     Every corpus file must declare one — see data/kb/README.md."
                )
            })
            .to_string();

        let file_entries = doc
            .get("entries")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("knowledge base: {relative} has no \"entries\" array"));

        for (index, entry) in file_entries.iter().enumerate() {
            let mut entry = entry.clone();
            let object = entry.as_object_mut().unwrap_or_else(|| {
                panic!("knowledge base: {relative} entry {index} is not an object")
            });

            for required in ["id", "match_kind", "path_template"] {
                if !object.contains_key(required) {
                    panic!("knowledge base: {relative} entry {index} has no \"{required}\"");
                }
            }

            let id = object
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if let Some(previous) = seen_ids.insert(id.clone(), relative.clone()) {
                panic!(
                    "knowledge base: duplicate id `{id}` in {relative} \
                     (already defined in {previous})"
                );
            }

            // An entry may override the file's layout, for the rare case of a location that
            // does not match its neighbours. Explicit either way — never guessed.
            object
                .entry("layout")
                .or_insert_with(|| serde_json::Value::String(layout.clone()));

            // Diagnostic only. Never persisted, never read by the resolver — it exists so a
            // failing corpus test can say which file to open, which a merged blob would
            // otherwise make harder to debug than the single file it replaced.
            object.insert(
                "_origin".to_string(),
                serde_json::Value::String(relative.clone()),
            );

            entries.push(entry);
        }
    }

    if entries.is_empty() {
        panic!(
            "knowledge base: no entries found under {}. \
             A corpus that silently builds empty would ship a feature that looks broken.",
            kb_dir.display()
        );
    }

    let merged = serde_json::json!({
        "version": version,
        "entries": entries,
    });
    // Compact rather than pretty: nothing reads the merged file by hand, and the bytes are
    // hashed for idempotence, so smaller is strictly better.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("builtin-kb.json");
    std::fs::write(&out, serde_json::to_string(&merged).unwrap())
        .unwrap_or_else(|e| panic!("knowledge base: cannot write {}: {e}", out.display()));

    // Deliberately not a cargo:warning=. A message printed on every single build trains
    // people to ignore warnings, which is expensive the first time a real one appears. The
    // merge is verified by kb::organisation_tests instead.
}

fn read_version(kb_dir: &Path) -> String {
    let path = kb_dir.join("manifest.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("knowledge base: cannot read manifest.json: {e}"));
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(&raw);
    let doc: serde_json::Value = serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("knowledge base: manifest.json is not valid JSON: {e}"));
    doc.get("version")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("knowledge base: manifest.json has no \"version\""))
        .to_string()
}

/// Recursively collect `*.json`, keyed by forward-slashed path relative to `root`.
///
/// Recursive on purpose: it makes corpus granularity a data decision. Splitting a shard into
/// per-game files later needs no change here.
fn collect_json_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(relative, path);
        }
    }
}
