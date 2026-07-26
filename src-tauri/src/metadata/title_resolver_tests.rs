//! Tests for identity enrichment and the resolution pass.
//!
//! The point of the feature is that a resolved app-id makes the *existing*
//! providers work for games that have none, so what matters here is that the
//! app-id reaches `GameIdentity` — the single thing every Steam-backed provider
//! keys on — and that a real app-id is never displaced by an inferred one.

use crate::metadata::identity::identity_for;
use crate::metadata::providers::steam_title_search;
use crate::metadata::title_resolver::TitleResolver;
use crate::test_support::{seed_game, seed_installation, test_db};

async fn game_row(db: &crate::db::Db, game_id: &str) -> crate::models::Game {
    db.get_game(game_id).await.unwrap().expect("game exists")
}

async fn seed_epic_game(db: &crate::db::Db, title: &str) -> String {
    let game = seed_game(db, title).await;
    seed_installation(
        db,
        &game,
        "epic",
        &format!("C:/epic/{title}"),
        Some("g.exe"),
        true,
        "installed",
    )
    .await;
    sqlx::query("UPDATE game_installations SET source_app_id = 'EpicAppName' WHERE game_id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();
    game
}

/// The integration point: with a resolved match, an Epic game's identity carries a
/// Steam app-id, which is what `steam_cdn` and `steam_local` require. Nothing in
/// those providers changes.
#[tokio::test]
async fn a_resolved_match_gives_an_epic_game_a_steam_identity() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Alan Wake").await;

    let before = identity_for(&db, &game_row(&db, &game).await).await.unwrap();
    assert_eq!(
        before.source_app_id("steam"),
        None,
        "without a match there is nothing for a Steam provider to act on"
    );
    assert_eq!(before.source_app_id("epic"), Some("EpicAppName"));

    db.record_steam_title_match(&game, Some("108710"), Some("Alan Wake"), None, "r/1")
        .await
        .unwrap();

    let after = identity_for(&db, &game_row(&db, &game).await).await.unwrap();
    assert_eq!(after.source_app_id("steam"), Some("108710"));
    assert_eq!(
        after.source_app_id("epic"),
        Some("EpicAppName"),
        "the real launcher identity is kept: it is what launches the game"
    );
}

/// A recorded non-match must not manufacture an identity.
#[tokio::test]
async fn a_recorded_non_match_adds_no_identifier() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Fortnite").await;
    db.record_steam_title_match(&game, None, None, None, "r/1")
        .await
        .unwrap();

    let identity = identity_for(&db, &game_row(&db, &game).await).await.unwrap();
    assert_eq!(identity.source_app_id("steam"), None);
}

/// A fact beats an inference. If a stale or wrong row ever existed for a game that
/// really is a Steam game, the installation's own app-id must still win.
#[tokio::test]
async fn a_real_steam_app_id_is_never_displaced_by_a_title_match() {
    let db = test_db().await;
    let game = seed_game(&db, "Half-Life 2").await;
    seed_installation(&db, &game, "steam", "C:/steam/hl2", None, true, "installed").await;
    sqlx::query("UPDATE game_installations SET source_app_id = '220' WHERE game_id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();
    db.record_steam_title_match(&game, Some("999999"), Some("Wrong Game"), None, "r/1")
        .await
        .unwrap();

    let identity = identity_for(&db, &game_row(&db, &game).await).await.unwrap();
    assert_eq!(
        identity.source_app_id("steam"),
        Some("220"),
        "the installation's own app-id is authoritative"
    );
    assert_eq!(
        identity
            .identifiers
            .iter()
            .filter(|id| matches!(
                id,
                crate::metadata::GameIdentifier::SourceAppId { source, .. } if source == "steam"
            ))
            .count(),
        1,
        "and no second, contradictory Steam identifier is added"
    );
}

/// Building an identity is a pure offline read, so it must work with no network
/// permission at all — the resolution pass is the only network caller.
#[tokio::test]
async fn building_an_identity_needs_no_network() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Cached Already").await;
    db.record_steam_title_match(&game, Some("1"), Some("Cached Already"), None, "r/1")
        .await
        .unwrap();

    // No client, no gate, no pass — just the stored answer.
    let identity = identity_for(&db, &game_row(&db, &game).await).await.unwrap();
    assert_eq!(identity.source_app_id("steam"), Some("1"));
}

// ── the resolution pass ─────────────────────────────────────────────────

fn resolver(db: &crate::db::Db) -> TitleResolver {
    TitleResolver::new(
        db.clone(),
        reqwest::Client::new(),
        std::sync::Arc::new(crate::metadata::throttle::Throttle::new(
            1,
            std::time::Duration::ZERO,
        )),
    )
}

/// The whole pass requires the network, so without permission it does nothing at
/// all — it does not walk the library, and it records no outcomes that would later
/// be mistaken for real answers.
#[tokio::test]
async fn the_pass_does_nothing_without_network_permission() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Alan Wake").await;

    let report = resolver(&db).resolve_missing(false).await.unwrap();
    assert_eq!(report.checked, 0);
    assert_eq!(
        db.steam_title_match(&game).await.unwrap(),
        None,
        "no outcome may be recorded, or a later sweep would treat it as settled"
    );
}

#[tokio::test]
async fn a_single_game_refresh_does_nothing_without_network_permission() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Alan Wake").await;

    assert!(!resolver(&db).resolve_one(&game, false).await.unwrap());
    assert_eq!(db.steam_title_match(&game).await.unwrap(), None);
}

/// A game Steam already identifies is never re-keyed by title, even when the user
/// explicitly asks for a refresh.
#[tokio::test]
async fn a_single_game_refresh_skips_games_that_already_have_a_steam_app_id() {
    let db = test_db().await;
    let game = seed_game(&db, "Half-Life 2").await;
    seed_installation(&db, &game, "steam", "C:/steam/hl2", None, true, "installed").await;
    sqlx::query("UPDATE game_installations SET source_app_id = '220' WHERE game_id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();

    // `allow_network` is true, so only the app-id check can prevent a lookup.
    assert!(!resolver(&db).resolve_one(&game, true).await.unwrap());
    assert_eq!(
        db.steam_title_match(&game).await.unwrap(),
        None,
        "no search was performed, so no outcome was recorded"
    );
}

#[tokio::test]
async fn a_refresh_of_an_unknown_game_is_a_no_op() {
    let db = test_db().await;
    assert!(!resolver(&db)
        .resolve_one("no-such-game", true)
        .await
        .unwrap());
}

/// The pass and the cache must agree on the fingerprint, or every sweep re-searches
/// every game.
#[tokio::test]
async fn the_pass_settles_games_under_the_resolvers_own_fingerprint() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Already Settled").await;
    db.record_steam_title_match(&game, None, None, None, &steam_title_search::fingerprint())
        .await
        .unwrap();

    let pending = db
        .games_needing_title_search(&steam_title_search::fingerprint())
        .await
        .unwrap();
    assert!(
        pending.is_empty(),
        "a game settled by this resolver is not searched again"
    );
}