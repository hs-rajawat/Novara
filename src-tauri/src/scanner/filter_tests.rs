//! Library filter tests.
//!
//! The asymmetry that shapes this file: **wrongly hiding a game is far worse than wrongly
//! importing a component.** A component the user can hide themselves is a row of clutter; a
//! game the scanner refuses to import is a feature that looks broken and gives no clue why.
//! So the "must still import" cases outnumber the "must skip" ones.

use std::path::Path;

use super::*;

fn steam(app_id: &str, title: &str) -> Candidate<'static> {
    // Leaked deliberately: test-only, and it keeps the call sites readable.
    Candidate {
        source_code: "steam",
        source_app_id: Some(Box::leak(app_id.to_string().into_boxed_str())),
        title: Box::leak(title.to_string().into_boxed_str()),
        install_dir: Path::new("D:/SteamLibrary/steamapps/common/x"),
        has_executable: None,
    }
}

fn manual(title: &str, has_exe: bool) -> Candidate<'static> {
    Candidate {
        source_code: "manual",
        source_app_id: None,
        title: Box::leak(title.to_string().into_boxed_str()),
        install_dir: Path::new("D:/Games/x"),
        has_executable: Some(has_exe),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Rule 1 — app ids
// ─────────────────────────────────────────────────────────────────────────

/// The case that started this: it was imported into the real library and appeared in the
/// UI as a game.
#[test]
fn steamworks_common_redistributables_is_skipped() {
    let v = classify(&steam("228980", "Steamworks Common Redistributables"));
    let skip = v.skip().expect("must be skipped");
    assert_eq!(skip.rule, "steam_system_app_id");
    assert!(skip.reason.contains("228980"), "reason: {}", skip.reason);
}

#[test]
fn the_runtime_and_proton_families_are_skipped_by_id() {
    for (id, what) in [
        ("1070560", "Steam Linux Runtime"),
        ("1391110", "soldier"),
        ("1628350", "sniper"),
        ("1493710", "Proton Experimental"),
        ("2805730", "Proton 9.0"),
        ("1007", "Steamworks SDK Redist"),
    ] {
        let v = classify(&steam(id, what));
        assert_eq!(
            v.skip().map(|s| s.rule),
            Some("steam_system_app_id"),
            "app {id} ({what}) should be skipped by id"
        );
    }
}

/// **The reason rule 1 matches ids rather than names.** A renamed or localised component is
/// still the same app, and the id still catches it.
#[test]
fn an_app_id_match_survives_a_renamed_or_localised_title() {
    for title in [
        "Steamworks Common Redistributables",
        "Steamworks: allgemeine Redistributables",
        "共通の再配布可能パッケージ",
        "",
    ] {
        assert!(
            !classify(&steam("228980", title)).is_import(),
            "the id should decide, whatever the title says: `{title}`"
        );
    }
}

/// And the converse: an id NOT on the list is imported even if its name looks systemy.
/// A game is a game.
#[test]
fn an_unlisted_app_id_with_an_ordinary_title_is_imported() {
    assert!(classify(&steam("489830", "Skyrim Special Edition")).is_import());
    assert!(classify(&steam("1245620", "ELDEN RING")).is_import());
}

// ─────────────────────────────────────────────────────────────────────────
// Rule 2 — nothing launchable
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_install_with_no_launchable_program_is_skipped() {
    let v = classify(&manual("Some Redistributable Bundle", false));
    assert_eq!(v.skip().map(|s| s.rule), Some("no_launchable_executable"));
}

#[test]
fn an_install_with_a_program_is_imported() {
    assert!(classify(&manual("My Manually Installed Game", true)).is_import());
}

/// **The trap in rule 2.** Steam launches through `steam://` and never resolves an
/// executable, so `has_executable` is `None` for every Steam entry. Treating `None` as
/// "no executable" would reject the entire Steam library.
#[test]
fn an_unknown_executable_state_is_not_evidence() {
    let mut c = steam("489830", "Skyrim Special Edition");
    c.has_executable = None;
    assert!(
        classify(&c).is_import(),
        "`None` means the scanner does not resolve executables, not that there are none"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Rule 3 — name patterns
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn system_components_are_skipped_by_name_when_the_id_is_unknown() {
    for title in [
        "Steamworks Common Redistributables",
        "Steam Linux Runtime",
        "Steam Linux Runtime 3.0 (sniper)",
        "Proton",
        "Proton 8.0",
        "Proton Experimental",
        "Proton Hotfix",
        "Steamworks SDK",
        "DirectX Redist",
        "VC Redist 2015",
    ] {
        // No app id, so rule 1 cannot fire and rule 3 must.
        let c = Candidate {
            source_code: "steam",
            source_app_id: None,
            title,
            install_dir: Path::new("D:/x"),
            has_executable: None,
        };
        assert!(
            !classify(&c).is_import(),
            "`{title}` should be recognised as a system component"
        );
    }
}

/// **The most important test in this file.** Real games whose titles brush against the
/// pattern list must still import. Every one of these is a shipped game.
#[test]
fn real_games_with_systemy_titles_are_still_imported() {
    for title in [
        // Contains "proton" but is not a Proton release.
        "Protonwar",
        "Proteus",
        // Contains "tools" / "runtime" / "benchmark", which is why those bare words are
        // deliberately absent from the pattern list.
        "Sanctum 2 Tools",
        "RPG Maker Tools",
        "Final Fantasy XV Benchmark",
        "3DMark",
        "Runtime Terror",
        "The Runtime",
        // Contains "steam" but is a game.
        "Steamworld Dig",
        "SteamWorld Heist",
        "Steam Marines",
        "Airships: Conquer the Skies",
        // Contains "sdk" as a substring of a word.
        "Sdkfz Panzer Simulator",
        // Ordinary titles.
        "Red Dead Redemption 2",
        "Dying Light The Following",
        "THE FINALS",
    ] {
        let c = Candidate {
            source_code: "steam",
            source_app_id: Some("999999"),
            title,
            install_dir: Path::new("D:/x"),
            has_executable: None,
        };
        assert!(
            classify(&c).is_import(),
            "`{title}` is a real game and must not be filtered out"
        );
    }
}

/// `Proton` needs a prefix test, not a substring test — `Protonwar` is a game.
#[test]
fn proton_is_matched_as_a_prefix_not_a_substring() {
    assert!(is_proton_release("proton"));
    assert!(is_proton_release("proton80"));
    assert!(is_proton_release("protonexperimental"));
    assert!(!is_proton_release("protonwar"));
    assert!(!is_proton_release("protoncannon"));
    assert!(!is_proton_release("xproton"));
}

// ─────────────────────────────────────────────────────────────────────────
// Properties
// ─────────────────────────────────────────────────────────────────────────

/// Every skip must carry a rule identity and a sentence, or the future "Import anyway" UI
/// has nothing to show and a support question has no answer.
#[test]
fn every_skip_is_explainable() {
    let a = steam("228980", "Steamworks Common Redistributables");
    let b = steam("1493710", "Proton Experimental");
    let c = manual("Nothing Here", false);
    for candidate in [&a, &b, &c] {
        let skip = classify(candidate).skip().expect("skipped").clone();
        assert!(!skip.rule.trim().is_empty());
        assert!(
            !skip.reason.trim().is_empty(),
            "rule `{}` produced no reason",
            skip.rule
        );
        assert!(
            skip.reason.contains("not a game") || skip.reason.contains("no launchable"),
            "reason should say why: {}",
            skip.reason
        );
    }
}

#[test]
fn classification_is_deterministic() {
    let c = steam("228980", "Steamworks Common Redistributables");
    let first = classify(&c);
    for _ in 0..20 {
        assert_eq!(classify(&c), first);
    }
}

/// A non-Steam source must not be judged by Steam's app id table — ids are only unique
/// within a storefront, so app "1007" on another store is somebody else's game.
#[test]
fn the_steam_app_id_table_only_applies_to_steam() {
    let c = Candidate {
        source_code: "gog",
        source_app_id: Some("228980"),
        title: "Some GOG Game",
        install_dir: Path::new("D:/x"),
        has_executable: Some(true),
    };
    assert!(
        classify(&c).is_import(),
        "an id table is storefront-specific"
    );
}
