//! Heuristic save-path candidate generation.
//!
//! Searches the well-known roots supplied by a [`FileSystem`] for a directory whose
//! name is a close match to the game title, and returns candidates sorted by
//! confidence.
//!
//! Reads through the injected filesystem rather than calling `dirs::`/`std::fs`
//! directly, which is what makes this testable at all — see
//! [`crate::saves::fs`] and ADR-0012. Nothing here opens a file.
//!
//! **Scope note.** This is candidate generation *only*. Content plausibility
//! (`verifier`), knowledge-base matching (`kb`), write observation (`witness`) and
//! the decision that binds a path (`resolver`) are separate Phase 1 subsystems —
//! see `docs/architecture/GAME_SAVE_DETECTION.md`. In particular `confidence` here
//! scores *name similarity*, which §5.2 of that document explains is not the same
//! question as "is this the save folder". It is deliberately unchanged in Phase 0.

use serde::Serialize;

use super::fs::FileSystem;

#[derive(Debug, Clone, Serialize)]
pub struct DetectedPath {
    pub path: String,
    /// 0.0 – 1.0. Higher = more confident this is the right folder.
    pub confidence: f32,
    /// Human-readable hint describing which heuristic matched.
    pub hint: String,
}

/// Run detection for `title` and return all candidate paths, sorted by
/// confidence descending, deduplicated.
pub fn detect(fs: &dyn FileSystem, title: &str) -> Vec<DetectedPath> {
    let mut results: Vec<DetectedPath> = Vec::new();

    for root in fs.roots() {
        if !fs.exists(&root.path) {
            continue;
        }
        let label = root.kind.label();
        for (confidence, dir_name) in title_variants(title) {
            let candidate = root.path.join(&dir_name);
            if fs.is_dir(&candidate) {
                results.push(DetectedPath {
                    path: candidate.display().to_string(),
                    confidence,
                    hint: format!("{label}/{dir_name}"),
                });
            }
        }
    }

    // Sort descending by confidence, then keep the first occurrence of each
    // path — which, after the sort, is its highest-confidence copy.
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // `dedup_by` only removes *adjacent* equal elements, and after sorting by
    // confidence two entries for the same path are adjacent only if their
    // confidences happen to tie. The same folder reached through two candidate
    // roots therefore appeared twice in the detection panel, despite the comment
    // claiming otherwise. Tracking what has been seen is what actually
    // deduplicates a list that is not sorted by the dedup key.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    results.retain(|candidate| seen.insert(candidate.path.clone()));

    results
}

/// Generate (confidence, dir_name_candidate) pairs for matching against a
/// game title.  More transformations = more chances to find the folder.
fn title_variants(title: &str) -> Vec<(f32, String)> {
    let mut v: Vec<(f32, String)> = Vec::new();

    // Exact title
    v.push((1.0, title.to_string()));

    // Lowercase exact
    let lc = title.to_lowercase();
    if lc != title {
        v.push((0.92, lc.clone()));
    }

    // Strip trailing number / roman numeral token
    let stripped = strip_trailing_number(title);
    if stripped != title {
        v.push((0.75, stripped.clone()));
        v.push((0.68, stripped.to_lowercase()));
    }

    // Spaces → underscores
    let underscored = title.replace(' ', "_");
    if underscored != title {
        v.push((0.72, underscored));
    }

    // Spaces removed
    let compact = title.replace(' ', "");
    if compact != title && compact != title.replace(' ', "_") {
        v.push((0.60, compact.clone()));
        v.push((0.55, compact.to_lowercase()));
    }

    // First word only (only useful when the word is long enough to be distinctive)
    if let Some(first) = title.split_whitespace().next() {
        if first.len() >= 5 {
            v.push((0.40, first.to_string()));
            v.push((0.35, first.to_lowercase()));
        }
    }

    v
}

fn strip_trailing_number(s: &str) -> String {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if let Some(last) = tokens.last() {
        if is_number_token(last) && tokens.len() > 1 {
            return tokens[..tokens.len() - 1].join(" ");
        }
    }
    s.to_string()
}

fn is_number_token(s: &str) -> bool {
    if s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    matches!(
        s,
        "I" | "II"
            | "III"
            | "IV"
            | "V"
            | "VI"
            | "VII"
            | "VIII"
            | "IX"
            | "X"
            | "XI"
            | "XII"
            | "XIII"
            | "XIV"
            | "XV"
            | "XVI"
            | "XVII"
            | "XVIII"
            | "XIX"
            | "XX"
    )
}

#[cfg(test)]
mod tests;
