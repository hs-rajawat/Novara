//! Concrete provider implementations. Each submodule is a single provider
//! struct implementing `MetadataTextProvider` and/or `ArtworkProvider` —
//! see `crate::metadata` for the trait split and identifier-agnostic
//! lookup contract every provider here must honor.

pub mod epic_catalog;
pub mod steam_cdn;
pub mod steam_local;
