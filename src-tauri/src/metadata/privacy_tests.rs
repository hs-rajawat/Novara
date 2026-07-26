//! The privacy guarantee, asserted at the socket.
//!
//! The README's headline promise is "zero telemetry, no network calls by
//! default". Until now that was upheld only by review: the services take an
//! `allow_network` flag, every network provider checks it, and nothing in the test
//! suite would have noticed if one stopped. A provider that forgot the check, or a
//! new one that never had it, would ship silently — and the failure mode is
//! invisible to the user it harms.
//!
//! These tests do not inspect `allow_network` handling. They count TCP
//! connections. Every service is built with a `reqwest::Client` whose proxy points
//! at a listener owned by the test, so *any* outbound HTTP or HTTPS request —
//! whatever host it names, from whichever provider — arrives here and is counted.
//! Hardcoded URLs in the providers need no test seam, and a future provider is
//! covered the day it is registered.
//!
//! Each assertion is paired with the same fill run with network allowed, which
//! must reach the listener. Without that pair, a test asserting "no connections"
//! would keep passing if the counter, the proxy or the fill itself stopped working.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::net::TcpListener;

use crate::events::EventBus;
use crate::metadata::artwork_service::ArtworkService;
use crate::metadata::text_service::MetadataService;
use crate::metadata::throttle::Throttle;
use crate::test_support::{seed_game, test_db};

/// A listener that counts connection attempts and refuses to serve any.
///
/// Connections are dropped immediately rather than answered: the point is to
/// detect the attempt, and closing at once makes the client fail fast instead of
/// waiting out the ten-second request timeout.
async fn counting_listener() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a local listener");
    let addr = listener.local_addr().expect("listener address");
    let count = Arc::new(AtomicUsize::new(0));

    let seen = count.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });

    (addr, count)
}

/// A client that can only reach the test's listener.
///
/// `Proxy::all` covers HTTPS as well as HTTP — an HTTPS request becomes a CONNECT
/// to the proxy, which is still a connection to this listener and is still
/// counted. `no_proxy` is not set, so there is no host a provider could name that
/// would escape.
fn proxied_client(addr: SocketAddr) -> reqwest::Client {
    reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{addr}")).expect("proxy"))
        .build()
        .expect("build client")
}

/// A Steam game, so the network providers have an identity they can act on and
/// would genuinely try to fetch. A game they cannot identify would produce zero
/// connections for the wrong reason.
async fn seed_steam_game(db: &crate::db::Db) -> String {
    let game = seed_game(db, "Half-Life 2").await;
    crate::test_support::seed_installation(
        db,
        &game,
        "steam",
        "C:/Steam/steamapps/common/Half-Life 2",
        Some("hl2.exe"),
        true,
        "installed",
    )
    .await;
    sqlx::query("UPDATE game_installations SET source_app_id = '220' WHERE game_id = ?1")
        .bind(&game)
        .execute(&db.pool)
        .await
        .expect("give the installation a steam app id");
    game
}

fn throttle() -> Arc<Throttle> {
    Arc::new(Throttle::new(2, std::time::Duration::ZERO))
}

// ── the text fill ───────────────────────────────────────────────────────

#[tokio::test]
async fn a_text_fill_opens_no_connection_when_network_is_not_allowed() {
    let db = test_db().await;
    seed_steam_game(&db).await;
    let (addr, connections) = counting_listener().await;
    let service = MetadataService::new(
        db.clone(),
        EventBus::new(256),
        proxied_client(addr),
        throttle(),
    );

    service.fill_missing(false).await.expect("fill");

    assert_eq!(
        connections.load(Ordering::SeqCst),
        0,
        "a metadata fill must not open a single connection while the user has \
         metadata disabled"
    );
}

#[tokio::test]
async fn a_text_fill_does_reach_the_network_when_allowed() {
    let db = test_db().await;
    seed_steam_game(&db).await;
    let (addr, connections) = counting_listener().await;
    let service = MetadataService::new(
        db.clone(),
        EventBus::new(256),
        proxied_client(addr),
        throttle(),
    );

    service.fill_missing(true).await.expect("fill");

    assert!(
        connections.load(Ordering::SeqCst) > 0,
        "the paired test above is only meaningful if this fill really would have \
         gone to the network"
    );
}

// ── the artwork fill ────────────────────────────────────────────────────

#[tokio::test]
async fn an_artwork_fill_opens_no_connection_when_network_is_not_allowed() {
    let db = test_db().await;
    seed_steam_game(&db).await;
    let (addr, connections) = counting_listener().await;
    let service = ArtworkService::new(
        db.clone(),
        EventBus::new(256),
        std::env::temp_dir().join("novara-privacy-test"),
        proxied_client(addr),
        throttle(),
    );

    service.fill_missing(false).await.expect("fill");

    assert_eq!(
        connections.load(Ordering::SeqCst),
        0,
        "an artwork fill must not open a single connection while the user has \
         metadata disabled"
    );
}

#[tokio::test]
async fn an_artwork_fill_does_reach_the_network_when_allowed() {
    let db = test_db().await;
    seed_steam_game(&db).await;
    let (addr, connections) = counting_listener().await;
    let service = ArtworkService::new(
        db.clone(),
        EventBus::new(256),
        std::env::temp_dir().join("novara-privacy-test"),
        proxied_client(addr),
        throttle(),
    );

    service.fill_missing(true).await.expect("fill");

    assert!(
        connections.load(Ordering::SeqCst) > 0,
        "the paired test above is only meaningful if this fill really would have \
         gone to the network"
    );
}

// ── an explicit single-game refresh ─────────────────────────────────────

/// The "Refresh Metadata" button is a deliberate user action, but it is still
/// bound by the same gate — asking for a refresh is not consent to use the
/// network, and the command reports `network_allowed: false` so the UI can explain
/// the no-op rather than appearing to do nothing.
#[tokio::test]
async fn an_explicit_refresh_opens_no_connection_when_network_is_not_allowed() {
    let db = test_db().await;
    let game = seed_steam_game(&db).await;
    let (addr, connections) = counting_listener().await;
    let text = MetadataService::new(
        db.clone(),
        EventBus::new(256),
        proxied_client(addr),
        throttle(),
    );
    let artwork = ArtworkService::new(
        db.clone(),
        EventBus::new(256),
        std::env::temp_dir().join("novara-privacy-test"),
        proxied_client(addr),
        throttle(),
    );

    text.refresh_game(&game, false).await.expect("text refresh");
    artwork
        .refresh_game(&game, false)
        .await
        .expect("artwork refresh");

    assert_eq!(
        connections.load(Ordering::SeqCst),
        0,
        "an explicit refresh is still subject to the user's settings"
    );
}

// ── the gate itself ─────────────────────────────────────────────────────

/// The flag the services are handed must follow the user's settings, including
/// when they have never been written.
#[tokio::test]
async fn the_network_gate_denies_by_default_and_obeys_both_settings() {
    // (metadata_enabled, offline_mode, expected)
    let cases = [
        (None, None, false),
        (Some(false), None, false),
        (Some(true), None, true),
        (Some(true), Some(false), true),
        (Some(true), Some(true), false),
        (Some(false), Some(true), false),
    ];

    for (enabled, offline, expected) in cases {
        let db = test_db().await;
        // Migration 0001 seeds these keys, so an "unset" case has to remove them —
        // which also covers a database where a row was lost.
        for key in ["metadata_enabled", "offline_mode"] {
            sqlx::query("DELETE FROM settings WHERE key = ?1")
                .bind(key)
                .execute(&db.pool)
                .await
                .expect("clear setting");
        }
        if let Some(v) = enabled {
            db.set_setting("metadata_enabled", &serde_json::Value::Bool(v))
                .await
                .expect("set metadata_enabled");
        }
        if let Some(v) = offline {
            db.set_setting("offline_mode", &serde_json::Value::Bool(v))
                .await
                .expect("set offline_mode");
        }

        assert_eq!(
            db.allow_metadata_network().await.expect("read the gate"),
            expected,
            "metadata_enabled={enabled:?}, offline_mode={offline:?}"
        );
    }
}
