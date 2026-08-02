//! The built-in knowledge base: compiled in, loaded at startup.
//!
//! Embedded with `include_str!` rather than shipped as a sidecar file, for three
//! reasons: a fresh offline install detects saves on first launch with no network
//! and no first-run download; the data cannot drift from the code that validates
//! it; and it cannot be edited in place to point NOVARA at an arbitrary directory,
//! which would turn a data file into a code path.
//!
//! ## Where the corpus lives
//!
//! Not in one file. `data/kb/` is organised into category directories — `official/`,
//! `engine/`, `os/`, `community/`, `portable/` — and `build.rs` merges them into a single
//! document in `OUT_DIR` which this module embeds. See `data/kb/README.md`.
//!
//! The directory structure is **organisation only**. It has no effect on matching,
//! authority, evidence or the decision table; what governs behaviour is the `layout` each
//! file declares and every entry in it inherits. Runtime cost is unchanged by the split:
//! still one embedded string and one parse, with no directory walking at startup.
//!
//! ## Loading is deterministic and idempotent
//!
//! [`load`] computes a SHA-256 over the embedded bytes and compares it with the
//! checksum recorded for the `builtin` layer. Identical checksum, no writes at all.
//! Startup therefore costs one small query in the overwhelmingly common case, and
//! running it twice is indistinguishable from running it once.
//!
//! `build.rs` merges files in sorted order for exactly this reason: unstable ordering would
//! change the checksum on every build and reload the corpus on every launch.
//!
//! ## Invariant I7
//!
//! Replacement goes through [`Db::replace_kb_layer`], whose `WHERE layer = ?` delete
//! is scoped to `builtin`. A KB refresh cannot remove a user's own corrections.

use serde::Deserialize;

use crate::db::save_kb::NewKbEntry;
use crate::db::Db;
use crate::error::AppResult;

use super::validate::{validate_entry, EntryError};

/// The corpus, merged from `data/kb/**/*.json` by `build.rs`.
const BUILTIN_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/builtin-kb.json"));

pub const LAYER: &str = "builtin";

/// A parsed corpus: its version, and each entry with the file it came from.
///
/// The origin is `Option` because it is stamped by `build.rs` and would therefore be absent
/// if the merged document were ever hand-written. Diagnostic only — see `RawEntry::origin`.
pub type ParsedCorpus = (String, Vec<(NewKbEntry, Option<String>)>);

#[derive(Debug, Deserialize)]
struct BuiltinFile {
    version: String,
    #[serde(default)]
    entries: Vec<RawEntry>,
}

/// Defaults exist so the data file states only what varies. Every entry so far is
/// a Windows save-role entry, and repeating that 40 times would bury the parts that
/// actually differ.
#[derive(Debug, Deserialize)]
struct RawEntry {
    id: String,
    match_kind: String,
    #[serde(default)]
    match_value: String,
    #[serde(default = "default_platform")]
    platform: String,
    #[serde(default = "default_role")]
    role: String,
    /// Free-form; see super::layout. Absent means unclassified rather than invalid, so
    /// an older corpus still loads.
    #[serde(default = "default_layout")]
    layout: String,
    path_template: String,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default = "default_priority")]
    priority: i64,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    /// Which corpus file this came from, stamped by uild.rs.
    ///
    /// **Diagnostic only.** Never persisted, never read by matching or the resolver. It
    /// exists so a failing corpus test can name the file to open — without it, merging many
    /// files into one blob would make the corpus harder to debug than the single file it
    /// replaced, which would be a regression dressed up as an improvement.
    #[serde(default, rename = "_origin")]
    origin: Option<String>,
}

fn default_platform() -> String {
    "windows".into()
}
fn default_role() -> String {
    "saves".into()
}
fn default_priority() -> i64 {
    100
}
fn default_layout() -> String {
    super::layout::UNSPECIFIED.into()
}

impl From<RawEntry> for NewKbEntry {
    fn from(r: RawEntry) -> Self {
        NewKbEntry {
            id: r.id,
            match_kind: r.match_kind,
            match_value: r.match_value,
            platform: r.platform,
            role: r.role,
            layout: r.layout,
            path_template: r.path_template,
            glob: r.glob,
            priority: r.priority,
            note: r.note,
            source_ref: r.source_ref,
        }
    }
}

/// Why the embedded KB could not be used.
///
/// Every variant is a defect in the shipped binary rather than anything a user did,
/// which is why [`parsed`] is asserted by a test: the loudest possible place to fail
/// is `cargo test`, long before a release.
#[derive(Debug)]
pub enum BuiltinError {
    Parse(String),
    Invalid {
        id: String,
        /// Which corpus file to open. Merging many files into one blob would otherwise make
        /// a failure harder to act on than it was when the corpus was a single file.
        origin: String,
        reason: EntryError,
    },
    DuplicateId(String),
    Empty,
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::Parse(e) => write!(f, "built-in KB is not valid JSON: {e}"),
            BuiltinError::Invalid { id, origin, reason } => {
                write!(f, "built-in KB entry `{id}` ({origin}) is invalid: {reason}")
            }
            BuiltinError::DuplicateId(id) => {
                write!(f, "built-in KB contains duplicate id `{id}`")
            }
            BuiltinError::Empty => write!(f, "built-in KB contains no entries"),
        }
    }
}

/// Parse and fully validate the embedded corpus.
///
/// **Every entry is validated before any is inserted, and one bad entry rejects the
/// whole file.** Loading 39 of 40 entries would leave a database state that no
/// source file describes, and the missing one would present as a detection bug.
pub fn parsed_with_origins() -> Result<ParsedCorpus, BuiltinError> {
    let file: BuiltinFile =
        serde_json::from_str(BUILTIN_JSON).map_err(|e| BuiltinError::Parse(e.to_string()))?;

    if file.entries.is_empty() {
        return Err(BuiltinError::Empty);
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(file.entries.len());
    for raw in file.entries {
        let origin = raw.origin.clone();
        let entry: NewKbEntry = raw.into();
        if !seen.insert(entry.id.clone()) {
            return Err(BuiltinError::DuplicateId(entry.id));
        }
        validate_entry(LAYER, &entry).map_err(|reason| BuiltinError::Invalid {
            id: entry.id.clone(),
            origin: origin.clone().unwrap_or_else(|| "unknown file".into()),
            reason,
        })?;
        out.push((entry, origin));
    }
    Ok((file.version, out))
}

/// Parse the corpus, discarding the per-entry origin.
///
/// The common shape: nothing outside diagnostics cares which file an entry came from.
pub fn parsed() -> Result<(String, Vec<NewKbEntry>), BuiltinError> {
    let (version, with_origins) = parsed_with_origins()?;
    Ok((version, with_origins.into_iter().map(|(e, _)| e).collect()))
}

/// SHA-256 of the embedded bytes, hex-encoded.
///
/// Over the raw file rather than the parsed entries, so that a change to a comment
/// or to field ordering still counts as a new version. Cheap, and it means "the
/// bytes I shipped" is the thing being compared.
pub fn checksum() -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, BUILTIN_JSON.as_bytes());
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// What a [`load`] call did. Returned rather than logged so tests can assert on it.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The stored checksum already matched. No writes were performed.
    AlreadyCurrent { version: String },
    Applied { version: String, entries: usize },
}

/// Load the embedded KB into the `builtin` layer if it is not already there.
///
/// Safe to call on every startup.
pub async fn load(db: &Db) -> AppResult<Result<LoadOutcome, BuiltinError>> {
    let (version, entries) = match parsed() {
        Ok(v) => v,
        Err(e) => return Ok(Err(e)),
    };
    let checksum = checksum();

    if let Some(current) = db.kb_version(LAYER).await? {
        if current.checksum == checksum {
            return Ok(Ok(LoadOutcome::AlreadyCurrent { version }));
        }
    }

    let count = db
        .replace_kb_layer(LAYER, &version, &checksum, None, &entries)
        .await?;
    Ok(Ok(LoadOutcome::Applied {
        version,
        entries: count,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::kb::normalise_title;
    use crate::saves::kb::template::{self, TemplateVars};
    use crate::test_support::test_db;

    /// The loud failure the module docs promise. An invalid built-in KB cannot reach
    /// a release because this test runs on every `cargo test`.
    #[test]
    fn the_embedded_kb_parses_and_every_entry_validates() {
        match parsed() {
            Ok((version, entries)) => {
                assert!(!version.is_empty(), "the corpus must carry a version");
                assert!(
                    entries.len() >= 25,
                    "Phase 1 targets 25-50 curated titles plus convention rules, got {}",
                    entries.len()
                );
            }
            Err(e) => panic!("embedded built-in KB is invalid: {e}"),
        }
    }

    /// Guards the failure mode that is invisible in production: an unnormalised
    /// `title_norm` is a well-formed entry that can never match anything.
    #[test]
    fn every_curated_key_is_reachable_by_a_lookup() {
        let (_, entries) = parsed().expect("valid corpus");
        for e in entries.iter().filter(|e| e.match_kind == "title_norm") {
            assert_eq!(
                normalise_title(&e.match_value),
                e.match_value,
                "entry `{}` has a key no lookup can produce",
                e.id
            );
        }
    }

    /// Convention rules apply to every game in the library, so a mistake in one is
    /// felt everywhere. They must carry no key and must sort below curated entries.
    #[test]
    fn convention_rules_are_library_wide_and_lowest_precedence() {
        let (_, entries) = parsed().expect("valid corpus");
        let conventions: Vec<_> = entries.iter().filter(|e| e.match_kind == "any").collect();
        assert!(!conventions.is_empty(), "the corpus should carry convention rules");

        let worst_curated = entries
            .iter()
            .filter(|e| e.match_kind != "any")
            .map(|e| e.priority)
            .max()
            .expect("curated entries exist");

        for c in conventions {
            assert!(c.match_value.is_empty(), "`{}` must carry no key", c.id);
            assert!(
                c.priority > worst_curated,
                "convention `{}` (priority {}) must sort below every curated entry \
                 (worst {worst_curated})",
                c.id,
                c.priority
            );
        }
    }

    /// The security property, asserted over the shipped data rather than over
    /// hand-written examples: nothing in the corpus can name a path outside an
    /// anchor. This is the check that would catch a bad merge into the data file.
    #[test]
    fn no_shipped_template_can_escape_its_anchor() {
        let (_, entries) = parsed().expect("valid corpus");
        for e in &entries {
            assert!(
                template::validate(&e.path_template).is_ok(),
                "`{}` has an unsafe template: {}",
                e.id,
                e.path_template
            );
        }
    }

    /// Hostile inputs substituted into the shipped templates must still not escape.
    /// Validation covers the template; this covers the *data* flowing through it.
    ///
    /// Counts what it examined. A first version of this test looped over results
    /// without counting, and since `expand` refuses a title containing a separator
    /// it would have passed while asserting nothing. A second version asserted zero
    /// paths were produced — also wrong, and the counting is what revealed why:
    /// **most curated entries name a literal folder and never interpolate the
    /// title at all**, so they expand normally no matter what the game is called.
    /// The property that actually holds is that every produced path stays under an
    /// anchor.
    #[test]
    fn hostile_game_metadata_cannot_escape_an_anchor() {
        let (_, entries) = parsed().expect("valid corpus");
        let fs = hostile_world();
        let mut examined = 0usize;

        for hostile in [
            "../../../../Windows/System32",
            "..",
            "..\\..\\Windows",
            "C:/Windows",
            "%SYSTEMROOT%",
        ] {
            let vars = TemplateVars {
                title: hostile,
                publisher: Some(hostile),
                developer: Some(hostile),
                steam_appid: Some(hostile),
                steam_userid: None,
                // Left unset so every anchor lives under HOME and the assertion
                // below can be a single prefix check.
                install_dir: None,
            };
            for e in &entries {
                for path in template::expand(&fs, &e.path_template, &vars) {
                    examined += 1;
                    let text = path.to_string_lossy().replace('\\', "/");
                    assert!(
                        text.starts_with(HOME),
                        "`{}` left its anchor with title `{hostile}`: {text}",
                        e.id
                    );
                    assert!(
                        !text.contains(".."),
                        "`{}` produced a traversal with title `{hostile}`: {text}",
                        e.id
                    );
                    assert!(
                        !text.to_lowercase().contains("windows/system32"),
                        "`{}` reached a system directory: {text}",
                        e.id
                    );
                }
            }
        }
        assert!(
            examined > 0,
            "the corpus produced no paths, so this test proved nothing"
        );
    }

    /// The control: ordinary metadata expands across the corpus, so a refusal above
    /// means "refused", not "expand is broken".
    #[test]
    fn benign_metadata_does_produce_paths() {
        let (_, entries) = parsed().expect("valid corpus");
        let fs = hostile_world();
        let vars = TemplateVars {
            title: "Hollow Knight",
            publisher: Some("Team Cherry"),
            developer: Some("Team Cherry"),
            steam_appid: Some("367520"),
            steam_userid: None,
            install_dir: Some("D:/Games/Hollow Knight"),
        };
        let produced: usize = entries
            .iter()
            .map(|e| template::expand(&fs, &e.path_template, &vars).len())
            .sum();
        assert!(
            produced >= 10,
            "ordinary metadata should expand across the corpus, got {produced}"
        );
    }

    /// A title that interpolates must be refused even though literal-path entries
    /// alongside it still expand — the distinction the test above turns on.
    #[test]
    fn a_hostile_title_is_refused_by_the_entries_that_use_it() {
        let fs = hostile_world();
        let vars = TemplateVars {
            title: "../../../Windows",
            publisher: None,
            developer: None,
            steam_appid: None,
            steam_userid: None,
            install_dir: None,
        };
        assert!(
            template::expand(&fs, "{MYGAMES}/{TITLE}", &vars).is_empty(),
            "an interpolating template must refuse a separator-bearing title"
        );
    }

    const HOME: &str = "C:/Users/test";

    /// Every anchor the corpus uses, so no entry is skipped for want of a root.
    fn hostile_world() -> crate::test_support::VirtualFs {
        use crate::saves::fs::RootKind;
        crate::test_support::VirtualFs::new()
            .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
            .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
            .with_root(RootKind::AppDataLocalLow, &format!("{HOME}/AppData/LocalLow"))
            .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
            .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
            .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
    }

    #[test]
    fn the_checksum_is_stable_across_calls() {
        assert_eq!(checksum(), checksum());
        assert_eq!(checksum().len(), 64, "hex-encoded SHA-256");
    }

    // ── Loading ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_first_load_applies_the_corpus() {
        let db = test_db().await;
        let outcome = load(&db).await.unwrap().expect("valid corpus");

        let (version, entries) = parsed().unwrap();
        assert_eq!(
            outcome,
            LoadOutcome::Applied {
                version,
                entries: entries.len()
            }
        );
        assert_eq!(
            db.count_kb_entries(LAYER).await.unwrap(),
            entries.len() as i64
        );
    }

    /// Startup runs this on every launch, so a second call must neither duplicate
    /// entries nor rewrite the layer.
    #[tokio::test]
    async fn loading_twice_is_a_no_op_the_second_time() {
        let db = test_db().await;
        load(&db).await.unwrap().unwrap();
        let first = db.kb_version(LAYER).await.unwrap().unwrap();

        let second = load(&db).await.unwrap().unwrap();
        assert!(
            matches!(second, LoadOutcome::AlreadyCurrent { .. }),
            "expected no writes, got {second:?}"
        );

        let after = db.kb_version(LAYER).await.unwrap().unwrap();
        assert_eq!(
            first.applied_at, after.applied_at,
            "an idempotent load must not touch the version row"
        );
        let (_, entries) = parsed().unwrap();
        assert_eq!(
            db.count_kb_entries(LAYER).await.unwrap(),
            entries.len() as i64,
            "entries must not accumulate"
        );
    }

    /// A stale checksum stands in for shipping a new build with a changed corpus.
    #[tokio::test]
    async fn a_changed_corpus_replaces_the_layer() {
        let db = test_db().await;
        db.replace_kb_layer(LAYER, "old", "an-old-checksum", None, &[])
            .await
            .unwrap();
        assert_eq!(db.count_kb_entries(LAYER).await.unwrap(), 0);

        let outcome = load(&db).await.unwrap().unwrap();
        assert!(matches!(outcome, LoadOutcome::Applied { .. }));
        assert_eq!(
            db.kb_version(LAYER).await.unwrap().unwrap().checksum,
            checksum()
        );
    }

    /// Invariant I7, at the layer that actually performs refreshes.
    #[tokio::test]
    async fn a_builtin_refresh_preserves_user_entries() {
        let db = test_db().await;
        load(&db).await.unwrap().unwrap();

        db.add_kb_entry(
            "user",
            &NewKbEntry {
                id: "user:mine".into(),
                match_kind: "title_norm".into(),
                match_value: "mygame".into(),
                platform: "windows".into(),
                role: "saves".into(),
                layout: crate::saves::kb::layout::USER_DEFINED.into(),
                path_template: "{MYGAMES}/{TITLE}".into(),
                glob: None,
                priority: 5,
                note: None,
                source_ref: None,
            },
        )
        .await
        .unwrap();

        // Force a replacement rather than the idempotent path.
        db.replace_kb_layer(LAYER, "old", "stale", None, &[]).await.unwrap();
        load(&db).await.unwrap().unwrap();

        assert_eq!(
            db.count_kb_entries("user").await.unwrap(),
            1,
            "a built-in refresh must not touch the user layer (I7)"
        );
    }
}
