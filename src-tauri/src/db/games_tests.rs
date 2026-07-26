//! Regression tests for the Batch 3 data-correctness repairs.
//!
//! Every test here corresponds to a defect that existed in shipped code and
//! that nothing caught, because `db/*` had no tests at all.

use crate::db::games::{normalize_sort_title, primary_installation, ExecutableSource};
use crate::error::AppError;
use crate::models::now_rfc3339;
use crate::test_support::{seed_game, seed_genre, seed_installation, test_db};

const NOW: &str = "2026-07-25T12:00:00+00:00";

// ── helpers ─────────────────────────────────────────────────────────────

async fn seed_session(db: &crate::db::Db, game_id: &str, seconds: i64, started_at: &str) {
    sqlx::query(
        "INSERT INTO play_sessions (game_id, started_at, ended_at, duration_seconds, idle_seconds) \
         VALUES (?1, ?2, ?2, ?3, 0)",
    )
    .bind(game_id)
    .bind(started_at)
    .bind(seconds)
    .execute(&db.pool)
    .await
    .expect("seed session");
}

async fn seed_achievement(db: &crate::db::Db, game_id: &str, name: &str, unlocked: bool) {
    sqlx::query(
        "INSERT INTO achievements (id, game_id, name, points, is_secret, is_unlocked, sort_order) \
         VALUES (?1, ?2, ?3, 0, 0, ?4, 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(game_id)
    .bind(name)
    .bind(i64::from(unlocked))
    .execute(&db.pool)
    .await
    .expect("seed achievement");
}

async fn seed_artwork(
    db: &crate::db::Db,
    game_id: &str,
    kind: &str,
    source: &str,
    local_path: &str,
    user_locked: bool,
) {
    sqlx::query(
        "INSERT INTO artwork_assets \
           (game_id, kind, source, local_path, state, user_locked, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'ready', ?5, ?6)",
    )
    .bind(game_id)
    .bind(kind)
    .bind(source)
    .bind(local_path)
    .bind(i64::from(user_locked))
    .bind(NOW)
    .execute(&db.pool)
    .await
    .expect("seed artwork");
}

async fn count(db: &crate::db::Db, table: &str, game_id: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE game_id = ?1"))
        .bind(game_id)
        .fetch_one(&db.pool)
        .await
        .expect("count")
}

// ── 3.1 dashboard totals on an empty library ────────────────────────────

/// The aggregate decoded `NULL` into `i64` on an empty `games` table, so
/// `dashboard_stats` failed for every fresh install and the Dashboard stayed
/// blank until the first game was added.
#[tokio::test]
async fn library_totals_decode_on_an_empty_library() {
    let db = test_db().await;
    let totals = crate::commands::analytics::library_totals(&db.pool)
        .await
        .expect("totals must decode with no rows at all");
    assert_eq!(totals, (0, 0, 0, 0));
}

#[tokio::test]
async fn library_totals_aggregate_correctly() {
    let db = test_db().await;
    let a = seed_game(&db, "Alpha").await;
    let b = seed_game(&db, "Beta").await;
    seed_game(&db, "Gamma").await;
    sqlx::query(
        "UPDATE games SET completion_state = 'completed', is_favorite = 1, \
         total_playtime_seconds = 3600 WHERE id = ?1",
    )
    .bind(&a)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("UPDATE games SET total_playtime_seconds = 1800 WHERE id = ?1")
        .bind(&b)
        .execute(&db.pool)
        .await
        .unwrap();

    let (total, completed, playtime, favorites) =
        crate::commands::analytics::library_totals(&db.pool).await.unwrap();
    assert_eq!((total, completed, playtime, favorites), (3, 1, 5400, 1));
}

// ── merge: validation ───────────────────────────────────────────────────

#[tokio::test]
async fn merge_rejects_merging_a_game_into_itself() {
    let db = test_db().await;
    let g = seed_game(&db, "Hades").await;
    let err = db.merge_games(&g, &g).await.expect_err("must reject");
    assert!(matches!(err, AppError::Invalid(_)), "got {err:?}");
}

/// Previously a silent success: every UPDATE matched zero rows, the DELETE
/// matched zero rows, and the command reported that it had merged.
#[tokio::test]
async fn merge_rejects_an_unknown_source_game() {
    let db = test_db().await;
    let survivor = seed_game(&db, "Hades").await;
    let err = db
        .merge_games("no-such-game", &survivor)
        .await
        .expect_err("must reject an unknown from_id");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn merge_rejects_an_unknown_target_game() {
    let db = test_db().await;
    let loser = seed_game(&db, "Hades").await;
    let err = db
        .merge_games(&loser, "no-such-game")
        .await
        .expect_err("must reject an unknown to_id");
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    // The rejection must not have consumed the real game on the way out.
    let still_there: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games WHERE id = ?1")
        .bind(&loser)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(still_there, 1, "a failed merge must not delete anything");
}

// ── merge: the genre primary-key collision ──────────────────────────────

/// `UPDATE game_genres SET game_id = ...` violates
/// `PRIMARY KEY (game_id, genre_id)` when both games share a genre, which
/// aborted the entire merge transaction. Latent until `set_game_metadata`
/// began writing genres for every Steam game, at which point any two Steam
/// titles sharing a genre could not be merged at all.
#[tokio::test]
async fn merge_survives_a_shared_genre_and_unions_the_rest() {
    let db = test_db().await;
    let loser = seed_game(&db, "Portal").await;
    let survivor = seed_game(&db, "Portal 2").await;

    seed_genre(&db, &loser, "Puzzle").await;
    seed_genre(&db, &loser, "Indie").await;
    seed_genre(&db, &survivor, "Puzzle").await; // the collision
    seed_genre(&db, &survivor, "Action").await;

    db.merge_games(&loser, &survivor)
        .await
        .expect("a shared genre must not abort the merge");

    let mut names: Vec<String> = sqlx::query_scalar(
        "SELECT g.name FROM genres g JOIN game_genres gg ON gg.genre_id = g.id \
         WHERE gg.game_id = ?1",
    )
    .bind(&survivor)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    names.sort();
    assert_eq!(names, vec!["Action", "Indie", "Puzzle"], "genres must be unioned");
    assert_eq!(count(&db, "game_genres", &loser).await, 0);
}

// ── merge: the artwork ledger ───────────────────────────────────────────

/// `artwork_assets` was absent from the reparent list, so `ON DELETE CASCADE`
/// destroyed the absorbed game's entire artwork ledger, including
/// `user_locked` flags recording deliberate user choices.
#[tokio::test]
async fn merge_preserves_the_artwork_ledger() {
    let db = test_db().await;
    let loser = seed_game(&db, "Celeste (dup)").await;
    let survivor = seed_game(&db, "Celeste").await;

    // Survivor already owns a cover; loser has a cover *and* a unique logo.
    seed_artwork(&db, &survivor, "cover", "steam_local", "C:/keep/cover.jpg", false).await;
    seed_artwork(&db, &loser, "cover", "steam_cdn", "C:/lose/cover.jpg", true).await;
    seed_artwork(&db, &loser, "logo", "steam_cdn", "C:/lose/logo.png", false).await;

    db.merge_games(&loser, &survivor).await.expect("merge");

    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT kind, source, local_path FROM artwork_assets WHERE game_id = ?1 ORDER BY kind",
    )
    .bind(&survivor)
    .fetch_all(&db.pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 2, "the unique kind must be adopted, not dropped");
    let cover = rows.iter().find(|r| r.0 == "cover").unwrap();
    assert_eq!(
        (cover.1.as_str(), cover.2.as_str()),
        ("steam_local", "C:/keep/cover.jpg"),
        "the survivor's own artwork must win, even over a locked incoming row"
    );
    let logo = rows.iter().find(|r| r.0 == "logo").unwrap();
    assert_eq!(logo.2, "C:/lose/logo.png", "a kind the survivor lacked is adopted");

    assert_eq!(count(&db, "artwork_assets", &loser).await, 0);
}

/// `games.*_path` is what the UI renders. An adopted ledger row for a kind the
/// survivor has no path for would otherwise read `ready` while nothing showed.
#[tokio::test]
async fn merge_adopts_render_paths_for_newly_acquired_artwork() {
    let db = test_db().await;
    let loser = seed_game(&db, "Hollow Knight (dup)").await;
    let survivor = seed_game(&db, "Hollow Knight").await;

    seed_artwork(&db, &loser, "hero", "steam_cdn", "C:/lose/hero.jpg", false).await;
    // Survivor keeps its own cover path, which must not be overwritten.
    sqlx::query("UPDATE games SET cover_path = 'C:/keep/cover.jpg' WHERE id = ?1")
        .bind(&survivor)
        .execute(&db.pool)
        .await
        .unwrap();
    seed_artwork(&db, &survivor, "cover", "steam_local", "C:/keep/cover.jpg", false).await;

    db.merge_games(&loser, &survivor).await.expect("merge");

    let (cover, hero): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT cover_path, hero_path FROM games WHERE id = ?1")
            .bind(&survivor)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(cover.as_deref(), Some("C:/keep/cover.jpg"), "existing path untouched");
    assert_eq!(hero.as_deref(), Some("C:/lose/hero.jpg"), "new kind's path adopted");
}

// ── merge: reparenting and cached aggregates ────────────────────────────

#[tokio::test]
async fn merge_reparents_every_referencing_table() {
    let db = test_db().await;
    let loser = seed_game(&db, "Dying Light (dup)").await;
    let survivor = seed_game(&db, "Dying Light").await;

    seed_installation(&db, &loser, "manual", "D:/dup", Some("D:/dup/g.exe"), true, "installed").await;
    seed_session(&db, &loser, 600, NOW).await;
    seed_achievement(&db, &loser, "First blood", true).await;
    sqlx::query(
        "INSERT INTO save_profiles (id, game_id, label, source_dir, auto_backup, created_at, is_manual_override) \
         VALUES (?1, ?2, 'Default', 'D:/saves', 0, ?3, 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&loser)
    .bind(NOW)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO mods (game_id, name, path, enabled, load_order, added_at) \
         VALUES (?1, 'Test mod', 'D:/mods/x', 1, 0, ?2)",
    )
    .bind(&loser)
    .bind(NOW)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO media (game_id, kind, path) VALUES (?1, 'screenshot', 'D:/shot.png')")
        .bind(&loser)
        .execute(&db.pool)
        .await
        .unwrap();

    db.merge_games(&loser, &survivor).await.expect("merge");

    for table in [
        "game_installations",
        "play_sessions",
        "achievements",
        "save_profiles",
        "mods",
        "media",
    ] {
        assert_eq!(count(&db, table, &survivor).await, 1, "{table} must be reparented");
        assert_eq!(count(&db, table, &loser).await, 0, "{table} must not be left behind");
    }

    let gone: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM games WHERE id = ?1")
        .bind(&loser)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(gone, 0, "the absorbed game row must be deleted");
}

/// `total_playtime_seconds` and `last_played_at` are cached on `games`, not
/// derived from `play_sessions`, so moving sessions alone silently lost the
/// absorbed game's playtime from every screen.
#[tokio::test]
async fn merge_sums_cached_playtime_and_keeps_the_later_last_played() {
    let db = test_db().await;
    let loser = seed_game(&db, "Loser").await;
    let survivor = seed_game(&db, "Survivor").await;

    sqlx::query(
        "UPDATE games SET total_playtime_seconds = 7200, \
         last_played_at = '2026-07-20T10:00:00+00:00' WHERE id = ?1",
    )
    .bind(&loser)
    .execute(&db.pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE games SET total_playtime_seconds = 1800, \
         last_played_at = '2026-07-01T10:00:00+00:00' WHERE id = ?1",
    )
    .bind(&survivor)
    .execute(&db.pool)
    .await
    .unwrap();

    db.merge_games(&loser, &survivor).await.expect("merge");

    let (secs, last): (i64, Option<String>) = sqlx::query_as(
        "SELECT total_playtime_seconds, last_played_at FROM games WHERE id = ?1",
    )
    .bind(&survivor)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(secs, 9000, "playtime must be summed");
    assert_eq!(
        last.as_deref(),
        Some("2026-07-20T10:00:00+00:00"),
        "the more recent last_played_at must win"
    );
}

#[tokio::test]
async fn merge_leaves_never_played_as_null_rather_than_empty_string() {
    let db = test_db().await;
    let loser = seed_game(&db, "Loser").await;
    let survivor = seed_game(&db, "Survivor").await;

    db.merge_games(&loser, &survivor).await.expect("merge");

    let last: Option<String> =
        sqlx::query_scalar("SELECT last_played_at FROM games WHERE id = ?1")
            .bind(&survivor)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(last.is_none(), "expected NULL, got {last:?}");
}

#[tokio::test]
async fn merge_recomputes_completion_over_the_combined_achievements() {
    let db = test_db().await;
    let loser = seed_game(&db, "Loser").await;
    let survivor = seed_game(&db, "Survivor").await;

    seed_achievement(&db, &survivor, "A", true).await;
    seed_achievement(&db, &loser, "B", true).await;
    seed_achievement(&db, &loser, "C", false).await;
    seed_achievement(&db, &loser, "D", false).await;

    db.merge_games(&loser, &survivor).await.expect("merge");

    let pct: f64 = sqlx::query_scalar("SELECT completion_pct FROM games WHERE id = ?1")
        .bind(&survivor)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert!((pct - 50.0).abs() < 1e-9, "2 of 4 unlocked, got {pct}");
}

// ── 3.5 one primary-installation rule ───────────────────────────────────

#[tokio::test]
async fn primary_installation_prefers_health_over_the_is_primary_flag() {
    let db = test_db().await;
    let game = seed_game(&db, "Multi").await;
    // The stale ghost carries is_primary = 1; the live install does not.
    seed_installation(&db, &game, "steam", "D:/ghost", Some("D:/ghost/g.exe"), true, "deleted").await;
    let live =
        seed_installation(&db, &game, "steam", "D:/live", Some("D:/live/g.exe"), false, "installed")
            .await;

    let installs = db.list_installations(&game).await.unwrap();
    let picked = primary_installation(&installs).expect("one must be picked");
    assert_eq!(picked.id, live, "health must outrank the is_primary flag");
}

#[tokio::test]
async fn primary_installation_orders_all_statuses_like_the_sql_rule() {
    let db = test_db().await;
    let game = seed_game(&db, "Ordered").await;
    for (dir, status) in [
        ("D:/d", "deleted"),
        ("D:/m", "missing"),
        ("D:/o", "offline"),
    ] {
        seed_installation(&db, &game, "manual", dir, Some("x.exe"), false, status).await;
    }
    let installs = db.list_installations(&game).await.unwrap();
    assert_eq!(
        primary_installation(&installs).unwrap().install_dir,
        "D:/o",
        "offline outranks missing and deleted"
    );
}

#[tokio::test]
async fn primary_installation_uses_the_flag_only_to_break_ties() {
    let db = test_db().await;
    let game = seed_game(&db, "Tied").await;
    seed_installation(&db, &game, "manual", "D:/a", Some("x.exe"), false, "installed").await;
    let flagged =
        seed_installation(&db, &game, "manual", "D:/b", Some("x.exe"), true, "installed").await;
    let installs = db.list_installations(&game).await.unwrap();
    assert_eq!(primary_installation(&installs).unwrap().id, flagged);
}

#[tokio::test]
async fn primary_installation_is_none_without_installations() {
    assert!(primary_installation(&[]).is_none());
}

// ── 3.4 destination clearing must not cross games ───────────────────────

/// `idx_install_dir` is UNIQUE table-wide, so the row occupying a move's
/// destination may belong to a different game. The delete was unscoped, so a
/// Steam move silently destroyed another game's installation row along with
/// its manual executable override.
#[tokio::test]
async fn relink_refuses_a_folder_registered_to_another_game() {
    let db = test_db().await;
    let mine = seed_game(&db, "Mine").await;
    let theirs = seed_game(&db, "Theirs").await;

    let my_install =
        seed_installation(&db, &mine, "manual", "D:/mine", Some("D:/mine/g.exe"), true, "installed")
            .await;
    seed_installation(
        &db,
        &theirs,
        "manual",
        "D:/contested",
        Some("D:/contested/g.exe"),
        true,
        "installed",
    )
    .await;

    let err = db
        .relink_installation(&my_install, "D:/contested")
        .await
        .expect_err("must refuse to take another game's folder");
    assert!(matches!(err, AppError::Invalid(_)), "got {err:?}");

    // Nothing may have been destroyed by the rejected attempt.
    assert_eq!(count(&db, "game_installations", &theirs).await, 1);
    assert_eq!(count(&db, "game_installations", &mine).await, 1);
}

#[tokio::test]
async fn relink_clears_this_games_own_ghost_at_the_destination() {
    let db = test_db().await;
    let game = seed_game(&db, "Mover").await;
    let live =
        seed_installation(&db, &game, "manual", "D:/old", Some("D:/old/g.exe"), true, "installed")
            .await;
    seed_installation(&db, &game, "manual", "D:/new", Some("D:/new/g.exe"), false, "deleted").await;

    db.relink_installation(&live, "D:/new")
        .await
        .expect("its own ghost may be cleared");

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, install_dir FROM game_installations WHERE game_id = ?1")
            .bind(&game)
            .fetch_all(&db.pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "the ghost must be gone: {rows:?}");
    assert_eq!(rows[0], (live, "D:/new".to_string()));
}

/// The scan-time equivalent: a launcher move whose destination is claimed by a
/// different game must not delete that game's row.
#[tokio::test]
async fn upsert_move_detection_leaves_another_games_row_intact() {
    let db = test_db().await;

    // A Steam game currently recorded at D:/old.
    let first = db
        .upsert_game(crate::db::games::UpsertGame {
            title: "Mover",
            source_code: "steam",
            source_app_id: Some("123"),
            install_dir: "D:/old",
            executable: Some("D:/old/g.exe"),
            install_size_bytes: None,
            executable_source: ExecutableSource::Scanner,
            install_state_hint: Some(true),
        })
        .await
        .expect("first upsert");

    // An unrelated game already occupies the folder Steam now reports.
    let other = seed_game(&db, "Squatter").await;
    seed_installation(
        &db,
        &other,
        "manual",
        "D:/new",
        Some("D:/new/other.exe"),
        true,
        "installed",
    )
    .await;

    // Steam now reports app 123 at D:/new.
    db.upsert_game(crate::db::games::UpsertGame {
        title: "Mover",
        source_code: "steam",
        source_app_id: Some("123"),
        install_dir: "D:/new",
        executable: Some("D:/new/g.exe"),
        install_size_bytes: None,
        executable_source: ExecutableSource::Scanner,
        install_state_hint: Some(true),
    })
    .await
    .expect("second upsert must not fail");

    let other_rows = count(&db, "game_installations", &other).await;
    assert_eq!(
        other_rows, 1,
        "the other game's installation must survive the move"
    );

    // The squatter's row must be untouched, not merely present: step 4's
    // ON CONFLICT(install_dir) DO UPDATE does not reassign game_id, so an
    // unguarded upsert would stamp this game's identity onto that row.
    let (app_id, exe): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT source_app_id, executable FROM game_installations WHERE game_id = ?1",
    )
    .bind(&other)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(app_id, None, "the other game's row must not acquire our app id");
    assert_eq!(exe.as_deref(), Some("D:/new/other.exe"), "its executable must stand");

    // And the mover keeps its own row rather than losing history.
    let mover = first.game_id_owned.to_string();
    let mover_rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT install_dir, source_app_id FROM game_installations WHERE game_id = ?1",
    )
    .bind(&mover)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        mover_rows,
        vec![("D:/old".to_string(), Some("123".to_string()))],
        "the moved game keeps its existing row at the old directory"
    );
}

/// The documented contract: "duplicate detection is keyed off
/// `(source, source_app_id)` first, falling back to `install_dir`". The lookup
/// was `WHERE install_dir = ? OR (source + app_id) LIMIT 1` with no ORDER BY,
/// so when both arms matched *different* games SQLite could return either —
/// and the scan would be attributed to the wrong game. This is the defect the
/// move test uncovered.
#[tokio::test]
async fn duplicate_detection_prefers_launcher_identity_over_install_dir() {
    let db = test_db().await;

    // Game A is the launcher-identified one (steam app 555) at D:/a.
    let a = db
        .upsert_game(crate::db::games::UpsertGame {
            title: "Launcher Game",
            source_code: "steam",
            source_app_id: Some("555"),
            install_dir: "D:/a",
            executable: Some("D:/a/g.exe"),
            install_size_bytes: None,
            executable_source: ExecutableSource::Scanner,
            install_state_hint: Some(true),
        })
        .await
        .expect("seed A")
        .game_id_owned
        .to_string();

    // Game B independently occupies D:/b.
    let b = seed_game(&db, "Folder Game").await;
    seed_installation(&db, &b, "manual", "D:/b", Some("D:/b/g.exe"), true, "installed").await;

    // Steam now reports app 555 at D:/b — both arms of the lookup match, but
    // different games. The app-id arm must win.
    let result = db
        .upsert_game(crate::db::games::UpsertGame {
            title: "Launcher Game",
            source_code: "steam",
            source_app_id: Some("555"),
            install_dir: "D:/b",
            executable: Some("D:/b/g.exe"),
            install_size_bytes: None,
            executable_source: ExecutableSource::Scanner,
            install_state_hint: Some(true),
        })
        .await
        .expect("upsert must succeed");

    assert_eq!(
        result.game_id_owned.to_string(),
        a,
        "the scan must resolve to the launcher-identified game, not the folder's owner"
    );
    assert!(!result.created, "it must not create a third game");

    // Neither game may lose its installation.
    assert_eq!(count(&db, "game_installations", &a).await, 1);
    assert_eq!(count(&db, "game_installations", &b).await, 1);
}

/// Regression guard for the SQL NULL trap that made the first version of the
/// scoped delete look correct: `source_app_id` is NULL for every manual
/// installation, and `NULL = '123'` is NULL rather than false, so a
/// `NOT (source_id = ? AND source_app_id = ?)` predicate matched nothing and
/// the foreign row was deleted anyway. The decision now happens in Rust.
#[tokio::test]
async fn move_detection_is_null_safe_against_manual_occupants() {
    let db = test_db().await;

    let mover = db
        .upsert_game(crate::db::games::UpsertGame {
            title: "Mover",
            source_code: "steam",
            source_app_id: Some("777"),
            install_dir: "D:/from",
            executable: Some("D:/from/g.exe"),
            install_size_bytes: None,
            executable_source: ExecutableSource::Scanner,
            install_state_hint: Some(true),
        })
        .await
        .unwrap()
        .game_id_owned
        .to_string();

    // A *manual* occupant, i.e. source_app_id IS NULL — the NULL-trap case.
    let manual = seed_game(&db, "Manual Occupant").await;
    seed_installation(&db, &manual, "manual", "D:/to", Some("D:/to/m.exe"), true, "installed").await;

    db.upsert_game(crate::db::games::UpsertGame {
        title: "Mover",
        source_code: "steam",
        source_app_id: Some("777"),
        install_dir: "D:/to",
        executable: Some("D:/to/g.exe"),
        install_size_bytes: None,
        executable_source: ExecutableSource::Scanner,
        install_state_hint: Some(true),
    })
    .await
    .unwrap();

    assert_eq!(
        count(&db, "game_installations", &manual).await,
        1,
        "a NULL source_app_id occupant must not be deleted"
    );
    assert_eq!(
        count(&db, "game_installations", &mover).await,
        1,
        "the mover keeps exactly one row"
    );
}

/// The legitimate case must still work: the destination is occupied by this
/// same game's own stale ghost, so the move proceeds and the ghost is cleared.
#[tokio::test]
async fn move_detection_clears_its_own_ghost_and_relinks() {
    let db = test_db().await;

    let game = db
        .upsert_game(crate::db::games::UpsertGame {
            title: "Ghosted",
            source_code: "steam",
            source_app_id: Some("888"),
            install_dir: "D:/old",
            executable: Some("D:/old/g.exe"),
            install_size_bytes: None,
            executable_source: ExecutableSource::Scanner,
            install_state_hint: Some(true),
        })
        .await
        .unwrap()
        .game_id_owned
        .to_string();

    // This same game also has a stale row at the destination.
    seed_installation(&db, &game, "steam", "D:/new", Some("D:/new/g.exe"), false, "deleted").await;

    db.upsert_game(crate::db::games::UpsertGame {
        title: "Ghosted",
        source_code: "steam",
        source_app_id: Some("888"),
        install_dir: "D:/new",
        executable: Some("D:/new/g.exe"),
        install_size_bytes: None,
        executable_source: ExecutableSource::Scanner,
        install_state_hint: Some(true),
    })
    .await
    .expect("move onto its own ghost must succeed");

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT install_dir, status FROM game_installations WHERE game_id = ?1",
    )
    .bind(&game)
    .fetch_all(&db.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "ghost must be collapsed into one row: {rows:?}");
    assert_eq!(rows[0].0, "D:/new");
    assert_eq!(rows[0].1, "installed", "the relinked row is live again");
}

/// A manual executable override must survive a launcher move, which is the
/// whole reason the move relinks in place instead of inserting a new row.
#[tokio::test]
async fn move_detection_preserves_a_manual_executable_override() {
    let db = test_db().await;

    db.upsert_game(crate::db::games::UpsertGame {
        title: "Overridden",
        source_code: "steam",
        source_app_id: Some("999"),
        install_dir: "D:/old",
        executable: Some("D:/old/user-chosen.exe"),
        install_size_bytes: None,
        executable_source: ExecutableSource::User,
        install_state_hint: Some(true),
    })
    .await
    .unwrap();

    db.upsert_game(crate::db::games::UpsertGame {
        title: "Overridden",
        source_code: "steam",
        source_app_id: Some("999"),
        install_dir: "D:/new",
        executable: Some("D:/new/scanner-guess.exe"),
        install_size_bytes: None,
        executable_source: ExecutableSource::Scanner,
        install_state_hint: Some(true),
    })
    .await
    .unwrap();

    let (dir, exe, overridden): (String, Option<String>, i64) = sqlx::query_as(
        "SELECT install_dir, executable, executable_override FROM game_installations",
    )
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(dir, "D:/new", "the row moved");
    assert_eq!(overridden, 1, "the override flag latches");
    assert_eq!(
        exe.as_deref(),
        Some("D:/old/user-chosen.exe"),
        "the scanner must not replace a user-chosen executable"
    );
}

// ── heatmap day bucketing across timezones ──────────────────────────────

/// Insert a session that started at an explicit UTC instant.
async fn seed_session_at(db: &crate::db::Db, game_id: &str, started_at: &str, seconds: i64) {
    sqlx::query(
        "INSERT INTO play_sessions (game_id, started_at, ended_at, duration_seconds, idle_seconds) \
         VALUES (?1, ?2, ?2, ?3, 0)",
    )
    .bind(game_id)
    .bind(started_at)
    .bind(seconds)
    .execute(&db.pool)
    .await
    .expect("seed session");
}

/// Sessions were bucketed by UTC date while the frontend grid was built from
/// local midnights, so the whole heatmap shifted by a day for any timezone away
/// from UTC. A session at 18:30 UTC is already the next day in +05:30 and must be
/// counted there.
#[tokio::test]
async fn heatmap_buckets_sessions_by_local_day_not_utc_day() {
    let db = test_db().await;
    let game = seed_game(&db, "Late Night").await;
    let now = chrono::Utc::now();
    // 18:30 UTC today → 00:00 tomorrow at +05:30.
    let instant = now
        .date_naive()
        .and_hms_opt(18, 30, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();
    seed_session_at(&db, &game, &instant, 600).await;

    let utc = crate::commands::analytics::heatmap_rows(&db.pool, 365, "utc")
        .await
        .unwrap();
    let kolkata = crate::commands::analytics::heatmap_rows(&db.pool, 365, "+330 minutes")
        .await
        .unwrap();

    assert_eq!(utc.len(), 1);
    assert_eq!(kolkata.len(), 1);
    assert_eq!(
        utc[0].day,
        now.format("%Y-%m-%d").to_string(),
        "UTC bucketing keeps it on today"
    );
    assert_eq!(
        kolkata[0].day,
        (now + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
        "at +05:30 the session belongs to the next local day"
    );
}

/// The same instant west of UTC lands on the previous local day.
#[tokio::test]
async fn heatmap_buckets_correctly_west_of_utc() {
    let db = test_db().await;
    let game = seed_game(&db, "Early Morning").await;
    let now = chrono::Utc::now();
    let instant = now
        .date_naive()
        .and_hms_opt(2, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();
    seed_session_at(&db, &game, &instant, 300).await;

    let pacific = crate::commands::analytics::heatmap_rows(&db.pool, 365, "-480 minutes")
        .await
        .unwrap();
    assert_eq!(
        pacific[0].day,
        (now - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
        "02:00 UTC is still the previous day at -08:00"
    );
}

/// Two sessions on different UTC days but the same local day must be one bucket.
#[tokio::test]
async fn heatmap_merges_sessions_that_share_a_local_day() {
    let db = test_db().await;
    let game = seed_game(&db, "Across Midnight").await;
    let today = chrono::Utc::now().date_naive();

    // 19:00 and 20:00 UTC are both the next day at +05:30.
    for hour in [19, 20] {
        let instant = today.and_hms_opt(hour, 0, 0).unwrap().and_utc().to_rfc3339();
        seed_session_at(&db, &game, &instant, 100).await;
    }

    let rows = crate::commands::analytics::heatmap_rows(&db.pool, 365, "+330 minutes")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "one local day, one bucket: {rows:?}");
    assert_eq!(rows[0].seconds, 200, "durations are summed within a day");
}

#[tokio::test]
async fn heatmap_reports_active_time_excluding_idle() {
    let db = test_db().await;
    let game = seed_game(&db, "Idled").await;
    let instant = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO play_sessions (game_id, started_at, ended_at, duration_seconds, idle_seconds) \
         VALUES (?1, ?2, ?2, 600, 150)",
    )
    .bind(&game)
    .bind(&instant)
    .execute(&db.pool)
    .await
    .unwrap();

    let rows = crate::commands::analytics::heatmap_rows(&db.pool, 365, "utc")
        .await
        .unwrap();
    assert_eq!(rows[0].seconds, 450, "duration minus idle");
}

/// The cutoff compared an RFC3339 stamp (`…T…`) against SQLite's space-separated
/// `datetime('now', …)` lexicographically, and `'T' > ' '`, so the boundary
/// silently became "start of the cutoff day". Both sides now go through
/// `datetime()`.
#[tokio::test]
async fn heatmap_window_excludes_sessions_older_than_the_cutoff() {
    let db = test_db().await;
    let game = seed_game(&db, "Windowed").await;
    let now = chrono::Utc::now();

    seed_session_at(&db, &game, &(now - chrono::Duration::days(3)).to_rfc3339(), 60).await;
    seed_session_at(&db, &game, &(now - chrono::Duration::days(40)).to_rfc3339(), 60).await;

    let rows = crate::commands::analytics::heatmap_rows(&db.pool, 30, "utc")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "only the session inside the window: {rows:?}");

    let wide = crate::commands::analytics::heatmap_rows(&db.pool, 365, "utc")
        .await
        .unwrap();
    assert_eq!(wide.len(), 2, "a wider window includes both");
}

#[tokio::test]
async fn heatmap_is_empty_for_a_library_with_no_sessions() {
    let db = test_db().await;
    let rows = crate::commands::analytics::heatmap_rows(&db.pool, 365, "localtime")
        .await
        .unwrap();
    assert!(rows.is_empty());
}

// ── executable precedence by caller intent ──────────────────────────────

async fn upsert_exe(
    db: &crate::db::Db,
    dir: &str,
    exe: &str,
    source: ExecutableSource,
) -> crate::db::games::UpsertResult {
    db.upsert_game(crate::db::games::UpsertGame {
        title: "Red Dead Redemption 2",
        source_code: "manual",
        source_app_id: None,
        install_dir: dir,
        executable: Some(exe),
        install_size_bytes: None,
        executable_source: source,
        install_state_hint: Some(true),
    })
    .await
    .expect("upsert")
}

async fn stored_exe(db: &crate::db::Db, dir: &str) -> (Option<String>, i64) {
    sqlx::query_as(
        "SELECT executable, executable_override FROM game_installations WHERE install_dir = ?1",
    )
    .bind(dir)
    .fetch_one(&db.pool)
    .await
    .unwrap()
}

/// The manual-import bug. The conflict clause kept the stored executable whenever
/// `executable_override = 1`, so once a user had chosen one, their *own* later
/// choice was discarded too — the guard could not tell a scanner overwriting a
/// user's choice from the user making a new one.
#[tokio::test]
async fn a_user_choice_overrides_an_earlier_user_choice() {
    let db = test_db().await;
    let dir = "D:/Games/Red Dead Redemption 2";

    upsert_exe(&db, dir, "RDR2.exe", ExecutableSource::User).await;
    assert_eq!(stored_exe(&db, dir).await, (Some("RDR2.exe".into()), 1));

    // The user changes their mind — this must win.
    upsert_exe(&db, dir, "Launcher.exe", ExecutableSource::User).await;
    assert_eq!(
        stored_exe(&db, dir).await,
        (Some("Launcher.exe".into()), 1),
        "an explicit user import must replace an earlier user choice"
    );
}

/// The protection that must survive: scanners still cannot overwrite a choice.
#[tokio::test]
async fn a_scanner_cannot_overwrite_a_user_choice() {
    let db = test_db().await;
    let dir = "D:/Games/Red Dead Redemption 2";

    upsert_exe(&db, dir, "Launcher.exe", ExecutableSource::User).await;
    upsert_exe(&db, dir, "RDR2.exe", ExecutableSource::Scanner).await;

    assert_eq!(
        stored_exe(&db, dir).await,
        (Some("Launcher.exe".into()), 1),
        "a rescan must leave the user's executable alone"
    );
}

#[tokio::test]
async fn a_scanner_may_update_its_own_earlier_detection() {
    let db = test_db().await;
    let dir = "D:/Games/Some Game";

    upsert_exe(&db, dir, "old.exe", ExecutableSource::Scanner).await;
    upsert_exe(&db, dir, "new.exe", ExecutableSource::Scanner).await;

    assert_eq!(
        stored_exe(&db, dir).await,
        (Some("new.exe".into()), 0),
        "scanner detections are not user choices and may be refined"
    );
}

/// The override flag latches: a user choice followed by scans keeps the flag set.
#[tokio::test]
async fn the_override_flag_latches_once_a_user_has_chosen() {
    let db = test_db().await;
    let dir = "D:/Games/Latched";

    upsert_exe(&db, dir, "detected.exe", ExecutableSource::Scanner).await;
    assert_eq!(stored_exe(&db, dir).await.1, 0);

    upsert_exe(&db, dir, "chosen.exe", ExecutableSource::User).await;
    upsert_exe(&db, dir, "detected-again.exe", ExecutableSource::Scanner).await;

    let (exe, flag) = stored_exe(&db, dir).await;
    assert_eq!(exe.as_deref(), Some("chosen.exe"));
    assert_eq!(flag, 1, "a later scan must not clear the override");
}

// ── unrelated pure helper, cheap to pin ─────────────────────────────────

#[test]
fn sort_title_strips_leading_articles() {
    assert_eq!(normalize_sort_title("The Witcher 3"), "Witcher 3");
    assert_eq!(normalize_sort_title("A Plague Tale"), "Plague Tale");
    assert_eq!(normalize_sort_title("An Odd Game"), "Odd Game");
    assert_eq!(normalize_sort_title("Portal 2"), "Portal 2");
    assert_eq!(normalize_sort_title("  Hades  "), "Hades");
}

#[test]
fn now_rfc3339_is_sortable_and_parseable() {
    let a = now_rfc3339();
    assert!(a.contains('T'), "{a} must be RFC3339, not a SQLite datetime");
    assert!(chrono::DateTime::parse_from_rfc3339(&a).is_ok(), "{a}");
}
