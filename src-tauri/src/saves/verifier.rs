//! Content plausibility for a directory that is *already* a candidate.
//!
//! The verifier answers one question: **do this directory's contents look like
//! saves?** It never considers the name — that is the locator's job, and
//! `GAME_SAVE_DETECTION.md` §9 is explicit that mixing them is how a single
//! confidence number ends up meaning nothing.
//!
//! ## It cannot invent a candidate
//!
//! [`verify`] takes a path and returns an [`Assessment`]. There is no return path by
//! which a new directory could be proposed, so "the verifier never invents
//! candidates" is a property of the signature rather than a rule to remember. What it
//! can do is **reject**: say that a directory it was handed does not look like saves.
//!
//! ## It cannot read a file
//!
//! [`FileSystem`] exposes no method that opens a file — that is ADR-0003's structural
//! guarantee, and §7.2 records the verifier's maximum read size as literally
//! `0 bytes`. So "avoid loading file contents" is not a discipline here, it is
//! unrepresentable. Everything below is derived from names, sizes and mtimes.
//!
//! ## Cost
//!
//! Two different budgets, because the two kinds of information cost differently:
//!
//! | Information | Source | Cost |
//! |---|---|---|
//! | Extension histogram, file/dir counts | `read_dir` names | **free** — already returned by the listing |
//! | Sizes, mtimes | `metadata()` per file | one syscall each, capped at 64 |
//!
//! That split is the whole performance story. Classifying a thousand files by
//! extension costs one listing; characterising their sizes costs 64 stats and no
//! more. Total per candidate: at most `VERIFIER_MAX_ENTRIES` listing entries across
//! at most `VERIFIER_MAX_DEPTH` levels, plus at most
//! `VERIFIER_MAX_METADATA_READS` stats. **Nothing scales with the size of a save
//! file** — only with how many files there are, and that is capped.
//!
//! ## Rejection is deliberately conservative
//!
//! Rejecting removes a candidate a user might have wanted, so every disqualifying
//! signal requires positive evidence of something *else*: executables present with no
//! save-like files at all, media overwhelming the directory, or a fully-examined empty
//! tree. A truncated or unreadable scan never rejects — absence of evidence is not
//! evidence of absence, and the honest answer to "I could not look properly" is to say
//! nothing.

use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use super::bounds;
use super::evidence::Evidence;
use super::fs::{join_under, FileSystem};
use super::ignore;

/// Extensions that indicate save data. §9's list, plus formats common enough to be
/// worth naming.
const SAVE_LIKE: &[&str] = &[
    "bak", "bin", "cfg", "dat", "db", "es3", "ess", "fos", "ini", "json", "profile", "sav",
    "save", "sgd", "sl2", "slot", "sqlite", "xml",
];

/// Extensions that mean this is program content, not save data. §9.
const EXECUTABLE_LIKE: &[&str] = &[
    "asset", "assets", "dll", "exe", "pak", "so", "sys", "uasset", "umap",
];

/// Extensions that mean this is a person's media, not save data.
const MEDIA_LIKE: &[&str] = &[
    "aac", "avi", "bmp", "flac", "gif", "jpeg", "jpg", "mkv", "mov", "mp3", "mp4", "png", "raw",
    "tif", "tiff", "wav", "webm", "webp",
];

/// What a bounded metadata walk of a directory found.
///
/// Counts are separated by how they were obtained, because that determines what they
/// can be trusted for. `*_seen` fields come from listings and cover everything walked;
/// the size and mtime fields cover only the sampled subset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectoryShape {
    /// Files encountered in listings, at any examined depth.
    pub files_seen: usize,
    /// Subdirectories encountered.
    pub dirs_seen: usize,
    /// Files for which `metadata()` was called.
    pub sampled: usize,

    /// A bound stopped the walk, so the counts are a floor rather than a total.
    pub truncated: bool,
    /// A directory could not be listed. Distinct from `truncated`: it means the walk
    /// was *blocked*, not that it was cut short.
    pub unreadable: bool,

    pub save_like: usize,
    pub executable_like: usize,
    pub media_like: usize,

    pub tiny_files: usize,
    pub newest: Option<SystemTime>,
    pub oldest: Option<SystemTime>,
}

impl DirectoryShape {
    /// Whether the walk saw everything there was to see.
    ///
    /// Only a complete walk may support a rejection.
    pub fn is_complete(&self) -> bool {
        !self.truncated && !self.unreadable
    }

    /// The span between the oldest and newest sampled file.
    pub fn mtime_span(&self) -> Option<Duration> {
        let (oldest, newest) = (self.oldest?, self.newest?);
        newest.duration_since(oldest).ok()
    }
}

/// How closely a directory's newest write sits to when the game was last played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closeness {
    /// Within an hour — the game almost certainly wrote this on that session.
    Tight,
    /// Within a day.
    Loose,
    /// Within a week. Weak, but better than nothing for a library that predates
    /// any write monitoring.
    Distant,
}

impl Closeness {
    fn for_delta(delta: Duration) -> Option<Closeness> {
        match delta.as_secs() {
            0..=3_600 => Some(Closeness::Tight),
            3_601..=86_400 => Some(Closeness::Loose),
            86_401..=604_800 => Some(Closeness::Distant),
            _ => None,
        }
    }
}

/// One independent observation about a directory.
///
/// Deliberately *not* combined into a score. Aggregation is the evidence model's job
/// (task 1.19); emitting a number here would pre-empt a decision this layer is not
/// entitled to make, and would lose the reason behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    /// Files with save-like extensions were found.
    SaveLikeExtensions { count: usize, of: usize },
    /// Sampled files were modified close together — one save event.
    WriteBurst { span_secs: u64, files: usize },
    /// The newest write correlates with `games.last_played_at` (task 1.18).
    PlayedAtCorrelation { closeness: Closeness, delta_secs: u64 },

    // ── Disqualifying ────────────────────────────────────────────────────
    /// A fully-examined tree with no files in it.
    NoFilesAtAll,
    /// Program content and not one save-like file.
    LooksLikeInstallDirectory { executables: usize },
    /// A person's media, overwhelmingly.
    LooksLikeMediaFolder { media: usize, of: usize },
    /// Far too many files to be a save folder.
    LooksLikeCache { files: usize },
    /// Everything present is too small to hold save data.
    LooksLikeMarkerDirectory { files: usize },
}

impl Signal {
    /// Whether this signal alone disqualifies a directory.
    ///
    /// A closed set, checked by an exhaustive match so a signal added later has to
    /// state which side it is on rather than defaulting to harmless.
    pub fn is_contradiction(&self) -> bool {
        match self {
            Signal::NoFilesAtAll
            | Signal::LooksLikeInstallDirectory { .. }
            | Signal::LooksLikeMediaFolder { .. }
            | Signal::LooksLikeCache { .. }
            | Signal::LooksLikeMarkerDirectory { .. } => true,

            Signal::SaveLikeExtensions { .. }
            | Signal::WriteBurst { .. }
            | Signal::PlayedAtCorrelation { .. } => false,
        }
    }

    /// A short reason string, for logs and for the rejection record.
    pub fn reason(&self) -> String {
        match self {
            Signal::SaveLikeExtensions { count, of } => {
                format!("{count} of {of} files look like save data")
            }
            Signal::WriteBurst { span_secs, files } => {
                format!("{files} files written within {span_secs}s of each other")
            }
            Signal::PlayedAtCorrelation { closeness, delta_secs } => {
                format!("newest write is {delta_secs}s from last played ({closeness:?})")
            }
            Signal::NoFilesAtAll => "directory contains no files".into(),
            Signal::LooksLikeInstallDirectory { executables } => {
                format!("{executables} executable or library files and no save data")
            }
            Signal::LooksLikeMediaFolder { media, of } => {
                format!("{media} of {of} files are images or video")
            }
            Signal::LooksLikeCache { files } => format!("{files} files — this is a cache"),
            Signal::LooksLikeMarkerDirectory { files } => {
                format!("all {files} files are {} bytes or smaller", bounds::TINY_FILE_BYTES)
            }
        }
    }
}

/// Everything the verifier concluded about one directory.
#[derive(Debug, Clone)]
pub struct Assessment {
    pub shape: DirectoryShape,
    pub signals: Vec<Signal>,
}

impl Assessment {
    /// The signal that contradicts this being a save folder, if any.
    ///
    /// **Naming matters here.** This used to be called `rejection()`, which made the
    /// verifier sound like it was deciding. It is not: it reports that an observation
    /// of a contradicting kind was made. Turning that into `rejected` is
    /// [`crate::saves::resolver`]'s job, via the decision table row for
    /// [`Evidence::ContentMismatch`], and only that module may do it.
    pub fn contradiction(&self) -> Option<&Signal> {
        self.signals.iter().find(|s| s.is_contradiction())
    }

    pub fn contradicts_saves(&self) -> bool {
        self.contradiction().is_some()
    }

    /// Signals that argue *for* the directory. Returned as a list, not a score.
    pub fn supporting(&self) -> impl Iterator<Item = &Signal> {
        self.signals.iter().filter(|s| !s.is_contradiction())
    }

    /// Translate this assessment into evidence for the decision table.
    ///
    /// Descriptive throughout: [`Evidence::ContentShape`] records what was seen, and
    /// [`Evidence::ContentMismatch`] records that something inconsistent with saves was
    /// seen, with the reason attached. Neither says what should happen.
    ///
    /// An unreadable directory yields **nothing at all** rather than an empty shape. An
    /// empty shape would read as "I looked and there was nothing here", which is a
    /// different and much stronger claim than "I could not look".
    pub fn observations(&self) -> Vec<Evidence> {
        if self.shape.unreadable {
            return Vec::new();
        }

        let mut out = vec![Evidence::ContentShape {
            save_like: self.shape.save_like as u32,
            total: self.shape.files_seen as u32,
            max_depth: bounds::VERIFIER_MAX_DEPTH as u8,
            newest_mtime: self.shape.newest.map(format_systemtime),
        }];

        if let Some(signal) = self.contradiction() {
            out.push(Evidence::ContentMismatch {
                reason: signal.reason(),
            });
        }
        out
    }
}

/// RFC3339, so a stored mtime is comparable with every other timestamp in the schema.
fn format_systemtime(t: SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

/// Characterise a candidate directory.
///
/// `last_played_at` enables the mtime correlation of task 1.18 and is optional: its
/// absence produces no signal, never a rejection.
pub fn verify(
    fs: &dyn FileSystem,
    dir: &std::path::Path,
    last_played_at: Option<SystemTime>,
) -> Assessment {
    let shape = walk(fs, dir);
    let signals = signals_for(&shape, last_played_at);
    Assessment { shape, signals }
}

/// Bounded breadth-first walk. Never recurses in the call stack, and never past the
/// depth, entry and metadata ceilings.
fn walk(fs: &dyn FileSystem, dir: &std::path::Path) -> DirectoryShape {
    let mut shape = DirectoryShape::default();
    let mut queue: VecDeque<(std::path::PathBuf, usize)> = VecDeque::new();
    queue.push_back((dir.to_path_buf(), 0));

    let mut entries_walked = 0usize;

    while let Some((current, depth)) = queue.pop_front() {
        let Ok(entries) = fs.read_dir(&current) else {
            // The candidate root being unreadable is a blocked scan; a subdirectory
            // being unreadable only makes the picture partial.
            if depth == 0 {
                shape.unreadable = true;
            } else {
                shape.truncated = true;
            }
            continue;
        };

        for entry in entries {
            if entries_walked >= bounds::VERIFIER_MAX_ENTRIES {
                shape.truncated = true;
                return shape;
            }
            entries_walked += 1;

            // An entry name comes from the filesystem, so it is joined through the
            // same guard the locator uses rather than with `Path::join`.
            let Some(child) = join_under(&current, &entry.name) else {
                continue;
            };

            if entry.is_dir {
                shape.dirs_seen += 1;
                // Generated-data directories are skipped rather than counted: a
                // `Cache/` subfolder inside a real save folder must not drag the
                // whole assessment towards "cache".
                if ignore::is_engine_noise(&entry.name) {
                    continue;
                }
                if depth < bounds::VERIFIER_MAX_DEPTH {
                    queue.push_back((child, depth + 1));
                } else {
                    shape.truncated = true;
                }
                continue;
            }

            shape.files_seen += 1;
            classify(&entry.name, &mut shape);

            // Sizes and mtimes are the only per-file cost, so they are the only thing
            // rationed. Classification above already happened for free.
            if shape.sampled < bounds::VERIFIER_MAX_METADATA_READS {
                if let Ok(meta) = fs.metadata(&child) {
                    shape.sampled += 1;
                    if meta.len <= bounds::TINY_FILE_BYTES {
                        shape.tiny_files += 1;
                    }
                    if let Some(modified) = meta.modified {
                        shape.newest = Some(match shape.newest {
                            Some(n) if n >= modified => n,
                            _ => modified,
                        });
                        shape.oldest = Some(match shape.oldest {
                            Some(o) if o <= modified => o,
                            _ => modified,
                        });
                    }
                }
            }
        }
    }

    shape
}

/// Bucket one filename by extension.
fn classify(name: &str, shape: &mut DirectoryShape) {
    let Some(ext) = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()) else {
        return;
    };
    let ext = ext.as_str();
    if SAVE_LIKE.contains(&ext) {
        shape.save_like += 1;
    }
    if EXECUTABLE_LIKE.contains(&ext) {
        shape.executable_like += 1;
    }
    if MEDIA_LIKE.contains(&ext) {
        shape.media_like += 1;
    }
}

/// Derive independent signals from a shape.
fn signals_for(shape: &DirectoryShape, last_played_at: Option<SystemTime>) -> Vec<Signal> {
    let mut signals = Vec::new();

    // ── Supporting ───────────────────────────────────────────────────────
    if shape.save_like > 0 {
        signals.push(Signal::SaveLikeExtensions {
            count: shape.save_like,
            of: shape.files_seen,
        });
    }

    // A burst needs at least two files to be a burst at all.
    if shape.sampled >= 2 {
        if let Some(span) = shape.mtime_span() {
            if span.as_secs() <= bounds::WRITE_BURST_WINDOW_SECS {
                signals.push(Signal::WriteBurst {
                    span_secs: span.as_secs(),
                    files: shape.sampled,
                });
            }
        }
    }

    // Task 1.18. Absence of correlation is never a rejection: a game may not write on
    // every launch, and a library imported from elsewhere may have no play history at
    // all.
    if let (Some(played), Some(newest)) = (last_played_at, shape.newest) {
        // Unsigned either way round, so a save written *after* the recorded session —
        // clock skew, a background sync — reads as the same distance rather than
        // panicking or wrapping.
        let delta = newest
            .duration_since(played)
            .or_else(|_| played.duration_since(newest))
            .unwrap_or_default();
        if let Some(closeness) = Closeness::for_delta(delta) {
            signals.push(Signal::PlayedAtCorrelation {
                closeness,
                delta_secs: delta.as_secs(),
            });
        }
    }

    // ── Disqualifying ────────────────────────────────────────────────────

    // Checked *before* the completeness gate, because it only needs a lower bound.
    //
    // A truncated walk makes every count a floor rather than a total — and a floor
    // already above the threshold is conclusive. Putting this behind the gate had it
    // exactly backwards: a directory of 400 files was rejected as a cache while one of
    // 40,000 was not, because the larger walk truncated and truncation forbade
    // rejecting. The more cache-like the directory, the more it got away with.
    if shape.files_seen > bounds::VERIFIER_MANY_FILES {
        signals.push(Signal::LooksLikeCache {
            files: shape.files_seen,
        });
    }

    // Everything below needs a *complete* scan. A partial view must not reject:
    // "I could not look properly" is not evidence about the contents. Unlike the count
    // above, these all rest on the *absence* of something, and absence is exactly what
    // a partial view cannot establish.
    if !shape.is_complete() {
        return signals;
    }

    if shape.files_seen == 0 {
        signals.push(Signal::NoFilesAtAll);
        return signals;
    }

    // Program content with nothing save-like alongside it. The `save_like == 0` guard
    // matters: plenty of real save folders sit next to a `.dll`, and a game that keeps
    // its saves in the install directory would otherwise be rejected outright.
    if shape.executable_like > 0 && shape.save_like == 0 {
        signals.push(Signal::LooksLikeInstallDirectory {
            executables: shape.executable_like,
        });
    }

    // Media folders. Same guard, for the same reason: a game that stores screenshots
    // beside its saves keeps its save-like evidence and is not rejected.
    let media_ratio = shape.media_like as f32 / shape.files_seen as f32;
    if shape.save_like == 0 && media_ratio >= bounds::MEDIA_DOMINANCE_RATIO {
        signals.push(Signal::LooksLikeMediaFolder {
            media: shape.media_like,
            of: shape.files_seen,
        });
    }

    // Every file too small to hold anything. Requires the sample to cover the whole
    // directory, or "all files" would mean "all 64 files I happened to look at".
    if shape.sampled == shape.files_seen && shape.tiny_files == shape.files_seen {
        signals.push(Signal::LooksLikeMarkerDirectory {
            files: shape.files_seen,
        });
    }

    signals
}

#[cfg(test)]
mod tests;
