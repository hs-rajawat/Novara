//! Heuristic save-path candidate generation.
//!
//! Searches the well-known roots supplied by a [`FileSystem`], plus the game's own
//! installation directory, for a directory whose name is a close match to the game
//! title, and returns candidates sorted by confidence.
//!
//! Reads through the injected filesystem rather than calling `dirs::`/`std::fs`
//! directly, which is what makes this testable at all — see
//! [`crate::saves::fs`] and ADR-0012. Nothing here opens a file.
//!
//! **Scope note.** This is candidate generation *only*. Content plausibility
//! (`verifier`), knowledge-base matching (`kb`), write observation (`witness`) and
//! the decision that binds a path (`resolver`) are separate subsystems — see
//! `docs/architecture/GAME_SAVE_DETECTION.md`. In particular `confidence` here
//! scores *name similarity*, which §5.2 of that document explains is not the same
//! question as "is this the save folder".
//!
//! ## How it looks, and what it refuses to do
//!
//! Two strategies per root, in this order:
//!
//! 1. **Direct probe.** For each alias, ask whether `root/alias` is a directory. One
//!    metadata call per alias, no listing. This is the whole of the original
//!    detector and remains the cheapest path.
//! 2. **Bounded enumeration.** List the root's *immediate* children once and compare
//!    each name to the aliases with a normalised edit distance, so `Witcher3` can
//!    match a folder actually called `witcher 3`.
//!
//! **There is no recursion.** Each root is read exactly one level deep, and
//! [`crate::saves::bounds`] caps how many entries that may examine. A detector that
//! walks `Documents` freely turns a library refresh into a disk thrash, and depth is
//! the axis where that goes wrong fastest.
//!
//! Enumeration is what makes false positives possible, so everything that guards
//! against them lives on that path: the ignore lists
//! ([`crate::saves::ignore`]), the sequel rule inside
//! [`alias::similarity`], the exact-only restriction on weak aliases, and the
//! requirement that a fuzzy match multiply its alias's confidence down rather than
//! replace it.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::bounds;
use super::fs::{join_under, FileSystem, Root, RootKind};
use super::ignore;

pub mod alias;

use alias::Alias;

/// How a path was found.
///
/// Provenance for the evidence model: [`crate::saves::evidence::Evidence::NameMatch`]
/// needs the alias and the similarity that produced it, and `InstallLocal` needs the
/// subdirectory name. Without this the locator's contribution would arrive at the
/// decision table as an unattributable number.
#[derive(Debug, Clone, PartialEq)]
pub enum Origin {
    /// Matched the title, through `alias`, at `similarity`.
    Name { alias: String, similarity: f32 },
    /// A conventionally-named save folder inside the game's install directory.
    InstallLocal { subdir: String },
}

impl Default for Origin {
    fn default() -> Self {
        Origin::Name {
            alias: String::new(),
            similarity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedPath {
    pub path: String,
    /// 0.0 – 1.0. Higher = more confident this is the right folder.
    ///
    /// **Name similarity only.** Deliberately not a combined score — see
    /// `GAME_SAVE_DETECTION.md` §5.2. Ordering of the suggestion list comes from the
    /// decision's `score`, not from this.
    pub confidence: f32,
    /// Human-readable hint describing which heuristic matched.
    pub hint: String,
    /// Internal provenance. Skipped on the wire: `DetectedSavePath` on the frontend is
    /// a fixed three-field shape, and evidence is not an IPC concern in Phase 1.
    #[serde(skip)]
    pub origin: Origin,
}

/// What the locator needs to know about a game.
///
/// Deliberately narrower than `pipeline::GameContext`: the locator has no business
/// with store ids or play history, and taking only what it uses keeps the dependency
/// pointing one way.
#[derive(Debug, Clone, Default)]
pub struct TitleContext<'a> {
    pub title: &'a str,
    pub developer: Option<&'a str>,
    pub publisher: Option<&'a str>,
    /// Enables the install-directory root (ADR-0004).
    pub install_dir: Option<&'a str>,
}

impl<'a> TitleContext<'a> {
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            ..Default::default()
        }
    }
}

/// Directory names that conventionally hold saves beside a game's executable.
///
/// Used **only** for the install-directory root, where name similarity to the title
/// is the wrong question: a portable release's saves sit in `saves/`, not in a folder
/// named after the game. Folded, so `SaveGames`, `savegames` and `Save Games` are one
/// entry.
const PORTABLE_SAVE_DIR_NAMES: &[&str] = &[
    "player",
    "players",
    "profile",
    "profiles",
    "save",
    "savedata",
    "savedgames",
    "savefiles",
    "savegame",
    "savegames",
    "saves",
    "userdata",
];

/// Run detection for `title` and return all candidate paths, sorted by confidence
/// descending, deduplicated.
///
/// Kept for callers that have nothing but a title. Prefer [`detect_with`], which can
/// use developer/publisher pairs and the install directory.
pub fn detect(fs: &dyn FileSystem, title: &str) -> Vec<DetectedPath> {
    detect_with(fs, &TitleContext::new(title))
}

/// Run detection with everything the locator can use.
pub fn detect_with(fs: &dyn FileSystem, ctx: &TitleContext<'_>) -> Vec<DetectedPath> {
    let aliases = alias::aliases(ctx.title, ctx.developer, ctx.publisher);
    let mut results: Vec<DetectedPath> = Vec::new();

    for root in search_roots(fs, ctx) {
        if !fs.exists(&root.path) {
            continue;
        }

        if root.kind == RootKind::InstallDir {
            // A different question entirely — see `PORTABLE_SAVE_DIR_NAMES`.
            collect_portable(fs, &root, &mut results);
        } else {
            probe_aliases(fs, &root, &aliases, &mut results);
            enumerate_root(fs, &root, &aliases, &mut results);
        }

        // Checked per root rather than only at the end so a pathological root
        // cannot run up an unbounded list before anyone notices.
        if results.len() >= bounds::MAX_CANDIDATES_PER_GAME {
            break;
        }
    }

    finalise(results)
}

/// The roots to search: the machine's well-known locations, plus this game's install
/// directory if it has one.
///
/// The install directory is **not** added to [`FileSystem::roots`], which stays a
/// description of the machine and must keep returning exactly the six well-known
/// locations. A per-game root belongs to the per-game call.
fn search_roots(fs: &dyn FileSystem, ctx: &TitleContext<'_>) -> Vec<Root> {
    let mut roots = fs.roots();
    if let Some(dir) = ctx.install_dir {
        let dir = dir.trim();
        if !dir.is_empty() {
            roots.push(Root {
                kind: RootKind::InstallDir,
                path: PathBuf::from(dir),
            });
        }
    }
    roots
}

/// Strategy 1: ask directly whether `root/alias` exists.
fn probe_aliases(
    fs: &dyn FileSystem,
    root: &Root,
    aliases: &[Alias],
    out: &mut Vec<DetectedPath>,
) {
    for alias in aliases {
        // Never `root.path.join(&alias.name)`: an alias is built from game metadata,
        // and an absolute string passed to `join` replaces the base instead of
        // extending it.
        let Some(candidate) = join_under(&root.path, &alias.name) else {
            continue;
        };
        if !fs.is_dir(&candidate) {
            continue;
        }
        if last_segment_is_ignored(&candidate) {
            continue;
        }
        out.push(DetectedPath {
            path: candidate.display().to_string(),
            confidence: alias.confidence,
            hint: format!("{}/{}", root.kind.label(), alias.name),
            // A direct probe is an exact hit on the alias by construction.
            origin: Origin::Name {
                alias: alias.name.clone(),
                similarity: 1.0,
            },
        });
    }
}

/// Strategy 2: list the root one level deep and match names approximately.
fn enumerate_root(
    fs: &dyn FileSystem,
    root: &Root,
    aliases: &[Alias],
    out: &mut Vec<DetectedPath>,
) {
    // Only aliases that survive an edit distance. If none do, the listing itself is
    // wasted work — skip it and keep the cheap path cheap.
    let fuzzy: Vec<&Alias> = aliases.iter().filter(|a| a.allows_fuzzy()).collect();
    if fuzzy.is_empty() {
        return;
    }

    let Ok(entries) = fs.read_dir(&root.path) else {
        return;
    };

    for entry in entries.into_iter().take(bounds::MAX_ENTRIES_PER_ROOT) {
        if !entry.is_dir || ignore::is_ignored(&entry.name) {
            continue;
        }

        // Best alias for this directory, so one folder yields one candidate rather
        // than one per alias that happens to be close.
        //
        // Both numbers are kept: `confidence` orders the candidate, `similarity` is the
        // provenance the evidence model needs. They are not the same thing — confidence
        // is deliberately scaled down by the alias's own strength.
        let mut best: Option<(f32, f32, &Alias)> = None;
        for alias in &fuzzy {
            let score = alias::similarity(&alias.name, &entry.name);
            if score < bounds::SIMILARITY_THRESHOLD {
                continue;
            }
            // A weak alias matched cleanly must not outrank a strong alias matched
            // loosely, so confidence stays anchored to the transform.
            let confidence = alias.confidence * score;
            // `map_or` rather than `is_none_or`: the latter is stable only since
            // 1.82 and this crate's MSRV is 1.77.
            if best.map_or(true, |(b, _, _)| confidence > b) {
                best = Some((confidence, score, alias));
            }
        }

        let Some((confidence, similarity, matched)) = best else {
            continue;
        };
        let Some(candidate) = join_under(&root.path, &entry.name) else {
            continue;
        };

        out.push(DetectedPath {
            path: candidate.display().to_string(),
            confidence,
            origin: Origin::Name {
                alias: matched.name.clone(),
                similarity,
            },
            // Names the folder found *and* the alias that found it: `≈` is the
            // difference between a user understanding a suggestion and guessing.
            hint: if matched.name == entry.name {
                format!("{}/{}", root.kind.label(), entry.name)
            } else {
                format!("{}/{} (≈ {})", root.kind.label(), entry.name, matched.name)
            },
        });
    }
}

/// The install-directory strategy: conventional save subfolders, one level down.
///
/// The install root itself is never offered. Binding it would archive the entire
/// game — tens of gigabytes of redistributable content to protect a few kilobytes of
/// saves — which is the one outcome a portable-game user must not get.
fn collect_portable(fs: &dyn FileSystem, root: &Root, out: &mut Vec<DetectedPath>) {
    let Ok(entries) = fs.read_dir(&root.path) else {
        return;
    };

    for entry in entries.into_iter().take(bounds::MAX_ENTRIES_PER_ROOT) {
        if !entry.is_dir || ignore::is_ignored(&entry.name) {
            continue;
        }
        if !is_portable_save_dir(&entry.name) {
            continue;
        }
        let Some(candidate) = join_under(&root.path, &entry.name) else {
            continue;
        };
        out.push(DetectedPath {
            path: candidate.display().to_string(),
            // A conventional name beside the executable is decent evidence but not
            // proof; it sits below an exact title match on a well-known root.
            confidence: 0.70,
            hint: format!("{}/{}", root.kind.label(), entry.name),
            origin: Origin::InstallLocal {
                subdir: entry.name.clone(),
            },
        });
    }
}

fn is_portable_save_dir(name: &str) -> bool {
    let folded = crate::saves::kb::normalise_title(name);
    PORTABLE_SAVE_DIR_NAMES.contains(&folded.as_str())
}

/// True when the final component of a path is on an ignore list.
///
/// Applies to direct probes too, not just enumeration: a game called `Cache` would
/// otherwise probe its way straight to a cache directory.
fn last_segment_is_ignored(path: &Path) -> bool {
    path.file_name()
        .map(|n| ignore::is_ignored(&n.to_string_lossy()))
        .unwrap_or(false)
}

/// Sort, deduplicate, and apply the per-game ceiling.
fn finalise(mut results: Vec<DetectedPath>) -> Vec<DetectedPath> {
    // Sort descending by confidence, then keep the first occurrence of each
    // path — which, after the sort, is its highest-confidence copy.
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            // Ties broken by path so the order is total and tests are stable.
            .then_with(|| a.path.cmp(&b.path))
    });
    // `dedup_by` only removes *adjacent* equal elements, and after sorting by
    // confidence two entries for the same path are adjacent only if their
    // confidences happen to tie. The same folder reached through two candidate
    // roots therefore appeared twice in the detection panel, despite the comment
    // claiming otherwise. Tracking what has been seen is what actually
    // deduplicates a list that is not sorted by the dedup key.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    results.retain(|candidate| seen.insert(candidate.path.clone()));

    // §7.2: past this point the alias generator is malfunctioning, so truncating is
    // the honest response rather than returning a list nobody will read.
    results.truncate(bounds::MAX_CANDIDATES_PER_GAME);
    results
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod track_d_tests;
