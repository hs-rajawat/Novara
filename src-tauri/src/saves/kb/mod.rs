//! The save-location knowledge base.
//!
//! A long-lived data asset, not a lookup table — see
//! `docs/architecture/KNOWLEDGE_BASE.md`. Three layers (`builtin`, `community`,
//! `user`) coexist; this module matches a game against them and turns the matched
//! entries' templates into concrete paths.
//!
//! Two boundaries hold here:
//!
//!   * The KB **produces candidates, never bindings**. It describes the typical
//!     installation; the machine in front of us is the authority, and the user is
//!     the authority above that.
//!   * [`template`] is a **security boundary**. Templates are data that steers
//!     filesystem access, so a closed variable set and traversal rejection are
//!     enforced at import *and* at expansion.
//!
//! Phase 1 ships the `builtin` and `user` layers. The `community` layer needs
//! network access and lands in Phase 8.

pub mod builtin;
pub mod import;
pub mod template;
pub mod validate;

#[cfg(test)]
mod corpus_tests;

use std::path::PathBuf;

use crate::db::save_kb::MatchKey;
use crate::models::SaveKbEntry;
use crate::saves::fs::FileSystem;
use crate::saves::pipeline::GameContext;

use template::TemplateVars;

/// A path a knowledge-base entry claims for this game, with the entry that claimed
/// it.
#[derive(Debug, Clone)]
pub struct KbCandidate {
    pub path: PathBuf,
    pub entry_id: String,
    pub layer: String,
    pub note: Option<String>,
    pub priority: i64,
    /// True when the entry matched an *identity* of this game, false for a convention
    /// rule (`match_kind = 'any'`) that applies to the whole library.
    ///
    /// The distinction is load-bearing for the decision table. `KNOWLEDGE_BASE.md` and
    /// `GAME_SAVE_DETECTION.md` §5.3 both rate built-in KB evidence as *strong*, and
    /// that is true of a curated entry naming this game. A convention rule says only
    /// "this path shape is conventional" and matches every game, so treating it as a
    /// curated claim would let the first conventional-looking folder bind — including a
    /// photo folder sitting under `{DOCUMENTS}/{TITLE}`.
    pub keyed: bool,
}

/// The identities this game can be matched by, most specific first.
///
/// `exe_name` deliberately outranks `title_norm`: a repack frequently renames the
/// game but ships the original executable, so the binary's name is the most stable
/// identity a manually installed game has. See `KNOWLEDGE_BASE.md` §4.
pub fn match_keys(ctx: &GameContext) -> Vec<MatchKey> {
    let mut keys = Vec::new();
    if let Some(id) = &ctx.steam_appid {
        keys.push(MatchKey::new("steam_appid", id));
    }
    if let Some(id) = &ctx.gog_id {
        keys.push(MatchKey::new("gog_id", id));
    }
    if let Some(id) = &ctx.epic_id {
        keys.push(MatchKey::new("epic_id", id));
    }
    if let Some(exe) = &ctx.exe_name {
        keys.push(MatchKey::new("exe_name", &normalise_exe(exe)));
    }
    keys.push(MatchKey::new("title_norm", &normalise_title(&ctx.title)));
    keys
}

/// Fold a title into a stable matching key.
///
/// **Every non-alphanumeric character is removed, including spaces.** Case,
/// punctuation and spacing all vary between a store listing and a folder name, and
/// none of that variation is meaningful:
///
/// ```text
/// Marvel's Spider-Man │ MARVELS SPIDER-MAN │ Spider Man  →  marvelsspiderman / spiderman
/// S.T.A.L.K.E.R.      │ STALKER                          →  stalker
/// ```
///
/// An earlier version collapsed punctuation to a *space* instead of removing it,
/// which split words at apostrophes — `Marvel's` became `marvel s` and could never
/// match `Marvels`. Removing separators entirely makes hyphenated, spaced and
/// compacted spellings all collide, which is the direction that helps: two genuinely
/// different games almost never differ only by punctuation.
///
/// The result is a key, not a display string. Readability is not a goal.
pub fn normalise_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Executable names are compared case-insensitively, without the extension.
pub fn normalise_exe(exe: &str) -> String {
    let name = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".EXE"))
        .unwrap_or(name)
        .to_lowercase()
}

/// Precedence of a layer, lowest first.
///
/// **Mirrors the `CASE layer` in [`Db::match_kb_entries`]'s `ORDER BY`.** The
/// duplication is deliberate and the cost is one function: `candidates` must not
/// depend on its caller having sorted correctly, because the symptom of a caller
/// that forgets is not an error but a *wrong path silently winning* — the kind of
/// bug that surfaces as a failed restore months later.
///
/// [`Db::match_kb_entries`]: crate::db::Db::match_kb_entries
pub fn layer_rank(layer: &str) -> u8 {
    match layer {
        "user" => 0,
        "community" => 1,
        _ => 2,
    }
}

/// Turn matched entries into concrete paths that exist on this filesystem.
///
/// An entry that does not apply here — an anchor this machine lacks, a variable the
/// game does not supply, a path that is not present — contributes nothing. That is
/// the normal case for most entries and is not a failure.
///
/// **A KB entry is a claim, not a fact.** A path is only returned if it actually
/// exists, because binding to a directory the KB merely predicted would produce a
/// binding that fails on first snapshot.
///
/// Where two entries name the same directory the higher-precedence one owns it, so
/// the reported provenance is the one a user would expect to see.
pub fn candidates(
    fs: &dyn FileSystem,
    entries: &[SaveKbEntry],
    ctx: &GameContext,
) -> Vec<KbCandidate> {
    let vars = TemplateVars {
        title: &ctx.title,
        publisher: ctx.publisher.as_deref(),
        developer: ctx.developer.as_deref(),
        steam_appid: ctx.steam_appid.as_deref(),
        steam_userid: None, // resolved from the local Steam install in a later task
        install_dir: ctx.install_dir.as_deref(),
    };

    // Sorted here rather than trusted from the caller — see `layer_rank`. Stable so
    // that entries tying on layer and priority keep the repository's `id` order.
    let mut ordered: Vec<&SaveKbEntry> = entries.iter().collect();
    ordered.sort_by(|a, b| {
        layer_rank(&a.layer)
            .cmp(&layer_rank(&b.layer))
            .then(a.priority.cmp(&b.priority))
            .then(a.id.cmp(&b.id))
    });

    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for entry in ordered {
        for path in template::expand(fs, &entry.path_template, &vars) {
            if !fs.is_dir(&path) {
                continue;
            }
            // First claim wins, and ordering above makes that the strongest claim.
            if seen.insert(path.clone()) {
                out.push(KbCandidate {
                    path,
                    entry_id: entry.id.clone(),
                    layer: entry.layer.clone(),
                    note: entry.note.clone(),
                    priority: entry.priority,
                    keyed: entry.match_kind != "any",
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SaveKbEntry;
    use crate::saves::fs::RootKind;
    use crate::test_support::VirtualFs;

    const HOME: &str = "C:/Users/test";

    fn entry(id: &str, layer: &str, template: &str) -> SaveKbEntry {
        SaveKbEntry {
            id: id.into(),
            layer: layer.into(),
            match_kind: "steam_appid".into(),
            match_value: "1".into(),
            platform: "windows".into(),
            role: "saves".into(),
            path_template: template.into(),
            glob: None,
            priority: 100,
            note: None,
            source_ref: None,
            kb_version: "v1".into(),
            created_at: "2026-01-01T00:00:00+00:00".into(),
        }
    }

    fn world() -> VirtualFs {
        VirtualFs::new()
            .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
            .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
            .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
    }

    // ── Title normalisation ───────────────────────────────────────────────

    #[test]
    fn titles_fold_case_punctuation_and_spacing() {
        assert_eq!(normalise_title("The Witcher 3: Wild Hunt"), "thewitcher3wildhunt");
        assert_eq!(normalise_title("S.T.A.L.K.E.R."), "stalker");
        assert_eq!(normalise_title("NieR:Automata™"), "nierautomata");
        assert_eq!(normalise_title("Hollow  Knight"), "hollowknight");
        assert_eq!(normalise_title("  Celeste  "), "celeste");
    }

    #[test]
    fn normalisation_is_stable_across_common_variations() {
        // Every one of these is the same game written differently. The apostrophe
        // case is why punctuation is removed rather than turned into a space.
        let forms = [
            "Marvel's Spider-Man",
            "Marvel’s Spider Man",
            "MARVELS SPIDER-MAN",
            "Marvel's  Spider--Man",
            "MarvelsSpiderMan",
        ];
        let normalised: Vec<String> = forms.iter().map(|f| normalise_title(f)).collect();
        assert!(
            normalised.windows(2).all(|w| w[0] == w[1]),
            "variations should normalise alike: {normalised:?}"
        );
        assert_eq!(normalised[0], "marvelsspiderman");
    }

    #[test]
    fn spacing_and_hyphenation_variants_collide() {
        // The three spellings a folder might use for one game.
        assert_eq!(normalise_title("Half-Life 2"), normalise_title("Half Life 2"));
        assert_eq!(normalise_title("Half-Life 2"), normalise_title("HalfLife2"));
    }

    #[test]
    fn distinct_games_do_not_collide() {
        assert_ne!(normalise_title("Portal"), normalise_title("Portal 2"));
        assert_ne!(normalise_title("Fallout 3"), normalise_title("Fallout 4"));
        assert_ne!(normalise_title("Doom"), normalise_title("Doom Eternal"));
    }

    #[test]
    fn executables_lose_their_path_and_extension() {
        assert_eq!(normalise_exe("Game.exe"), "game");
        assert_eq!(normalise_exe("D:/Games/Foo/Bin/Launcher.EXE"), "launcher");
        assert_eq!(normalise_exe("nixware"), "nixware");
    }

    // ── Match key precedence ──────────────────────────────────────────────

    #[test]
    fn match_keys_are_ordered_most_specific_first() {
        let ctx = GameContext {
            title: "Hollow Knight".into(),
            steam_appid: Some("367520".into()),
            gog_id: Some("1234".into()),
            exe_name: Some("hollow_knight.exe".into()),
            ..Default::default()
        };
        let kinds: Vec<String> = match_keys(&ctx).into_iter().map(|k| k.kind).collect();
        assert_eq!(kinds, vec!["steam_appid", "gog_id", "exe_name", "title_norm"]);
    }

    #[test]
    fn a_title_only_game_still_produces_a_key() {
        let keys = match_keys(&GameContext::new("Celeste"));
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], MatchKey::new("title_norm", "celeste"));
    }

    // ── Candidate production ──────────────────────────────────────────────

    #[test]
    fn an_entry_whose_path_exists_produces_a_candidate() {
        let dir = format!("{HOME}/Documents/My Games/Hollow Knight");
        let fs = world().with_dir(&dir);

        let got = candidates(
            &fs,
            &[entry("builtin:a", "builtin", "{MYGAMES}/{TITLE}")],
            &GameContext::new("Hollow Knight"),
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, PathBuf::from(&dir));
        assert_eq!(got[0].entry_id, "builtin:a");
        assert_eq!(got[0].layer, "builtin");
    }

    /// A KB entry is a claim, not a fact. Predicting a path that is not there must
    /// not produce a candidate — otherwise a binding would fail on first snapshot.
    #[test]
    fn an_entry_whose_path_is_absent_produces_nothing() {
        let fs = world();
        let got = candidates(
            &fs,
            &[entry("builtin:a", "builtin", "{MYGAMES}/{TITLE}")],
            &GameContext::new("Not Installed"),
        );
        assert!(got.is_empty());
    }

    #[test]
    fn a_file_at_the_templated_path_is_not_a_candidate() {
        let fs = world().with_file(&format!("{HOME}/Documents/My Games/Foo"), 1024);
        let got = candidates(
            &fs,
            &[entry("builtin:a", "builtin", "{MYGAMES}/{TITLE}")],
            &GameContext::new("Foo"),
        );
        assert!(got.is_empty(), "a file is not a save directory");
    }

    /// Supplied in the **wrong** order deliberately. An earlier version of
    /// `candidates` trusted the caller's ordering, and this test passed while
    /// exercising nothing because the entries happened to arrive sorted.
    #[test]
    fn the_strongest_layer_claims_a_path_whatever_order_it_arrives_in() {
        let dir = format!("{HOME}/Documents/My Games/Foo");
        let fs = world().with_dir(&dir);

        let builtin = entry("builtin:theirs", "builtin", "{MYGAMES}/{TITLE}");
        let user = entry("user:mine", "user", "{MYGAMES}/{TITLE}");

        for order in [
            vec![builtin.clone(), user.clone()],
            vec![user.clone(), builtin.clone()],
        ] {
            let got = candidates(&fs, &order, &GameContext::new("Foo"));
            assert_eq!(got.len(), 1, "the same path must not appear twice");
            assert_eq!(
                got[0].layer, "user",
                "the user layer must win regardless of input order"
            );
        }
    }

    /// Priority breaks ties within a layer, again independent of input order.
    #[test]
    fn priority_orders_entries_inside_one_layer() {
        let dir = format!("{HOME}/Documents/My Games/Foo");
        let fs = world().with_dir(&dir);

        let mut curated = entry("builtin:curated", "builtin", "{MYGAMES}/{TITLE}");
        curated.priority = 10;
        let mut convention = entry("builtin:convention", "builtin", "{MYGAMES}/{TITLE}");
        convention.priority = 100;

        let got = candidates(
            &fs,
            &[convention, curated],
            &GameContext::new("Foo"),
        );
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].entry_id, "builtin:curated",
            "the lower priority number should own the claim"
        );
    }

    #[test]
    fn several_entries_can_contribute_different_paths() {
        let a = format!("{HOME}/Documents/My Games/Foo");
        let b = format!("{HOME}/AppData/Roaming/Foo");
        let fs = world().with_dir(&a).with_dir(&b);

        let got = candidates(
            &fs,
            &[
                entry("builtin:a", "builtin", "{MYGAMES}/{TITLE}"),
                entry("builtin:b", "builtin", "{APPDATA}/{TITLE}"),
            ],
            &GameContext::new("Foo"),
        );
        let paths: Vec<String> = got
            .iter()
            .map(|c| c.path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(paths, vec![a, b]);
    }

    #[test]
    fn an_entry_needing_an_undeclared_variable_is_skipped() {
        let fs = world().with_dir(&format!("{HOME}/AppData/Roaming/Team Cherry/Hollow Knight"));
        // No publisher on the context, so the entry cannot apply — and the game
        // must not be dragged in by a partially expanded path.
        let got = candidates(
            &fs,
            &[entry("builtin:pub", "builtin", "{APPDATA}/{PUBLISHER}/{TITLE}")],
            &GameContext::new("Hollow Knight"),
        );
        assert!(got.is_empty());
    }

    #[test]
    fn a_publisher_entry_applies_once_the_publisher_is_known() {
        let dir = format!("{HOME}/AppData/Roaming/Team Cherry/Hollow Knight");
        let fs = world().with_dir(&dir);
        let ctx = GameContext {
            title: "Hollow Knight".into(),
            publisher: Some("Team Cherry".into()),
            ..Default::default()
        };
        let got = candidates(
            &fs,
            &[entry("builtin:pub", "builtin", "{APPDATA}/{PUBLISHER}/{TITLE}")],
            &ctx,
        );
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, PathBuf::from(&dir));
    }

    /// The security property, restated at this layer: a malicious entry that
    /// reached storage still cannot produce a path outside an anchor.
    #[test]
    fn a_traversal_entry_produces_no_candidate() {
        let fs = world().with_dir("C:/Windows/System32");
        let got = candidates(
            &fs,
            &[
                entry("evil:1", "community", "{APPDATA}/../../../Windows/System32"),
                entry("evil:2", "community", "C:/Windows/System32"),
            ],
            &GameContext::new("Foo"),
        );
        assert!(got.is_empty(), "traversal or absolute entries must yield nothing");
    }

    #[test]
    fn a_wildcard_entry_produces_one_candidate_per_account_directory() {
        let base = format!("{HOME}/AppData/Roaming/EldenRing");
        let fs = world()
            .with_dir(&base)
            .with_dir(&format!("{base}/76561198000000001"))
            .with_dir(&format!("{base}/76561198000000002"));

        let got = candidates(
            &fs,
            &[entry("builtin:er", "builtin", "{APPDATA}/EldenRing/{WILDCARD}")],
            &GameContext::new("Elden Ring"),
        );
        assert_eq!(got.len(), 2);
    }
}
