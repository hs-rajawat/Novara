//! Local-only Steam artwork provider — reads assets Steam has already
//! cached on disk at `<steam_root>/appcache/librarycache/<appid>/`, with
//! zero network access. `requires_network()` is false, so this provider
//! runs regardless of the `metadata_enabled` setting and `offline_mode`
//! (see `LookupContext::allow_network` — a `requires_network() == false`
//! provider is always eligible).
//!
//! Steam's cache also contains per-asset files named only by an opaque
//! content hash, with no `.jpg`/`.png` name tying them to a kind (observed
//! for the small in-library icon). This provider deliberately does not
//! guess at those — the well-known, stable filenames below are the only
//! ones it trusts. A kind it can't confidently resolve here is simply left
//! for the network-based CDN provider (when enabled) or a user-provided
//! image; see `ArtworkService`'s per-kind, priority-ordered fill.

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::scanner::steam::SteamContext;

use crate::metadata::{
    ArtworkKind, ArtworkProvider, AssetDescriptor, AssetSource, Lookup, LookupContext,
    ProviderCapabilities, ProviderIdentity,
};

pub struct SteamLocalProvider {
    ctx: SteamContext,
}

impl SteamLocalProvider {
    pub fn new(ctx: SteamContext) -> Self {
        Self { ctx }
    }

    fn cache_dir(&self, app_id: &str) -> Option<PathBuf> {
        let root = self.ctx.steam_root()?;
        let dir = root.join("appcache").join("librarycache").join(app_id);
        dir.is_dir().then_some(dir)
    }

    fn find(dir: &Path, names: &[&str]) -> Option<PathBuf> {
        names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
    }
}

impl ProviderIdentity for SteamLocalProvider {
    fn code(&self) -> &'static str {
        "steam_local"
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::ARTWORK
    }
}

#[async_trait]
impl ArtworkProvider for SteamLocalProvider {
    fn priority(&self) -> u8 {
        0
    }
    fn requires_network(&self) -> bool {
        false
    }
    async fn resolve_artwork(&self, ctx: &LookupContext<'_>) -> Lookup<Vec<AssetDescriptor>> {
        // See `steam_cdn`: a title that resolved to a DLC borrows its base game's
        // artwork, and Steam's local cache is keyed the same way.
        let Some(app_id) = ctx.identity.artwork_app_id("steam") else {
            return Lookup::Unsupported;
        };
        let Some(dir) = self.cache_dir(app_id) else {
            return Lookup::Unsupported;
        };

        let mut out = Vec::new();
        let mut push = |kind: ArtworkKind, names: &[&str]| {
            if let Some(path) = Self::find(&dir, names) {
                out.push(AssetDescriptor {
                    kind,
                    source: AssetSource::LocalFile(path),
                    provider: self.code(),
                });
            }
        };
        push(
            ArtworkKind::Cover,
            &["library_600x900.jpg", "library_600x900_2x.jpg"],
        );
        push(
            ArtworkKind::Hero,
            &["library_hero.jpg", "library_hero_2x.jpg"],
        );
        push(ArtworkKind::Logo, &["logo.png", "logo_2x.png"]);

        if out.is_empty() {
            Lookup::Unsupported
        } else {
            Lookup::Found(out)
        }
    }
}
