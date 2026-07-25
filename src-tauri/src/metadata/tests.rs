//! Regression tests for the Batch 5 artwork pipeline repairs.

use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};

use crate::db::artwork::{is_retry_due, retry_delay_for, Validators};
use crate::events::EventBus;
use crate::metadata::artwork_service::{eligible_kinds, ArtworkService};
use crate::metadata::{
    ArtworkKind, ArtworkProvider, AssetDescriptor, AssetSource, Lookup, PermanentReason,
};
use crate::models::ArtworkAsset;
use crate::test_support::{seed_game, FakeArtworkProvider, test_db};

const NOW: &str = "2026-07-25T12:00:00+00:00";

fn asset(kind: &str, state: &str, next_retry_at: Option<&str>, user_locked: i64) -> ArtworkAsset {
    ArtworkAsset {
        id: 1,
        game_id: "g".into(),
        kind: kind.into(),
        source: "steam_cdn".into(),
        remote_url: None,
        local_path: None,
        state: state.into(),
        etag: None,
        user_locked,
        fetched_at: None,
        updated_at: NOW.into(),
        attempts: 0,
        next_retry_at: next_retry_at.map(str::to_string),
        last_modified: None,
    }
}

/// Shorthand for a stored ETag with no `Last-Modified`.
fn etag(v: &str) -> Validators<'_> {
    Validators {
        etag: Some(v),
        last_modified: None,
    }
}

// ── eligibility and backoff ─────────────────────────────────────────────

/// The heart of the never-terminating loop: `ready` was the only terminal
/// state, so a kind no provider supplies (`icon`) kept every game eligible
/// forever.
#[test]
fn terminal_states_are_not_eligible() {
    let now = Utc::now();
    let existing = vec![
        asset("cover", "ready", None, 0),
        asset("hero", "skipped", None, 0),
    ];
    let eligible = eligible_kinds(&existing, now);
    assert!(!eligible.contains(&ArtworkKind::Cover), "ready is terminal");
    assert!(!eligible.contains(&ArtworkKind::Hero), "skipped is terminal");
    assert!(eligible.contains(&ArtworkKind::Logo), "no row means eligible");
    assert!(eligible.contains(&ArtworkKind::Icon));
}

#[test]
fn a_fully_settled_game_has_nothing_eligible() {
    let now = Utc::now();
    let existing = vec![
        asset("cover", "ready", None, 0),
        asset("hero", "ready", None, 0),
        asset("logo", "ready", None, 0),
        asset("icon", "skipped", None, 0),
    ];
    assert!(
        eligible_kinds(&existing, now).is_empty(),
        "a settled library must not consult providers at all"
    );
}

#[test]
fn a_failed_slot_waits_for_its_backoff() {
    let now = Utc::now();
    let future = (now + ChronoDuration::hours(1)).to_rfc3339();
    let past = (now - ChronoDuration::hours(1)).to_rfc3339();

    let waiting = vec![asset("cover", "failed", Some(&future), 0)];
    assert!(!eligible_kinds(&waiting, now).contains(&ArtworkKind::Cover));

    let due = vec![asset("cover", "failed", Some(&past), 0)];
    assert!(eligible_kinds(&due, now).contains(&ArtworkKind::Cover));
}

#[test]
fn a_user_locked_slot_is_never_contested() {
    let now = Utc::now();
    let existing = vec![asset("cover", "pending", None, 1)];
    assert!(!eligible_kinds(&existing, now).contains(&ArtworkKind::Cover));
}

#[test]
fn pending_and_missing_rows_are_eligible() {
    let now = Utc::now();
    assert!(eligible_kinds(&[asset("cover", "pending", None, 0)], now)
        .contains(&ArtworkKind::Cover));
    assert!(eligible_kinds(&[], now).len() == ArtworkKind::ALL.len());
}

#[test]
fn backoff_grows_then_holds() {
    let schedule: Vec<i64> = (1..=7).map(retry_delay_for).collect();
    assert_eq!(
        schedule,
        vec![3600, 21600, 86400, 259200, 604800, 604800, 604800],
        "backoff must grow and then cap rather than growing without bound"
    );
    assert_eq!(retry_delay_for(0), 0);
}

/// A corrupt timestamp must not strand a slot permanently.
#[test]
fn an_unparseable_retry_stamp_is_treated_as_due() {
    assert!(is_retry_due("failed", Some("not a timestamp"), Utc::now()));
    assert!(is_retry_due("failed", None, Utc::now()));
}

// ── ledger behaviour ────────────────────────────────────────────────────

#[tokio::test]
async fn failures_increment_attempts_and_schedule_a_retry() {
    let db = test_db().await;
    let game = seed_game(&db, "Failing").await;

    db.mark_artwork_failed(&game, "cover", "steam_cdn").await.unwrap();
    let (attempts, next): (i64, Option<String>) = sqlx::query_as(
        "SELECT attempts, next_retry_at FROM artwork_assets WHERE game_id = ?1 AND kind = 'cover'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(attempts, 1);
    assert!(next.is_some(), "a retry must be scheduled");

    db.mark_artwork_failed(&game, "cover", "steam_cdn").await.unwrap();
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM artwork_assets WHERE game_id = ?1 AND kind = 'cover'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(attempts, 2, "consecutive failures accumulate");
}

#[tokio::test]
async fn success_clears_the_backoff() {
    let db = test_db().await;
    let game = seed_game(&db, "Recovers").await;

    db.mark_artwork_failed(&game, "cover", "steam_cdn").await.unwrap();
    db.upsert_artwork_ready(&game, "cover", "steam_cdn", None, "C:/a.jpg", etag("\"v1\""))
        .await
        .unwrap();

    let (state, attempts, next, etag): (String, i64, Option<String>, Option<String>) =
        sqlx::query_as(
            "SELECT state, attempts, next_retry_at, etag FROM artwork_assets \
             WHERE game_id = ?1 AND kind = 'cover'",
        )
        .bind(&game)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(state, "ready");
    assert_eq!(attempts, 0, "a recovered slot is not punished for its history");
    assert!(next.is_none());
    assert_eq!(etag.as_deref(), Some("\"v1\""), "the validator is stored");
}

#[tokio::test]
async fn skipped_is_written_but_never_displaces_ready_or_locked_artwork() {
    let db = test_db().await;
    let game = seed_game(&db, "Mixed").await;

    // A kind nothing supplies.
    assert!(db.mark_artwork_skipped(&game, "icon").await.unwrap());
    let state: String = sqlx::query_scalar(
        "SELECT state FROM artwork_assets WHERE game_id = ?1 AND kind = 'icon'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(state, "skipped");

    // A ready asset must not be downgraded.
    db.upsert_artwork_ready(&game, "cover", "steam_local", None, "C:/c.jpg", Validators::default())
        .await
        .unwrap();
    assert!(!db.mark_artwork_skipped(&game, "cover").await.unwrap());
    let state: String = sqlx::query_scalar(
        "SELECT state FROM artwork_assets WHERE game_id = ?1 AND kind = 'cover'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(state, "ready", "skipped must never downgrade real artwork");

    // Nor may it touch a user's choice.
    db.lock_artwork_asset(&game, "hero", "C:/user.jpg").await.unwrap();
    assert!(!db.mark_artwork_skipped(&game, "hero").await.unwrap());
}

/// A user setting artwork must clear a terminal `skipped`, so the slot is not
/// stuck once the user supplies it themselves.
#[tokio::test]
async fn a_user_choice_overrides_a_skipped_slot() {
    let db = test_db().await;
    let game = seed_game(&db, "Manual Icon").await;
    db.mark_artwork_skipped(&game, "icon").await.unwrap();
    db.lock_artwork_asset(&game, "icon", "C:/user-icon.png").await.unwrap();

    let (state, locked): (String, i64) = sqlx::query_as(
        "SELECT state, user_locked FROM artwork_assets WHERE game_id = ?1 AND kind = 'icon'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!((state.as_str(), locked), ("ready", 1));
}

/// Both validators must round-trip. Storing only the ETag would leave the
/// column populated and the bandwidth saving unrealised against the Steam CDN,
/// which ignores `If-None-Match` but honours `If-Modified-Since`.
#[tokio::test]
async fn both_validators_round_trip() {
    let db = test_db().await;
    let game = seed_game(&db, "Validated").await;
    db.upsert_artwork_ready(
        &game,
        "cover",
        "steam_cdn",
        Some("https://example.invalid/c.jpg"),
        "C:/c.jpg",
        Validators {
            etag: Some("\"abc\""),
            last_modified: Some("Thu, 12 Dec 2024 08:51:04 GMT"),
        },
    )
    .await
    .unwrap();

    let (etag, last_modified) = db.artwork_validators(&game, "cover", "steam_cdn").await.unwrap();
    assert_eq!(etag.as_deref(), Some("\"abc\""));
    assert_eq!(last_modified.as_deref(), Some("Thu, 12 Dec 2024 08:51:04 GMT"));
}

#[tokio::test]
async fn etags_are_scoped_to_the_owning_provider() {
    let db = test_db().await;
    let game = seed_game(&db, "Tagged").await;
    db.upsert_artwork_ready(&game, "cover", "steam_cdn", None, "C:/a.jpg", etag("\"v1\""))
        .await
        .unwrap();

    assert_eq!(
        db.artwork_validators(&game, "cover", "steam_cdn").await.unwrap().0.as_deref(),
        Some("\"v1\"")
    );
    assert_eq!(
        db.artwork_validators(&game, "cover", "some_other_provider").await.unwrap().0,
        None,
        "a validator is only meaningful to the origin that issued it"
    );
}

#[tokio::test]
async fn an_unchanged_asset_refreshes_bookkeeping_without_touching_the_path() {
    let db = test_db().await;
    let game = seed_game(&db, "Unchanged").await;
    db.upsert_artwork_ready(&game, "cover", "steam_cdn", None, "C:/kept.jpg", etag("\"v1\""))
        .await
        .unwrap();
    db.mark_artwork_failed(&game, "cover", "steam_cdn").await.unwrap();

    db.touch_artwork_unchanged(&game, "cover", "steam_cdn", etag("\"v1\""))
        .await
        .unwrap();

    let (state, path, attempts): (String, Option<String>, i64) = sqlx::query_as(
        "SELECT state, local_path, attempts FROM artwork_assets \
         WHERE game_id = ?1 AND kind = 'cover'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(state, "ready");
    assert_eq!(path.as_deref(), Some("C:/kept.jpg"), "bytes on disk are still valid");
    assert_eq!(attempts, 0);
}

// ── the fill loop: termination ──────────────────────────────────────────

/// A temporary directory removed when the test ends, so runs do not leave
/// artwork fixtures behind in the system temp folder.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("novara-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Real files on disk, because `store_local_asset` refuses anything that is not
/// a file — a fake path would exercise the failure path instead.
fn temp_assets(kinds: &[ArtworkKind]) -> (TempDir, Vec<AssetDescriptor>) {
    let dir = TempDir::new("b5-src");
    let mut descriptors = Vec::new();
    for kind in kinds {
        let path = dir.path().join(format!("{}.png", kind.as_str()));
        std::fs::write(&path, b"not really a png").unwrap();
        descriptors.push(AssetDescriptor {
            kind: *kind,
            source: AssetSource::LocalFile(path),
            provider: "fake",
        });
    }
    (dir, descriptors)
}

fn service(
    db: &crate::db::Db,
    providers: Vec<Arc<dyn ArtworkProvider>>,
) -> (ArtworkService, TempDir) {
    let app_data = TempDir::new("appdata");
    let svc = ArtworkService::with_providers(
        db.clone(),
        EventBus::new(256),
        app_data.path().to_path_buf(),
        providers,
    );
    (svc, app_data)
}

/// The requirement in full: once every slot has reached a terminal state, a
/// further scan must not consult providers at all.
#[tokio::test]
async fn repeated_scans_stop_consulting_providers_once_every_slot_is_terminal() {
    let db = test_db().await;
    let game = seed_game(&db, "Settles").await;

    // Supplies three kinds; nothing supplies `icon`.
    let (_fixture, descriptors) =
        temp_assets(&[ArtworkKind::Cover, ArtworkKind::Hero, ArtworkKind::Logo]);
    let provider = Arc::new(FakeArtworkProvider::new(
        "fake",
        0,
        Lookup::Found(descriptors),
    ));
    let calls = provider.calls.clone();
    let (svc, _app_data) = service(&db, vec![provider]);

    // First pass fills what it can and settles the rest.
    let first = svc.fill_missing(true).await.unwrap();
    assert_eq!(first.checked, 1);
    assert_eq!(first.updated, 3, "cover, hero and logo");
    assert_eq!(calls.count(), 1, "provider consulted once");

    let states: Vec<(String, String)> = sqlx::query_as(
        "SELECT kind, state FROM artwork_assets WHERE game_id = ?1 ORDER BY kind",
    )
    .bind(&game)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        states,
        vec![
            ("cover".to_string(), "ready".to_string()),
            ("hero".to_string(), "ready".to_string()),
            ("icon".to_string(), "skipped".to_string()),
            ("logo".to_string(), "ready".to_string()),
        ],
        "the unsatisfiable kind must settle as skipped"
    );

    // Second and third passes must not touch the provider at all.
    let second = svc.fill_missing(true).await.unwrap();
    let third = svc.fill_missing(true).await.unwrap();
    assert_eq!(second.checked, 0, "nothing eligible");
    assert_eq!(third.checked, 0);
    assert_eq!(
        calls.count(),
        1,
        "a settled library must issue no further provider calls"
    );
}

/// The escape hatch: when network was not permitted, the pass is not conclusive
/// and nothing may be settled as unavailable.
#[tokio::test]
async fn nothing_is_settled_when_a_provider_was_never_consulted() {
    let db = test_db().await;
    let game = seed_game(&db, "Offline").await;

    let provider = Arc::new(
        FakeArtworkProvider::new("fake", 0, Lookup::Found(vec![])).requiring_network(),
    );
    let calls = provider.calls.clone();
    let (svc, _app_data) = service(&db, vec![provider]);

    svc.fill_missing(false).await.unwrap();
    assert_eq!(calls.count(), 0, "a network provider must not run");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM artwork_assets WHERE game_id = ?1")
        .bind(&game)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(rows, 0, "an unconsulted provider's silence settles nothing");

    // With network allowed the same slots are still eligible.
    let (svc2, _d) = service(
        &db,
        vec![Arc::new(FakeArtworkProvider::new("fake", 0, Lookup::Found(vec![])))],
    );
    let report = svc2.fill_missing(true).await.unwrap();
    assert_eq!(report.checked, 1, "the game is still eligible later");
}

/// A transient failure is not evidence of absence.
#[tokio::test]
async fn a_temporary_failure_does_not_settle_slots() {
    let db = test_db().await;
    let game = seed_game(&db, "Flaky").await;

    let provider = Arc::new(FakeArtworkProvider::new(
        "fake",
        0,
        Lookup::Temporary(crate::metadata::TemporaryReason::Timeout),
    ));
    let (svc, _app_data) = service(&db, vec![provider]);
    svc.fill_missing(true).await.unwrap();

    let skipped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM artwork_assets WHERE game_id = ?1 AND state = 'skipped'",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(skipped, 0, "a timeout must not settle a slot as unavailable");
}

/// A provider-level `Permanent` is definitive, so the slots settle — but it must
/// not be recorded as a per-kind failure, which is what gave the ledger false
/// provenance before.
#[tokio::test]
async fn a_permanent_miss_settles_slots_without_recording_false_failures() {
    let db = test_db().await;
    let game = seed_game(&db, "Not Here").await;

    let provider = Arc::new(FakeArtworkProvider::new(
        "fake",
        0,
        Lookup::Permanent(PermanentReason::NotFound),
    ));
    let (svc, _app_data) = service(&db, vec![provider]);
    svc.fill_missing(true).await.unwrap();

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT kind, state, source FROM artwork_assets WHERE game_id = ?1",
    )
    .bind(&game)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 4, "every kind settles");
    for (kind, state, source) in rows {
        assert_eq!(state, "skipped", "{kind} should be skipped, not failed");
        assert_eq!(source, "none", "{kind} must not claim a provider tried it");
    }
}

/// Hidden games were still being fetched for, despite the user having removed
/// them from the library.
#[tokio::test]
async fn hidden_games_are_not_fetched_for() {
    let db = test_db().await;
    let hidden = seed_game(&db, "Removed").await;
    sqlx::query("UPDATE games SET is_hidden = 1 WHERE id = ?1")
        .bind(&hidden)
        .execute(&db.pool)
        .await
        .unwrap();

    let provider = Arc::new(FakeArtworkProvider::new("fake", 0, Lookup::Found(vec![])));
    let calls = provider.calls.clone();
    let (svc, _app_data) = service(&db, vec![provider]);

    let report = svc.fill_missing(true).await.unwrap();
    assert_eq!(report.checked, 0);
    assert_eq!(calls.count(), 0, "a hidden game must not cost a provider call");
}

/// The first pass must actually place the bytes and point the render column at
/// them, or a `ready` ledger row would describe artwork that does not display.
#[tokio::test]
async fn filling_writes_the_render_path_and_the_file() {
    let db = test_db().await;
    let game = seed_game(&db, "Rendered").await;
    let (_fixture, descriptors) = temp_assets(&[ArtworkKind::Cover]);
    let provider = Arc::new(FakeArtworkProvider::new("fake", 0, Lookup::Found(descriptors)));
    let (svc, app_data) = service(&db, vec![provider]);

    svc.fill_missing(true).await.unwrap();

    let cover: Option<String> = sqlx::query_scalar("SELECT cover_path FROM games WHERE id = ?1")
        .bind(&game)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    let cover = cover.expect("cover_path must be set");
    assert!(
        cover.starts_with(&app_data.path().display().to_string()),
        "stored under app data: {cover}"
    );
    assert!(std::path::Path::new(&cover).is_file(), "bytes must exist on disk");
}
