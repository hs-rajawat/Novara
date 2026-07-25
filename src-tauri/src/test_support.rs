//! Test-only fixtures. Compiled out of any non-test build.
//!
//! Why this exists: the audit that produced the remediation plan found
//! three data-correctness bugs in `db/*` (a `NULL` decode that fails on
//! every fresh install, and two in `merge_games`) that had been present
//! since the code was written. None were caught, because there was no way
//! to exercise a repository without a real application. This module is
//! that way.
//!
//! Two harnesses:
//!   • [`test_db`] — a real migrated SQLite database, in memory, isolated
//!     per call. Runs the actual migrations, so a schema mistake fails a
//!     test rather than surfacing at runtime.
//!   • [`FakeTextProvider`] / [`FakeArtworkProvider`] — scriptable
//!     providers that record their call count, so provider-chain
//!     behaviour, circuit breaking, priority tie-breaks and the
//!     "no network unless explicitly allowed" guarantee can be asserted
//!     without touching a network.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::db::Db;
use crate::metadata::{
    ArtworkKind, AssetDescriptor, AssetSource, GameIdentifier, GameIdentity, GameMetadata, Lookup,
    LookupContext, MetadataTextProvider, ProviderCapabilities, ProviderIdentity,
};
use crate::models::now_rfc3339;

/// Ensures every `test_db()` call gets its own isolated database even when
/// tests run concurrently on separate threads.
static DB_SEQ: AtomicUsize = AtomicUsize::new(0);

/// A migrated, empty database held entirely in memory.
///
/// Implementation notes, both load-bearing:
///   • The URI form `file:<name>?mode=memory&cache=shared` is used rather
///     than plain `:memory:` because a plain in-memory database is private
///     to a single connection, so a pool of more than one would silently
///     hand out *different* empty databases. Shared cache plus a unique
///     name per fixture gives production-like pooling with real isolation
///     between tests.
///   • `min_connections(1)` with no idle timeout keeps one connection alive
///     for the fixture's lifetime. A shared-cache memory database is
///     destroyed when its last connection closes, so without this a pool
///     that went briefly idle would drop the schema mid-test.
///   • WAL is deliberately *not* set, unlike production: it is meaningless
///     for a memory database. Everything else — foreign key enforcement,
///     pool size, the migrations themselves — matches production.
pub async fn test_db() -> Db {
    let id = DB_SEQ.fetch_add(1, Ordering::SeqCst);
    let url = format!("sqlite:file:novara_test_{id}?mode=memory&cache=shared");

    let opts = SqliteConnectOptions::from_str(&url)
        .expect("test db connect options")
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .min_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect_with(opts)
        .await
        .expect("open in-memory test database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations against test database");

    Db { pool }
}

/// Insert a bare `games` row and return its id.
///
/// Deliberately raw SQL rather than going through `upsert_game`: fixtures
/// for a test of `upsert_game` must not be built by the function under
/// test.
pub async fn seed_game(db: &Db, title: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO games (id, title, sort_title, added_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
    )
    .bind(&id)
    .bind(title)
    .bind(title.to_lowercase())
    .bind(&now)
    .execute(&db.pool)
    .await
    .expect("seed game");
    id
}

/// Insert an installation row for `game_id` and return its id.
///
/// `source` is a `sources.code` value (`"steam"`, `"epic"`, `"manual"`, …)
/// which is resolved to the seeded `sources.id`; the column is
/// `source_id INTEGER`, not a text code.
pub async fn seed_installation(
    db: &Db,
    game_id: &str,
    source: &str,
    install_dir: &str,
    executable: Option<&str>,
    is_primary: bool,
    status: &str,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE code = ?1")
        .bind(source)
        .fetch_one(&db.pool)
        .await
        .unwrap_or_else(|e| panic!("unknown source code {source:?}: {e}"));
    sqlx::query(
        "INSERT INTO game_installations
           (id, game_id, source_id, install_dir, executable, launch_args,
            source_app_id, install_size_bytes, is_primary, detected_at,
            executable_override, status, last_verified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, 0, ?6, ?7, 0, ?8, NULL)",
    )
    .bind(&id)
    .bind(game_id)
    .bind(source_id)
    .bind(install_dir)
    .bind(executable)
    .bind(i64::from(is_primary))
    .bind(&now)
    .bind(status)
    .execute(&db.pool)
    .await
    .expect("seed installation");
    id
}

/// Attach a genre to a game, creating the genre row if needed.
pub async fn seed_genre(db: &Db, game_id: &str, genre: &str) {
    sqlx::query("INSERT OR IGNORE INTO genres (name) VALUES (?1)")
        .bind(genre)
        .execute(&db.pool)
        .await
        .expect("seed genre");
    sqlx::query(
        "INSERT OR IGNORE INTO game_genres (game_id, genre_id)
         SELECT ?1, id FROM genres WHERE name = ?2",
    )
    .bind(game_id)
    .bind(genre)
    .execute(&db.pool)
    .await
    .expect("link genre");
}

/// A `GameIdentity` carrying a source app-id, for provider tests.
pub fn identity_with_app_id(title: &str, source: &str, app_id: &str) -> GameIdentity {
    GameIdentity {
        title: title.to_string(),
        identifiers: vec![GameIdentifier::SourceAppId {
            source: source.to_string(),
            id: app_id.to_string(),
        }],
    }
}

/// A `GameIdentity` with no identifiers beyond its title.
pub fn identity_title_only(title: &str) -> GameIdentity {
    GameIdentity {
        title: title.to_string(),
        identifiers: vec![],
    }
}

/// Records how many times a fake provider was asked to resolve something.
/// Cloneable so a test can hold a handle while the registry owns the
/// provider as a trait object.
#[derive(Clone, Default)]
pub struct CallLog(Arc<AtomicU32>);

impl CallLog {
    pub fn count(&self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }

    fn record(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A scriptable [`MetadataTextProvider`].
pub struct FakeTextProvider {
    code: &'static str,
    priority: u8,
    requires_network: bool,
    response: Lookup<GameMetadata>,
    pub calls: CallLog,
}

impl FakeTextProvider {
    pub fn new(code: &'static str, priority: u8, response: Lookup<GameMetadata>) -> Self {
        Self {
            code,
            priority,
            requires_network: false,
            response,
            calls: CallLog::default(),
        }
    }

    /// Mark this provider as network-dependent. Services must filter it out
    /// entirely when network access is not permitted — a test asserting the
    /// privacy guarantee checks `calls.count() == 0`, which is stronger
    /// than checking that it returned nothing.
    pub fn requiring_network(mut self) -> Self {
        self.requires_network = true;
        self
    }
}

impl ProviderIdentity for FakeTextProvider {
    fn code(&self) -> &'static str {
        self.code
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::TEXT
    }
}

#[async_trait]
impl MetadataTextProvider for FakeTextProvider {
    fn priority(&self) -> u8 {
        self.priority
    }
    fn requires_network(&self) -> bool {
        self.requires_network
    }
    async fn resolve_text(&self, _ctx: &LookupContext<'_>) -> Lookup<GameMetadata> {
        self.calls.record();
        self.response.clone()
    }
}

/// A scriptable [`crate::metadata::ArtworkProvider`].
pub struct FakeArtworkProvider {
    code: &'static str,
    priority: u8,
    requires_network: bool,
    response: Lookup<Vec<AssetDescriptor>>,
    pub calls: CallLog,
}

impl FakeArtworkProvider {
    pub fn new(code: &'static str, priority: u8, response: Lookup<Vec<AssetDescriptor>>) -> Self {
        Self {
            code,
            priority,
            requires_network: false,
            response,
            calls: CallLog::default(),
        }
    }

    /// Supplies exactly the given kinds as local files, so no network and
    /// no real bytes are involved.
    pub fn supplying(code: &'static str, priority: u8, kinds: &[ArtworkKind]) -> Self {
        let descriptors = kinds
            .iter()
            .map(|kind| AssetDescriptor {
                kind: *kind,
                source: AssetSource::LocalFile(PathBuf::from(format!(
                    "C:/fake/{code}/{}.png",
                    kind.as_str()
                ))),
                provider: code,
            })
            .collect();
        Self::new(code, priority, Lookup::Found(descriptors))
    }

    pub fn requiring_network(mut self) -> Self {
        self.requires_network = true;
        self
    }
}

impl ProviderIdentity for FakeArtworkProvider {
    fn code(&self) -> &'static str {
        self.code
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::ARTWORK
    }
}

#[async_trait]
impl crate::metadata::ArtworkProvider for FakeArtworkProvider {
    fn priority(&self) -> u8 {
        self.priority
    }
    fn requires_network(&self) -> bool {
        self.requires_network
    }
    async fn resolve_artwork(&self, _ctx: &LookupContext<'_>) -> Lookup<Vec<AssetDescriptor>> {
        self.calls.record();
        self.response.clone()
    }
}

#[cfg(test)]
mod harness_tests {
    use super::*;

    /// The fixture must produce a real, migrated schema — not an empty
    /// database that silently passes every assertion.
    ///
    /// The expected version is derived from the migrations directory rather
    /// than hardcoded, so adding a migration cannot leave this test asserting
    /// a stale number.
    #[tokio::test]
    async fn fixture_runs_all_migrations() {
        let db = test_db().await;
        let version: i64 =
            sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&db.pool)
                .await
                .expect("read migration state");

        let expected = std::fs::read_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations"),
        )
        .expect("read migrations dir")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .count() as i64;

        assert_eq!(
            version, expected,
            "every migration on disk should be applied"
        );
    }

    /// Regression guard for the pooling subtlety documented on `test_db`:
    /// with a private in-memory database this fails, because a second
    /// pooled connection would see a different, empty database.
    #[tokio::test]
    async fn fixture_shares_one_database_across_pooled_connections() {
        let db = test_db().await;
        let id = seed_game(&db, "Portal 2").await;

        let mut handles = Vec::new();
        for _ in 0..4 {
            let db = db.clone();
            let id = id.clone();
            handles.push(tokio::spawn(async move {
                let found: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM games WHERE id = ?1")
                        .bind(&id)
                        .fetch_one(&db.pool)
                        .await
                        .expect("query game");
                found
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 1, "every connection sees the same database");
        }
    }

    #[tokio::test]
    async fn fixtures_are_isolated_from_each_other() {
        let a = test_db().await;
        let b = test_db().await;
        seed_game(&a, "Hades").await;

        let in_a: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
            .fetch_one(&a.pool)
            .await
            .unwrap();
        let in_b: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games")
            .fetch_one(&b.pool)
            .await
            .unwrap();
        assert_eq!((in_a, in_b), (1, 0), "fixtures must not share state");
    }

    /// Foreign keys must be enforced, or tests would pass against data the
    /// real application could never produce.
    #[tokio::test]
    async fn fixture_enforces_foreign_keys() {
        let db = test_db().await;
        let result = sqlx::query(
            "INSERT INTO artwork_assets (game_id, kind, source, updated_at)
             VALUES ('no-such-game', 'cover', 'steam_cdn', '2026-01-01T00:00:00+00:00')",
        )
        .execute(&db.pool)
        .await;
        assert!(result.is_err(), "orphan row should violate the foreign key");
    }

    /// The CHECK constraints added to migration 0006 before it was
    /// committed. If a later change loosens them, this fails.
    #[tokio::test]
    async fn artwork_check_constraints_are_enforced() {
        let db = test_db().await;
        let game = seed_game(&db, "Celeste").await;

        for (column, kind, state) in [("kind", "banner", "ready"), ("state", "cover", "downloading")]
        {
            let result = sqlx::query(
                "INSERT INTO artwork_assets (game_id, kind, source, state, updated_at)
                 VALUES (?1, ?2, 'steam_cdn', ?3, '2026-01-01T00:00:00+00:00')",
            )
            .bind(&game)
            .bind(kind)
            .bind(state)
            .execute(&db.pool)
            .await;
            assert!(result.is_err(), "{column} CHECK should reject out-of-set values");
        }
    }

    /// The seed helpers must match the real schema. This exists because the
    /// first version of `seed_installation` was written against guessed
    /// column names (`source_code`, `size_bytes`) that do not exist — a
    /// mistake that would otherwise have surfaced as a confusing failure in
    /// the first test that used it.
    #[tokio::test]
    async fn seed_helpers_match_the_real_schema() {
        let db = test_db().await;
        let game = seed_game(&db, "Dying Light").await;
        let install = seed_installation(
            &db,
            &game,
            "steam",
            "D:/Games/Dying Light",
            Some("D:/Games/Dying Light/DyingLightGame.exe"),
            true,
            "installed",
        )
        .await;
        seed_genre(&db, &game, "Action").await;

        let (gid, is_primary, status): (String, i64, String) = sqlx::query_as(
            "SELECT game_id, is_primary, status FROM game_installations WHERE id = ?1",
        )
        .bind(&install)
        .fetch_one(&db.pool)
        .await
        .expect("read back installation");
        assert_eq!((gid.as_str(), is_primary, status.as_str()), (game.as_str(), 1, "installed"));

        let genres: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM game_genres WHERE game_id = ?1")
                .bind(&game)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(genres, 1, "genre should be linked");

        // Seeding the same genre twice must not violate the composite PK —
        // this is the shape of the bug that breaks merge_games (Batch 3).
        seed_genre(&db, &game, "Action").await;
        let genres_again: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM game_genres WHERE game_id = ?1")
                .bind(&game)
                .fetch_one(&db.pool)
                .await
                .unwrap();
        assert_eq!(genres_again, 1, "duplicate genre link should be idempotent");
    }

    #[tokio::test]
    async fn fake_provider_records_calls() {
        let provider = FakeArtworkProvider::supplying("fake", 0, &[ArtworkKind::Cover]);
        let calls = provider.calls.clone();
        let identity = identity_with_app_id("Hollow Knight", "steam", "367520");
        let ctx = LookupContext {
            identity: &identity,
            allow_network: false,
        };

        assert_eq!(calls.count(), 0);
        let result = crate::metadata::ArtworkProvider::resolve_artwork(&provider, &ctx).await;
        assert_eq!(calls.count(), 1);
        assert!(matches!(result, Lookup::Found(ref d) if d.len() == 1));
    }
}
