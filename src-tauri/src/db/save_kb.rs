//! Knowledge-base persistence.
//!
//! One table holds all three layers (`builtin`, `community`, `user`) because
//! matching wants them at once — see `docs/architecture/KNOWLEDGE_BASE.md` §3.
//! Layer ordering is applied in the query, not by table.
//!
//! The invariant this module exists to protect: **a layer replacement touches only
//! that layer.** A KB refresh must never disturb a user's own entries
//! (invariant I7). It is enforced by a `WHERE layer = ?` delete inside a
//! transaction, and asserted by `refreshing_builtin_leaves_the_user_layer_intact`.

use crate::error::AppResult;
use crate::models::{now_rfc3339, SaveKbEntry, SaveKbVersion};

use super::Db;

/// An entry to be written. Distinct from [`SaveKbEntry`] because `created_at` is
/// assigned here, not supplied by a caller.
#[derive(Debug, Clone)]
pub struct NewKbEntry {
    pub id: String,
    pub match_kind: String,
    pub match_value: String,
    pub platform: String,
    pub role: String,
    /// See crate::saves::kb::layout. Empty is normalised to unspecified.
    pub layout: String,
    pub path_template: String,
    pub glob: Option<String>,
    pub priority: i64,
    pub note: Option<String>,
    pub source_ref: Option<String>,
}

/// One identity a game can be matched by, in precedence order when several are
/// known. See `KNOWLEDGE_BASE.md` §4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchKey {
    pub kind: String,
    pub value: String,
}

impl MatchKey {
    pub fn new(kind: &str, value: &str) -> Self {
        Self {
            kind: kind.to_string(),
            value: value.to_string(),
        }
    }
}

/// Store `unspecified` rather than an empty layout.
///
/// An empty string would be indistinguishable from "not classified" while sorting and
/// comparing differently, and `layout::authority` would classify both the same way. One
/// spelling for one meaning.
fn normalised_layout(layout: &str) -> &str {
    let trimmed = layout.trim();
    if trimmed.is_empty() {
        crate::saves::kb::layout::UNSPECIFIED
    } else {
        trimmed
    }
}

impl Db {
    /// Replace one layer wholesale, in a single transaction.
    ///
    /// A partially applied KB is worse than an old one, because the failure is
    /// silent and the symptom (a missing game) looks like a detection bug. Either
    /// the whole layer and its version row land, or nothing does.
    pub async fn replace_kb_layer(
        &self,
        layer: &str,
        version: &str,
        checksum: &str,
        source_url: Option<&str>,
        entries: &[NewKbEntry],
    ) -> AppResult<usize> {
        let now = now_rfc3339();
        let mut tx = self.pool.begin().await?;

        // Scoped to the layer. This single `WHERE` is what keeps a refresh from
        // deleting a user's entries.
        sqlx::query("DELETE FROM save_kb_entries WHERE layer = ?1")
            .bind(layer)
            .execute(&mut *tx)
            .await?;

        for e in entries {
            sqlx::query(
                r#"
                INSERT INTO save_kb_entries
                  (id, layer, match_kind, match_value, platform, role, layout,
                   path_template, glob, priority, note, source_ref,
                   kb_version, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                "#,
            )
            .bind(&e.id)
            .bind(layer)
            .bind(&e.match_kind)
            .bind(&e.match_value)
            .bind(&e.platform)
            .bind(&e.role)
            .bind(normalised_layout(&e.layout))
            .bind(&e.path_template)
            .bind(e.glob.as_deref())
            .bind(e.priority)
            .bind(e.note.as_deref())
            .bind(e.source_ref.as_deref())
            .bind(version)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            INSERT INTO save_kb_versions
              (layer, version, checksum, entry_count, applied_at, source_url)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(layer) DO UPDATE SET
              version = excluded.version,
              checksum = excluded.checksum,
              entry_count = excluded.entry_count,
              applied_at = excluded.applied_at,
              source_url = excluded.source_url
            "#,
        )
        .bind(layer)
        .bind(version)
        .bind(checksum)
        .bind(entries.len() as i64)
        .bind(&now)
        .bind(source_url)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(entries.len())
    }

    /// Append a single entry without disturbing the rest of its layer.
    ///
    /// Used for the user layer, where entries accumulate one correction at a time
    /// rather than arriving as a versioned payload.
    pub async fn add_kb_entry(&self, layer: &str, entry: &NewKbEntry) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO save_kb_entries
              (id, layer, match_kind, match_value, platform, role, layout,
               path_template, glob, priority, note, source_ref,
               kb_version, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'user', ?13)
            "#,
        )
        .bind(&entry.id)
        .bind(layer)
        .bind(&entry.match_kind)
        .bind(&entry.match_value)
        .bind(&entry.platform)
        .bind(&entry.role)
        .bind(normalised_layout(&entry.layout))
        .bind(&entry.path_template)
        .bind(entry.glob.as_deref())
        .bind(entry.priority)
        .bind(entry.note.as_deref())
        .bind(entry.source_ref.as_deref())
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_kb_entry(&self, id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM save_kb_entries WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Entries matching any of `keys`, plus every library-wide (`any`) entry.
    ///
    /// Ordered by layer (`user` → `community` → `builtin`), then `priority`, then
    /// `id` so the result is stable — scenario tests depend on a deterministic
    /// order.
    pub async fn match_kb_entries(
        &self,
        platform: &str,
        role: &str,
        keys: &[MatchKey],
    ) -> AppResult<Vec<SaveKbEntry>> {
        // Placeholders are generated, values are bound — never interpolated.
        let pairs = (0..keys.len())
            .map(|i| format!("(match_kind = ?{} AND match_value = ?{})", i * 2 + 3, i * 2 + 4))
            .collect::<Vec<_>>()
            .join(" OR ");
        let predicate = if pairs.is_empty() {
            "match_kind = 'any'".to_string()
        } else {
            format!("match_kind = 'any' OR {pairs}")
        };

        let sql = format!(
            r#"
            SELECT * FROM save_kb_entries
            WHERE platform = ?1 AND role = ?2 AND ({predicate})
            ORDER BY
              CASE layer WHEN 'user' THEN 0 WHEN 'community' THEN 1 ELSE 2 END,
              priority,
              id
            "#
        );

        let mut q = sqlx::query_as::<_, SaveKbEntry>(&sql).bind(platform).bind(role);
        for k in keys {
            q = q.bind(&k.kind).bind(&k.value);
        }
        Ok(q.fetch_all(&self.pool).await?)
    }

    pub async fn kb_versions(&self) -> AppResult<Vec<SaveKbVersion>> {
        Ok(
            sqlx::query_as::<_, SaveKbVersion>("SELECT * FROM save_kb_versions ORDER BY layer")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn kb_version(&self, layer: &str) -> AppResult<Option<SaveKbVersion>> {
        Ok(
            sqlx::query_as::<_, SaveKbVersion>("SELECT * FROM save_kb_versions WHERE layer = ?1")
                .bind(layer)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// One entry by id, whatever layer it belongs to.
    ///
    /// The layer is returned rather than filtered so a caller can enforce its own
    /// scope — `saves::kb::import` uses it to refuse deleting a shipped entry.
    pub async fn kb_entry(&self, id: &str) -> AppResult<Option<SaveKbEntry>> {
        Ok(
            sqlx::query_as::<_, SaveKbEntry>("SELECT * FROM save_kb_entries WHERE id = ?1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn count_kb_entries(&self, layer: &str) -> AppResult<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM save_kb_entries WHERE layer = ?1")
                .bind(layer)
                .fetch_one(&self.pool)
                .await?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_db;

    fn entry(id: &str, kind: &str, value: &str, template: &str) -> NewKbEntry {
        NewKbEntry {
            id: id.into(),
            match_kind: kind.into(),
            match_value: value.into(),
            platform: "windows".into(),
            role: "saves".into(),
            layout: crate::saves::kb::layout::OFFICIAL.into(),
            path_template: template.into(),
            glob: None,
            priority: 100,
            note: None,
            source_ref: Some("test".into()),
        }
    }

    #[tokio::test]
    async fn a_layer_and_its_version_are_written_together() {
        let db = test_db().await;
        db.replace_kb_layer(
            "builtin",
            "v1",
            "abc123",
            None,
            &[entry("builtin:a", "steam_appid", "220", "{MYGAMES}/Half-Life 2")],
        )
        .await
        .unwrap();

        assert_eq!(db.count_kb_entries("builtin").await.unwrap(), 1);
        let v = db.kb_version("builtin").await.unwrap().expect("version row");
        assert_eq!(v.version, "v1");
        assert_eq!(v.checksum, "abc123");
        assert_eq!(v.entry_count, 1);
    }

    /// Invariant I7. The single most important property of this module: a KB
    /// refresh must never touch a user's own entries.
    #[tokio::test]
    async fn refreshing_builtin_leaves_the_user_layer_intact() {
        let db = test_db().await;
        db.add_kb_entry(
            "user",
            &entry("user:mine", "any", "", "D:/Games/Saves/{TITLE}"),
        )
        .await
        .unwrap();
        db.replace_kb_layer("builtin", "v1", "c1", None, &[entry("builtin:a", "steam_appid", "220", "{MYGAMES}/HL2")])
            .await
            .unwrap();

        // Replace builtin again — the user entry must survive both writes.
        db.replace_kb_layer("builtin", "v2", "c2", None, &[entry("builtin:b", "steam_appid", "440", "{MYGAMES}/TF2")])
            .await
            .unwrap();

        assert_eq!(db.count_kb_entries("user").await.unwrap(), 1, "user layer was disturbed");
        assert_eq!(db.count_kb_entries("builtin").await.unwrap(), 1, "builtin was not replaced");
    }

    #[tokio::test]
    async fn replacing_a_layer_removes_its_previous_entries() {
        let db = test_db().await;
        db.replace_kb_layer("builtin", "v1", "c1", None, &[entry("builtin:a", "steam_appid", "1", "{MYGAMES}/A"), entry("builtin:b", "steam_appid", "2", "{MYGAMES}/B")])
            .await
            .unwrap();
        db.replace_kb_layer("builtin", "v2", "c2", None, &[entry("builtin:c", "steam_appid", "3", "{MYGAMES}/C")])
            .await
            .unwrap();

        assert_eq!(db.count_kb_entries("builtin").await.unwrap(), 1);
        let v = db.kb_version("builtin").await.unwrap().unwrap();
        assert_eq!((v.version.as_str(), v.entry_count), ("v2", 1));
    }

    #[tokio::test]
    async fn matching_returns_only_the_requested_platform_and_role() {
        let db = test_db().await;
        let mut linux = entry("builtin:linux", "steam_appid", "220", "~/.local/share/HL2");
        linux.platform = "linux".into();
        let mut config = entry("builtin:config", "steam_appid", "220", "{APPDATA}/HL2");
        config.role = "config".into();

        db.replace_kb_layer(
            "builtin",
            "v1",
            "c",
            None,
            &[entry("builtin:win", "steam_appid", "220", "{MYGAMES}/HL2"), linux, config],
        )
        .await
        .unwrap();

        let found = db
            .match_kb_entries("windows", "saves", &[MatchKey::new("steam_appid", "220")])
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "builtin:win");
    }

    #[tokio::test]
    async fn library_wide_any_entries_always_match() {
        let db = test_db().await;
        db.add_kb_entry("user", &entry("user:all", "any", "", "D:/Saves/{TITLE}"))
            .await
            .unwrap();

        // No keys at all — an `any` rule must still be returned.
        let found = db.match_kb_entries("windows", "saves", &[]).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "user:all");
    }

    #[tokio::test]
    async fn results_are_ordered_user_then_community_then_builtin() {
        let db = test_db().await;
        db.replace_kb_layer("builtin", "v1", "c", None, &[entry("z:builtin", "steam_appid", "220", "{MYGAMES}/B")])
            .await
            .unwrap();
        db.replace_kb_layer("community", "v1", "c", None, &[entry("y:community", "steam_appid", "220", "{MYGAMES}/C")])
            .await
            .unwrap();
        db.add_kb_entry("user", &entry("x:user", "steam_appid", "220", "{MYGAMES}/U"))
            .await
            .unwrap();

        let layers: Vec<String> = db
            .match_kb_entries("windows", "saves", &[MatchKey::new("steam_appid", "220")])
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.layer)
            .collect();
        assert_eq!(layers, vec!["user", "community", "builtin"]);
    }

    #[tokio::test]
    async fn a_non_matching_key_returns_nothing() {
        let db = test_db().await;
        db.replace_kb_layer("builtin", "v1", "c", None, &[entry("builtin:a", "steam_appid", "220", "{MYGAMES}/HL2")])
            .await
            .unwrap();

        let found = db
            .match_kb_entries("windows", "saves", &[MatchKey::new("steam_appid", "999")])
            .await
            .unwrap();
        assert!(found.is_empty());
    }

    #[tokio::test]
    async fn the_check_constraints_reject_out_of_set_values() {
        let db = test_db().await;
        let mut bad_layer = entry("bad:1", "steam_appid", "1", "{MYGAMES}/X");
        bad_layer.platform = "windows".into();
        assert!(
            db.add_kb_entry("nonsense", &bad_layer).await.is_err(),
            "layer CHECK should reject an unknown layer"
        );

        let mut bad_kind = entry("bad:2", "vibes", "1", "{MYGAMES}/X");
        bad_kind.platform = "windows".into();
        assert!(
            db.add_kb_entry("user", &bad_kind).await.is_err(),
            "match_kind CHECK should reject an unknown kind"
        );
    }

    #[tokio::test]
    async fn deleting_a_user_entry_leaves_the_others() {
        let db = test_db().await;
        db.add_kb_entry("user", &entry("user:a", "any", "", "D:/A/{TITLE}"))
            .await
            .unwrap();
        db.add_kb_entry("user", &entry("user:b", "any", "", "D:/B/{TITLE}"))
            .await
            .unwrap();

        db.delete_kb_entry("user:a").await.unwrap();
        assert_eq!(db.count_kb_entries("user").await.unwrap(), 1);
    }
}
