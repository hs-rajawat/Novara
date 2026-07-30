//! Shared resolution machinery.
//!
//! Types and policies that any "ask a provider, it might not know" subsystem
//! needs, independent of what is being resolved. Extracted from `metadata/`
//! so the save system's knowledge-base fetch and its content extractors reuse
//! the same result classification, rate limiter and circuit breaker rather
//! than growing their own — three circuit breakers that behave differently
//! under load is the failure this module exists to prevent.
//!
//! Deliberately *not* moved here from `metadata/`:
//!
//!   - `LookupContext` — carries `&GameIdentity`, so it is metadata-specific.
//!     A save-system context is a different type with different fields.
//!   - `offline.rs` — a null `MetadataTextProvider`, not a network gate.
//!   - The network gate itself (`Db::allow_metadata_network`) — currently
//!     specific to the metadata settings pair (`metadata_enabled` &&
//!     `!offline_mode`).
//!
//! See `docs/architecture/SAVE_SYSTEM_ARCHITECTURE.md` §5.

pub mod breaker;
pub mod throttle;

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
/// can come back empty, so a calling service can react differently to each
/// instead of treating every miss the same:
///
///   - `Unsupported`: this provider fundamentally cannot resolve this
///     request — no matching identifier, or (like `OfflineProvider`)
///     it never resolves anything, or (like `SteamLocalProvider` on a game
///     Steam hasn't cached art for) it looked and there's nothing there to
///     find. Not a failure: never logged as one, never recorded as a failed
///     attempt, and services always fall through to the next provider
///     immediately.
///   - `Temporary(reason)`: a transient condition. Services still fall
///     through to the next provider for *this* subject, but repeated
///     `Temporary` misses from the same provider across a batch — and
///     `RateLimited` in particular — are the signal to circuit-break that
///     provider for the rest of the run rather than hammering it further.
///     The subject stays eligible for a full retry next sweep — services must
///     not persist this as a hard failure.
///   - `Permanent(reason)`: a definitive negative for this specific
///     (provider, subject) pair. Services fall through to the next provider (a
///     different provider or identifier scheme may still work) but stop
///     asking *this* provider about *this* subject, and should persist that so
///     a casual re-sweep doesn't repeat a call expected to fail the same way
///     again.
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
