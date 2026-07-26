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
pub mod throttle;

#[cfg(test)]
mod tests;

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

/// Why a provider reported `Lookup::Temporary` — see `Lookup` docs. Typed
/// rather than a bare `String` so a future circuit breaker or backoff
/// policy can match on `RateLimited` specifically (or `Timeout` vs.
/// `NetworkError`) without every provider needing to agree on a string
/// convention for the same underlying condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporaryReason {
    /// The request timed out.
    Timeout,
    /// HTTP 429, or a provider-specific rate-limit signal.
    RateLimited,
    /// A transport-level failure — DNS, TLS, connection refused/reset.
    NetworkError(String),
    /// HTTP 5xx or an equivalent "the provider is having trouble" signal.
    ServerError(String),
    /// Any other condition a provider expects to potentially succeed on
    /// retry but that doesn't fit a variant above.
    Other(String),
}

/// Why a provider reported `Lookup::Permanent` — see `Lookup` docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermanentReason {
    /// HTTP 404, or an equivalent "this id doesn't exist" signal.
    NotFound,
    /// The identifier this provider needs was present but malformed —
    /// absent is `Lookup::Unsupported`, not this.
    InvalidIdentifier,
    /// The response was received but didn't parse, or parsed into a shape
    /// this provider can't use.
    MalformedResponse(String),
    /// Any other condition a provider is confident won't change on retry.
    Other(String),
}

/// The outcome of one provider lookup — `Found` plus three ways a provider
/// can come back empty, so `MetadataService`/`ArtworkService` can react
/// differently to each instead of treating every miss the same:
///
///   - `Unsupported`: this provider fundamentally cannot resolve this
///     identity — no matching `GameIdentifier`, or (like `OfflineProvider`)
///     it never resolves anything, or (like `SteamLocalProvider` on a game
///     Steam hasn't cached art for) it looked and there's nothing there to
///     find. Not a failure: never logged as one, never recorded as a failed
///     attempt, and services always fall through to the next provider
///     immediately.
///   - `Temporary(reason)`: a transient condition. Services still fall
///     through to the next provider for *this* game, but repeated
///     `Temporary` misses from the same provider across a batch — and
///     `RateLimited` in particular — are the signal to circuit-break that
///     provider for the rest of the run rather than hammering it further.
///     The game stays eligible for a full retry next sweep — services must
///     not persist this as a hard failure.
///   - `Permanent(reason)`: a definitive negative for this specific
///     (provider, game) pair. Services fall through to the next provider (a
///     different provider or identifier scheme may still work) but stop
///     asking *this* provider about *this* game — `ArtworkService`
///     persists this via `Db::mark_artwork_failed` so a casual re-sweep
///     doesn't repeat a call expected to fail the same way again;
///     `MetadataService` should do the analogous thing once it has its own
///     persisted state.
///
/// A provider that hits an error it didn't specifically anticipate (an I/O
/// bug, an unexpected shape in an otherwise-successful response) should
/// still classify it — `Permanent(Other(..))` if there's no reason to
/// expect a retry would behave differently, `Temporary(Other(..))` if it
/// plausibly would — rather than propagating a raw, unclassified error.
/// There is deliberately no fourth, unclassified channel: forcing every
/// miss through one of these three variants is what lets the
/// classification actually be trusted.
#[derive(Debug, Clone)]
pub enum Lookup<T> {
    Found(T),
    Unsupported,
    Temporary(TemporaryReason),
    Permanent(PermanentReason),
}

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
