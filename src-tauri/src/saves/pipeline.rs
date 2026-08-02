//! The detection pipeline's composition root.
//!
//! One place where the evidence producers and the decider are assembled, so a caller —
//! a command, or the scenario runner — has a single entry point and does not reach into
//! `locator`, `kb`, `verifier` or `resolver` individually. That is the layer boundary
//! from `SAVE_SYSTEM_ARCHITECTURE.md` §1 made concrete.
//!
//! ## Who is allowed to conclude anything
//!
//! Everything this module calls **emits observations**:
//!
//! * `locator` — this path's name resembles the title / sits in the install directory
//! * `kb` — this path is claimed by knowledge-base entry X
//! * `verifier` — this path's contents look like *this*
//!
//! Exactly one component turns observations into `bind_eligible`, `suggested` or
//! `rejected`: [`resolver::decide`]. This file assembles evidence and calls it. It must
//! not filter, threshold or short-circuit — if it starts making decisions, the layering
//! has been lost and the explanation a user sees stops matching the reason.
//!
//! The verifier used to drop candidates here. It no longer does: a contradicting
//! observation becomes [`Evidence::ContentMismatch`] and the table's row 6 rejects it,
//! which means the rejection is now attributable to a rule and reproducible from the
//! stored evidence.
//!
//! ## Determinism
//!
//! Candidate order is fully determined: by decision score descending, then by path.
//! Nothing depends on filesystem listing order, so the same world always produces the
//! same list — a property the scenario corpus depends on.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::SaveKbEntry;

use super::bounds;
use super::evidence::{Evidence, EvidenceSet, KbLayer};
use super::fs::FileSystem;
use super::kb;
use super::locator::{self, DetectedPath, Origin};
use super::resolver::{self, Decision, Outcome};
use super::verifier;

/// Everything detection knows about a game before it starts looking.
///
/// Built by the caller, never by a subsystem — the same discipline
/// `metadata::LookupContext` follows.
#[derive(Debug, Clone, Default)]
pub struct GameContext {
    pub title: String,
    pub steam_appid: Option<String>,
    pub gog_id: Option<String>,
    pub epic_id: Option<String>,
    pub exe_name: Option<String>,
    pub install_dir: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    /// Feeds the verifier's mtime-correlation signal, which gives a library that
    /// predates the Write Witness some retroactive evidence.
    pub last_played_at: Option<String>,
}

impl GameContext {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }
}

/// One candidate path, its evidence, and the decision that evidence produced.
#[derive(Debug, Clone)]
pub struct Assessed {
    pub path: String,
    /// Name similarity only, for display. Not the ordering key.
    pub confidence: f32,
    pub hint: String,
    pub evidence: EvidenceSet,
    pub decision: Decision,
}

impl Assessed {
    fn as_detected(&self) -> DetectedPath {
        DetectedPath {
            path: self.path.clone(),
            confidence: self.confidence,
            hint: self.hint.clone(),
            origin: Origin::default(),
        }
    }
}

/// A candidate the decision table ruled out, and why.
///
/// Recorded rather than silently dropped. A candidate that vanishes with no explanation
/// is the hardest kind of detection bug to diagnose — the symptom is a missing game,
/// which looks identical to the locator never having found it.
#[derive(Debug, Clone)]
pub struct Rejected {
    pub path: String,
    /// The sentence from the decision, so the reason survives to the log.
    pub reason: String,
    /// Which table row rejected it.
    pub rule: u8,
}

/// What one detection run produced.
#[derive(Debug, Clone, Default)]
pub struct DetectionOutcome {
    /// Everything considered, including rejections. Ordered deterministically.
    pub assessed: Vec<Assessed>,
    /// Candidates worth showing: `bind_eligible` and `suggested`, best first.
    pub candidates: Vec<DetectedPath>,
    /// Candidates the decision table rejected.
    pub rejected: Vec<Rejected>,
}

impl DetectionOutcome {
    /// The leading candidate, if the table found anything worth offering.
    pub fn leader(&self) -> Option<&Assessed> {
        self.assessed
            .iter()
            .find(|a| a.decision.outcome != Outcome::Rejected)
    }

    /// Candidates the table marked `bind_eligible`.
    pub fn bind_eligible(&self) -> impl Iterator<Item = &Assessed> {
        self.assessed
            .iter()
            .filter(|a| a.decision.outcome == Outcome::BindEligible)
    }
}

/// Run detection for one game.
///
/// Pure with respect to the database: this returns what was found and persists nothing.
/// `kb_entries` is supplied by the caller — see [`detect_with_kb`] for the convenience
/// wrapper that fetches them — which keeps this function testable without a database
/// and keeps the KB query out of the hot path when a caller already has the rows.
pub fn detect(
    fs: &dyn FileSystem,
    ctx: &GameContext,
    kb_entries: &[SaveKbEntry],
) -> DetectionOutcome {
    // ── Observations ─────────────────────────────────────────────────────
    //
    // Evidence is accumulated per path in a BTreeMap so that the set of paths, and the
    // order evidence was added within each, are both independent of how the filesystem
    // happened to enumerate.
    let mut per_path: BTreeMap<String, (Vec<Evidence>, f32, String)> = BTreeMap::new();

    for found in locator::detect_with(
        fs,
        &locator::TitleContext {
            title: &ctx.title,
            developer: ctx.developer.as_deref(),
            publisher: ctx.publisher.as_deref(),
            install_dir: ctx.install_dir.as_deref(),
        },
    ) {
        let observation = match &found.origin {
            Origin::Name { alias, similarity } => Evidence::NameMatch {
                alias: alias.clone(),
                similarity: *similarity,
            },
            Origin::InstallLocal { subdir } => Evidence::InstallLocal {
                subdir: subdir.clone(),
            },
        };
        let slot = per_path
            .entry(normalise(&found.path))
            .or_insert_with(|| (Vec::new(), found.confidence, found.hint.clone()));
        // Highest name confidence wins for display; evidence keeps every observation.
        if found.confidence > slot.1 {
            slot.1 = found.confidence;
            slot.2 = found.hint.clone();
        }
        slot.0.push(observation);
    }

    for claim in kb::candidates(fs, kb_entries, ctx) {
        let layer = KbLayer::parse(&claim.layer).unwrap_or(KbLayer::Builtin);
        let slot = per_path
            .entry(normalise(&claim.path.display().to_string()))
            .or_insert_with(|| (Vec::new(), 0.0, format!("Knowledge base/{}", claim.entry_id)));
        slot.0.push(Evidence::KbMatch {
            entry_id: claim.entry_id.clone(),
            layer,
            priority: claim.priority.clamp(0, u16::MAX as i64) as u16,
            keyed: claim.keyed,
            layout: claim.layout.clone(),
        });
    }

    // ── Verification, then decision ──────────────────────────────────────
    let played = ctx
        .last_played_at
        .as_deref()
        .and_then(system_time_from_rfc3339);

    let mut assessed: Vec<Assessed> = Vec::with_capacity(per_path.len());
    for (index, (path, (mut items, confidence, hint))) in per_path.into_iter().enumerate() {
        // Only the leading candidates are verified. Each costs up to
        // `VERIFIER_MAX_METADATA_READS` stats and the locator may legitimately return
        // up to `MAX_CANDIDATES_PER_GAME`; verifying all would be 200 × 64 syscalls for
        // one game.
        //
        // An unverified candidate simply carries no content evidence. Not having looked
        // is not an observation, so the table judges it on what is actually known.
        if index < bounds::VERIFIER_MAX_CANDIDATES_PER_GAME {
            let assessment = verifier::verify(fs, Path::new(&path), played);
            items.extend(assessment.observations());
        }

        let evidence = EvidenceSet::new(items);
        let decision = resolver::decide(&evidence, fs.is_dir(Path::new(&path)));
        assessed.push(Assessed {
            path,
            confidence,
            hint,
            evidence,
            decision,
        });
    }

    // Score orders; path breaks ties so the order is total. Never depends on listing
    // order, which is what makes the scenario corpus reproducible.
    assessed.sort_by(|a, b| {
        b.decision
            .score
            .partial_cmp(&a.decision.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    let candidates = assessed
        .iter()
        .filter(|a| a.decision.outcome != Outcome::Rejected)
        .map(Assessed::as_detected)
        .collect();
    let rejected = assessed
        .iter()
        .filter(|a| a.decision.outcome == Outcome::Rejected)
        .map(|a| Rejected {
            path: a.path.clone(),
            reason: a.decision.explanation.clone(),
            rule: a.decision.rule,
        })
        .collect();

    DetectionOutcome {
        assessed,
        candidates,
        rejected,
    }
}

/// [`detect`], fetching the knowledge-base rows for this game first.
pub async fn detect_with_kb(
    db: &crate::db::Db,
    fs: &dyn FileSystem,
    ctx: &GameContext,
) -> crate::error::AppResult<DetectionOutcome> {
    let keys = kb::match_keys(ctx);
    let entries = db.match_kb_entries("windows", "saves", &keys).await?;
    Ok(detect(fs, ctx, &entries))
}

/// `Path::join` produces `\` on Windows; fixtures and comparisons use `/`.
///
/// Applied to the map key so a path reached through two producers — the locator and a KB
/// template — is recognised as one candidate rather than two.
fn normalise(path: &str) -> String {
    path.replace('\\', "/")
}

/// Convert a stored RFC3339 timestamp to a [`SystemTime`].
///
/// `None` for anything unparseable or before the Unix epoch, so a malformed
/// `last_played_at` costs the correlation signal rather than the whole scan.
fn system_time_from_rfc3339(raw: &str) -> Option<SystemTime> {
    let seconds = crate::models::parse_rfc3339(raw)?.timestamp();
    let seconds = u64::try_from(seconds).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests;
