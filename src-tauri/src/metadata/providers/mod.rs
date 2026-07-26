//! Concrete provider implementations. Each submodule is a single provider
//! struct implementing `MetadataTextProvider` and/or `ArtworkProvider` —
//! see `crate::metadata` for the trait split and identifier-agnostic
//! lookup contract every provider here must honor.
//!
//! `steam_title_search` is the exception: it resolves a title to a Steam app-id
//! rather than returning metadata, so it implements neither trait. It belongs
//! here because it is an integration with the same external source, and because
//! it is what makes the Steam providers usable for games that have no app-id.

pub mod epic_catalog;
pub mod steam_cdn;
pub mod steam_local;
pub mod steam_title_search;
