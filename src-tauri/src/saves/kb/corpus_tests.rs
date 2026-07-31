//! Coverage over the shipped corpus, driven by the corpus itself.
//!
//! Rather than one fixture file per curated title — forty near-identical TOMLs that
//! would rot as a set — this walks every entry in the built-in KB and asserts the
//! universal property that matters: **a shipped entry must be reachable.** For each
//! entry it synthesises the world in which that entry should fire, then requires
//! `kb::candidates` to return it.
//!
//! That catches the failure the corpus is most likely to develop as it grows: an
//! entry that is well-formed, validates cleanly, and can never match anything —
//! because its key is unreachable, its anchor is one the context never supplies, or
//! its template needs a variable no game carries. Such an entry looks present in
//! review and does nothing in production.
//!
//! Being derived from the data, coverage cannot fall behind it. Adding an entry
//! automatically adds a case.

use std::collections::HashSet;

use crate::db::save_kb::NewKbEntry;
use crate::models::SaveKbEntry;
use crate::saves::fs::RootKind;
use crate::saves::kb::{self, builtin, template};
use crate::saves::pipeline::GameContext;
use crate::test_support::VirtualFs;

const HOME: &str = "C:/Users/test";
const INSTALL: &str = "D:/Games/Test Game";
/// Stands in for the account-id directory a `{WILDCARD}` fans into.
const ACCOUNT: &str = "76561198000000001";

fn world() -> VirtualFs {
    VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
        .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
        .with_root(RootKind::AppDataLocalLow, &format!("{HOME}/AppData/LocalLow"))
        .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
        .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
        .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
}

/// A context that satisfies every variable, so an entry is only unreachable if it is
/// genuinely unreachable rather than merely under-supplied here.
fn context_for(entry: &NewKbEntry) -> GameContext {
    GameContext {
        // A `title_norm` entry only fires for a title that folds to its key, so use
        // the key itself — it is already in normalised form.
        title: if entry.match_kind == "title_norm" {
            entry.match_value.clone()
        } else {
            "Test Game".to_string()
        },
        publisher: Some("Test Publisher".into()),
        developer: Some("Test Developer".into()),
        steam_appid: Some("1".into()),
        install_dir: Some(INSTALL.into()),
        ..Default::default()
    }
}

fn vars_for(ctx: &GameContext) -> template::TemplateVars<'_> {
    template::TemplateVars {
        title: &ctx.title,
        publisher: ctx.publisher.as_deref(),
        developer: ctx.developer.as_deref(),
        steam_appid: ctx.steam_appid.as_deref(),
        steam_userid: None,
        install_dir: ctx.install_dir.as_deref(),
    }
}

/// Build a world in which `entry` should fire, and return the directory created.
///
/// `{WILDCARD}` is replaced with a literal account id **for world-building only** —
/// the assertion still expands the real template, so the wildcard fan-out is
/// exercised rather than bypassed.
fn seed_world_for(entry: &NewKbEntry, ctx: &GameContext) -> (VirtualFs, Vec<String>) {
    let literal = entry.path_template.replace("{WILDCARD}", ACCOUNT);
    let bare = world().with_dir(INSTALL);
    let dirs: Vec<String> = template::expand(&bare, &literal, &vars_for(ctx))
        .into_iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();

    let mut fs = world().with_dir(INSTALL);
    for d in &dirs {
        // Tree, not a bare leaf: `{WILDCARD}` expansion lists the parent.
        fs = fs.with_dir_tree(d);
    }
    (fs, dirs)
}

fn as_stored(entry: &NewKbEntry, layer: &str) -> SaveKbEntry {
    SaveKbEntry {
        id: entry.id.clone(),
        layer: layer.into(),
        match_kind: entry.match_kind.clone(),
        match_value: entry.match_value.clone(),
        platform: entry.platform.clone(),
        role: entry.role.clone(),
        path_template: entry.path_template.clone(),
        glob: entry.glob.clone(),
        priority: entry.priority,
        note: entry.note.clone(),
        source_ref: entry.source_ref.clone(),
        kb_version: "test".into(),
        created_at: "2026-01-01T00:00:00+00:00".into(),
    }
}

/// The core property: every shipped entry can fire.
#[test]
fn every_shipped_entry_is_reachable() {
    let (_, entries) = builtin::parsed().expect("valid corpus");
    let mut checked = 0usize;

    for entry in &entries {
        let ctx = context_for(entry);
        let (fs, dirs) = seed_world_for(entry, &ctx);
        assert!(
            !dirs.is_empty(),
            "`{}` expands to nothing even with every variable supplied — template: {}",
            entry.id,
            entry.path_template
        );

        let got = kb::candidates(&fs, &[as_stored(entry, "builtin")], &ctx);
        assert!(
            !got.is_empty(),
            "`{}` produced no candidate in a world containing {dirs:?}",
            entry.id
        );
        assert_eq!(got[0].entry_id, entry.id);
        checked += 1;
    }

    assert_eq!(
        checked,
        entries.len(),
        "every entry in the corpus should have been exercised"
    );
    assert!(checked >= 25, "corpus is smaller than expected: {checked}");
}

/// A `title_norm` entry must fire for the title a user would actually have stored,
/// not only for the pre-normalised key. This is the round trip from display title
/// through normalisation to lookup.
#[test]
fn curated_entries_match_a_realistic_display_title() {
    let (_, entries) = builtin::parsed().expect("valid corpus");

    // Display forms a library would plausibly hold, paired with the entry each
    // should reach. Deliberately includes punctuation and casing variation.
    let cases = [
        ("The Witcher 3: Wild Hunt", "builtin:the-witcher-3-wild-hunt"),
        ("Cyberpunk 2077", "builtin:cyberpunk-2077"),
        ("ELDEN RING", "builtin:elden-ring"),
        ("Hollow Knight", "builtin:hollow-knight"),
        ("Skyrim Special Edition", "builtin:skyrim-special-edition"),
        ("Baldur's Gate 3", "builtin:baldurs-gate-3"),
        ("Sid Meier's Civilization VI", "builtin:civilization-vi"),
        ("No Man's Sky", "builtin:no-mans-sky"),
        ("Dark Souls III", "builtin:dark-souls-iii"),
        ("Sekiro: Shadows Die Twice", "builtin:sekiro"),
    ];

    for (display, expected_id) in cases {
        let entry = entries
            .iter()
            .find(|e| e.id == expected_id)
            .unwrap_or_else(|| panic!("corpus should contain `{expected_id}`"));

        assert_eq!(
            kb::normalise_title(display),
            entry.match_value,
            "`{display}` should fold to the key of `{expected_id}`"
        );

        // And end-to-end: the display title alone must produce the candidate.
        let ctx = GameContext {
            title: display.into(),
            ..Default::default()
        };
        let (fs, dirs) = seed_world_for(entry, &context_for(entry));
        let got = kb::candidates(&fs, &[as_stored(entry, "builtin")], &ctx);
        assert!(
            !got.is_empty(),
            "`{display}` produced no candidate against `{expected_id}` in {dirs:?}"
        );
    }
}

/// One representative case per distinct anchor the corpus uses, so a regression in
/// anchor resolution is attributed to the anchor rather than to a title.
#[test]
fn every_anchor_the_corpus_relies_on_resolves() {
    let (_, entries) = builtin::parsed().expect("valid corpus");

    let mut anchors_seen: HashSet<&str> = HashSet::new();
    for e in &entries {
        for anchor in [
            "{APPDATA}",
            "{LOCALAPPDATA}",
            "{LOCALLOW}",
            "{MYGAMES}",
            "{DOCUMENTS}",
            "{SAVEDGAMES}",
            "{INSTALL}",
        ] {
            if e.path_template.contains(anchor) {
                anchors_seen.insert(anchor);
            }
        }
    }

    // Guards against a silent narrowing of the corpus to one or two anchors.
    assert!(
        anchors_seen.len() >= 6,
        "the corpus should exercise most anchors, saw {anchors_seen:?}"
    );

    for anchor in &anchors_seen {
        let entry = entries
            .iter()
            .find(|e| e.path_template.contains(*anchor))
            .expect("anchor came from an entry");
        let ctx = context_for(entry);
        let (fs, dirs) = seed_world_for(entry, &ctx);
        assert!(
            !kb::candidates(&fs, &[as_stored(entry, "builtin")], &ctx).is_empty(),
            "anchor {anchor} did not resolve (via `{}`, world {dirs:?})",
            entry.id
        );
    }
}

/// A convention rule is library-wide, so its cost is paid for every game. It must
/// stay silent when the directory is absent — otherwise every game in a library
/// acquires a dozen phantom candidates.
#[test]
fn convention_rules_stay_silent_in_an_empty_world() {
    let (_, entries) = builtin::parsed().expect("valid corpus");
    let conventions: Vec<SaveKbEntry> = entries
        .iter()
        .filter(|e| e.match_kind == "any")
        .map(|e| as_stored(e, "builtin"))
        .collect();
    assert!(!conventions.is_empty(), "corpus should carry convention rules");

    // Anchors exist; no game directory does.
    let fs = world();
    let ctx = GameContext {
        title: "Some Unknown Game".into(),
        publisher: Some("Nobody".into()),
        developer: Some("Nobody".into()),
        install_dir: Some(INSTALL.into()),
        ..Default::default()
    };

    let got = kb::candidates(&fs, &conventions, &ctx);
    assert!(
        got.is_empty(),
        "convention rules must produce nothing when no directory exists, got {:?}",
        got.iter().map(|c| c.path.clone()).collect::<Vec<_>>()
    );
}

/// The whole corpus at once, against one game. Proves the layered result is
/// deduplicated and that an unrelated game picks up nothing.
#[test]
fn the_full_corpus_against_one_game_yields_only_its_own_paths() {
    let (_, entries) = builtin::parsed().expect("valid corpus");
    let stored: Vec<SaveKbEntry> = entries.iter().map(|e| as_stored(e, "builtin")).collect();

    let hollow = format!("{HOME}/AppData/LocalLow/Team Cherry/Hollow Knight");
    let fs = world().with_dir(&hollow);
    let ctx = GameContext {
        title: "Hollow Knight".into(),
        developer: Some("Team Cherry".into()),
        ..Default::default()
    };

    let got = kb::candidates(&fs, &stored, &ctx);
    let paths: Vec<String> = got
        .iter()
        .map(|c| c.path.to_string_lossy().replace('\\', "/"))
        .collect();

    assert!(paths.contains(&hollow), "expected {hollow} in {paths:?}");

    let unique: HashSet<&String> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len(), "candidates must be deduplicated: {paths:?}");

    // The curated entry and the Unity convention rule both name this directory;
    // the curated one should own the claim.
    let claim = got.iter().find(|c| c.path.to_string_lossy().replace('\\', "/") == hollow);
    assert_eq!(
        claim.map(|c| c.entry_id.as_str()),
        Some("builtin:hollow-knight"),
        "the curated entry should outrank the convention rule for the same path"
    );
}
