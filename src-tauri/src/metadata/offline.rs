//! Offline text provider — a placeholder that returns nothing. Keeps
//! `MetadataService` working with zero network access until a real
//! provider is configured/enabled. Replace or extend with a bundled DB
//! (e.g. a shipped SQLite of public-domain titles) if desired.

use async_trait::async_trait;

use super::{
    GameMetadata, Lookup, LookupContext, MetadataTextProvider, ProviderCapabilities,
    ProviderIdentity,
};

#[derive(Default)]
pub struct OfflineProvider;

impl ProviderIdentity for OfflineProvider {
    fn code(&self) -> &'static str {
        "offline"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::TEXT
    }
}

#[async_trait]
impl MetadataTextProvider for OfflineProvider {
    fn priority(&self) -> u8 {
        255
    }
    fn requires_network(&self) -> bool {
        false
    }
    async fn resolve_text(&self, _ctx: &LookupContext<'_>) -> Lookup<GameMetadata> {
        Lookup::Unsupported
    }
}
