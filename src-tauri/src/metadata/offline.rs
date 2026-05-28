//! Offline metadata provider — a placeholder that returns nothing.
//! Keeps the rest of the app working with zero network access until a
//! real provider is configured. Replace or extend with a bundled DB
//! (e.g. a shipped SQLite of public-domain titles) if desired.

use async_trait::async_trait;

use crate::error::AppResult;

use super::{GameMetadata, MetadataProvider};

#[derive(Default)]
pub struct OfflineProvider;

#[async_trait]
impl MetadataProvider for OfflineProvider {
    fn code(&self) -> &'static str {
        "offline"
    }
    async fn lookup(&self, _title: &str) -> AppResult<Option<GameMetadata>> {
        Ok(None)
    }
}
