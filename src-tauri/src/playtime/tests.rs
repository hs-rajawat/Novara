//! Regression tests for the Batch 4 playtime repairs.

use std::collections::HashMap;
use std::time::Duration;

use super::*;
use crate::events::EventBus;
use crate::test_support::{seed_game, seed_installation, test_db};

fn target(game: &str, dir: &str, exe: Option<&str>) -> WatchTarget {
    WatchTarget {
        game_id: game.to_string(),
        install_dir: normalize_path(dir),
        exe_name: exe.and_then(file_name_of),
        executable: exe.map(normalize_path),
    }
}

// ── path matching ───────────────────────────────────────────────────────

#[test]
fn normalizes_separators_and_case() {
    assert_eq!(normalize_path(r"D:\Games\Portal\"), "d:/games/portal");
    assert_eq!(normalize_path("D:/Games/Portal"), "d:/games/portal");
}

/// A plain `starts_with` would place `D:/games/portal2` inside
/// `D:/games/portal`, crediting one game's process to another.
#[test]
fn containment_respects_path_segments() {
    assert!(is_under("d:/games/portal/bin/game.exe", "d:/games/portal"));
    assert!(!is_under("d:/games/portal2/bin/game.exe", "d:/games/portal"));
    assert!(!is_under("d:/games/portal", "d:/games/portal"));
}

/// The core of bug 4.1: Steam installations carry no executable, so name-based
/// matching could never find them. Directory containment can.
#[test]
fn matches_a_steam_process_with_no_recorded_executable() {
    let targets = vec![target("steam-game", r"D:\Steam\steamapps\common\Portal 2", None)];
    let unique = unique_exe_names(&targets);
    let hit = match_process(
        &targets,
        Some("d:/steam/steamapps/common/portal 2/bin/portal2.exe"),
        Some("portal2.exe"),
        &unique,
    );
    assert_eq!(hit, Some(("steam-game".to_string(), "portal2.exe".to_string())));
}

#[test]
fn prefers_an_exact_recorded_executable_over_directory_containment() {
    let targets = vec![
        target("by-dir", r"D:\Games", None),
        target("by-exe", r"D:\Games\Sub", Some(r"D:\Games\Sub\game.exe")),
    ];
    let unique = unique_exe_names(&targets);
    let hit = match_process(&targets, Some("d:/games/sub/game.exe"), Some("game.exe"), &unique);
    assert_eq!(hit.unwrap().0, "by-exe");
}

/// Nested installations must resolve to the most specific directory, not to
/// whichever happens to be checked first.
#[test]
fn deepest_matching_install_dir_wins() {
    let targets = vec![
        target("parent", r"D:\Games", None),
        target("child", r"D:\Games\Inner", None),
    ];
    let unique = unique_exe_names(&targets);
    let hit = match_process(&targets, Some("d:/games/inner/bin/g.exe"), Some("g.exe"), &unique);
    assert_eq!(hit.unwrap().0, "child");
}

#[test]
fn unrelated_processes_are_not_attributed() {
    let targets = vec![target("g", r"D:\Games\Portal", None)];
    let unique = unique_exe_names(&targets);
    assert!(match_process(&targets, Some("c:/windows/explorer.exe"), Some("explorer.exe"), &unique)
        .is_none());
}

/// Bug 4.4: matching by bare file name silently credited whichever game was
/// indexed last. When a path is unavailable the name is only trusted if it is
/// unambiguous.
#[test]
fn ambiguous_file_names_are_refused_when_no_path_is_available() {
    let targets = vec![
        target("a", r"D:\A", Some(r"D:\A\game.exe")),
        target("b", r"D:\B", Some(r"D:\B\game.exe")),
    ];
    let unique = unique_exe_names(&targets);
    assert!(unique.is_empty(), "a shared name identifies nothing");
    assert!(match_process(&targets, None, Some("game.exe"), &unique).is_none());
}

#[test]
fn unique_file_names_still_match_when_no_path_is_available() {
    let targets = vec![
        target("a", r"D:\A", Some(r"D:\A\alpha.exe")),
        target("b", r"D:\B", Some(r"D:\B\beta.exe")),
    ];
    let unique = unique_exe_names(&targets);
    let hit = match_process(&targets, None, Some("beta.exe"), &unique);
    assert_eq!(hit.unwrap().0, "b");
}

// ── session lifecycle ───────────────────────────────────────────────────

async fn tracker(db: &crate::db::Db) -> Arc<PlaytimeTracker> {
    // Capacity mirrors production; nothing subscribes in these tests, and the
    // bus drops rather than blocks, so emitted events are simply discarded.
    Arc::new(PlaytimeTracker::new(db.clone(), EventBus::new(256)))
}

async fn session_row(db: &crate::db::Db, id: i64) -> Option<(Option<String>, i64)> {
    sqlx::query_as("SELECT ended_at, duration_seconds FROM play_sessions WHERE id = ?1")
        .bind(id)
        .fetch_optional(&db.pool)
        .await
        .unwrap()
}

/// Bug 4.1, at the lifecycle level: an explicitly launched session must not be
/// closed just because the game's process has not appeared yet.
#[tokio::test]
async fn an_explicit_session_survives_ticks_before_the_process_appears() {
    let db = test_db().await;
    let game = seed_game(&db, "Portal 2").await;
    let pt = tracker(&db).await;

    let session = pt.start(&game, None).await.unwrap();
    // Several passes with nothing running, still inside the grace window.
    for _ in 0..3 {
        pt.reconcile(HashMap::new(), Duration::from_secs(180)).await.unwrap();
    }

    assert!(pt.is_active(&game).await, "session must still be open");
    assert_eq!(
        session_row(&db, session).await.unwrap().0,
        None,
        "the row must not have been closed"
    );
}

#[tokio::test]
async fn a_session_closes_once_its_process_has_run_and_exited() {
    let db = test_db().await;
    let game = seed_game(&db, "Portal 2").await;
    let pt = tracker(&db).await;
    let session = pt.start(&game, None).await.unwrap();

    // The process appears...
    let mut seen = HashMap::new();
    seen.insert(game.clone(), "portal2.exe".to_string());
    pt.reconcile(seen, Duration::from_secs(180)).await.unwrap();
    assert!(pt.is_active(&game).await, "still running");

    // ...and then exits.
    pt.reconcile(HashMap::new(), Duration::from_secs(180)).await.unwrap();
    assert!(!pt.is_active(&game).await, "session must close on exit");
    let row = session_row(&db, session).await.unwrap();
    assert!(row.0.is_some(), "ended_at must be set");
}

/// A launch that never produces a process must leave no trace, rather than
/// recording a phantom session as long as the grace window.
#[tokio::test]
async fn a_launch_that_never_starts_is_discarded_after_the_grace_window() {
    let db = test_db().await;
    let game = seed_game(&db, "Never Starts").await;
    let pt = tracker(&db).await;
    let session = pt.start(&game, None).await.unwrap();

    // Zero grace: the window has already expired.
    pt.reconcile(HashMap::new(), Duration::ZERO).await.unwrap();

    assert!(!pt.is_active(&game).await);
    assert!(
        session_row(&db, session).await.is_none(),
        "the phantom session row must be removed"
    );
    let playtime: i64 =
        sqlx::query_scalar("SELECT total_playtime_seconds FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(playtime, 0, "nothing may be credited for a launch that failed");
}

/// `start_session` stamps `last_played_at` optimistically, so discarding must
/// roll it back or the game claims a play it never had.
#[tokio::test]
async fn discarding_a_session_rolls_back_last_played_at() {
    let db = test_db().await;
    let game = seed_game(&db, "Never Starts").await;
    let pt = tracker(&db).await;

    pt.start(&game, None).await.unwrap();
    let during: Option<String> =
        sqlx::query_scalar("SELECT last_played_at FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(during.is_some(), "start stamps last_played_at");

    pt.reconcile(HashMap::new(), Duration::ZERO).await.unwrap();

    let after: Option<String> =
        sqlx::query_scalar("SELECT last_played_at FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(after.is_none(), "expected NULL after discard, got {after:?}");
}

/// An earlier genuine play must survive the rollback.
#[tokio::test]
async fn discarding_preserves_an_earlier_real_play() {
    let db = test_db().await;
    let game = seed_game(&db, "Played Before").await;
    let pt = tracker(&db).await;

    // A real, completed session.
    let mut seen = HashMap::new();
    seen.insert(game.clone(), "g.exe".to_string());
    pt.reconcile(seen, Duration::from_secs(180)).await.unwrap();
    pt.reconcile(HashMap::new(), Duration::from_secs(180)).await.unwrap();
    let real_last: Option<String> =
        sqlx::query_scalar("SELECT last_played_at FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert!(real_last.is_some());

    // Now a failed launch.
    pt.start(&game, None).await.unwrap();
    pt.reconcile(HashMap::new(), Duration::ZERO).await.unwrap();

    let after: Option<String> =
        sqlx::query_scalar("SELECT last_played_at FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(after, real_last, "the earlier real play must remain");
}

/// The watcher may open a session on its own for a game that was started
/// outside NOVARA.
#[tokio::test]
async fn the_watcher_opens_a_session_for_an_externally_started_game() {
    let db = test_db().await;
    let game = seed_game(&db, "Started Outside").await;
    let pt = tracker(&db).await;

    let mut seen = HashMap::new();
    seen.insert(game.clone(), "g.exe".to_string());
    pt.reconcile(seen, Duration::from_secs(180)).await.unwrap();

    assert!(pt.is_active(&game).await);
    let (name,): (Option<String>,) =
        sqlx::query_as("SELECT process_name FROM play_sessions WHERE game_id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(name.as_deref(), Some("g.exe"));
}

// ── 4.2 graceful shutdown ───────────────────────────────────────────────

#[tokio::test]
async fn stop_all_closes_open_sessions_and_credits_time() {
    let db = test_db().await;
    let a = seed_game(&db, "A").await;
    let b = seed_game(&db, "B").await;
    let pt = tracker(&db).await;

    let sa = pt.start(&a, Some("a.exe")).await.unwrap();
    let sb = pt.start(&b, Some("b.exe")).await.unwrap();

    let closed = pt.stop_all().await.unwrap();
    assert_eq!(closed, 2);
    for id in [sa, sb] {
        let row = session_row(&db, id).await.unwrap();
        assert!(row.0.is_some(), "session {id} must be closed");
    }
    assert!(!pt.is_active(&a).await && !pt.is_active(&b).await);
}

#[tokio::test]
async fn stop_all_is_a_no_op_with_nothing_running() {
    let db = test_db().await;
    let pt = tracker(&db).await;
    assert_eq!(pt.stop_all().await.unwrap(), 0);
}

// ── 4.3 orphan repair ───────────────────────────────────────────────────

/// Rows left open by a previous run have no owner after a restart, because the
/// in-memory map starts empty. They must be closed without inventing playtime.
#[tokio::test]
async fn orphaned_sessions_are_closed_without_crediting_playtime() {
    let db = test_db().await;
    let game = seed_game(&db, "Crashed").await;
    sqlx::query(
        "INSERT INTO play_sessions (game_id, started_at, duration_seconds, idle_seconds) \
         VALUES (?1, '2026-07-20T10:00:00+00:00', 0, 0)",
    )
    .bind(&game)
    .execute(&db.pool)
    .await
    .unwrap();

    let closed = db.close_orphaned_sessions().await.unwrap();
    assert_eq!(closed, 1);

    let (ended, dur): (Option<String>, i64) = sqlx::query_as(
        "SELECT ended_at, duration_seconds FROM play_sessions WHERE game_id = ?1",
    )
    .bind(&game)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(
        ended.as_deref(),
        Some("2026-07-20T10:00:00+00:00"),
        "ended_at falls back to started_at rather than being invented"
    );
    assert_eq!(dur, 0);

    let playtime: i64 =
        sqlx::query_scalar("SELECT total_playtime_seconds FROM games WHERE id = ?1")
            .bind(&game)
            .fetch_one(&db.pool)
            .await
            .unwrap();
    assert_eq!(playtime, 0, "an unknowable duration must credit nothing");
}

#[tokio::test]
async fn orphan_repair_leaves_completed_sessions_alone() {
    let db = test_db().await;
    let game = seed_game(&db, "Fine").await;
    sqlx::query(
        "INSERT INTO play_sessions (game_id, started_at, ended_at, duration_seconds, idle_seconds) \
         VALUES (?1, '2026-07-20T10:00:00+00:00', '2026-07-20T11:00:00+00:00', 3600, 0)",
    )
    .bind(&game)
    .execute(&db.pool)
    .await
    .unwrap();

    assert_eq!(db.close_orphaned_sessions().await.unwrap(), 0);
    let dur: i64 = sqlx::query_scalar("SELECT duration_seconds FROM play_sessions")
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(dur, 3600, "a completed session must not be rewritten");
}

// ── 4.5 idle accounting ─────────────────────────────────────────────────

#[tokio::test]
async fn reported_idle_time_is_subtracted_from_credited_playtime() {
    let db = test_db().await;
    let game = seed_game(&db, "Idled").await;
    let pt = tracker(&db).await;

    let session = pt.start(&game, Some("g.exe")).await.unwrap();
    pt.report_idle(&game, 30).await;
    pt.report_idle(&game, 12).await;
    pt.stop(&game).await.unwrap();

    let idle: i64 = sqlx::query_scalar("SELECT idle_seconds FROM play_sessions WHERE id = ?1")
        .bind(session)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(idle, 42, "idle deltas accumulate");
}

#[tokio::test]
async fn reporting_idle_for_an_unknown_game_is_ignored() {
    let db = test_db().await;
    let pt = tracker(&db).await;
    // Must not panic or create state.
    pt.report_idle("no-such-game", 10).await;
    assert!(!pt.is_active("no-such-game").await);
}

// ── watch target selection ──────────────────────────────────────────────

/// The old index filtered to `executable IS NOT NULL`, which excluded every
/// Steam installation — the root of bug 4.1.
#[tokio::test]
async fn watch_targets_include_installations_without_an_executable() {
    let db = test_db().await;
    let steam = seed_game(&db, "Steam Game").await;
    seed_installation(&db, &steam, "steam", "D:/steam/portal", None, true, "installed").await;

    let targets = db.list_watch_targets().await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].0, steam);
    assert_eq!(targets[0].2, None, "no executable, still watched");
}

#[tokio::test]
async fn watch_targets_exclude_deleted_installations() {
    let db = test_db().await;
    let gone = seed_game(&db, "Uninstalled").await;
    seed_installation(&db, &gone, "steam", "D:/gone", Some("D:/gone/g.exe"), true, "deleted").await;
    let live = seed_game(&db, "Present").await;
    seed_installation(&db, &live, "steam", "D:/live", None, true, "installed").await;

    let targets = db.list_watch_targets().await.unwrap();
    assert_eq!(targets.len(), 1, "a deleted install has no process to attribute");
    assert_eq!(targets[0].0, live);
}
