//! Metadata providers (cover art, description, release year, genres).
//!
//! The trait is async and provider-agnostic so we can plug IGDB, RAWG,
//! ScreenScraper (for ROMs), or a fully-offline cache. Adding a key is
//! a user-opt-in step — by default no network is reached, preserving
//! "privacy first".

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

pub mod offline;
// pub mod igdb;   // TODO: implement when user supplies a client id/secret
// pub mod rawg;   // TODO

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameMetadata {
    pub description: Option<String>,
    pub release_year: Option<i64>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub genres: Vec<String>,
    pub cover_url: Option<String>,
    pub hero_url: Option<String>,
    pub source: String, // 'igdb' | 'rawg' | 'offline' | 'manual'
}

#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn code(&self) -> &'static str;
    async fn lookup(&self, title: &str) -> AppResult<Option<GameMetadata>>;
}
