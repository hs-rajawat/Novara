//! Tests for Steam's on-disk formats.
//!
//! These parsers were the largest untested surface in the crate, and they decide
//! whether a game is detected at all: `libraryfolders.vdf` decides where NOVARA
//! looks, and the ACF manifest decides what it finds and whether the game is
//! reported as installed. Both formats are Valve's, informal, and outside our
//! control — exactly the kind of input worth pinning to examples.
//!
//! The `StateFlags` policy is the most consequential of the three, because getting
//! it wrong in the cautious direction hides a game the user has installed.

use super::*;
use crate::test_support::TempDir;

/// A manifest with the fields the scanner reads. `state_flags` is rendered
/// verbatim so malformed values can be exercised too.
fn manifest(appid: &str, name: &str, installdir: &str, state_flags: &str, size: &str) -> String {
    format!(
        r#"
"AppState"
{{
    "appid"      "{appid}"
    "name"       "{name}"
    "installdir" "{installdir}"
    "StateFlags" {state_flags}
    "SizeOnDisk" "{size}"
}}
"#
    )
}

// ── StateFlags policy ───────────────────────────────────────────────────

#[test]
fn a_fully_installed_manifest_reports_installed() {
    let text = manifest("220", "Half-Life 2", "Half-Life 2", "\"4\"", "1000");
    assert!(manifest_reports_installed(&text));
}

/// Bit 1 is the only flag trusted as "not installed".
#[test]
fn an_uninstalling_manifest_reports_not_installed() {
    let text = manifest("220", "Half-Life 2", "Half-Life 2", "\"1\"", "1000");
    assert!(!manifest_reports_installed(&text));
    // Bit 1 set alongside other bits is still uninstalling.
    let combined = manifest("220", "Half-Life 2", "Half-Life 2", "\"7\"", "1000");
    assert!(!manifest_reports_installed(&combined));
}

/// Every transitional state is a state a perfectly good game passes through.
/// Reporting these as missing would disable a game that is fine, which is a worse
/// error than briefly showing one that is mid-update.
#[test]
fn transitional_states_still_report_installed() {
    for flags in ["2", "4", "6", "64", "68", "1024", "0"] {
        let text = manifest("220", "Half-Life 2", "Half-Life 2", &format!("\"{flags}\""), "1000");
        assert!(
            manifest_reports_installed(&text),
            "StateFlags {flags} must not be treated as uninstalled"
        );
    }
}

/// Absent, non-numeric or unparseable input all mean "we do not know", and the
/// only unconditional uninstall signal is Steam deleting the manifest entirely —
/// which the caller handles, not this function.
#[test]
fn unreadable_state_flags_default_to_installed() {
    let missing = r#""AppState" { "appid" "220" "name" "HL2" }"#;
    assert!(manifest_reports_installed(missing));

    let not_a_number = manifest("220", "HL2", "HL2", "\"not-a-number\"", "1000");
    assert!(manifest_reports_installed(&not_a_number));

    assert!(
        manifest_reports_installed("this is not a VDF document at all {{{"),
        "an unparseable manifest must not hide the game"
    );
}

// ── installdir ──────────────────────────────────────────────────────────

#[test]
fn the_install_subdir_is_read_from_the_manifest() {
    let text = manifest("220", "Half-Life 2", "Half-Life 2 Deluxe", "\"4\"", "1000");
    assert_eq!(
        manifest_install_subdir(&text).as_deref(),
        Some("Half-Life 2 Deluxe")
    );
}

#[test]
fn a_missing_or_unparseable_install_subdir_is_unknown() {
    assert_eq!(
        manifest_install_subdir(r#""AppState" { "appid" "220" }"#),
        None
    );
    assert_eq!(manifest_install_subdir("not a vdf {{{"), None);
}

// ── library discovery ───────────────────────────────────────────────────

#[test]
fn the_steam_directory_is_always_a_library() {
    let tmp = TempDir::new("steam-libs");
    let libs = discover_libraries(tmp.path());
    assert_eq!(libs, vec![tmp.path().to_path_buf()]);
}

#[test]
fn additional_libraries_are_discovered_and_deduplicated() {
    let tmp = TempDir::new("steam-libs");
    let second = tmp.child("SecondLibrary");
    std::fs::create_dir_all(&second).unwrap();

    // Paths in this file are written with escaped separators, and the primary
    // library is listed alongside the extra one — so this also covers the
    // duplicate the root entry would otherwise create.
    let escaped = |p: &std::path::Path| p.to_string_lossy().replace('\\', "\\\\");
    tmp.write(
        "steamapps/libraryfolders.vdf",
        &format!(
            r#"
"libraryfolders"
{{
    "0" {{ "path" "{}" }}
    "1" {{ "path" "{}" }}
}}
"#,
            escaped(tmp.path()),
            escaped(&second)
        ),
    );

    let libs = discover_libraries(tmp.path());
    assert_eq!(
        libs,
        vec![tmp.path().to_path_buf(), second],
        "the extra library is added once and the root is not repeated"
    );
}

/// A library on a drive that is not mounted must not be returned as a place to
/// walk — the scanner would find nothing there and report the games as gone.
#[test]
fn a_library_path_that_does_not_exist_is_ignored() {
    let tmp = TempDir::new("steam-libs");
    tmp.write(
        "steamapps/libraryfolders.vdf",
        r#""libraryfolders" { "0" { "path" "Z:\\NotMounted" } }"#,
    );

    assert_eq!(discover_libraries(tmp.path()), vec![tmp.path().to_path_buf()]);
}

#[test]
fn a_missing_or_unparseable_libraryfolders_file_leaves_the_root_library() {
    let tmp = TempDir::new("steam-libs");
    assert_eq!(discover_libraries(tmp.path()), vec![tmp.path().to_path_buf()]);

    tmp.write("steamapps/libraryfolders.vdf", "{{{ not vdf");
    assert_eq!(discover_libraries(tmp.path()), vec![tmp.path().to_path_buf()]);
}

// ── whole-manifest parsing ──────────────────────────────────────────────

#[test]
fn a_manifest_becomes_a_detected_game() {
    let tmp = TempDir::new("steam-acf");
    let steamapps = tmp.child("steamapps");
    let path = tmp.write(
        "steamapps/appmanifest_220.acf",
        &manifest("220", "Half-Life 2", "Half-Life 2", "\"4\"", "12345"),
    );

    let game = parse_manifest(&path, &steamapps).expect("parse manifest");

    assert_eq!(game.source_code, "steam");
    assert_eq!(game.title, "Half-Life 2");
    assert_eq!(game.source_app_id.as_deref(), Some("220"));
    assert_eq!(game.install_dir, steamapps.join("common").join("Half-Life 2"));
    assert_eq!(game.install_size_bytes, Some(12345));
    assert_eq!(game.install_state_hint, Some(true));
    assert_eq!(
        game.executable, None,
        "Steam titles are launched by URI, so no binary is resolved here"
    );
}

/// The title is the fallback for the directory name, since a manifest without
/// `installdir` still describes a real installation.
#[test]
fn a_manifest_without_an_installdir_falls_back_to_the_title() {
    let tmp = TempDir::new("steam-acf");
    let steamapps = tmp.child("steamapps");
    let path = tmp.write(
        "steamapps/appmanifest_220.acf",
        r#""AppState" { "appid" "220" "name" "Half-Life 2" }"#,
    );

    let game = parse_manifest(&path, &steamapps).expect("parse manifest");
    assert_eq!(game.install_dir, steamapps.join("common").join("Half-Life 2"));
    assert_eq!(
        game.install_size_bytes, None,
        "an absent SizeOnDisk is unknown, not zero"
    );
}

/// Identity is what makes a detection useful: without an appid there is nothing
/// to key duplicate detection on, and without a name there is nothing to show.
#[test]
fn a_manifest_missing_its_identity_is_rejected() {
    let tmp = TempDir::new("steam-acf");
    let steamapps = tmp.child("steamapps");

    let no_appid = tmp.write("steamapps/a.acf", r#""AppState" { "name" "HL2" }"#);
    assert!(parse_manifest(&no_appid, &steamapps).is_err());

    let no_name = tmp.write("steamapps/b.acf", r#""AppState" { "appid" "220" }"#);
    assert!(parse_manifest(&no_name, &steamapps).is_err());
}
