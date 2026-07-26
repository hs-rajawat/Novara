//! The Steam title-match cache.
//!
//! Epic and manually-imported games carry no Steam app-id, so every Steam-backed
//! provider returns `Unsupported` for them and they get no metadata or artwork at
//! all. Resolving their title to a Steam app-id once, and remembering the answer,
//! makes the whole existing pipeline work for them.
//!
//! Reads here are pure and offline: the network lookup that populates this table
//! is a separate pass, so building a `GameIdentity` never touches the network.

use crate::error::AppResult;
use crate::models::now_rfc3339;

use super::Db;

/// A recorded search outcome. `app_id` is `None` when Steam was asked and had
/// nothing — a real answer, not a missing one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamTitleMatch {
    pub app_id: Option<String>,
    pub matched_title: Option<String>,
    pub settled_by: String,
}

impl Db {
    /// The recorded outcome for a game, or `None` if it has never been searched.
    pub async fn steam_title_match(&self, game_id: &str) -> AppResult<Option<SteamTitleMatch>> {
        let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT app_id, matched_title, settled_by FROM steam_title_matches WHERE game_id = ?1",
        )
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(app_id, matched_title, settled_by)| SteamTitleMatch {
            app_id,
            matched_title,
            settled_by,
        }))
    }

    /// Record a search outcome, replacing any previous one for the game.
    ///
    /// A re-search only happens when the resolver's fingerprint has changed, so
    /// overwriting is the intended behaviour: the newer resolver's answer
    /// supersedes the older one's.
    pub async fn record_steam_title_match(
        &self,
        game_id: &str,
        app_id: Option<&str>,
        matched_title: Option<&str>,
        settled_by: &str,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO steam_title_matches
                (game_id, app_id, matched_title, settled_by, resolved_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(game_id) DO UPDATE SET
                app_id        = excluded.app_id,
                matched_title = excluded.matched_title,
                settled_by    = excluded.settled_by,
                resolved_at   = excluded.resolved_at
            "#,
        )
        .bind(game_id)
        .bind(app_id)
        .bind(matched_title)
        .bind(settled_by)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Games worth asking Steam about, as `(game_id, title)`.
    ///
    /// A game qualifies when it has no Steam app-id of its own and either has
    /// never been searched or was last searched by a different resolver.
    ///
    /// Three exclusions, each load-bearing:
    ///   * games that already carry a Steam app-id — their identity is known, and
    ///     a title search could only contradict it;
    ///   * hidden games — the user removed them from the library, so looking them
    ///     up is work nobody asked for (matching both fill services);
    ///   * games settled by the current resolver, whether matched or not, which is
    ///     what stops an unmatchable game being re-queried on every sweep.
    pub async fn games_needing_title_search(
        &self,
        settled_by: &str,
    ) -> AppResult<Vec<(String, String)>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT g.id, g.title
            FROM games g
            LEFT JOIN steam_title_matches m ON m.game_id = g.id
            WHERE g.is_hidden = 0
              AND NOT EXISTS (
                    SELECT 1
                    FROM game_installations gi
                    JOIN sources s ON s.id = gi.source_id
                    WHERE gi.game_id = g.id
                      AND s.code = 'steam'
                      AND gi.source_app_id IS NOT NULL
              )
              AND (m.game_id IS NULL OR m.settled_by <> ?1)
            ORDER BY g.sort_title
            "#,
        )
        .bind(settled_by)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
