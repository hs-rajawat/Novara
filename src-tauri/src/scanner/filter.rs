//! Deciding what belongs in the library.
//!
//! Steam installs its own system components as ordinary apps, in the same
//! `steamapps/common` directory, with the same `appmanifest_*.acf` shape as a game.
//! Redistributables, runtimes, Proton builds and SDKs all arrive looking exactly like
//! library entries. They are not games and must never reach the library UI.
//!
//! ## Why this lives in the scanner
//!
//! Because it is an *import* question, not a detection question. A component that never
//! enters the library costs nothing downstream: no save scan, no artwork lookup, no
//! playtime row, no UI clutter. Filtering later would mean every subsystem carrying its
//! own idea of what is a game.
//!
//! ## Rule order, and why
//!
//! Rules are evaluated in order and the first match wins, strongest signal first:
//!
//! 1. **Known system app id** — a stable machine identifier assigned by the storefront.
//! 2. **Structural** — the install contains nothing launchable.
//! 3. **Anchored name pattern** — last resort, deliberately narrow.
//!
//! ## An honest correction about "prefer Steam metadata"
//!
//! Steam's *app type* — the field that would answer this directly — is not available
//! locally. `appmanifest_*.acf` carries only `appid`, `name`, `installdir`, `SizeOnDisk`
//! and `StateFlags`. Type lives in `appcache/appinfo.vdf`, a binary undocumented format
//! that changes across client releases, or in the Web API, which needs network access
//! NOVARA treats as optional.
//!
//! So the metadata-first signal that *is* available is the **app id**, and rule 1 uses it.
//! That is categorically different from matching display names: app id 228980 is
//! Steamworks Common Redistributables permanently, whereas its name is localised and can
//! be renamed. Reading `appinfo.vdf` remains an option if rule 1 ever proves insufficient;
//! the cost is a reverse-engineered binary parser on the scan path, which is not worth
//! paying for a list that changes a few times a year.
//!
//! ## Nothing is silently dropped
//!
//! A skip is recorded in `skipped_library_items` with the rule and a sentence, and the
//! table carries `override_import` for a future "Import anyway" action. Same principle as
//! detection recording its rejections: an item that disappears without explanation is
//! indistinguishable from a bug, and "why is my game missing?" deserves an answer.

use std::path::Path;

/// Why an item was kept out of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    /// Machine-readable rule identity, stored so a skip can be explained later and so a
    /// rule can be retired without re-deriving its effects.
    pub rule: &'static str,
    /// The sentence a user would read.
    pub reason: String,
}

/// The filter's verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Import,
    Skipped(Skip),
}

impl Verdict {
    pub fn is_import(&self) -> bool {
        matches!(self, Verdict::Import)
    }
    pub fn skip(&self) -> Option<&Skip> {
        match self {
            Verdict::Skipped(s) => Some(s),
            Verdict::Import => None,
        }
    }
}

/// What the filter needs to judge one candidate.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub source_code: &'a str,
    pub source_app_id: Option<&'a str>,
    pub title: &'a str,
    pub install_dir: &'a Path,
    /// Whether the scanner found anything launchable. `None` means the scanner does not
    /// resolve executables for this source — Steam launches through `steam://`, so a
    /// missing executable there is not evidence of anything.
    pub has_executable: Option<bool>,
}

// ─────────────────────────────────────────────────────────────────────────
// Rule 1 — known system app ids
// ─────────────────────────────────────────────────────────────────────────

/// Steam app ids that are system components rather than games.
///
/// **Ids, not names.** An app id is assigned by Steam and never changes; a display name is
/// localised and can be edited. Matching ids is metadata matching. Matching names is
/// guessing.
///
/// Grouped by what each family is, so a future addition has somewhere obvious to go.
const STEAM_SYSTEM_APP_IDS: &[(&str, &str)] = &[
    // Redistributables and SDKs. Shipped as dependencies of other apps.
    ("228980", "Steamworks Common Redistributables"),
    ("1007", "Steamworks SDK Redist"),
    ("1070", "Steam SDK"),
    // Steam Linux Runtime family. Container runtimes used to launch other apps.
    ("1070560", "Steam Linux Runtime"),
    ("1391110", "Steam Linux Runtime 2.0 (soldier)"),
    ("1628350", "Steam Linux Runtime 3.0 (sniper)"),
    ("1493710", "Proton Experimental"),
    // Proton compatibility tools. Each release is a separate app id.
    ("858280", "Proton 3.7"),
    ("930400", "Proton 3.16"),
    ("961940", "Proton 4.2"),
    ("1054830", "Proton 4.11"),
    ("1113280", "Proton 5.0"),
    ("1245040", "Proton 5.13"),
    ("1420170", "Proton 6.3"),
    ("1580130", "Proton 7.0"),
    ("2180100", "Proton Hotfix"),
    ("2230260", "Proton 8.0"),
    ("2805730", "Proton 9.0"),
    ("3658110", "Proton 10.0"),
    // Steamworks tooling that installs into the library.
    ("1826330", "Steam Deck Tools"),
    ("1391460", "Steam Runtime"),
];

// ─────────────────────────────────────────────────────────────────────────
// Rule 3 — anchored name patterns
// ─────────────────────────────────────────────────────────────────────────

/// Structural phrases that identify a system component by name.
///
/// **The last resort, and deliberately narrow.** Each is anchored or specific enough that
/// a real game title cannot plausibly match it. Bare words that could appear in a game's
/// name — "Runtime", "Tools", "Benchmark" on their own — are excluded on purpose: the cost
/// of wrongly hiding somebody's game is far higher than the cost of one extra library row
/// they can hide themselves.
///
/// Compared against the folded title, so spacing and punctuation do not matter.
const SYSTEM_NAME_PATTERNS: &[(&str, &str)] = &[
    ("steamworkscommon", "a Steamworks redistributable bundle"),
    ("steamworksredist", "a Steamworks redistributable bundle"),
    ("steamlinuxruntime", "a Steam Linux container runtime"),
    ("steamruntime", "a Steam container runtime"),
    ("protonexperimental", "a Proton compatibility tool"),
    ("protonhotfix", "a Proton compatibility tool"),
    ("steamworkssdk", "a Steamworks SDK"),
    ("steamvrdriver", "a SteamVR driver"),
    ("directxredist", "a DirectX redistributable"),
    ("vcredist", "a Visual C++ redistributable"),
    ("dotnetredist", "a .NET redistributable"),
];

/// Titles that are a Proton release: `Proton`, `Proton 8.0`, `Proton - Experimental`.
///
/// Separate from the pattern table because it needs a prefix test rather than a substring
/// one. `Proton` alone is a system component; a game whose title merely *contains* "proton"
/// — `Protonwar`, say — must not match, which a substring test would get wrong.
fn is_proton_release(folded: &str) -> bool {
    let Some(rest) = folded.strip_prefix("proton") else {
        return false;
    };
    // Nothing after it, or only a version/qualifier made of digits and known words.
    rest.is_empty()
        || rest.chars().all(|c| c.is_ascii_digit())
        || matches!(rest, "experimental" | "hotfix" | "next" | "ge" | "beta")
}

/// Judge one candidate.
pub fn classify(candidate: &Candidate<'_>) -> Verdict {
    let folded = crate::saves::kb::normalise_title(candidate.title);

    // ── Rule 1: a known system app id ────────────────────────────────
    if candidate.source_code == "steam" {
        if let Some(app_id) = candidate.source_app_id {
            if let Some((_, what)) = STEAM_SYSTEM_APP_IDS.iter().find(|(id, _)| *id == app_id) {
                return Verdict::Skipped(Skip {
                    rule: "steam_system_app_id",
                    reason: format!("{what} (Steam app {app_id}), not a game"),
                });
            }
        }
    }

    // ── Rule 2: nothing launchable ───────────────────────────────────
    //
    // Only consulted where the scanner actually resolves executables. Steam launches via
    // `steam://` and leaves `executable` unset for everything, so applying this there
    // would reject the entire library.
    if candidate.has_executable == Some(false) {
        return Verdict::Skipped(Skip {
            rule: "no_launchable_executable",
            reason: "no launchable program was found in the install folder".into(),
        });
    }

    // ── Rule 3: an anchored name pattern ─────────────────────────────
    if is_proton_release(&folded) {
        return Verdict::Skipped(Skip {
            rule: "system_name_pattern",
            reason: "a Proton compatibility tool, not a game".into(),
        });
    }
    for (pattern, what) in SYSTEM_NAME_PATTERNS {
        if folded.contains(pattern) {
            return Verdict::Skipped(Skip {
                rule: "system_name_pattern",
                reason: format!("{what}, not a game"),
            });
        }
    }

    Verdict::Import
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod filter_tests;
