//! Epic Games Store catalog provider — intentionally a stub.
//!
//! Epic does not publish a stable, documented catalog API comparable to
//! Steam's public `appdetails`/CDN. What's reachable is Epic's internal
//! GraphQL catalog (the one the Epic Games Store website itself calls),
//! which is unversioned, undocumented, and known to change shape without
//! notice. Per NOVARA's scope for this milestone, Epic support goes only as
//! far as reliable and maintainable — reverse-engineering an unstable
//! GraphQL endpoint just to reach feature parity with Steam is explicitly
//! out of scope, not an oversight. This provider therefore always reports
//! `Lookup::Unsupported` on both traits, so `MetadataService`/
//! `ArtworkService` fall through to whatever else can resolve an
//! Epic-sourced game (another provider, or nothing) without ever recording
//! a false failure against it.
//!
//! Structurally this is still a complete, `ProviderIdentity`-compliant
//! provider — a real implementation later (if Epic ever publishes something
//! stable) is a matter of filling in `resolve_text`/`resolve_artwork`, not
//! redesigning anything here or in `metadata::mod`.

use async_trait::async_trait;

use crate::metadata::{
    ArtworkProvider, AssetDescriptor, GameMetadata, Lookup, LookupContext, MetadataTextProvider,
    ProviderCapabilities, ProviderIdentity,
};

#[derive(Default)]
pub struct EpicCatalogProvider;

impl EpicCatalogProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderIdentity for EpicCatalogProvider {
    fn code(&self) -> &'static str {
        "epic_catalog"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::BOTH
    }
}

#[async_trait]
impl MetadataTextProvider for EpicCatalogProvider {
    fn priority(&self) -> u8 {
        20
    }
    fn requires_network(&self) -> bool {
        true
    }
    async fn resolve_text(&self, _ctx: &LookupContext<'_>) -> Lookup<GameMetadata> {
        Lookup::Unsupported
    }
}

#[async_trait]
impl ArtworkProvider for EpicCatalogProvider {
    fn priority(&self) -> u8 {
        20
    }
    fn requires_network(&self) -> bool {
        true
    }
    async fn resolve_artwork(&self, _ctx: &LookupContext<'_>) -> Lookup<Vec<AssetDescriptor>> {
        Lookup::Unsupported
    }
}
