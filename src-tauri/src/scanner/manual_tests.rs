//! Tests for the manual scanner's executable selection.
//!
//! This is the only scanner that has to *guess*. Launcher-managed sources are told
//! what a game is; here a folder full of binaries has to be reduced to one, and
//! choosing wrong gives the user a game whose Play button launches an uninstaller
//! or a crash reporter. The rules — prefer a name match, otherwise the largest
//! binary, never an installer or uninstaller — were previously unverified.

use super::*;
use crate::test_support::TempDir;

/// Create a file of a given size, so "largest wins" can be exercised.
fn exe(dir: &std::path::Path, relative: &str, bytes: usize) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
    std::fs::write(&path, vec![0u8; bytes]).expect("write exe");
}

#[test]
fn an_executable_named_after_its_folder_wins_over_a_larger_one() {
    let tmp = TempDir::new("manual-named");
    let game = tmp.child("Hollow Knight");
    exe(&game, "Hollow Knight.exe", 10);
    exe(&game, "UnityCrashHandler64.exe", 5_000);
    exe(&game, "launcher.exe", 9_000);

    assert_eq!(
        find_executable(&game).unwrap().file_name().unwrap(),
        "Hollow Knight.exe",
        "a name match is the strongest signal available and beats file size"
    );
}

#[test]
fn the_name_match_is_case_insensitive() {
    let tmp = TempDir::new("manual-case");
    let game = tmp.child("Celeste");
    exe(&game, "CELESTE.exe", 10);
    exe(&game, "other.exe", 9_000);

    assert_eq!(
        find_executable(&game).unwrap().file_name().unwrap(),
        "CELESTE.exe"
    );
}

#[test]
fn without_a_name_match_the_largest_executable_wins() {
    let tmp = TempDir::new("manual-largest");
    let game = tmp.child("Some Game");
    exe(&game, "small.exe", 100);
    exe(&game, "bin/big.exe", 50_000);

    assert_eq!(
        find_executable(&game).unwrap().file_name().unwrap(),
        "big.exe",
        "the game binary is usually the biggest thing in the folder"
    );
}

/// These are the binaries that ruin the feature: they are often larger than the
/// game's own launcher and would be picked by size alone.
#[test]
fn installers_uninstallers_and_helpers_are_never_chosen() {
    for name in [
        "unins000.exe",
        "Uninstall.exe",
        "setup.exe",
        "SETUP_x64.exe",
        "CrashReporter.exe",
        "vc_redist.x64.exe",
    ] {
        let tmp = TempDir::new("manual-skip");
        let game = tmp.child("Game");
        exe(&game, name, 90_000);
        exe(&game, "game-binary.exe", 10);

        assert_eq!(
            find_executable(&game).unwrap().file_name().unwrap(),
            "game-binary.exe",
            "{name} must never be selected, however large it is"
        );
    }
}

/// Excluded names must not win even when they are the only name match.
#[test]
fn an_excluded_name_does_not_win_by_matching_the_folder() {
    let tmp = TempDir::new("manual-setup-folder");
    let game = tmp.child("Setup");
    exe(&game, "Setup.exe", 90_000);
    exe(&game, "realgame.exe", 10);

    assert_eq!(
        find_executable(&game).unwrap().file_name().unwrap(),
        "realgame.exe"
    );
}

#[test]
fn non_windows_executable_extensions_are_recognised() {
    for name in ["start.sh", "Game.AppImage", "game.x86_64", "run.bat"] {
        let tmp = TempDir::new("manual-ext");
        let game = tmp.child("Game");
        exe(&game, name, 100);

        assert_eq!(
            find_executable(&game).unwrap().file_name().unwrap(),
            name,
            "{name} should be recognised as launchable"
        );
    }
}

#[test]
fn a_folder_with_no_executable_yields_nothing() {
    let tmp = TempDir::new("manual-empty");
    let game = tmp.child("Data Only");
    exe(&game, "readme.txt", 10);
    exe(&game, "assets/textures.pak", 5_000);

    assert_eq!(find_executable(&game), None);
}

/// The walk is depth-limited, so a binary buried deeper than the limit is not
/// found — a deliberate bound to keep scans predictable on large trees.
#[test]
fn the_search_is_depth_limited() {
    let tmp = TempDir::new("manual-depth");
    let game = tmp.child("Deep");
    exe(&game, "a/b/c/d/e/buried.exe", 100);

    assert_eq!(
        find_executable(&game),
        None,
        "MAX_DEPTH bounds the walk; a binary below it is out of scope by design"
    );
}

// ── whole-root scanning ─────────────────────────────────────────────────

#[test]
fn each_child_folder_with_an_executable_becomes_a_game() {
    let tmp = TempDir::new("manual-root");
    exe(&tmp.child("Game One"), "Game One.exe", 100);
    exe(&tmp.child("Game Two"), "play.exe", 100);
    exe(&tmp.child("Not A Game"), "notes.txt", 10);
    std::fs::write(tmp.child("loose.exe"), b"x").expect("write loose file");

    let mut found = Vec::new();
    scan_root(tmp.path(), &mut found);
    found.sort_by(|a, b| a.title.cmp(&b.title));

    let titles: Vec<&str> = found.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Game One", "Game Two"],
        "folders without an executable are skipped, and loose files are not games"
    );

    let one = &found[0];
    assert_eq!(one.source_code, "manual");
    assert_eq!(one.install_dir, tmp.child("Game One"));
    assert_eq!(
        one.executable.as_deref(),
        Some(std::path::Path::new("Game One.exe")),
        "the executable is stored relative to the install directory"
    );
    assert_eq!(one.source_app_id, None);
    assert_eq!(
        one.install_size_bytes, None,
        "sizing is the orchestrator's job, not the scanner's"
    );
}

#[tokio::test]
async fn a_root_that_does_not_exist_is_skipped_rather_than_failing() {
    let scanner = ManualScanner;
    let found = scanner
        .scan(&[std::path::PathBuf::from("Z:/definitely-not-mounted")])
        .await
        .expect("a missing root is not an error");
    assert!(found.is_empty());
}
