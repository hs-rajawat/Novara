//! The runtime entry point for save detection.
//!
//! This is the `SaveService` façade deferred at the end of Phase 0. It was correctly
//! deferred then — it would have been a wrapper with no behaviour of its own, and the
//! layer violation it prevents was not yet expressible because nothing existed below
//! `locator` for a command to skip past. Both of those changed with Track F: there is
//! now a `resolver` a handler could call directly, and there is real behaviour to own
//! (context assembly, persistence, backoff).
//!
//! ## One detection path
//!
//! The scenario runner is the reference implementation, and the runtime must exercise
//! the same code. Both call [`pipeline::detect_with_kb`]:
//!
//! ```text
//!   scenario runner ──┐
//!                     ├──► pipeline::detect_with_kb ──► locator ─► kb ─► verifier ─► resolver
//!   this service ─────┘
//! ```
//!
//! What this module adds is everything *around* that call which a test does not need:
//! reading the game and its installations out of the database, writing candidates and
//! decisions back, and honouring the retry ladder. The pipeline itself stays pure with
//! respect to the database, which is what keeps it testable from a fixture.
//!
//! ## Detection never binds
//!
//! Nothing here creates a save profile. The decision table's strongest outcome is
//! `bind_eligible`, which records that a candidate *would* be bound — the conversion
//! into an actual binding needs a correction UI and a binding store, and is Phase 3.
//! `save_candidates.status` enforces this independently: its CHECK constraint rejects
//! `'bound'`.

use crate::db::Db;
use crate::error::AppResult;
use crate::models::now_rfc3339;

use super::backoff::{self, ScanOutcome};
use super::fs::FileSystem;
use super::pipeline::{self, DetectionOutcome, GameContext};
use super::resolver::Outcome;

/// Why a scan was requested.
///
/// The distinction is the backoff. A user who asks for detection gets it now — making
/// them wait out a retry ladder they cannot see would look like a broken button. A
/// scheduled sweep across the whole library is exactly what the ladder exists to damp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The user asked. Ignores the retry ladder.
    User,
    /// A background or bulk scan. Honours the retry ladder.
    Scheduled,
}

/// What one runtime detection run did.
#[derive(Debug, Clone)]
pub struct DetectionRun {
    pub outcome: DetectionOutcome,
    /// Candidates written or updated, including rejected ones — a rejection is a
    /// result worth keeping, not an absence.
    pub persisted: usize,
    pub scan_outcome: ScanOutcome,
    /// True when the retry ladder said "not yet" and no scan was performed.
    pub skipped_by_backoff: bool,
}

impl DetectionRun {
    fn skipped() -> Self {
        Self {
            outcome: DetectionOutcome::default(),
            persisted: 0,
            scan_outcome: ScanOutcome::Nothing,
            skipped_by_backoff: true,
        }
    }
}

/// Assemble everything detection can use about a game.
///
/// Every field the pipeline reads is populated from stored data; none are invented.
/// Two notes on provenance:
///
/// * **Store ids come from installations, not from the game.** A game row has no app
///   id; an installation has `source_app_id` plus the source it came from. A game can
///   have more than one installation — the same title owned on Steam and Epic — so all
///   of them are read and each id lands in its own field.
/// * **`install_dir` and `exe_name` come from the primary installation.** They describe
///   one copy on disk, so unlike the ids they cannot be merged across installations. If
///   no installation is flagged primary the first is used, which matches how
///   `commands::games` resolves a launch target.
pub async fn context_for(db: &Db, game_id: &str) -> AppResult<Option<GameContext>> {
    let Some(game) = db.get_game(game_id).await? else {
        return Ok(None);
    };

    let mut ctx = GameContext {
        title: game.title,
        developer: game.developer,
        publisher: game.publisher,
        last_played_at: game.last_played_at,
        ..Default::default()
    };

    let installations = db.list_installations(game_id).await?;
    let primary = installations
        .iter()
        .find(|i| i.is_primary == 1)
        .or_else(|| installations.first());

    if let Some(install) = primary {
        ctx.install_dir = Some(install.install_dir.clone());
        // Stored as a path or a bare filename depending on the scanner; `kb::normalise_exe`
        // strips the directory and the extension either way.
        ctx.exe_name = install.executable.clone();
    }

    for install in &installations {
        let Some(app_id) = install.source_app_id.as_deref() else {
            continue;
        };
        match db.source_code_for(install.source_id).await?.as_str() {
            "steam" => ctx.steam_appid = Some(app_id.to_string()),
            "epic" => ctx.epic_id = Some(app_id.to_string()),
            "gog" => ctx.gog_id = Some(app_id.to_string()),
            // xbox, ubisoft, battle, emulator and manual have no KB match_kind yet.
            // Recording them would require a schema change to `save_kb_entries`, so they
            // are deliberately left unset rather than squeezed into a field that means
            // something else.
            _ => {}
        }
    }

    Ok(Some(ctx))
}

/// Detect save locations for one game, persist what was found, and update the retry
/// ladder.
///
/// Returns `Ok(None)` when the game does not exist.
pub async fn detect_and_persist(
    db: &Db,
    fs: &dyn FileSystem,
    game_id: &str,
    trigger: Trigger,
) -> AppResult<Option<DetectionRun>> {
    let Some(ctx) = context_for(db, game_id).await? else {
        return Ok(None);
    };

    if trigger == Trigger::Scheduled {
        let attempt = db.scan_attempt(game_id).await?;
        let due = attempt
            .as_ref()
            .map(|a| backoff::is_due(a.next_retry_at.as_deref(), chrono::Utc::now()))
            .unwrap_or(true);
        if !due {
            return Ok(Some(DetectionRun::skipped()));
        }
    }

    // The same call the scenario runner makes. Any divergence between test and runtime
    // behaviour would have to originate here, and there is nothing here to diverge.
    let outcome = pipeline::detect_with_kb(db, fs, &ctx).await?;

    let persisted = persist(db, game_id, &outcome).await?;
    let scan_outcome = classify(&outcome);
    record_attempt(db, game_id, scan_outcome).await?;

    Ok(Some(DetectionRun {
        outcome,
        persisted,
        scan_outcome,
        skipped_by_backoff: false,
    }))
}

/// Write candidates, their evidence and their decisions through the repository layer.
///
/// **Rejected candidates are persisted too.** A path the table ruled out is a result:
/// it stops the next scan re-deriving the same conclusion from scratch, and it gives a
/// user asking "why isn't my save folder detected" something to read.
async fn persist(db: &Db, game_id: &str, outcome: &DetectionOutcome) -> AppResult<usize> {
    let mut count = 0usize;

    for assessed in &outcome.assessed {
        // Evidence crosses into the repository as opaque JSON values. That layer merges
        // append-only by exact equality and preserves shapes it does not recognise, so
        // the typed model here and the envelope there agree on the wire without either
        // depending on the other's types.
        let items: Vec<serde_json::Value> = assessed
            .evidence
            .items
            .iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();

        let id = db
            .upsert_save_candidate(game_id, &assessed.path, "saves", &items)
            .await?;

        db.set_candidate_decision(
            id,
            assessed.decision.outcome.status(),
            i64::from(assessed.decision.rule),
            &assessed.decision.explanation,
            assessed.decision.score,
        )
        .await?;
        count += 1;
    }

    Ok(count)
}

/// The strongest thing this scan concluded, which is what the retry ladder keys on.
fn classify(outcome: &DetectionOutcome) -> ScanOutcome {
    if outcome.assessed.iter().any(|a| a.decision.outcome == Outcome::BindEligible) {
        ScanOutcome::BindEligible
    } else if outcome
        .assessed
        .iter()
        .any(|a| matches!(a.decision.outcome, Outcome::Suggested(_)))
    {
        ScanOutcome::Suggested
    } else {
        ScanOutcome::Nothing
    }
}

/// Record the attempt and schedule the next one.
///
/// The rung is chosen from the count this attempt will produce, so a first failure takes
/// the first rung. A positive outcome yields `None`, which the UPSERT writes as `NULL` —
/// clearing any retry state a previous fruitless scan had set.
async fn record_attempt(db: &Db, game_id: &str, outcome: ScanOutcome) -> AppResult<()> {
    let previous = db
        .scan_attempt(game_id)
        .await?
        .map(|a| a.attempt_count)
        .unwrap_or(0);

    let next_retry_at = backoff::next_retry_after(outcome, previous + 1).map(|wait| {
        let due = chrono::Utc::now()
            + chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::hours(1));
        due.to_rfc3339()
    });

    db.record_scan_attempt(game_id, outcome.as_str(), next_retry_at.as_deref())
        .await
}

/// Record that a scan failed, so the error ladder applies.
///
/// Separate from [`detect_and_persist`] because a failure has nothing to persist and
/// must not be mistaken for "looked and found nothing" — the two ladders differ.
pub async fn record_scan_error(db: &Db, game_id: &str) -> AppResult<()> {
    record_attempt(db, game_id, ScanOutcome::Error).await
}

/// Timestamp helper kept here so the service and its tests agree on the format.
pub fn now() -> String {
    now_rfc3339()
}

#[cfg(test)]
mod tests;
