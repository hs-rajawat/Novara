//! Tests for the Steam title-match cache.
//!
//! The behaviour worth pinning is not the round-trip — it is which games the
//! resolver is told to ask about. Getting that set wrong either wastes a network
//! request per game per sweep (the defect terminal artwork states exist to
//! prevent) or silently skips the games this feature exists for.

use crate::test_support::{seed_game, seed_installation, test_db};

const RESOLVER: &str = "steam_title_search/1";

/// Give a game a Steam installation with an app-id, i.e. an identity that needs
/// no searching.
async fn seed_steam_game(db: &crate::db::Db, title: &str, app_id: &str) -> String {
    let game = seed_game(db, title).await;
    seed_installation(db, &game, "steam", "C:/steam/common/g", None, true, "installed").await;
    sqlx::query("UPDATE game_installations SET source_app_id = ?1 WHERE game_id = ?2")
        .bind(app_id)
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();
    game
}

/// An Epic game: a real launcher identity, but nothing Steam can use.
///
/// `install_dir` is unique per game because the schema enforces that — two games
/// cannot be installed in the same directory.
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

#[tokio::test]
async fn a_game_that_has_never_been_searched_has_no_recorded_outcome() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Alan Wake").await;
    assert_eq!(db.steam_title_match(&game).await.unwrap(), None);
}

#[tokio::test]
async fn a_match_round_trips_with_its_provenance() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Dying Light The Following").await;

    db.record_steam_title_match(&game, Some("325724"), Some("Dying Light: The Following"), None, RESOLVER)
        .await
        .unwrap();

    let found = db.steam_title_match(&game).await.unwrap().unwrap();
    assert_eq!(found.app_id.as_deref(), Some("325724"));
    assert_eq!(
        found.matched_title.as_deref(),
        Some("Dying Light: The Following"),
        "the Steam title that matched is kept so a wrong match can be diagnosed"
    );
    assert_eq!(found.settled_by, RESOLVER);
}

/// The negative cache. "Searched and found nothing" is an answer, and it has to be
/// distinguishable from "never asked".
#[tokio::test]
async fn a_non_match_is_recorded_as_an_answer_not_an_absence() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Fortnite").await;

    db.record_steam_title_match(&game, None, None, None, RESOLVER)
        .await
        .unwrap();

    let found = db.steam_title_match(&game).await.unwrap();
    assert!(found.is_some(), "the search itself was recorded");
    let found = found.unwrap();
    assert_eq!(found.app_id, None);
    assert_eq!(found.matched_title, None);
}

/// A match must carry its provenance and a non-match must not claim any. The
/// constraint exists because SQLite cannot add one later.
#[tokio::test]
async fn a_half_recorded_outcome_is_rejected_by_the_schema() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Half Recorded").await;

    for (app_id, matched_title) in [(Some("220"), None), (None, Some("Half-Life 2"))] {
        let result = db
            .record_steam_title_match(&game, app_id, matched_title, None, RESOLVER)
            .await;
        assert!(
            result.is_err(),
            "app_id={app_id:?} with matched_title={matched_title:?} must not be storable"
        );
    }
}

#[tokio::test]
async fn a_later_search_supersedes_an_earlier_one() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Improved Later").await;

    db.record_steam_title_match(&game, None, None, None, "steam_title_search/1")
        .await
        .unwrap();
    db.record_steam_title_match(&game, Some("1174180"), Some("Red Dead Redemption 2"), None, "steam_title_search/2")
        .await
        .unwrap();

    let found = db.steam_title_match(&game).await.unwrap().unwrap();
    assert_eq!(found.app_id.as_deref(), Some("1174180"));
    assert_eq!(found.settled_by, "steam_title_search/2");

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM steam_title_matches WHERE game_id = ?1")
        .bind(&game)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(rows, 1, "one outcome per game, not a history");
}

/// A DLC match records the base game its artwork should come from, while the match
/// itself stays the DLC — that is the correct answer and what the description
/// should come from.
#[tokio::test]
async fn a_dlc_match_records_the_base_game_for_artwork() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Dying Light The Following").await;

    db.record_steam_title_match(
        &game,
        Some("325724"),
        Some("Dying Light: The Following"),
        Some("239140"),
        RESOLVER,
    )
    .await
    .unwrap();

    let found = db.steam_title_match(&game).await.unwrap().unwrap();
    assert_eq!(found.app_id.as_deref(), Some("325724"), "the match is the DLC");
    assert_eq!(
        found.artwork_app_id.as_deref(),
        Some("239140"),
        "and the artwork falls back to its base game"
    );
}

/// An ordinary match borrows nothing.
#[tokio::test]
async fn an_ordinary_match_records_no_artwork_fallback() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Alan Wake").await;
    db.record_steam_title_match(&game, Some("108710"), Some("Alan Wake"), None, RESOLVER)
        .await
        .unwrap();

    let found = db.steam_title_match(&game).await.unwrap().unwrap();
    assert_eq!(found.artwork_app_id, None);
}

/// A re-resolution that no longer finds a parent must clear the old one, not leave
/// a stale fallback behind.
#[tokio::test]
async fn a_later_search_clears_a_previous_artwork_fallback() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Reclassified").await;
    db.record_steam_title_match(&game, Some("1"), Some("X"), Some("999"), "r/1")
        .await
        .unwrap();

    db.record_steam_title_match(&game, Some("1"), Some("X"), None, "r/2")
        .await
        .unwrap();

    let found = db.steam_title_match(&game).await.unwrap().unwrap();
    assert_eq!(found.artwork_app_id, None);
}

// ── which games get searched ─────────────────────────────────────────────

#[tokio::test]
async fn games_without_a_steam_identity_are_searched() {
    let db = test_db().await;
    let epic = seed_epic_game(&db, "Alan Wake").await;
    let manual = seed_game(&db, "Old DVD Game").await;
    seed_installation(&db, &manual, "manual", "D:/Games/Old", Some("g.exe"), true, "installed").await;

    let pending = db.games_needing_title_search(RESOLVER).await.unwrap();
    let ids: Vec<&str> = pending.iter().map(|(id, _)| id.as_str()).collect();

    assert!(ids.contains(&epic.as_str()), "Epic games are the point of this");
    assert!(ids.contains(&manual.as_str()), "so are manual imports");
    assert_eq!(
        pending.iter().find(|(id, _)| id == &epic).map(|(_, t)| t.as_str()),
        Some("Alan Wake"),
        "the title is what the resolver searches on"
    );
}

/// A game whose identity Steam already knows must never be searched: the result
/// could only contradict a fact already established.
#[tokio::test]
async fn a_game_that_already_has_a_steam_app_id_is_never_searched() {
    let db = test_db().await;
    let steam = seed_steam_game(&db, "Half-Life 2", "220").await;

    let pending = db.games_needing_title_search(RESOLVER).await.unwrap();
    assert!(!pending.iter().any(|(id, _)| id == &steam));
}

/// This is what stops the sweep re-querying Steam about the same unmatchable game
/// for ever.
#[tokio::test]
async fn a_game_already_settled_by_this_resolver_is_not_searched_again() {
    let db = test_db().await;
    let matched = seed_epic_game(&db, "Matched").await;
    let unmatched = seed_epic_game(&db, "Unmatched").await;
    db.record_steam_title_match(&matched, Some("1"), Some("Matched"), None, RESOLVER)
        .await
        .unwrap();
    db.record_steam_title_match(&unmatched, None, None, None, RESOLVER)
        .await
        .unwrap();

    let pending = db.games_needing_title_search(RESOLVER).await.unwrap();
    assert!(
        pending.is_empty(),
        "both outcomes are terminal for this resolver, including the non-match"
    );
}

/// Improving the resolver must re-open past conclusions without manual repair —
/// the same mechanism the artwork ledger uses for capability changes.
#[tokio::test]
async fn a_new_resolver_re_opens_previous_outcomes() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Previously Unmatched").await;
    db.record_steam_title_match(&game, None, None, None, "steam_title_search/1")
        .await
        .unwrap();

    let pending = db
        .games_needing_title_search("steam_title_search/2")
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].0, game);
}

#[tokio::test]
async fn hidden_games_are_not_searched() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Removed From Library").await;
    sqlx::query("UPDATE games SET is_hidden = 1 WHERE id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();

    let pending = db.games_needing_title_search(RESOLVER).await.unwrap();
    assert!(
        pending.is_empty(),
        "the user removed it, so looking it up is work nobody asked for"
    );
}

/// Deleting a game must not leave its search outcome behind.
#[tokio::test]
async fn an_outcome_is_removed_with_its_game() {
    let db = test_db().await;
    let game = seed_epic_game(&db, "Doomed").await;
    db.record_steam_title_match(&game, Some("1"), Some("Doomed"), None, RESOLVER)
        .await
        .unwrap();

    sqlx::query("DELETE FROM games WHERE id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .unwrap();

    let orphans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM steam_title_matches")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(orphans, 0, "ON DELETE CASCADE, with foreign keys enforced");
}