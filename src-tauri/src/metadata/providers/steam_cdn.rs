//! Steam CDN provider — network text (via Steam's public, keyless
//! `appdetails` endpoint) and artwork (via Steam's public CDN, using the
//! same filename convention `providers::steam_local` reads locally so both
//! providers agree on what "cover"/"hero"/"logo" mean for a Steam app).
//! `requires_network()` is true for both traits, so `LookupContext`'s
//! `allow_network` (gated by `metadata_enabled && !offline_mode`) must be
//! true before either method runs — enforced by
//! `MetadataService`/`ArtworkService` filtering their registry up front,
//! not by this provider, though `resolve_text` checks it defensively too.
//!
//! Steam does not document a stable rate limit for `appdetails`, but it is
//! well known to return HTTP 429 under sustained load — every fallible
//! call here maps that (and timeouts, 5xx, and transport errors) to
//! `Lookup::Temporary` so a caller can circuit-break instead of hammering
//! it further across a large batch.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::metadata::throttle::Throttle;
use crate::metadata::{
    ArtworkKind, ArtworkProvider, AssetDescriptor, AssetSource, GameMetadata, Lookup,
    LookupContext, MetadataTextProvider, PermanentReason, ProviderCapabilities, ProviderIdentity,
    TemporaryReason,
};

const APPDETAILS_URL: &str = "https://store.steampowered.com/api/appdetails";
const CDN_BASE: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The artwork filenames this provider checks for, in the same order (and
/// with the same meaning) as `providers::steam_local`'s local-cache lookup.
/// Unlike the local provider, only the primary (non-`_2x`) filename is
/// tried — the CDN always serves the same bytes regardless of display
/// scale, so there's no "hi-DPI variant might exist instead" case to
/// account for.
const CDN_ASSETS: [(ArtworkKind, &str); 3] = [
    (ArtworkKind::Cover, "library_600x900.jpg"),
    (ArtworkKind::Hero, "library_hero.jpg"),
    (ArtworkKind::Logo, "logo.png"),
];

pub struct SteamCdnProvider {
    client: reqwest::Client,
    /// Shared with every other network provider and with asset downloads, so
    /// the cap applies to NOVARA's total outbound rate rather than per call
    /// site. Steam does not document a rate limit for these endpoints but
    /// answers 429 under sustained load, and a first scan of a large library
    /// otherwise issues one `appdetails` GET plus three CDN HEADs per game as
    /// fast as the fill loop runs.
    throttle: Arc<Throttle>,
}

impl SteamCdnProvider {
    pub fn new(client: reqwest::Client, throttle: Arc<Throttle>) -> Self {
        Self { client, throttle }
    }
}

#[derive(Debug, Deserialize)]
struct AppDetailsEntry {
    success: bool,
    #[serde(default)]
    data: Option<AppDetailsData>,
}

#[derive(Debug, Deserialize)]
struct AppDetailsData {
    #[serde(default)]
    short_description: Option<String>,
    #[serde(default)]
    developers: Option<Vec<String>>,
    #[serde(default)]
    publishers: Option<Vec<String>>,
    #[serde(default)]
    genres: Option<Vec<AppDetailsGenre>>,
    #[serde(default)]
    release_date: Option<AppDetailsReleaseDate>,
}

#[derive(Debug, Deserialize)]
struct AppDetailsGenre {
    description: String,
}

#[derive(Debug, Deserialize)]
struct AppDetailsReleaseDate {
    #[serde(default)]
    date: Option<String>,
}

/// Steam's `release_date.date` is a display string ("21 Oct, 2015", or
/// sometimes just "2015", or empty for unreleased titles) rather than a
/// structured date — pull the last 4-digit run out of it rather than
/// parsing a specific format, since Steam does not guarantee one.
fn extract_year(date_str: &str) -> Option<i64> {
    let mut run = String::new();
    let mut year = None;
    for ch in date_str.chars().chain(std::iter::once(',')) {
        if ch.is_ascii_digit() {
            run.push(ch);
            continue;
        }
        if run.len() == 4 {
            if let Ok(y) = run.parse() {
                year = Some(y);
            }
        }
        run.clear();
    }
    year
}

impl ProviderIdentity for SteamCdnProvider {
    fn code(&self) -> &'static str {
        "steam_cdn"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::BOTH
    }
}

#[async_trait]
impl MetadataTextProvider for SteamCdnProvider {
    fn priority(&self) -> u8 {
        10
    }
    fn requires_network(&self) -> bool {
        true
    }

    async fn resolve_text(&self, ctx: &LookupContext<'_>) -> Lookup<GameMetadata> {
        let Some(app_id) = ctx.identity.source_app_id("steam") else {
            return Lookup::Unsupported;
        };
        if !ctx.allow_network {
            return Lookup::Unsupported;
        }

        let resp = {
            let _slot = self.throttle.acquire().await;
            self.client
                .get(APPDETAILS_URL)
                .query(&[("appids", app_id), ("l", "english")])
                .timeout(REQUEST_TIMEOUT)
                .send()
                .await
        };
        let resp = match resp {
            Ok(r) => r,
            Err(e) if e.is_timeout() => return Lookup::Temporary(TemporaryReason::Timeout),
            Err(e) => return Lookup::Temporary(TemporaryReason::NetworkError(e.to_string())),
        };

        let status = resp.status();
        if status.as_u16() == 429 {
            return Lookup::Temporary(TemporaryReason::RateLimited);
        }
        if status.is_server_error() {
            return Lookup::Temporary(TemporaryReason::ServerError(status.to_string()));
        }
        if !status.is_success() {
            return Lookup::Permanent(PermanentReason::NotFound);
        }

        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => return Lookup::Temporary(TemporaryReason::NetworkError(e.to_string())),
        };

        let parsed: HashMap<String, AppDetailsEntry> = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return Lookup::Permanent(PermanentReason::MalformedResponse(e.to_string())),
        };

        let Some(entry) = parsed.get(app_id) else {
            return Lookup::Permanent(PermanentReason::MalformedResponse(
                "appdetails response did not include the requested appid".into(),
            ));
        };
        if !entry.success {
            return Lookup::Permanent(PermanentReason::NotFound);
        }
        let Some(data) = &entry.data else {
            return Lookup::Permanent(PermanentReason::NotFound);
        };

        Lookup::Found(GameMetadata {
            description: data.short_description.clone().filter(|s| !s.is_empty()),
            release_year: data
                .release_date
                .as_ref()
                .and_then(|r| r.date.as_deref())
                .and_then(extract_year),
            developer: data.developers.as_ref().and_then(|d| d.first().cloned()),
            publisher: data.publishers.as_ref().and_then(|p| p.first().cloned()),
            genres: data
                .genres
                .as_ref()
                .map(|gs| gs.iter().map(|g| g.description.clone()).collect())
                .unwrap_or_default(),
            raw_json: Some(body),
        })
    }
}

#[async_trait]
impl ArtworkProvider for SteamCdnProvider {
    fn priority(&self) -> u8 {
        10
    }
    fn requires_network(&self) -> bool {
        true
    }

    async fn resolve_artwork(&self, ctx: &LookupContext<'_>) -> Lookup<Vec<AssetDescriptor>> {
        let Some(app_id) = ctx.identity.source_app_id("steam") else {
            return Lookup::Unsupported;
        };
        if !ctx.allow_network {
            return Lookup::Unsupported;
        }

        let mut out = Vec::new();
        for (kind, filename) in CDN_ASSETS {
            let url = format!("{CDN_BASE}/{app_id}/{filename}");
            // HEAD-only existence check — the real bytes are downloaded
            // later by `metadata::store::store_remote_asset` once
            // `ArtworkService` decides this descriptor should actually be
            // fetched (e.g. it isn't shadowed by a `user_locked` asset).
            let resp = {
                let _slot = self.throttle.acquire().await;
                self.client.head(&url).timeout(REQUEST_TIMEOUT).send().await
            };
            let resp = match resp {
                Ok(r) => r,
                Err(e) if e.is_timeout() => return Lookup::Temporary(TemporaryReason::Timeout),
                Err(e) => return Lookup::Temporary(TemporaryReason::NetworkError(e.to_string())),
            };
            let status = resp.status();
            if status.as_u16() == 429 {
                return Lookup::Temporary(TemporaryReason::RateLimited);
            }
            if status.is_server_error() {
                return Lookup::Temporary(TemporaryReason::ServerError(status.to_string()));
            }
            if status.is_success() {
                out.push(AssetDescriptor {
                    kind,
                    source: AssetSource::RemoteUrl(url),
                    provider: self.code(),
                });
            }
            // Any other status (404 etc.) just means this one kind isn't
            // available for this app — keep checking the rest.
        }

        if out.is_empty() {
            Lookup::Permanent(PermanentReason::NotFound)
        } else {
            Lookup::Found(out)
        }
    }
}
