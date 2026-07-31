//! Typed evidence about a candidate save location.
//!
//! `GAME_SAVE_DETECTION.md` §5 calls this the conceptual core of the system, and §5.2
//! explains what it replaces: a single `confidence` float derived from title
//! similarity, which answered "does this folder's name look like the game's name"
//! rather than "is this the save folder", and which could only be tuned rather than
//! reasoned about.
//!
//! Three properties this module exists to hold:
//!
//! 1. **Independently explainable.** Every item names its own source — an entry id, an
//!    alias, a session — so a decision can be traced back to the observations that
//!    produced it. Nothing is folded together.
//! 2. **Append-only.** New observations are added; they never rewrite history. A
//!    `WriteWitness` from three sessions ago is still evidence.
//! 3. **Survivable across versions.** Stored as a versioned JSON array with a
//!    `schema` discriminator, and an unrecognised variant deserialises to
//!    [`Evidence::Unknown`] rather than failing — so a downgrade after a newer build
//!    has written rows does not corrupt the table.
//!
//! ## Nothing here decides anything
//!
//! Evidence is an observation. The only component permitted to turn observations into
//! `bind_eligible`, `suggested` or `rejected` is [`crate::saves::resolver`]. That is
//! why this module has no `is_good()`, no threshold, and no total.

use serde::{Deserialize, Serialize};

/// Current on-disk schema for `save_candidates.evidence_json`.
pub const SCHEMA_VERSION: u32 = 1;

/// Ceiling on stored items per candidate.
///
/// Rescans append, so without a cap a long-lived row would grow without bound. The
/// eviction policy is deliberately strength-ordered rather than chronological — see
/// [`EvidenceSet::merge`].
pub const MAX_ITEMS: usize = 64;

/// Which knowledge-base layer an entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KbLayer {
    Builtin,
    Community,
    User,
}

impl KbLayer {
    pub fn parse(layer: &str) -> Option<KbLayer> {
        match layer {
            "builtin" => Some(KbLayer::Builtin),
            "community" => Some(KbLayer::Community),
            "user" => Some(KbLayer::User),
            _ => None,
        }
    }
}

/// One observation about a candidate directory.
///
/// Internally tagged so the JSON is self-describing and an added variant does not
/// shift the meaning of existing rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evidence {
    /// A knowledge-base entry claimed this path.
    ///
    /// `keyed` distinguishes the two very different things a built-in entry can be.
    /// A curated entry matched on `steam_appid` or `title_norm` is a statement about
    /// *this game*. A convention rule (`match_kind = 'any'`) is a statement about
    /// path shapes in general and applies to every game in the library. §5.3 rates
    /// `KbMatch(builtin)` as strong evidence; that is only true of the first kind,
    /// and conflating them would let a convention rule bind a photo folder.
    KbMatch {
        entry_id: String,
        layer: KbLayer,
        priority: u16,
        keyed: bool,
    },

    /// The directory name resembles the game title.
    NameMatch { alias: String, similarity: f32 },

    /// The directory sits inside the game's own install directory.
    InstallLocal { subdir: String },

    /// What a bounded metadata inspection found. Purely descriptive.
    ContentShape {
        save_like: u32,
        total: u32,
        max_depth: u8,
        newest_mtime: Option<String>,
    },

    /// The contents are positively inconsistent with saves, with the reason.
    ///
    /// Separate from [`Evidence::ContentShape`] so that "I looked and saw nothing
    /// save-like" is distinguishable from "I looked and saw something that rules this
    /// out". The reason travels with it, because a rejection a user cannot understand
    /// is indistinguishable from a bug.
    ContentMismatch { reason: String },

    /// The game was observed writing here while running. **Phase 2.**
    WriteWitness {
        session_id: i64,
        file_count: u32,
        bytes: u64,
    },

    /// The user chose this folder. Nothing outranks it.
    UserConfirmed { at: String },
    /// The user rejected this folder.
    UserRejected { at: String },

    /// A variant written by a newer build.
    ///
    /// Preserved on read so a downgrade does not lose information, and skipped by
    /// every decision rule because nothing can be concluded from it.
    #[serde(other)]
    Unknown,
}

impl Evidence {
    /// Relative weight, used **only** for ordering a suggestion list and for deciding
    /// what to evict when [`MAX_ITEMS`] is reached.
    ///
    /// This is not a probability and must never gate an outcome. ADR-0002 rejected
    /// weighted scoring precisely because invented weights cannot be justified; these
    /// exist so two suggestions can be shown in a sensible order, nothing more. The
    /// ranking follows §5.3.
    pub fn ordering_weight(&self) -> f64 {
        match self {
            Evidence::UserConfirmed { .. } => 1000.0,
            Evidence::UserRejected { .. } => 900.0,
            Evidence::WriteWitness { .. } => 100.0,
            Evidence::KbMatch { layer, keyed, .. } => match (layer, keyed) {
                (KbLayer::User, _) => 90.0,
                (KbLayer::Builtin, true) => 60.0,
                (KbLayer::Community, true) => 40.0,
                // A convention rule. Barely more than a name match, and §5.3's
                // "strong" rating does not apply to it.
                (_, false) => 12.0,
            },
            Evidence::ContentMismatch { .. } => 25.0,
            Evidence::ContentShape { save_like, .. } => {
                // Diminishing: two save files say much more than none, twenty say
                // little more than two.
                20.0 * (1.0 - 1.0 / (1.0 + f64::from(*save_like)))
            }
            Evidence::InstallLocal { .. } => 18.0,
            Evidence::NameMatch { similarity, .. } => 10.0 * f64::from(*similarity),
            Evidence::Unknown => 0.0,
        }
    }

    /// A short label naming the source, for logs and explanations.
    pub fn source(&self) -> String {
        match self {
            Evidence::KbMatch { entry_id, .. } => format!("kb:{entry_id}"),
            Evidence::NameMatch { alias, .. } => format!("name:{alias}"),
            Evidence::InstallLocal { subdir } => format!("install:{subdir}"),
            Evidence::ContentShape { .. } => "content".into(),
            Evidence::ContentMismatch { reason } => format!("content-mismatch:{reason}"),
            Evidence::WriteWitness { session_id, .. } => format!("witness:{session_id}"),
            Evidence::UserConfirmed { .. } => "user-confirmed".into(),
            Evidence::UserRejected { .. } => "user-rejected".into(),
            Evidence::Unknown => "unknown".into(),
        }
    }
}

/// The stored evidence for one candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceSet {
    pub schema: u32,
    pub items: Vec<Evidence>,
}

impl Default for EvidenceSet {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            items: Vec::new(),
        }
    }
}

impl EvidenceSet {
    pub fn new(items: Vec<Evidence>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            items,
        }
    }

    /// Read a stored set.
    ///
    /// Malformed JSON yields an empty set rather than an error: a corrupt evidence
    /// column must degrade one candidate's explanation, not fail a library scan.
    pub fn parse(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        // Infallible in practice: every variant is a plain struct of owned scalars.
        serde_json::to_string(self).unwrap_or_else(|_| {
            format!("{{\"schema\":{SCHEMA_VERSION},\"items\":[]}}")
        })
    }

    /// Merge newly observed evidence into a stored set, append-only.
    ///
    /// **Deduplication is by exact equality only.** A `NameMatch` whose similarity
    /// changed is a *different* observation and is kept alongside the old one — that
    /// is what "never rewrite history" means, and collapsing them would silently
    /// discard the provenance of a decision already made.
    ///
    /// When [`MAX_ITEMS`] is reached, the weakest items are dropped rather than the
    /// oldest. Chronological eviction would lose a `UserConfirmed` to a flood of name
    /// matches, which is precisely backwards.
    pub fn merge(&mut self, observed: Vec<Evidence>) {
        for item in observed {
            if !self.items.contains(&item) {
                self.items.push(item);
            }
        }
        self.schema = SCHEMA_VERSION;

        if self.items.len() > MAX_ITEMS {
            // Stable sort by descending weight keeps insertion order within a weight,
            // so eviction is deterministic.
            let mut ranked: Vec<(usize, f64)> = self
                .items
                .iter()
                .enumerate()
                .map(|(i, e)| (i, e.ordering_weight()))
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let keep: std::collections::HashSet<usize> =
                ranked.into_iter().take(MAX_ITEMS).map(|(i, _)| i).collect();
            let mut index = 0usize;
            self.items.retain(|_| {
                let k = keep.contains(&index);
                index += 1;
                k
            });
        }
    }

    /// Sum of ordering weights. Display and ordering only — never an outcome.
    ///
    /// `save_candidates.score` exists for exactly this and the migration says so:
    /// "Ordering and display only. Never decides an outcome: that is the rule table's
    /// job (ADR-0002). Asserting on this value in a test is an anti-pattern."
    pub fn ordering_score(&self) -> f64 {
        self.items.iter().map(Evidence::ordering_weight).sum()
    }

    pub fn find<'a, T>(&'a self, f: impl Fn(&'a Evidence) -> Option<T>) -> Option<T> {
        self.items.iter().find_map(f)
    }

    pub fn has(&self, f: impl Fn(&Evidence) -> bool) -> bool {
        self.items.iter().any(f)
    }

    /// Human-readable provenance, one line per observation.
    pub fn explain(&self) -> Vec<String> {
        self.items.iter().map(Evidence::source).collect()
    }
}

#[cfg(test)]
mod tests;
