//! Metadata & artwork providers.
//!
//! Two separate trait families — resolving descriptive text (description,
//! genres, release year, developer, publisher) and resolving artwork
//! (cover/hero/logo/icon) are independent concerns with different cost
//! profiles (cheap/structured vs. I/O-heavy/download). A provider opts into
//! either or both by implementing the relevant trait(s) on its struct; nothing
//! forces a provider to supply both. `crate::metadata::text_service` and
//! `crate::metadata::artwork_service` each hold their own registry and are
//! independently gated, spawned, and rate-limited.
//!
//! Lookup is identifier-agnostic: `GameIdentity` carries an open-ended list
//! of `GameIdentifier`s (source app-id today; executable hash or another
//! provider's own id scheme tomorrow) so a future provider is a new struct
//! plus, if needed, a new `GameIdentifier` variant — never a trait redesign.
//! Providers that need launcher-specific discovery (Steam's library
//! locations, Epic's manifest directory) own that themselves, constructed
//! fresh per batch run by the composition root in each service — the same
//! "discover once per sweep" pattern `crate::integrity::service` already
//! uses for `SteamContext`/`EpicContext`. The generic types below never
//! reference a concrete source.
//!
//! No network is reached unless the caller sets `LookupContext::allow_network`
//! — see `text_service`/`artwork_service` for where that's gated behind the
//! `metadata_enabled` setting and the `offline_mode` kill-switch.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod artwork_service;
pub mod capability;
pub mod identity;
pub mod offline;
pub mod providers;
pub mod store;
pub mod text_service;
pub mod title_resolver;

/// The shared outbound rate limiter, re-exported from [`crate::resolve`].
///
/// One instance is constructed at the composition root and shared by every
/// network-touching provider, so the concurrency cap and minimum spacing bound
/// NOVARA's *total* outbound rate rather than each call site's. Re-exported as a
/// module so `metadata::throttle::Throttle` keeps resolving for providers.
pub use crate::resolve::throttle;

/// The per-provider circuit breaker, re-exported from [`crate::resolve`].
///
/// Trips a provider for the rest of a batch once it has produced enough
/// `Temporary` misses, so a struggling endpoint is not hammered for the whole
/// sweep. Generic over what is being resolved, so the save system's KB fetch will
/// use the same one.
pub use crate::resolve::breaker;

#[cfg(test)]
mod privacy_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod title_resolver_tests;

/// Descriptive (non-visual) metadata a `MetadataTextProvider` can supply.
/// Anything left `None`/empty is simply not written over the existing value.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameMetadata {
    pub description: Option<String>,
    pub release_year: Option<i64>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genres: Vec<String>,
    /// Provider's raw payload, stored verbatim in `games.metadata_json` for
    /// forward-compat (frontend or a future provider can read fields this
    /// struct doesn't model yet without another migration).
    pub raw_json: Option<String>,
}

/// One way to identify a game to a provider. Deliberately open-ended:
/// providers look for the variant they understand and ignore the rest, so
/// adding a new identification scheme (executable hash, a provider's own
/// catalog slug, ...) never requires changing `MetadataTextProvider` or
/// `ArtworkProvider`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameIdentifier {
    /// A launcher-assigned id — Steam's numeric appid, Epic's `AppName`
    /// catalog id, etc. `source` matches `sources.code`.
    SourceAppId { source: String, id: String },
    /// An id to use *only* when looking for artwork, overriding
    /// [`Self::SourceAppId`] for that purpose alone.
    ///
    /// Exists because a correct match is not always a good artwork source: a
    /// title that matches a DLC entry matches it rightly, but a DLC has no
    /// library artwork of its own, so the artwork lookup borrows its base game's
    /// while the identity — and therefore the description — stays the DLC.
    ///
    /// Deliberately a separate variant rather than a flag on `SourceAppId`: text
    /// and artwork providers read different things here, and a single field they
    /// both consumed could not express that.
    SourceArtworkAppId { source: String, id: String },
    /// Reserved for a future move-detection-style provider; unused by any
    /// v1 provider but requires no interface change to adopt.
    #[allow(dead_code)]
    ExecutableHash(String),
    /// Escape hatch for a provider-specific id scheme that doesn't warrant
    /// its own variant yet (e.g. a SteamGridDB or IGDB slug cached from a
    /// prior lookup).
    #[allow(dead_code)]
    Custom { key: String, value: String },
}

/// Everything known about a game that a provider might key a lookup on.
/// Built once per game by the calling service, not by providers themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameIdentity {
    pub title: String,
    pub identifiers: Vec<GameIdentifier>,
}

impl GameIdentity {
    /// Convenience accessor for the common case: does this identity carry a
    /// `SourceAppId` for the given source code?
    pub fn source_app_id(&self, source: &str) -> Option<&str> {
        self.identifiers.iter().find_map(|id| match id {
            GameIdentifier::SourceAppId { source: s, id } if s == source => Some(id.as_str()),
            _ => None,
        })
    }

    /// The id an **artwork** provider should use for this source.
    ///
    /// Prefers a [`GameIdentifier::SourceArtworkAppId`] override and falls back to
    /// the ordinary app-id, so a provider needs no knowledge of why an override
    /// exists. Text providers deliberately do not call this: a DLC's description
    /// should be the DLC's own.
    pub fn artwork_app_id(&self, source: &str) -> Option<&str> {
        self.identifiers
            .iter()
            .find_map(|id| match id {
                GameIdentifier::SourceArtworkAppId { source: s, id } if s == source => {
                    Some(id.as_str())
                }
                _ => None,
            })
            .or_else(|| self.source_app_id(source))
    }
}

/// Context passed to every provider call. Intentionally has no
/// Steam/Epic-specific fields — see module docs.
pub struct LookupContext<'a> {
    pub identity: &'a GameIdentity,
    /// Whether the caller currently permits network access at all
    /// (`metadata_enabled && !offline_mode`). A provider whose
    /// `requires_network()` is true is never invoked when this is false —
    /// services filter their registry up front — but providers may still
    /// check it defensively.
    pub allow_network: bool,
}

/// The four artwork slots NOVARA renders. Mirrors the existing
/// `games.cover_path`/`hero_path`/`icon_path` plus the new `logo_path`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtworkKind {
    Cover,
    Hero,
    Logo,
    Icon,
}

impl ArtworkKind {
    pub const ALL: [ArtworkKind; 4] = [
        ArtworkKind::Cover,
        ArtworkKind::Hero,
        ArtworkKind::Logo,
        ArtworkKind::Icon,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ArtworkKind::Cover => "cover",
            ArtworkKind::Hero => "hero",
            ArtworkKind::Logo => "logo",
            ArtworkKind::Icon => "icon",
        }
    }
}

/// Where an asset's bytes come from — the privacy-relevant distinction.
/// `LocalFile` is copied with zero network access (e.g. Steam's own
/// artwork cache on disk); `RemoteUrl` is only ever downloaded when the
/// caller resolved this descriptor with `allow_network = true`.
#[derive(Debug, Clone)]
pub enum AssetSource {
    LocalFile(std::path::PathBuf),
    RemoteUrl(String),
}

#[derive(Debug, Clone)]
pub struct AssetDescriptor {
    pub kind: ArtworkKind,
    pub source: AssetSource,
    pub provider: &'static str,
}

/// Result classification for a provider lookup.
///
/// Defined in [`crate::resolve`] and re-exported here: the four-way distinction
/// between "cannot", "try later" and "never again" is not metadata-specific, and
/// the save system's knowledge-base fetch and content extractors use the same
/// types. Providers may keep importing these from `crate::metadata`.
pub use crate::resolve::{Lookup, PermanentReason, TemporaryReason};

/// What a provider supplies — declared explicitly via
/// `ProviderIdentity::capabilities()` rather than left implicit in "which
/// of `MetadataTextProvider`/`ArtworkProvider` this struct happens to
/// implement". Nothing reads this today (one composition root, built by
/// hand, in each service), but it's what lets a diagnostics log, a test,
/// or a future settings UI ask a provider "what do you do" as one
/// synchronous call instead of reflecting on trait impls or maintaining a
/// separate capability table that can drift from the code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub text: bool,
    pub artwork: bool,
}

impl ProviderCapabilities {
    pub const TEXT: Self = Self {
        text: true,
        artwork: false,
    };
    pub const ARTWORK: Self = Self {
        text: false,
        artwork: true,
    };
    pub const BOTH: Self = Self {
        text: true,
        artwork: true,
    };
}

/// Identity shared by every provider regardless of which capability
/// trait(s) it implements. Both `MetadataTextProvider` and `ArtworkProvider`
/// require this as a supertrait, so a provider implementing both (e.g.
/// `SteamCdnProvider`) declares `code()` exactly once rather than once per
/// trait, and a caller holding either trait object can still call
/// `.code()`/`.capabilities()` on it directly.
pub trait ProviderIdentity: Send + Sync {
    fn code(&self) -> &'static str;
    /// What this provider supports. There is no compiler-enforced link
    /// between this and which of `MetadataTextProvider`/`ArtworkProvider`
    /// are actually implemented — a capability declaration and a trait
    /// impl are two different mechanisms — so get it right at this one
    /// `impl ProviderIdentity` site (`ProviderCapabilities::TEXT`/
    /// `::ARTWORK`/`::BOTH` cover every provider that exists today) and
    /// it's correct everywhere this provider is referenced.
    fn capabilities(&self) -> ProviderCapabilities;
}

/// Resolves descriptive text metadata for a game. See module docs for why
/// this is separate from `ArtworkProvider`.
#[async_trait]
pub trait MetadataTextProvider: ProviderIdentity {
    /// Lower runs first; the first provider to return `Some` wins (services
    /// do not merge partial text results field-by-field across providers).
    /// Deterministic across runs: `MetadataService` sorts its registry with
    /// a *stable* sort (`slice::sort_by_key`, never `sort_unstable_by_key`),
    /// and the composition root always registers providers in the same
    /// fixed literal order — so two providers sharing a priority value
    /// resolve ties by registration order, not by `HashMap`/filesystem/
    /// network-response ordering that could vary run to run.
    fn priority(&self) -> u8;
    fn requires_network(&self) -> bool;
    async fn resolve_text(&self, ctx: &LookupContext<'_>) -> Lookup<GameMetadata>;
}

/// Resolves artwork descriptors for a game. See module docs for why this is
/// separate from `MetadataTextProvider`.
#[async_trait]
pub trait ArtworkProvider: ProviderIdentity {
    /// Lower runs first; `ArtworkService` fills each `ArtworkKind`
    /// independently from the first provider (in priority order) that
    /// returns a descriptor for it — one provider can win `cover` while
    /// another wins `hero`. Same determinism/tie-break contract as
    /// `MetadataTextProvider::priority` — stable sort, fixed registration
    /// order.
    ///
    /// Priority only decides who *fills a missing kind first*. It does not
    /// grant permission to overwrite a kind another provider already
    /// filled: `Db::upsert_artwork_ready`'s `WHERE user_locked = 0 AND
    /// (state != 'ready' OR source = <this provider>)` is the actual
    /// enforcement, at the write itself rather than trusted to service
    /// logic — a `ready` asset is owned by whichever `source` produced it
    /// (or `'manual'`, permanently, once `user_locked`), and only that same
    /// provider's later run (a refresh) can update it. A higher-priority
    /// provider added later, or run in a different order, can still win an
    /// still-`pending`/`failed` kind, but never displaces an existing
    /// `ready` one from a different source.
    fn priority(&self) -> u8;
    fn requires_network(&self) -> bool;
    async fn resolve_artwork(&self, ctx: &LookupContext<'_>) -> Lookup<Vec<AssetDescriptor>>;
}
