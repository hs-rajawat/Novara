//! Runtime service tests.
//!
//! These cover the layer the scenario corpus cannot: context assembly from the database,
//! persistence through the repositories, and the retry ladder. Detection *behaviour* is
//! the corpus's job — duplicating it here would create the second implementation this
//! module exists to prevent.

use super::*;
use crate::saves::fs::RootKind;
use crate::test_support::{seed_game, test_db, VirtualFs};

const HOME: &str = "C:/Users/test";
const T0: u64 = 1_770_000_000;

fn world() -> VirtualFs {
    VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
        .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
        .with_root(RootKind::AppDataLocalLow, &format!("{HOME}/AppData/LocalLow"))
        .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
        .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
        .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
}

/// A world in which `title` has a plausible save folder under My Games.
fn world_with_saves(title: &str) -> (VirtualFs, String) {
    let dir = format!("{HOME}/Documents/My Games/{title}");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file_at(&format!("{dir}/slot0.sav"), 120_000, T0)
        .with_file_at(&format!("{dir}/slot1.sav"), 118_000, T0 + 20);
    (fs, dir)
}

// ─────────────────────────────────────────────────────────────────────────
// Context assembly
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_missing_game_produces_no_context() {
    let db = test_db().await;
    assert!(context_for(&db, "nope").await.unwrap().is_none());
}

#[tokio::test]
async fn the_context_carries_the_fields_stored_on_the_game() {
    let db = test_db().await;
    let id = seed_game(&db, "Hollow Knight").await;
    sqlx::query("UPDATE games SET developer = ?1, publisher = ?2, last_played_at = ?3 WHERE id = ?4")
        .bind("Team Cherry")
        .bind("Team Cherry")
        .bind("2026-01-04T19:22:31+00:00")
        .bind(&id)
        .execute(&db.pool)
        .await
        .unwrap();

    let ctx = context_for(&db, &id).await.unwrap().expect("context");
    assert_eq!(ctx.title, "Hollow Knight");
    assert_eq!(ctx.developer.as_deref(), Some("Team Cherry"));
    assert_eq!(ctx.publisher.as_deref(), Some("Team Cherry"));
    assert_eq!(ctx.last_played_at.as_deref(), Some("2026-01-04T19:22:31+00:00"));
}

/// Store ids live on installations, not on the game, and a title owned on two stores has
/// two of them. Each must land in its own field — squeezing them into one would make the
/// KB match on the wrong identity.
#[tokio::test]
async fn store_ids_are_read_from_every_installation() {
    let db = test_db().await;
    let id = seed_game(&db, "Multi Store Game").await;

    for (code, app_id, primary) in [("steam", "489830", 1), ("epic", "epic-abc", 0)] {
        let source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE code = ?1")
            .bind(code)
            .fetch_one(&db.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO game_installations
               (id, game_id, source_id, install_dir, executable, source_app_id, is_primary,
                detected_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'installed')",
        )
        .bind(format!("inst-{code}"))
        .bind(&id)
        .bind(source_id)
        .bind(format!("D:/Games/{code}/Multi Store Game"))
        .bind("Game.exe")
        .bind(app_id)
        .bind(primary)
        .bind(now())
        .execute(&db.pool)
        .await
        .unwrap();
    }

    let ctx = context_for(&db, &id).await.unwrap().expect("context");
    assert_eq!(ctx.steam_appid.as_deref(), Some("489830"));
    assert_eq!(ctx.epic_id.as_deref(), Some("epic-abc"));
    assert_eq!(ctx.gog_id, None, "no GOG installation exists");
    assert_eq!(
        ctx.install_dir.as_deref(),
        Some("D:/Games/steam/Multi Store Game"),
        "install_dir must come from the primary installation"
    );
    assert_eq!(ctx.exe_name.as_deref(), Some("Game.exe"));
}

#[tokio::test]
async fn a_game_with_no_installation_still_produces_a_usable_context() {
    let db = test_db().await;
    let id = seed_game(&db, "Manual Game").await;
    let ctx = context_for(&db, &id).await.unwrap().expect("context");
    assert_eq!(ctx.title, "Manual Game");
    assert!(ctx.install_dir.is_none(), "nothing to invent");
    assert!(ctx.exe_name.is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// Persistence
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_detection_run_persists_candidates_evidence_and_decisions() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    let (fs, dir) = world_with_saves("Test Game");

    let run = detect_and_persist(&db, &fs, &id, Trigger::User)
        .await
        .unwrap()
        .expect("game exists");
    assert!(run.persisted > 0, "nothing was written");

    let stored = db.list_save_candidates(&id).await.unwrap();
    let hit = stored
        .iter()
        .find(|c| c.path.replace('\\', "/") == dir)
        .expect("the save folder should be stored");

    assert_eq!(hit.status, "suggested");
    assert!(hit.decided_by_rule.is_some(), "the deciding rule must be recorded");
    assert!(
        hit.explanation.as_deref().is_some_and(|e| !e.trim().is_empty()),
        "invariant I9: every decision carries a sentence"
    );

    // Evidence written by the repository must be readable by the typed model. This is
    // the seam between `db::save_candidates::EvidenceEnvelope`, which treats items as
    // opaque JSON, and `saves::evidence::EvidenceSet`, which types them.
    let evidence = crate::saves::evidence::EvidenceSet::parse(&hit.evidence_json);
    assert!(
        !evidence.items.is_empty(),
        "evidence did not survive the round trip: {}",
        hit.evidence_json
    );
    assert!(evidence
        .items
        .iter()
        .all(|e| *e != crate::saves::evidence::Evidence::Unknown));
    assert!(evidence
        .has(|e| matches!(e, crate::saves::evidence::Evidence::ContentShape { .. })));
}

/// A rejection is a result. Persisting it stops the next scan re-deriving the same
/// conclusion and gives a user asking "why isn't my folder detected" something to read.
#[tokio::test]
async fn rejected_candidates_are_persisted_with_their_reason() {
    let db = test_db().await;
    let id = seed_game(&db, "Riverbound").await;

    let dir = format!("{HOME}/Documents/Riverbound");
    let mut fs = world().with_dir_tree(&dir);
    for i in 0..5 {
        fs = fs.with_file_at(&format!("{dir}/shot_{i}.jpg"), 4_000_000, T0 + i);
    }

    detect_and_persist(&db, &fs, &id, Trigger::User).await.unwrap();

    let stored = db.list_save_candidates(&id).await.unwrap();
    let hit = stored
        .iter()
        .find(|c| c.path.replace('\\', "/") == dir)
        .expect("the rejection should be stored");
    assert_eq!(hit.status, "rejected");
    assert_eq!(hit.decided_by_rule, Some(6));
    assert!(hit
        .explanation
        .as_deref()
        .is_some_and(|e| e.contains("images or video")));
}

/// Re-running must not duplicate rows or grow evidence without bound.
#[tokio::test]
async fn rescanning_is_idempotent() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    let (fs, _) = world_with_saves("Test Game");

    detect_and_persist(&db, &fs, &id, Trigger::User).await.unwrap();
    let first = db.list_save_candidates(&id).await.unwrap();
    let first_evidence: Vec<usize> = first
        .iter()
        .map(|c| crate::saves::evidence::EvidenceSet::parse(&c.evidence_json).items.len())
        .collect();

    detect_and_persist(&db, &fs, &id, Trigger::User).await.unwrap();
    let second = db.list_save_candidates(&id).await.unwrap();
    let second_evidence: Vec<usize> = second
        .iter()
        .map(|c| crate::saves::evidence::EvidenceSet::parse(&c.evidence_json).items.len())
        .collect();

    assert_eq!(first.len(), second.len(), "candidate rows duplicated");
    assert_eq!(
        first_evidence, second_evidence,
        "identical observations must not accumulate"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Binding stays disabled
// ─────────────────────────────────────────────────────────────────────────

/// **The guarantee this phase must not break.** Detection persists and scores; it never
/// creates a binding. A save profile is what a binding is today, and only a user action
/// may create one.
#[tokio::test]
async fn detection_never_creates_a_save_profile() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    let (fs, _) = world_with_saves("Test Game");

    assert!(db.list_save_profiles(&id).await.unwrap().is_empty());
    detect_and_persist(&db, &fs, &id, Trigger::User).await.unwrap();
    assert!(
        db.list_save_profiles(&id).await.unwrap().is_empty(),
        "detection created a binding"
    );
}

/// The schema backs the same guarantee independently: no candidate may ever be stored as
/// `bound`, whatever the decision table concluded.
#[tokio::test]
async fn no_candidate_is_ever_stored_as_bound() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    let (fs, _) = world_with_saves("Test Game");

    detect_and_persist(&db, &fs, &id, Trigger::User).await.unwrap();
    for c in db.list_save_candidates(&id).await.unwrap() {
        assert_ne!(c.status, "bound", "`{}` was stored as bound", c.path);
        assert!(
            ["candidate", "bind_eligible", "suggested", "rejected"].contains(&c.status.as_str()),
            "unexpected status `{}`",
            c.status
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Backoff
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_fruitless_scan_schedules_a_retry() {
    let db = test_db().await;
    let id = seed_game(&db, "Nothing Here").await;

    let run = detect_and_persist(&db, &world(), &id, Trigger::User)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.scan_outcome, ScanOutcome::Nothing);

    let attempt = db.scan_attempt(&id).await.unwrap().expect("recorded");
    assert_eq!(attempt.outcome, "nothing");
    assert_eq!(attempt.attempt_count, 1);
    assert!(
        attempt.next_retry_at.is_some(),
        "a fruitless scan must schedule a retry"
    );
}

/// "Successful scans should clear retry state." A game that failed and then succeeded
/// must not keep a pending retry.
#[tokio::test]
async fn a_successful_scan_clears_a_pending_retry() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;

    // First, a fruitless scan against an empty world.
    detect_and_persist(&db, &world(), &id, Trigger::User).await.unwrap();
    assert!(db.scan_attempt(&id).await.unwrap().unwrap().next_retry_at.is_some());

    // Then the same game, once its folder exists.
    let (fs, _) = world_with_saves("Test Game");
    let run = detect_and_persist(&db, &fs, &id, Trigger::User)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.scan_outcome, ScanOutcome::Suggested);

    let attempt = db.scan_attempt(&id).await.unwrap().unwrap();
    assert_eq!(attempt.next_retry_at, None, "retry state must be cleared");
    assert_eq!(attempt.attempt_count, 2, "attempts still accumulate");
}

/// The ladder lengthens across repeated failures. Asserted through the recorded count
/// plus the pure function, rather than by waiting.
#[tokio::test]
async fn repeated_fruitless_scans_climb_the_ladder() {
    let db = test_db().await;
    let id = seed_game(&db, "Nothing Here").await;

    let mut waits = Vec::new();
    for _ in 0..3 {
        detect_and_persist(&db, &world(), &id, Trigger::User).await.unwrap();
        let attempt = db.scan_attempt(&id).await.unwrap().unwrap();
        waits.push(backoff::next_retry_after(ScanOutcome::Nothing, attempt.attempt_count));
    }
    assert!(waits[0].unwrap() < waits[1].unwrap());
    assert!(waits[1].unwrap() < waits[2].unwrap());
}

/// A user pressing the button is not made to wait out a ladder they cannot see.
#[tokio::test]
async fn a_user_triggered_scan_ignores_the_backoff() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    db.record_scan_attempt(&id, "nothing", Some("2099-01-01T00:00:00+00:00"))
        .await
        .unwrap();

    let (fs, dir) = world_with_saves("Test Game");
    let run = detect_and_persist(&db, &fs, &id, Trigger::User)
        .await
        .unwrap()
        .unwrap();
    assert!(!run.skipped_by_backoff);
    assert!(run
        .outcome
        .candidates
        .iter()
        .any(|c| c.path.replace('\\', "/") == dir));
}

/// A scheduled sweep is exactly what the ladder exists to damp.
#[tokio::test]
async fn a_scheduled_scan_respects_the_backoff() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    db.record_scan_attempt(&id, "nothing", Some("2099-01-01T00:00:00+00:00"))
        .await
        .unwrap();

    let (fs, _) = world_with_saves("Test Game");
    let run = detect_and_persist(&db, &fs, &id, Trigger::Scheduled)
        .await
        .unwrap()
        .unwrap();
    assert!(run.skipped_by_backoff, "the ladder should have deferred this scan");
    assert_eq!(run.persisted, 0);
    assert!(db.list_save_candidates(&id).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_due_scheduled_scan_proceeds() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    db.record_scan_attempt(&id, "nothing", Some("2000-01-01T00:00:00+00:00"))
        .await
        .unwrap();

    let (fs, _) = world_with_saves("Test Game");
    let run = detect_and_persist(&db, &fs, &id, Trigger::Scheduled)
        .await
        .unwrap()
        .unwrap();
    assert!(!run.skipped_by_backoff);
    assert!(run.persisted > 0);
}

/// New information releases every waiting game — otherwise a KB update would take a week
/// to reach the games it was shipped to fix.
#[tokio::test]
async fn clearing_the_backoff_makes_a_scheduled_scan_due_again() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    db.record_scan_attempt(&id, "nothing", Some("2099-01-01T00:00:00+00:00"))
        .await
        .unwrap();

    db.clear_scan_backoff().await.unwrap();

    let (fs, _) = world_with_saves("Test Game");
    let run = detect_and_persist(&db, &fs, &id, Trigger::Scheduled)
        .await
        .unwrap()
        .unwrap();
    assert!(!run.skipped_by_backoff);
}

#[tokio::test]
async fn a_recorded_error_uses_the_error_ladder() {
    let db = test_db().await;
    let id = seed_game(&db, "Broken").await;
    record_scan_error(&db, &id).await.unwrap();

    let attempt = db.scan_attempt(&id).await.unwrap().unwrap();
    assert_eq!(attempt.outcome, "error");
    assert!(attempt.next_retry_at.is_some());
}

#[tokio::test]
async fn detecting_a_missing_game_returns_none() {
    let db = test_db().await;
    assert!(detect_and_persist(&db, &world(), "nope", Trigger::User)
        .await
        .unwrap()
        .is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// One detection path
// ─────────────────────────────────────────────────────────────────────────

/// The runtime and the scenario runner must reach the same conclusion for the same world,
/// because they call the same function. If a compatibility layer ever creeps in, the two
/// results diverge here.
#[tokio::test]
async fn the_runtime_agrees_with_the_pipeline_it_wraps() {
    let db = test_db().await;
    let id = seed_game(&db, "Test Game").await;
    let (fs, _) = world_with_saves("Test Game");

    let direct = crate::saves::pipeline::detect_with_kb(
        &db,
        &fs,
        &context_for(&db, &id).await.unwrap().unwrap(),
    )
    .await
    .unwrap();

    let run = detect_and_persist(&db, &fs, &id, Trigger::User)
        .await
        .unwrap()
        .unwrap();

    let a: Vec<(String, u8)> = direct
        .assessed
        .iter()
        .map(|x| (x.path.replace('\\', "/"), x.decision.rule))
        .collect();
    let b: Vec<(String, u8)> = run
        .outcome
        .assessed
        .iter()
        .map(|x| (x.path.replace('\\', "/"), x.decision.rule))
        .collect();
    assert_eq!(a, b, "the runtime diverged from the pipeline");
}
