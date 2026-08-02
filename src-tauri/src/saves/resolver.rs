//! The decision table: the only component permitted to produce an outcome.
//!
//! `GAME_SAVE_DETECTION.md` §6, revised by
//! [ADR-0002](../../../docs/architecture/adr/0002-evidence-tiers-over-weighted-scoring.md).
//! The original design proposed a noisy-OR combination with per-signal weights; that
//! was rejected because the weights were invented without data, the independence
//! assumption is false (§5.3 — `KbMatch` and `NameMatch` are correlated, since a KB
//! template usually contains the title), and the output is neither explainable nor
//! testable.
//!
//! What replaces it is a table evaluated top to bottom, **first match wins**. Each row
//! is a test case, each row has a sentence, and adding a signal means adding a row
//! rather than retuning weights.
//!
//! ## Properties
//!
//! * **Every decision is reproducible from the evidence set alone.** [`decide`] is a
//!   pure function of [`EvidenceSet`]. No clock, no filesystem, no database, no
//!   randomness — so a stored decision can be recomputed and compared, and a
//!   disagreement means the evidence changed rather than the mood.
//! * **The score never decides.** It is computed for ordering the suggestion list and
//!   nothing else, and `same_evidence_same_decision_regardless_of_score` holds that
//!   line.
//! * **Nothing below this module produces an outcome.** The locator, the KB and the
//!   verifier emit observations; this is where they become `bind_eligible`,
//!   `suggested` or `rejected`.

use super::evidence::{Evidence, EvidenceSet, KbLayer};
use super::kb::layout;

/// How strongly a suggestion is offered. Ordering hint for the UI, not an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    High,
    Medium,
    Low,
}

/// The three statuses Phase 1 can record.
///
/// `BindEligible` is what the table's "bind" rows produce. Phase 1 computes and stores
/// the decision; nothing acts on it until Phase 3 has a binding store and a correction
/// UI. `save_candidates.status` enforces this — its CHECK constraint rejects `'bound'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    BindEligible,
    Suggested(Strength),
    Rejected,
}

impl Outcome {
    /// The value stored in `save_candidates.status`.
    pub fn status(&self) -> &'static str {
        match self {
            Outcome::BindEligible => "bind_eligible",
            Outcome::Suggested(_) => "suggested",
            Outcome::Rejected => "rejected",
        }
    }
}

/// One decision, with everything needed to explain and reproduce it.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub outcome: Outcome,
    /// Which table row fired. Stored in `save_candidates.decided_by_rule`.
    pub rule: u8,
    /// The sentence shown to the user. Never empty (invariant I9).
    pub explanation: String,
    /// Ordering only. See [`EvidenceSet::ordering_score`].
    pub score: f64,
    /// True for the one row that produces a decision a later scan must not revise.
    pub locked: bool,
}

/// Count of distinct sessions in which this directory was seen being written.
fn witness_sessions(set: &EvidenceSet) -> usize {
    let mut sessions: Vec<i64> = set
        .items
        .iter()
        .filter_map(|e| match e {
            Evidence::WriteWitness { session_id, .. } => Some(*session_id),
            _ => None,
        })
        .collect();
    sessions.sort_unstable();
    sessions.dedup();
    sessions.len()
}

fn save_like(set: &EvidenceSet) -> u32 {
    set.items
        .iter()
        .filter_map(|e| match e {
            Evidence::ContentShape { save_like, .. } => Some(*save_like),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn best_name_similarity(set: &EvidenceSet) -> f32 {
    set.items
        .iter()
        .filter_map(|e| match e {
            Evidence::NameMatch { similarity, .. } => Some(*similarity),
            _ => None,
        })
        .fold(0.0, f32::max)
}

/// A knowledge-base match from `layer` that names *this game* rather than a path shape.
fn keyed_kb_match(set: &EvidenceSet, layer: KbLayer) -> bool {
    set.has(|e| {
        matches!(
            e,
            Evidence::KbMatch { layer: l, keyed: true, .. } if *l == layer
        )
    })
}

/// A keyed match from `layer` whose **layout** is trusted to settle the question alone.
///
/// Expressed over [`layout::Authority`], never over layout names. That is what makes a
/// new save layout a data change: an unrecognised layout classifies as
/// [`layout::Authority::Advisory`] and flows through the suggestion rows below without a
/// new decision row and without touching this file.
fn curated_kb_match(set: &EvidenceSet, layer: KbLayer) -> bool {
    set.has(|e| match e {
        Evidence::KbMatch {
            layer: l,
            keyed: true,
            layout,
            ..
        } => *l == layer && layout::authority(layout) == layout::Authority::Curated,
        _ => false,
    })
}

/// The best layout phrase available, for an explanation that says *why*.
fn layout_phrase(set: &EvidenceSet) -> Option<String> {
    set.items.iter().find_map(|e| match e {
        // `describe` falls back to the raw layout string for a value this build does not
        // recognise. Only a reviewed phrase belongs in a sentence shown to a user, so an
        // unknown layout yields `None` and the caller uses neutral wording.
        Evidence::KbMatch { layout, .. } if layout::KNOWN.contains(&layout.as_str()) => {
            Some(layout::describe(layout).to_string())
        }
        _ => None,
    })
}

/// Apply the table.
///
/// `path_exists` is the one input that is not evidence, because rows 5 and 6 are
/// conditioned on it in §6 and a KB claim about a path that is not there must not
/// bind. It is passed in rather than checked here so this function stays pure.
pub fn decide(set: &EvidenceSet, path_exists: bool) -> Decision {
    let score = set.ordering_score();

    let decided = |rule: u8, outcome: Outcome, explanation: &str, locked: bool| Decision {
        outcome,
        rule,
        explanation: explanation.to_string(),
        score,
        locked,
    };

    // 1 — the user said no.
    if set.has(|e| matches!(e, Evidence::UserRejected { .. })) {
        return decided(1, Outcome::Rejected, "You rejected this folder.", true);
    }

    // 2 — the user said yes. Nothing outranks it (§5.3, terminal).
    if set.has(|e| matches!(e, Evidence::UserConfirmed { .. })) {
        return decided(
            2,
            Outcome::BindEligible,
            "You chose this folder.",
            true,
        );
    }

    let sessions = witness_sessions(set);
    let saves = save_like(set);

    // 3 — two independent correlations with a running process. Near-conclusive.
    if sessions >= 2 {
        return decided(
            3,
            Outcome::BindEligible,
            "Changed while you were playing, twice.",
            false,
        );
    }

    // 4 — one correlation, corroborated by contents.
    if sessions == 1 && saves > 0 {
        return decided(
            4,
            Outcome::BindEligible,
            "Changed while you were playing, and contains save files.",
            false,
        );
    }

    // 5 — a curated built-in entry for this game, and the path is really there.
    //
    // Two conditions beyond "the KB matched", and both are load-bearing.
    //
    // `keyed`: a convention rule matches every game in the library, so treating it as a
    // curated claim would bind the first conventional-looking folder that existed —
    // including a photo folder under `{DOCUMENTS}/{TITLE}`.
    //
    // Curated *layout*: an entry may describe the official location, or it may describe
    // an engine convention, a storefront's folder, or an alternative layout used by a
    // class of installs. Only the first is a statement about this game's real save
    // location. The others are statements about a class, and whether this install belongs
    // to that class is exactly what is unknown — so they suggest and are promoted by
    // corroborating evidence at rule 8b below.
    if curated_kb_match(set, KbLayer::Builtin) && path_exists {
        return decided(
            5,
            Outcome::BindEligible,
            "Known save location for this game.",
            false,
        );
    }

    // 5b — a user's own correction is stronger than anything automatic (§5.3 ranks the
    // user terminal; an explicit KB entry they authored is the same voice).
    if keyed_kb_match(set, KbLayer::User) && path_exists {
        return decided(
            5,
            Outcome::BindEligible,
            "Your own saved location for this game.",
            false,
        );
    }

    // 6 — the contents positively rule this out.
    //
    // NOT IN THE ORIGINAL §6 TABLE. Added because row 10 ("NameMatch ≥ 0.9 only")
    // would otherwise suggest a folder named exactly after the game that contains
    // nothing but screenshots — the very case the negative corpus exists to reject.
    // ADR-0002 sanctions extension by adding rows, which is what this is.
    //
    // Placed *below* the observation- and curation-backed bind rows on purpose: a
    // Write Witness or a curated entry is direct knowledge, and a content heuristic
    // must not overrule it.
    if let Some(reason) = set.find(|e| match e {
        Evidence::ContentMismatch { reason } => Some(reason.clone()),
        _ => None,
    }) {
        return decided(
            6,
            Outcome::Rejected,
            &format!("Does not look like a save folder: {reason}."),
            false,
        );
    }

    // 7 — one correlation, nothing corroborating. Could be a log directory.
    if sessions == 1 {
        return decided(
            7,
            Outcome::Suggested(Strength::High),
            "Changed while you were playing.",
            false,
        );
    }

    // 8 — community-reported, corroborated by contents.
    if keyed_kb_match(set, KbLayer::Community) && saves > 0 {
        return decided(
            8,
            Outcome::Suggested(Strength::High),
            "Community-reported location, contains save files.",
            false,
        );
    }

    // 8b — a keyed entry whose layout is advisory, corroborated by contents.
    //
    // **The row that makes new save layouts a data change.** An entry naming this game
    // but describing a *class* of installs — an engine convention, a storefront folder,
    // an alternative layout — lands here rather than at rule 5. It suggests strongly, and
    // the content evidence is what raises it above a bare name match.
    //
    // Nothing in this row names a layout. A layout introduced by a future corpus flows
    // through it automatically, which is the whole point: supporting another save layout
    // means adding KB data, not adding a rule.
    if saves > 0
        && set.has(|e| {
            matches!(
                e,
                Evidence::KbMatch { keyed: true, layout, .. }
                    if layout::authority(layout) == layout::Authority::Advisory
            )
        })
    {
        let phrase = layout_phrase(set).unwrap_or_else(|| "a known save location".to_string());
        return decided(
            8,
            Outcome::Suggested(Strength::High),
            &format!("Contains save files, in {phrase} for this game."),
            false,
        );
    }

    let name = best_name_similarity(set);
    let install_local = set.has(|e| matches!(e, Evidence::InstallLocal { .. }));

    // 9 — save-shaped contents in a folder that also matches by name or sits in the
    // install directory.
    if saves >= 2 && (name >= 0.8 || install_local) {
        return decided(
            9,
            Outcome::Suggested(Strength::Medium),
            "Contains save files in a folder matching this game.",
            false,
        );
    }

    // 10 — name similarity and nothing else. §5.3: confirms almost nothing alone.
    if name >= 0.9 {
        return decided(
            10,
            Outcome::Suggested(Strength::Low),
            "Folder name matches this game.",
            false,
        );
    }

    // 10b — a conventional path shape with save-like contents. Weaker than a curated
    // entry, stronger than nothing, and the only thing that rescues a game whose folder
    // is named unlike its title.
    if !keyed_kb_match(set, KbLayer::Builtin)
        && set.has(|e| matches!(e, Evidence::KbMatch { keyed: false, .. }))
        && saves >= 1
    {
        return decided(
            10,
            Outcome::Suggested(Strength::Low),
            "A conventional save location for this kind of game, and it contains save files.",
            false,
        );
    }

    // 11 — otherwise.
    decided(11, Outcome::Rejected, "No supporting evidence.", false)
}

#[cfg(test)]
mod tests;
