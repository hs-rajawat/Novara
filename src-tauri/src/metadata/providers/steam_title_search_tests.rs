//! Tests for title matching.
//!
//! The matching rule is the whole safety story of this feature: a wrong match
//! writes another game's description and artwork over a real one, and the user has
//! no reason to distrust it. Every case here is either a real title pair from a
//! live library or a real decoy returned by an actual search.

use super::*;

fn item(id: u64, name: &str) -> SearchItem {
    SearchItem {
        id,
        name: name.to_string(),
        r#type: Some("app".to_string()),
    }
}

// ── normalisation ───────────────────────────────────────────────────────

/// The case that forces normalisation to exist at all: Epic writes the title
/// without the colon, Steam writes it with one.
#[test]
fn punctuation_differences_between_sources_are_absorbed() {
    assert_eq!(
        normalise("Dying Light The Following"),
        normalise("Dying Light: The Following")
    );
}

#[test]
fn case_and_spacing_differences_are_absorbed() {
    assert_eq!(normalise("HALF-LIFE 2"), normalise("Half-Life 2"));
    assert_eq!(normalise("  Portal   2  "), normalise("Portal 2"));
    assert_eq!(normalise("Warhammer 40,000: Boltgun"), normalise("Warhammer 40000 Boltgun"));
}

/// Apostrophes sit inside a word, so they are removed rather than treated as
/// separators — otherwise "Sid Meier's" becomes "sid meier s" and stops matching a
/// source that drops the apostrophe. Both the typewriter and typographic forms
/// occur in real titles.
#[test]
fn apostrophes_are_removed_rather_than_separating() {
    assert_eq!(
        normalise("Sid Meier's Civilization VI"),
        normalise("Sid Meiers Civilization VI")
    );
    assert_eq!(
        normalise("Assassin\u{2019}s Creed"),
        normalise("Assassin's Creed")
    );
    assert_eq!(normalise("Sid Meier's"), "sid meiers");
}

#[test]
fn trademark_and_edition_punctuation_is_absorbed() {
    assert_eq!(normalise("Game™"), normalise("Game"));
    assert_eq!(normalise("Game®  (2019)"), normalise("Game 2019"));
}

/// Punctuation separates words rather than vanishing, or "Half-Life" would become
/// "halflife" and stop matching Steam's own spacing variants.
#[test]
fn punctuation_separates_rather_than_joins() {
    assert_eq!(normalise("Half-Life"), "half life");
    assert_eq!(normalise("Half - Life"), "half life");
    assert_eq!(normalise("A:B"), "a b");
}

/// Normalisation must not make different titles equal.
#[test]
fn different_titles_never_normalise_together() {
    let pairs = [
        ("Red Dead Redemption 2", "Red Dead Redemption"),
        ("Portal", "Portal 2"),
        ("City of Gangsters", "City of Gangsters: Atlantic City"),
        ("Dying Light", "Dying Light 2"),
    ];
    for (a, b) in pairs {
        assert_ne!(normalise(a), normalise(b), "{a:?} and {b:?} are different games");
    }
}

#[test]
fn a_title_with_no_usable_characters_normalises_to_nothing() {
    assert_eq!(normalise("---"), "");
    assert_eq!(normalise(""), "");
}

// ── match selection ─────────────────────────────────────────────────────

#[test]
fn an_exact_match_is_chosen() {
    let candidates = [item(1174180, "Red Dead Redemption 2")];
    let chosen = choose_match("Red Dead Redemption 2", &candidates).expect("should match");
    assert_eq!(chosen.id, 1174180);
}

#[test]
fn a_match_survives_punctuation_differences() {
    let candidates = [item(325724, "Dying Light: The Following")];
    let chosen = choose_match("Dying Light The Following", &candidates).expect("should match");
    assert_eq!(chosen.id, 325724);
}

/// Real decoys, returned by real searches. Each would be selected by a substring
/// or nearest-neighbour rule.
#[test]
fn near_miss_candidates_are_rejected() {
    let candidates = [
        item(4147110, "Deadrock Redemption 2"),
        item(208520, "Omerta - City of Gangsters"),
        item(1811230, "City of Gangsters: Atlantic City"),
        item(1, "Red Dead Redemption"),
        item(2, "Red Dead Redemption 2: Special Edition"),
    ];
    assert!(
        choose_match("Red Dead Redemption 2", &candidates).is_none(),
        "no candidate is this game, so the answer is no match rather than the closest one"
    );
    assert!(choose_match("City of Gangsters", &candidates).is_none());
}

/// The decoy ordering matters: the wrong candidate comes first, so a rule that
/// took the top result would be wrong here.
#[test]
fn the_exact_match_wins_even_when_it_is_not_ranked_first() {
    let candidates = [
        item(4147110, "Deadrock Redemption 2"),
        item(1174180, "Red Dead Redemption 2"),
    ];
    let chosen = choose_match("Red Dead Redemption 2", &candidates).expect("should match");
    assert_eq!(chosen.id, 1174180);
}

#[test]
fn an_empty_result_set_is_no_match() {
    assert!(choose_match("Fortnite", &[]).is_none());
}

/// Search returns bundles, DLC, soundtracks and hardware alongside games.
#[test]
fn only_apps_are_considered() {
    let candidates = [
        SearchItem { id: 99, name: "Portal 2".into(), r#type: Some("bundle".into()) },
        SearchItem { id: 98, name: "Portal 2".into(), r#type: Some("dlc".into()) },
        SearchItem { id: 97, name: "Portal 2".into(), r#type: None },
    ];
    assert!(
        choose_match("Portal 2", &candidates).is_none(),
        "a bundle or DLC with the game's name is not the game"
    );

    let with_app = [
        SearchItem { id: 99, name: "Portal 2".into(), r#type: Some("bundle".into()) },
        item(620, "Portal 2"),
    ];
    assert_eq!(choose_match("Portal 2", &with_app).unwrap().id, 620);
}

/// A title that normalises to nothing cannot be matched against anything, or it
/// would match the first candidate that also normalises to nothing.
#[test]
fn an_unusable_title_matches_nothing() {
    let candidates = [item(1, "---"), item(2, "Real Game")];
    assert!(choose_match("---", &candidates).is_none());
}

#[test]
fn ties_are_resolved_deterministically_by_search_order() {
    let candidates = [item(10, "Remaster"), item(20, "Remaster")];
    for _ in 0..5 {
        assert_eq!(
            choose_match("Remaster", &candidates).unwrap().id,
            10,
            "the same response must always produce the same choice"
        );
    }
}

// ── the resolver's own gate ─────────────────────────────────────────────

#[tokio::test]
async fn no_search_is_attempted_without_network_permission() {
    let search = SteamTitleSearch::new(
        reqwest::Client::new(),
        std::sync::Arc::new(Throttle::new(1, Duration::ZERO)),
    );
    // A client with no proxy and a real URL would reach Steam if the gate failed;
    // the socket-level privacy tests prove no connection is opened.
    assert_eq!(
        search.resolve("Half-Life 2", false).await,
        TitleSearchOutcome::Unavailable
    );
}

#[tokio::test]
async fn a_blank_title_is_never_searched() {
    let search = SteamTitleSearch::new(
        reqwest::Client::new(),
        std::sync::Arc::new(Throttle::new(1, Duration::ZERO)),
    );
    assert_eq!(
        search.resolve("   ", true).await,
        TitleSearchOutcome::Unavailable
    );
}

// ── fingerprint ─────────────────────────────────────────────────────────

/// The fingerprint is what re-opens past conclusions, so it has to change with
/// the epoch and be stable otherwise.
#[test]
fn the_fingerprint_identifies_the_resolver_and_its_epoch() {
    assert_eq!(fingerprint(), format!("{RESOLVER_CODE}/{RESOLVER_EPOCH}"));
    assert_eq!(fingerprint(), fingerprint(), "stable across calls");
}
