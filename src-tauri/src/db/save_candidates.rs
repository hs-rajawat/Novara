//! Candidate and scan-attempt persistence.
//!
//! Two properties this module exists to guarantee.
//!
//! **Evidence is append-only.** Re-running a scan must never lose an earlier
//! observation, because the decision is derived from the accumulated evidence and
//! a re-score of stored evidence must equal a score at observation time
//! (invariant I4). Upsert therefore *merges* rather than overwrites, inside a
//! transaction.
//!
//! **Negative results expire, positive results do not** (ADR-0007). Candidates and
//! their evidence persist; `save_scan_attempts` carries the backoff for games where
//! nothing was found.
//!
//! The evidence *envelope* is defined here; the meaning of each item belongs to
//! `crate::saves::evidence`. Storing items as opaque JSON values is deliberate:
//! an item written by a newer build round-trips through an older one without loss,
//! so a downgrade is survivable.

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::models::{now_rfc3339, SaveCandidate, SaveScanAttempt};

use super::Db;

/// Current envelope version. Bump only if the *envelope* changes; adding an
/// evidence variant does not, because items are opaque here.
pub const EVIDENCE_SCHEMA: u32 = 1;

/// The stored shape of a candidate's evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceEnvelope {
    pub schema: u32,
    /// Opaque to this layer. Unknown shapes are preserved verbatim.
    pub items: Vec<serde_json::Value>,
}

impl Default for EvidenceEnvelope {
    fn default() -> Self {
        Self {
            schema: EVIDENCE_SCHEMA,
            items: Vec::new(),
        }
    }
}

impl EvidenceEnvelope {
    fn parse(raw: &str) -> Self {
        // A malformed or unreadable envelope is treated as empty rather than
        // failing the scan: losing evidence is bad, but refusing to detect
        // anything because one row is corrupt is worse.
        serde_json::from_str(raw).unwrap_or_default()
    }

    /// Append items not already present. Identical observations do not duplicate,
    /// so re-running a scan is idempotent; genuinely new ones accumulate.
    fn merge(&mut self, new_items: &[serde_json::Value]) {
        for item in new_items {
            if !self.items.contains(item) {
                self.items.push(item.clone());
            }
        }
    }
}

impl Db {
    /// Record a candidate and merge in whatever was observed about it.
    ///
    /// Returns the candidate's id. Safe to call repeatedly with the same evidence.
    pub async fn upsert_save_candidate(
        &self,
        game_id: &str,
        path: &str,
        role: &str,
        new_evidence: &[serde_json::Value],
    ) -> AppResult<i64> {
        let now = now_rfc3339();
        let mut tx = self.pool.begin().await?;

        let existing: Option<(i64, String)> = sqlx::query_as(
            "SELECT id, evidence_json FROM save_candidates
             WHERE game_id = ?1 AND path = ?2 AND role = ?3",
        )
        .bind(game_id)
        .bind(path)
        .bind(role)
        .fetch_optional(&mut *tx)
        .await?;

        let id = match existing {
            Some((id, raw)) => {
                let mut envelope = EvidenceEnvelope::parse(&raw);
                envelope.merge(new_evidence);
                sqlx::query("UPDATE save_candidates SET evidence_json = ?1 WHERE id = ?2")
                    .bind(serde_json::to_string(&envelope)?)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                id
            }
            None => {
                let mut envelope = EvidenceEnvelope::default();
                envelope.merge(new_evidence);
                let row: (i64,) = sqlx::query_as(
                    r#"
                    INSERT INTO save_candidates
                      (game_id, path, role, status, score, evidence_json, first_seen_at)
                    VALUES (?1, ?2, ?3, 'candidate', 0, ?4, ?5)
                    RETURNING id
                    "#,
                )
                .bind(game_id)
                .bind(path)
                .bind(role)
                .bind(serde_json::to_string(&envelope)?)
                .bind(&now)
                .fetch_one(&mut *tx)
                .await?;
                row.0
            }
        };

        tx.commit().await?;
        Ok(id)
    }

    /// Store the outcome of the decision table for one candidate.
    ///
    /// `explanation` is the sentence shown to the user and must not be empty
    /// (invariant I9); callers are responsible for supplying one.
    pub async fn set_candidate_decision(
        &self,
        id: i64,
        status: &str,
        rule: i64,
        explanation: &str,
        score: f64,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE save_candidates
               SET status = ?1, decided_by_rule = ?2, explanation = ?3,
                   score = ?4, last_scored_at = ?5
             WHERE id = ?6
            "#,
        )
        .bind(status)
        .bind(rule)
        .bind(explanation)
        .bind(score)
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Mark a candidate rejected by the user.
    ///
    /// Terminal: a rejected path is never suggested again for this game, even if a
    /// later KB update proposes it (decision-table row 1).
    pub async fn reject_save_candidate(&self, id: i64) -> AppResult<()> {
        sqlx::query(
            "UPDATE save_candidates
                SET status = 'rejected', explanation = 'You rejected this folder.',
                    last_scored_at = ?1
              WHERE id = ?2",
        )
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_save_candidates(&self, game_id: &str) -> AppResult<Vec<SaveCandidate>> {
        Ok(sqlx::query_as::<_, SaveCandidate>(
            "SELECT * FROM save_candidates WHERE game_id = ?1
             ORDER BY score DESC, first_seen_at, id",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_save_candidate(&self, id: i64) -> AppResult<Option<SaveCandidate>> {
        Ok(
            sqlx::query_as::<_, SaveCandidate>("SELECT * FROM save_candidates WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Record that a scan happened, and when the next one is allowed.
    ///
    /// `next_retry_at` is `None` for a scan that found something — there is nothing
    /// to retry, and a positive result does not expire.
    pub async fn record_scan_attempt(
        &self,
        game_id: &str,
        outcome: &str,
        next_retry_at: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO save_scan_attempts
              (game_id, last_attempt, attempt_count, outcome, next_retry_at)
            VALUES (?1, ?2, 1, ?3, ?4)
            ON CONFLICT(game_id) DO UPDATE SET
              last_attempt = excluded.last_attempt,
              attempt_count = save_scan_attempts.attempt_count + 1,
              outcome = excluded.outcome,
              next_retry_at = excluded.next_retry_at
            "#,
        )
        .bind(game_id)
        .bind(now_rfc3339())
        .bind(outcome)
        .bind(next_retry_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn scan_attempt(&self, game_id: &str) -> AppResult<Option<SaveScanAttempt>> {
        Ok(
            sqlx::query_as::<_, SaveScanAttempt>("SELECT * FROM save_scan_attempts WHERE game_id = ?1")
                .bind(game_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Make every game eligible for a rescan.
    ///
    /// Called when new information arrives that could change an outcome — a KB
    /// refresh being the motivating case. Clears the backoff without touching
    /// candidates or their evidence.
    pub async fn clear_scan_backoff(&self) -> AppResult<u64> {
        let r = sqlx::query("UPDATE save_scan_attempts SET next_retry_at = NULL")
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{seed_game, test_db};
    use serde_json::json;

    #[tokio::test]
    async fn a_new_candidate_is_created_with_its_evidence() {
        let db = test_db().await;
        let game = seed_game(&db, "Hollow Knight").await;

        let id = db
            .upsert_save_candidate(&game, "C:/Docs/Hollow Knight", "saves", &[json!({"NameMatch": {"alias": "Hollow Knight", "similarity": 1.0}})])
            .await
            .unwrap();

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        assert_eq!(c.status, "candidate");
        assert_eq!(c.score, 0.0);
        let env = EvidenceEnvelope::parse(&c.evidence_json);
        assert_eq!(env.schema, EVIDENCE_SCHEMA);
        assert_eq!(env.items.len(), 1);
    }

    /// Re-running a scan must not duplicate what it already observed.
    #[tokio::test]
    async fn upserting_identical_evidence_is_idempotent() {
        let db = test_db().await;
        let game = seed_game(&db, "Celeste").await;
        let ev = json!({"NameMatch": {"alias": "Celeste", "similarity": 1.0}});

        let a = db.upsert_save_candidate(&game, "C:/Docs/Celeste", "saves", std::slice::from_ref(&ev)).await.unwrap();
        let b = db.upsert_save_candidate(&game, "C:/Docs/Celeste", "saves", std::slice::from_ref(&ev)).await.unwrap();
        assert_eq!(a, b, "the same path must not create a second candidate");

        let c = db.get_save_candidate(a).await.unwrap().unwrap();
        assert_eq!(EvidenceEnvelope::parse(&c.evidence_json).items.len(), 1);
    }

    /// Invariant I4 depends on this: evidence accumulates, it is never replaced.
    #[tokio::test]
    async fn new_evidence_is_appended_to_old() {
        let db = test_db().await;
        let game = seed_game(&db, "Hades").await;
        let path = "C:/Docs/Hades";

        let id = db
            .upsert_save_candidate(&game, path, "saves", &[json!({"NameMatch": {"alias": "Hades", "similarity": 1.0}})])
            .await
            .unwrap();
        db.upsert_save_candidate(&game, path, "saves", &[json!({"ContentShape": {"save_like": 3, "total": 4}})])
            .await
            .unwrap();

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        let items = EvidenceEnvelope::parse(&c.evidence_json).items;
        assert_eq!(items.len(), 2, "earlier observation was lost: {items:?}");
    }

    /// Forward compatibility: an item written by a newer build must survive a
    /// round-trip through this one, because items are opaque at the storage layer.
    #[tokio::test]
    async fn an_unknown_evidence_shape_is_preserved() {
        let db = test_db().await;
        let game = seed_game(&db, "Tunic").await;
        let future = json!({"SomethingFromTheFuture": {"weight": 42, "nested": {"x": true}}});

        let id = db.upsert_save_candidate(&game, "C:/Docs/Tunic", "saves", std::slice::from_ref(&future)).await.unwrap();
        // A second, ordinary observation forces a read-modify-write of the envelope.
        db.upsert_save_candidate(&game, "C:/Docs/Tunic", "saves", &[json!({"NameMatch": {"alias": "Tunic", "similarity": 1.0}})])
            .await
            .unwrap();

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        let items = EvidenceEnvelope::parse(&c.evidence_json).items;
        assert!(items.contains(&future), "unknown evidence was dropped: {items:?}");
    }

    #[tokio::test]
    async fn a_corrupt_envelope_is_treated_as_empty_rather_than_failing() {
        let db = test_db().await;
        let game = seed_game(&db, "Braid").await;
        let id = db.upsert_save_candidate(&game, "C:/Docs/Braid", "saves", &[]).await.unwrap();

        sqlx::query("UPDATE save_candidates SET evidence_json = 'not json' WHERE id = ?1")
            .bind(id)
            .execute(&db.pool)
            .await
            .unwrap();

        // Must not error — detection continues, having lost only the unreadable row.
        db.upsert_save_candidate(&game, "C:/Docs/Braid", "saves", &[json!({"NameMatch": {"alias": "Braid", "similarity": 1.0}})])
            .await
            .expect("a corrupt envelope must not fail the scan");

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        assert_eq!(EvidenceEnvelope::parse(&c.evidence_json).items.len(), 1);
    }

    #[tokio::test]
    async fn a_decision_records_its_rule_and_explanation() {
        let db = test_db().await;
        let game = seed_game(&db, "Dishonored").await;
        let id = db.upsert_save_candidate(&game, "C:/Docs/Dishonored", "saves", &[]).await.unwrap();

        db.set_candidate_decision(id, "bind_eligible", 5, "Known save location for this game.", 0.9)
            .await
            .unwrap();

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        assert_eq!(c.status, "bind_eligible");
        assert_eq!(c.decided_by_rule, Some(5));
        assert!(!c.explanation.unwrap().is_empty());
        assert!(c.last_scored_at.is_some());
    }

    #[tokio::test]
    async fn rejection_is_terminal_and_explained() {
        let db = test_db().await;
        let game = seed_game(&db, "Outer Wilds").await;
        let id = db.upsert_save_candidate(&game, "C:/Docs/Photos", "saves", &[]).await.unwrap();

        db.reject_save_candidate(id).await.unwrap();
        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        assert_eq!(c.status, "rejected");
        assert!(c.explanation.is_some_and(|e| !e.is_empty()));
    }

    #[tokio::test]
    async fn candidates_are_listed_by_score_descending() {
        let db = test_db().await;
        let game = seed_game(&db, "Stray").await;
        let low = db.upsert_save_candidate(&game, "C:/Docs/Stray-ish", "saves", &[]).await.unwrap();
        let high = db.upsert_save_candidate(&game, "C:/Docs/Stray", "saves", &[]).await.unwrap();
        db.set_candidate_decision(low, "suggested", 9, "Folder name matches this game.", 0.2).await.unwrap();
        db.set_candidate_decision(high, "suggested", 8, "Contains save files.", 0.8).await.unwrap();

        let listed = db.list_save_candidates(&game).await.unwrap();
        assert_eq!(listed.first().map(|c| c.id), Some(high));
    }

    #[tokio::test]
    async fn a_scan_attempt_counts_up_and_carries_its_backoff() {
        let db = test_db().await;
        let game = seed_game(&db, "Inside").await;

        db.record_scan_attempt(&game, "nothing", Some("2026-08-01T00:00:00+00:00")).await.unwrap();
        db.record_scan_attempt(&game, "nothing", Some("2026-08-02T00:00:00+00:00")).await.unwrap();

        let a = db.scan_attempt(&game).await.unwrap().unwrap();
        assert_eq!(a.attempt_count, 2);
        assert_eq!(a.outcome, "nothing");
        assert_eq!(a.next_retry_at.as_deref(), Some("2026-08-02T00:00:00+00:00"));
    }

    /// A found result has nothing to retry — positive results do not expire.
    #[tokio::test]
    async fn a_successful_scan_records_no_retry_time() {
        let db = test_db().await;
        let game = seed_game(&db, "Limbo").await;
        db.record_scan_attempt(&game, "suggested", None).await.unwrap();

        let a = db.scan_attempt(&game).await.unwrap().unwrap();
        assert!(a.next_retry_at.is_none());
    }

    /// What a KB refresh calls: everything becomes eligible again, and no
    /// candidate or piece of evidence is touched.
    #[tokio::test]
    async fn clearing_backoff_reopens_scans_without_losing_evidence() {
        let db = test_db().await;
        let game = seed_game(&db, "Gris").await;
        let id = db
            .upsert_save_candidate(&game, "C:/Docs/Gris", "saves", &[json!({"NameMatch": {"alias": "Gris", "similarity": 1.0}})])
            .await
            .unwrap();
        db.record_scan_attempt(&game, "nothing", Some("2099-01-01T00:00:00+00:00")).await.unwrap();

        let cleared = db.clear_scan_backoff().await.unwrap();
        assert_eq!(cleared, 1);
        assert!(db.scan_attempt(&game).await.unwrap().unwrap().next_retry_at.is_none());

        let c = db.get_save_candidate(id).await.unwrap().unwrap();
        assert_eq!(EvidenceEnvelope::parse(&c.evidence_json).items.len(), 1, "evidence lost");
    }

    #[tokio::test]
    async fn candidates_and_attempts_are_removed_with_their_game() {
        let db = test_db().await;
        let game = seed_game(&db, "Journey").await;
        db.upsert_save_candidate(&game, "C:/Docs/Journey", "saves", &[]).await.unwrap();
        db.record_scan_attempt(&game, "nothing", None).await.unwrap();

        sqlx::query("DELETE FROM games WHERE id = ?1").bind(&game).execute(&db.pool).await.unwrap();

        assert!(db.list_save_candidates(&game).await.unwrap().is_empty());
        assert!(db.scan_attempt(&game).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn the_status_check_rejects_an_unknown_value() {
        let db = test_db().await;
        let game = seed_game(&db, "Cocoon").await;
        let id = db.upsert_save_candidate(&game, "C:/Docs/Cocoon", "saves", &[]).await.unwrap();

        let bad = sqlx::query("UPDATE save_candidates SET status = 'bound' WHERE id = ?1")
            .bind(id)
            .execute(&db.pool)
            .await;
        assert!(bad.is_err(), "status CHECK should reject 'bound' — binding is Phase 3");
    }
}
