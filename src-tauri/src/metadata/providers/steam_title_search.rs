//! Resolves a game title to a Steam app-id.
//!
//! This is what lets Epic and manually-imported games use the Steam-backed
//! providers: they have a title but no app-id, so every provider returns
//! `Unsupported` and they get nothing. Once a title is resolved, the existing
//! pipeline serves them with no provider changes at all.
//!
//! # Matching is exact, never fuzzy
//!
//! Every search returns plausible neighbours. Measured against a real library:
//! "Red Dead Redemption 2" also returns "Deadrock Redemption 2", and "City of
//! Gangsters" also returns "Omerta - City of Gangsters" and "City of Gangsters:
//! Atlantic City". Any scoring or substring rule eventually attaches one game's
//! artwork and description to another, which is worse than showing none — it is
//! wrong data the user has no reason to distrust.
//!
//! So the rule is normalise both sides and require equality. Normalisation exists
//! only to absorb differences in how the *same* title is punctuated by different
//! sources: Epic's "Dying Light The Following" against Steam's "Dying Light: The
//! Following" is a real case from that library, and raw equality fails it.
//!
//! A title that does not match exactly is reported as no match, which is an
//! answer. "Fortnite" returns nothing from Steam because it is not on Steam, and
//! that is the correct outcome rather than a gap to paper over.
//!
//! # Known limitation
//!
//! Diacritics are not folded, so "Pokémon" and "Pokemon" are different titles and
//! such a game resolves to no match. Folding correctly needs Unicode
//! normalisation, which is a dependency this does not currently justify — and the
//! resolver fingerprint means adding it later re-opens every past non-match
//! automatically.

use std::time::Duration;

use serde::Deserialize;
use tracing::warn;

use crate::metadata::throttle::Throttle;

/// Steam's store search. Undocumented and unsupported by Valve, but keyless —
/// the documented alternatives (IGDB, SteamGridDB) require an API key and an
/// account, which the project's "no account required" promise rules out.
/// `ISteamApps/GetAppList`, which would have allowed fully offline matching from a
/// single snapshot, returns 404 and is not available.
const STORE_SEARCH_URL: &str = "https://store.steampowered.com/api/storesearch/";

/// Country and language are pinned rather than taken from the user's locale, so
/// the same library resolves to the same app-ids on every machine. A locale-varying
/// query would make matching non-deterministic across users for no benefit.
const SEARCH_CC: &str = "us";
const SEARCH_LANG: &str = "en";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Identifies this resolver *and its matching behaviour* in the cache's
/// `settled_by`.
///
/// Bump [`RESOLVER_EPOCH`] whenever a change would make the resolver reach a
/// different conclusion — a smarter normaliser, a different tie-break, a new
/// source. Every previously recorded outcome, including non-matches, is then
/// re-opened on the next sweep with no manual repair. Leave it alone for changes
/// that cannot alter a result.
pub const RESOLVER_CODE: &str = "steam_title_search";
pub const RESOLVER_EPOCH: u32 = 1;

pub fn fingerprint() -> String {
    format!("{RESOLVER_CODE}/{RESOLVER_EPOCH}")
}

/// One candidate from a search response.
#[derive(Debug, Clone, Deserialize)]
pub struct SearchItem {
    pub id: u64,
    pub name: String,
    /// Steam returns bundles, DLC and hardware here too; only `app` is a game.
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

/// The outcome of a search.
///
/// `Unavailable` is deliberately distinct from `NoMatch`: a timeout says nothing
/// about whether the game is on Steam, so caching it would turn one bad request
/// into a permanent verdict. Only the two real answers are recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleSearchOutcome {
    Matched { app_id: String, matched_title: String },
    NoMatch,
    Unavailable,
}

/// Reduce a title to the form two sources should agree on.
///
/// Case is folded, runs of whitespace and punctuation collapse to a single space,
/// and apostrophes are removed outright. That absorbs colons, dashes, trademark
/// symbols, brackets and double spacing — the ways the same title is written
/// differently — without letting two *different* titles collide, because every
/// meaningful word survives in order.
///
/// Apostrophes and commas are the exception because they sit *inside* a token.
/// Treating an apostrophe as a separator turns "Sid Meier's Civilization VI" into
/// `sid meier s civilization vi`, which no longer matches a source that writes "Sid
/// Meiers Civilization VI"; treating a comma as one turns "Warhammer 40,000" into
/// `warhammer 40 000` rather than `warhammer 40000`. Dropping them cannot join two
/// separate words, because prose already puts a space after a comma. Hyphens and
/// colons are the opposite case — "Half-Life" must become `half life` so it still
/// matches a source that writes "Half - Life".
///
/// Periods separate rather than being dropped, which means "F.E.A.R." only matches
/// a source that also punctuates it. That direction is chosen deliberately:
/// dropping them would fold "S.T.A.L.K.E.R." into `stalker` and risk colliding
/// with a genuinely different game of that name, and a missed match is a far
/// cheaper mistake than a wrong one.
pub fn normalise(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(ch.to_lowercase());
        } else if !matches!(ch, '\'' | '\u{2019}' | '\u{02BC}' | ',') {
            pending_space = true;
        }
    }
    out
}

/// Pick the one candidate that is the same game as `title`, if any.
///
/// Ties are resolved by Steam's own ordering, which is relevance-ranked, making
/// the choice deterministic for a given response.
pub fn choose_match<'a>(title: &str, candidates: &'a [SearchItem]) -> Option<&'a SearchItem> {
    let wanted = normalise(title);
    if wanted.is_empty() {
        return None;
    }
    candidates.iter().find(|c| {
        c.r#type.as_deref() == Some("app") && normalise(&c.name) == wanted
    })
}

pub struct SteamTitleSearch {
    client: reqwest::Client,
    throttle: std::sync::Arc<Throttle>,
}

impl SteamTitleSearch {
    pub fn new(client: reqwest::Client, throttle: std::sync::Arc<Throttle>) -> Self {
        Self { client, throttle }
    }

    /// Ask Steam about one title.
    ///
    /// `allow_network` is checked here as well as by the caller: this is the only
    /// network caller in the resolution pass, and the privacy guarantee should not
    /// depend on a single call site remembering.
    pub async fn resolve(&self, title: &str, allow_network: bool) -> TitleSearchOutcome {
        if !allow_network || title.trim().is_empty() {
            return TitleSearchOutcome::Unavailable;
        }

        let response = {
            let _slot = self.throttle.acquire().await;
            self.client
                .get(STORE_SEARCH_URL)
                .query(&[("term", title), ("cc", SEARCH_CC), ("l", SEARCH_LANG)])
                .timeout(REQUEST_TIMEOUT)
                .send()
                .await
        };

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                warn!(title, error = %e, "steam title search failed");
                return TitleSearchOutcome::Unavailable;
            }
        };
        if !response.status().is_success() {
            warn!(title, status = %response.status(), "steam title search rejected");
            return TitleSearchOutcome::Unavailable;
        }

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                warn!(title, error = %e, "steam title search body unreadable");
                return TitleSearchOutcome::Unavailable;
            }
        };
        let parsed: SearchResponse = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                // A shape change in an undocumented endpoint is a provider
                // problem, not a statement about this game.
                warn!(title, error = %e, "steam title search response unparseable");
                return TitleSearchOutcome::Unavailable;
            }
        };

        match choose_match(title, &parsed.items) {
            Some(item) => TitleSearchOutcome::Matched {
                app_id: item.id.to_string(),
                matched_title: item.name.clone(),
            },
            None => TitleSearchOutcome::NoMatch,
        }
    }
}

#[cfg(test)]
#[path = "steam_title_search_tests.rs"]
mod steam_title_search_tests;
