//! Heuristic save-path detection for common OS locations.
//!
//! Searches AppData/Roaming, AppData/Local, AppData/LocalLow, Documents,
//! Documents/My Games, and Saved Games for a folder whose name is a close
//! match to the game title.  Returns results sorted by confidence.

use serde::Serialize;

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
pub fn detect(title: &str) -> Vec<DetectedPath> {
    let mut results: Vec<DetectedPath> = Vec::new();

    for (root, label) in candidate_roots() {
        if !root.exists() {
            continue;
        }
        for (confidence, dir_name) in title_variants(title) {
            let candidate = root.join(&dir_name);
            if candidate.is_dir() {
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

fn candidate_roots() -> Vec<(std::path::PathBuf, &'static str)> {
    let mut roots: Vec<(std::path::PathBuf, &'static str)> = Vec::new();

    // AppData/Roaming  (%APPDATA% / XDG_CONFIG_HOME / ~/Library/Application Support)
    if let Some(p) = dirs::config_dir() {
        roots.push((p, "AppData/Roaming"));
    }

    // AppData/Local  (%LOCALAPPDATA% on Windows)
    if let Some(p) = dirs::data_local_dir() {
        // On Windows: C:\Users\<user>\AppData\Local
        // LocalLow is the parent's sibling — no stdlib constant for it.
        if let Some(parent) = p.parent() {
            roots.push((parent.join("LocalLow"), "AppData/LocalLow"));
        }
        roots.push((p, "AppData/Local"));
    }

    // Documents
    if let Some(p) = dirs::document_dir() {
        roots.push((p.join("My Games"), "Documents/My Games"));
        roots.push((p.clone(), "Documents"));
    }

    // Saved Games  (%USERPROFILE%\Saved Games on Windows)
    if let Some(home) = dirs::home_dir() {
        roots.push((home.join("Saved Games"), "Saved Games"));
    }

    roots
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
