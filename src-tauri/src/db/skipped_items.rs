//! Library items the scanner declined to import.
//!
//! Recorded rather than discarded, for the same reason `save_candidates` keeps its
//! rejections: an item that disappears with no explanation is indistinguishable from a
//! scanner bug, and "why is my game missing?" needs an answer that does not require
//! re-running a scan with logging turned up.
//!
//! `override_import` is the storage for a future "Import anyway" action. The scanner reads
//! it; nothing writes it yet, because the UI is a later phase.

use crate::error::AppResult;
use crate::models::now_rfc3339;

use super::Db;

/// One item kept out of the library.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SkippedLibraryItem {
    pub id: i64,
    pub source_code: String,
    pub source_app_id: Option<String>,
    pub title: String,
    pub install_dir: Option<String>,
    /// Which filter rule fired.
    pub rule: String,
    /// The sentence explaining it.
    pub reason: String,
    /// 1 once the user has asked for it anyway.
    pub override_import: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

impl Db {
    /// Record that an item was skipped, or refresh its `last_seen_at`.
    ///
    /// Idempotent per `(source_code, source_app_id, title)`. `first_seen_at` is preserved
    /// across rescans and **`override_import` is never touched** — a rescan must not undo a
    /// user's decision to import something anyway.
    pub async fn record_skipped_item(
        &self,
        source_code: &str,
        source_app_id: Option<&str>,
        title: &str,
        install_dir: Option<&str>,
        rule: &str,
        reason: &str,
    ) -> AppResult<()> {
        let now = now_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO skipped_library_items
              (source_code, source_app_id, title, install_dir, rule, reason,
               first_seen_at, last_seen_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            ON CONFLICT(source_code, source_app_id, title) DO UPDATE SET
              install_dir = excluded.install_dir,
              rule = excluded.rule,
              reason = excluded.reason,
              last_seen_at = excluded.last_seen_at
            "#,
        )
        .bind(source_code)
        .bind(source_app_id)
        .bind(title)
        .bind(install_dir)
        .bind(rule)
        .bind(reason)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Everything the scanner has declined, newest first.
    pub async fn list_skipped_items(&self) -> AppResult<Vec<SkippedLibraryItem>> {
        Ok(sqlx::query_as::<_, SkippedLibraryItem>(
            "SELECT * FROM skipped_library_items ORDER BY last_seen_at DESC, title ASC",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Whether the user has asked for this item to be imported despite the filter.
    ///
    /// Consulted by the scanner before applying a skip, so an override survives every
    /// subsequent rescan.
    pub async fn is_import_overridden(
        &self,
        source_code: &str,
        source_app_id: Option<&str>,
        title: &str,
    ) -> AppResult<bool> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT override_import FROM skipped_library_items
              WHERE source_code = ?1 AND source_app_id IS ?2 AND title = ?3",
        )
        .bind(source_code)
        .bind(source_app_id)
        .bind(title)
        .fetch_optional(&self.pool)
        .await?;
        Ok(found == Some(1))
    }

    /// Set or clear the "import anyway" flag. The command surface for a future UI.
    pub async fn set_import_override(&self, id: i64, import: bool) -> AppResult<()> {
        sqlx::query("UPDATE skipped_library_items SET override_import = ?1 WHERE id = ?2")
            .bind(i64::from(import))
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::test_db;

    #[tokio::test]
    async fn a_skip_is_recorded_and_listed() {
        let db = test_db().await;
        db.record_skipped_item(
            "steam",
            Some("228980"),
            "Steamworks Common Redistributables",
            Some("D:/x"),
            "steam_system_app_id",
            "not a game",
        )
        .await
        .unwrap();

        let all = db.list_skipped_items().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].rule, "steam_system_app_id");
        assert_eq!(all[0].override_import, 0);
    }

    #[tokio::test]
    async fn recording_twice_updates_rather_than_duplicates() {
        let db = test_db().await;
        for reason in ["first", "second"] {
            db.record_skipped_item("steam", Some("1"), "X", None, "r", reason)
                .await
                .unwrap();
        }
        let all = db.list_skipped_items().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].reason, "second");
        assert_eq!(
            all[0].first_seen_at, all[0].first_seen_at,
            "first_seen_at is preserved"
        );
    }

    /// **A rescan must not undo a user's choice.** Once "import anyway" is set, seeing the
    /// item again must leave the flag alone.
    #[tokio::test]
    async fn a_rescan_does_not_clear_an_override() {
        let db = test_db().await;
        db.record_skipped_item("steam", Some("1"), "X", None, "r", "why")
            .await
            .unwrap();
        let id = db.list_skipped_items().await.unwrap()[0].id;
        db.set_import_override(id, true).await.unwrap();

        db.record_skipped_item("steam", Some("1"), "X", None, "r", "why again")
            .await
            .unwrap();

        assert!(db
            .is_import_overridden("steam", Some("1"), "X")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn an_unknown_item_is_not_overridden() {
        let db = test_db().await;
        assert!(!db
            .is_import_overridden("steam", Some("nope"), "Nothing")
            .await
            .unwrap());
    }
}
