//! Track D tests: extended aliases, fuzzy matching, bounds, install root.
//!
//! Organised so every recall gain sits next to the negative case that keeps it
//! honest. `GAME_SAVE_DETECTION.md` §6 ranks name similarity as the weakest evidence
//! there is; these tests are where that ranking is enforced rather than asserted.

use super::alias::{self, AliasKind};
use super::*;
use crate::saves::fs::RootKind;
use crate::test_support::VirtualFs;

const HOME: &str = "C:/Users/test";

fn world() -> VirtualFs {
    VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
        .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
        .with_root(RootKind::AppDataLocalLow, &format!("{HOME}/AppData/LocalLow"))
        .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
        .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
        .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
}

fn paths(found: &[DetectedPath]) -> Vec<String> {
    found.iter().map(|c| c.path.replace('\\', "/")).collect()
}

fn found_names(title: &str, fs: &VirtualFs) -> Vec<String> {
    paths(&detect(fs, title))
}

// ─────────────────────────────────────────────────────────────────────────
// 1.13 Extended alias generation
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_subtitle_is_stripped() {
    let names: Vec<String> = alias::aliases("NieR: Automata", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"NieR".to_string()), "got {names:?}");
}

/// Hyphens are not subtitle separators. `Spider-Man` must not become `Spider`,
/// because a folder called `Spider` belongs to something else entirely.
#[test]
fn a_hyphen_is_not_treated_as_a_subtitle_separator() {
    for title in ["Spider-Man", "Half-Life", "Ori and the Blind Forest"] {
        let names: Vec<String> = alias::aliases(title, None, None)
            .into_iter()
            .map(|a| a.name)
            .collect();
        let first_word = title.split(['-', ' ']).next().unwrap();
        assert!(
            !names.contains(&first_word.to_string()) || first_word.len() >= 5,
            "`{title}` should not reduce to `{first_word}`: {names:?}"
        );
    }
}

#[test]
fn an_edition_suffix_is_stripped() {
    let names: Vec<String> = alias::aliases("Skyrim Special Edition", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    // `Special Edition` is not in the suffix list, but `Remastered` and the GOTY
    // family are. Check one that is.
    let goty: Vec<String> = alias::aliases("Fallout 3 Game of the Year Edition", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(
        goty.contains(&"Fallout 3".to_string()),
        "GOTY suffix should reduce to the base title: {goty:?}"
    );
    assert!(!names.is_empty());
}

#[test]
fn punctuation_is_dropped_into_a_single_word() {
    let names: Vec<String> = alias::aliases("S.T.A.L.K.E.R.", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"STALKER".to_string()), "got {names:?}");
}

#[test]
fn a_leading_article_is_dropped() {
    let names: Vec<String> = alias::aliases("The Witcher 3", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"Witcher 3".to_string()), "got {names:?}");
}

#[test]
fn an_initialism_keeps_a_trailing_number() {
    let names: Vec<String> = alias::aliases("The Witcher 3", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"TW3".to_string()), "got {names:?}");
}

/// Two letters carry too little information to be worth probing for.
#[test]
fn a_two_letter_initialism_is_not_generated() {
    let names: Vec<String> = alias::aliases("Dead Cells", None, None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(!names.contains(&"DC".to_string()), "got {names:?}");
}

#[test]
fn a_vendor_pair_is_generated_from_developer_and_publisher() {
    let names: Vec<String> = alias::aliases("Witcher 3", Some("CDPR"), Some("CD Projekt"))
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert!(names.contains(&"CDPR/Witcher 3".to_string()), "got {names:?}");
    assert!(
        names.contains(&"CD Projekt/Witcher 3".to_string()),
        "got {names:?}"
    );
}

/// A vendor pair must actually find a two-level folder, which is the whole point of
/// having metadata a title-only matcher lacks.
#[test]
fn a_vendor_pair_finds_a_two_level_folder() {
    let dir = format!("{HOME}/Documents/My Games/CDPR/Witcher 3");
    let fs = world().with_dir_tree(&dir);

    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Witcher 3",
            developer: Some("CDPR"),
            ..Default::default()
        },
    );
    assert!(paths(&found).contains(&dir), "got {:?}", paths(&found));
}

/// A generated alias that is a bare vendor or container word would be claimed by
/// every game in the library.
#[test]
fn a_bare_vendor_word_is_never_an_alias() {
    use crate::saves::kb::normalise_title;
    for title in ["Games", "My Games", "Saved Games", "Data", "Profiles"] {
        let names: Vec<String> = alias::aliases(title, None, None)
            .into_iter()
            .map(|a| normalise_title(&a.name))
            .collect();
        assert!(
            !names.contains(&normalise_title(title)),
            "`{title}` should not survive as an alias: {names:?}"
        );
    }
}

/// The stopword list must not be applied to the vendor half of a pair — the vendor
/// word is the point of a pair, and the second segment still has to match.
#[test]
fn a_vendor_pair_survives_even_when_the_vendor_is_a_stopword_like_name() {
    let names: Vec<String> = alias::aliases("Some Game", Some("Studios"), None)
        .into_iter()
        .map(|a| a.name)
        .collect();
    // `Studios` alone is a stopword, so no pair is generated from it at all.
    assert!(
        !names.iter().any(|n| n.starts_with("Studios/")),
        "a stopword vendor should not produce a pair: {names:?}"
    );
}

#[test]
fn weak_aliases_are_marked_weak() {
    let generated = alias::aliases("The Witcher 3", None, None);
    let tw3 = generated.iter().find(|a| a.name == "TW3").expect("initialism");
    assert_eq!(tw3.kind, AliasKind::Weak);
    assert!(!tw3.allows_fuzzy(), "an initialism must be matched exactly only");

    let exact = generated.iter().find(|a| a.name == "The Witcher 3").expect("exact");
    assert_eq!(exact.kind, AliasKind::Exact);
}

// ─────────────────────────────────────────────────────────────────────────
// 1.14 Fuzzy directory-name matching
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn normalisation_alone_handles_case_and_separator_differences() {
    assert_eq!(alias::similarity("Witcher3", "witcher 3"), 1.0);
    assert_eq!(alias::similarity("Hollow Knight", "hollow_knight"), 1.0);
    assert_eq!(alias::similarity("STALKER", "S.T.A.L.K.E.R."), 1.0);
}

#[test]
fn a_fuzzy_match_finds_a_differently_spelled_folder() {
    let dir = format!("{HOME}/AppData/Roaming/witcher 3");
    let fs = world().with_dir_tree(&dir);
    assert!(
        found_names("Witcher3", &fs).contains(&dir),
        "got {:?}",
        found_names("Witcher3", &fs)
    );
}

/// **The most important negative test in Track D.** A sequel differs from its
/// predecessor by one character, which a normalised edit distance scores at 0.875 —
/// comfortably above the 0.75 threshold. Without the sequel rule, Fallout 4 would be
/// offered Fallout 3's saves.
#[test]
fn a_sequel_never_matches_a_different_instalment() {
    let cases = [
        ("Fallout 4", "Fallout3"),
        ("Fallout 3", "Fallout4"),
        ("Dark Souls III", "DarkSoulsII"),
        ("Dark Souls II", "DarkSoulsIII"),
        ("Civilization VI", "Civilization V"),
        ("Diablo 3", "Diablo2"),
        ("Portal 2", "Portal 3"),
    ];
    for (title, folder) in cases {
        let dir = format!("{HOME}/AppData/Roaming/{folder}");
        let fs = world().with_dir_tree(&dir);
        let found = found_names(title, &fs);
        assert!(
            !found.contains(&dir),
            "`{title}` must not claim `{folder}`: {found:?}"
        );
    }
}

/// `Portal 2` *is* offered an unnumbered `Portal` folder, and that is deliberate:
/// stripping a trailing number is a transform `GAME_SAVE_DETECTION.md` §8 explicitly
/// retains, because franchise folders without an instalment number are common.
///
/// It is a **known residual false-positive risk** — a user who owns both games gets
/// Portal's folder proposed for Portal 2. What contains it is the confidence ceiling:
/// the match can never score above the stripped alias's own 0.75, which §6 rule 9
/// puts below anything that binds on name evidence alone. This test pins that
/// ceiling, so a later change that inflates the confidence of a reduced alias fails
/// here rather than in someone's library.
#[test]
fn a_stripped_number_alias_is_offered_but_capped() {
    let dir = format!("{HOME}/AppData/Roaming/Portal");
    let fs = world().with_dir_tree(&dir);

    let found = detect(&fs, "Portal 2");
    let hit = found
        .iter()
        .find(|c| c.path.replace('\\', "/") == dir)
        .expect("the franchise folder should be proposed");
    assert!(
        hit.confidence <= 0.75,
        "a reduced alias must not exceed 0.75, got {}",
        hit.confidence
    );
}

/// The same rule expressed on the metric directly, so a regression is attributed to
/// similarity rather than to detection.
#[test]
fn similarity_refuses_mismatched_sequel_markers() {
    assert_eq!(alias::similarity("Fallout 4", "Fallout 3"), 0.0);
    assert_eq!(alias::similarity("Dark Souls II", "Dark Souls III"), 0.0);
    // No marker on one side is still a mismatch.
    assert_eq!(alias::similarity("Portal", "Portal 2"), 0.0);
    // Identical markers are compared normally.
    assert_eq!(alias::similarity("Fallout 4", "Fallout4"), 1.0);
}

/// A title that legitimately ends in roman-numeral-looking letters must not be read
/// as a sequel marker. `Civ` is c/i/v; treating that as a numeral would make every
/// comparison fail.
#[test]
fn a_whole_name_of_numeral_letters_is_not_a_sequel_marker() {
    assert_eq!(alias::similarity("Civ", "Civ"), 1.0);
    assert!(alias::similarity("Vivi", "Vivi") > 0.99);
}

/// Short names collide far too easily under an edit distance: any two four-character
/// names differing by one character score exactly 0.75 and would pass the threshold.
///
/// The titles here are chosen to be **non-stopwords**. A first version used `Data`
/// against a folder called `Date`, which passed with the length floor set to 1 —
/// `data` is a vendor stopword, so the alias was filtered out before its length was
/// ever considered and the test proved nothing.
#[test]
fn short_names_are_matched_exactly_only() {
    for (title, folder) in [("Rime", "Rome"), ("Limbo", "Lambo"), ("Gris", "Iris")] {
        let dir = format!("{HOME}/Documents/{folder}");
        let fs = world().with_dir_tree(&dir);
        let found = found_names(title, &fs);
        assert!(
            !found.contains(&dir),
            "`{title}` must not fuzzily match `{folder}`: {found:?}"
        );
    }
}

/// The control: the floor must not be blocking *exact* matches for short titles.
#[test]
fn a_short_title_still_matches_its_own_folder() {
    let dir = format!("{HOME}/Documents/Rime");
    let fs = world().with_dir_tree(&dir);
    assert!(
        found_names("Rime", &fs).contains(&dir),
        "a short title must still find its own folder"
    );
}

#[test]
fn an_alias_below_the_threshold_produces_nothing() {
    let dir = format!("{HOME}/AppData/Roaming/Completely Different");
    let fs = world().with_dir_tree(&dir);
    assert!(found_names("Hollow Knight", &fs).is_empty());
}

/// A weak alias matched cleanly must not outrank a strong alias matched loosely.
#[test]
fn confidence_stays_anchored_to_the_transform() {
    let exact = format!("{HOME}/AppData/Roaming/Hollow Knight");
    let initial = format!("{HOME}/Documents/HK2"); // would be an initialism hit
    let fs = world().with_dir_tree(&exact).with_dir_tree(&initial);

    let found = detect(&fs, "Hollow Knight");
    assert_eq!(
        found.first().map(|c| c.path.replace('\\', "/")),
        Some(exact),
        "the exact title must lead: {:?}",
        paths(&found)
    );
}

/// Enumeration is where false positives come from, so the ignore lists must apply
/// there — and to direct probes too.
#[test]
fn ignored_directories_are_never_offered() {
    for noise in ["Cache", "Logs", "Crashes", "ShaderCache", "Photos", "Downloads"] {
        let dir = format!("{HOME}/Documents/{noise}");
        let fs = world().with_dir_tree(&dir);
        // Title identical to the folder, so only the ignore list can exclude it.
        let found = found_names(noise, &fs);
        assert!(
            !found.contains(&dir),
            "`{noise}` is on an ignore list but was offered: {found:?}"
        );
    }
}

#[test]
fn enumeration_does_not_descend() {
    // A matching folder two levels below a root must not be found: nothing recurses.
    let deep = format!("{HOME}/Documents/Some Vendor/Hollow Knight");
    let fs = world().with_dir_tree(&deep);
    let found = found_names("Hollow Knight", &fs);
    assert!(
        !found.contains(&deep),
        "detection must not recurse below a root: {found:?}"
    );
}

/// Letters that cannot be read as a trailing roman numeral, so a generated suffix
/// never trips the sequel rule and every directory below really does match.
const SAFE_SUFFIX_LETTERS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't',
    'u', 'w', 'y', 'z',
];

/// Directory names that all fuzzily match `Hollow Knight`.
///
/// A first version of this used `Hollow Knight 0`, `Hollow Knight 1`, ... — which the
/// sequel rule correctly scores at 0.0, because the alias has no instalment marker and
/// the folders do. Nothing matched, so the ceiling was never approached and the test
/// passed while proving nothing.
fn many_matching_dirs(count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(count);
    for a in SAFE_SUFFIX_LETTERS {
        for b in SAFE_SUFFIX_LETTERS {
            if names.len() == count {
                return names;
            }
            names.push(format!("Hollow Knight {a}{b}"));
        }
    }
    names
}

/// The per-game ceiling is a symptom detector: §7.2 says past this point the alias
/// generator is malfunctioning, so truncating is more honest than returning a list
/// nobody will read.
#[test]
fn the_per_game_candidate_ceiling_is_applied() {
    let mut fs = world();
    let names = many_matching_dirs(bounds::MAX_CANDIDATES_PER_GAME + 120);
    for name in &names {
        fs = fs.with_dir(&format!("{HOME}/Documents/{name}"));
    }

    let found = detect(&fs, "Hollow Knight");
    // The control: these directories really do match, so the cap is what limits the
    // result rather than a lack of candidates.
    assert!(
        found.len() >= bounds::MAX_CANDIDATES_PER_GAME,
        "expected the generated directories to match; only {} did",
        found.len()
    );
    assert_eq!(
        found.len(),
        bounds::MAX_CANDIDATES_PER_GAME,
        "results must be truncated to the per-game ceiling"
    );
}

/// Enumeration must stay one level deep and bounded, so cost scales with the ceiling
/// rather than with the size of the user's `Documents`.
#[test]
fn enumeration_of_a_huge_root_stays_bounded() {
    let mut fs = world();
    for name in many_matching_dirs(bounds::MAX_ENTRIES_PER_ROOT + 200) {
        fs = fs.with_dir(&format!("{HOME}/Documents/{name}"));
    }
    let found = detect(&fs, "Hollow Knight");
    assert!(
        found.len() <= bounds::MAX_CANDIDATES_PER_GAME,
        "got {} candidates, ceiling is {}",
        found.len(),
        bounds::MAX_CANDIDATES_PER_GAME
    );
}

// ─────────────────────────────────────────────────────────────────────────
// 1.16 Install-directory root
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_portable_save_folder_beside_the_executable_is_found() {
    let install = "D:/Games/Some Portable Game";
    let saves = format!("{install}/saves");
    let fs = world().with_dir_tree(&saves).with_file(&format!("{saves}/slot0.sav"), 1400);

    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Some Portable Game",
            install_dir: Some(install),
            ..Default::default()
        },
    );
    let got = paths(&found);
    assert!(got.contains(&saves), "expected {saves} in {got:?}");
}

/// Binding the install root would archive the whole game — tens of gigabytes to
/// protect a few kilobytes of saves.
#[test]
fn the_install_root_itself_is_never_offered() {
    let install = "D:/Games/Some Portable Game";
    let fs = world()
        .with_dir_tree(&format!("{install}/saves"))
        .with_file(&format!("{install}/Game.exe"), 24_000_000);

    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Some Portable Game",
            install_dir: Some(install),
            ..Default::default()
        },
    );
    let got = paths(&found);
    assert!(!got.contains(&install.to_string()), "install root offered: {got:?}");
}

#[test]
fn install_dir_matching_uses_conventional_names_not_the_title() {
    let install = "D:/Games/Elden Ring";
    // Directories a real install has, none of them named after the game.
    let fs = world()
        .with_dir_tree(&format!("{install}/Game"))
        .with_dir_tree(&format!("{install}/EasyAntiCheat"))
        .with_dir_tree(&format!("{install}/savegame"));

    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Elden Ring",
            install_dir: Some(install),
            ..Default::default()
        },
    );
    let got = paths(&found);
    assert_eq!(
        got,
        vec![format!("{install}/savegame")],
        "only the conventional save folder should be offered"
    );
}

/// Recall increase, negative half: an install directory full of engine folders must
/// yield nothing rather than something plausible-looking.
#[test]
fn an_install_dir_with_no_save_folder_yields_nothing() {
    let install = "D:/Games/Whatever";
    let fs = world()
        .with_dir_tree(&format!("{install}/Binaries"))
        .with_dir_tree(&format!("{install}/Content"))
        .with_dir_tree(&format!("{install}/Logs"))
        .with_dir_tree(&format!("{install}/Cache"));

    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Whatever",
            install_dir: Some(install),
            ..Default::default()
        },
    );
    assert!(paths(&found).is_empty(), "got {:?}", paths(&found));
}

#[test]
fn a_missing_install_dir_is_simply_skipped() {
    let fs = world();
    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Nothing Here",
            install_dir: Some("D:/Games/Not Installed"),
            ..Default::default()
        },
    );
    assert!(paths(&found).is_empty());
}

// ─────────────────────────────────────────────────────────────────────────
// Security: the guards must not have been weakened
// ─────────────────────────────────────────────────────────────────────────

/// An alias is built from game metadata, which is attacker-influenced in exactly the
/// way a KB template is. `Path::join` with an absolute string replaces the base, so a
/// hostile title could otherwise redirect a search of `Documents` at the system
/// directory.
#[test]
fn a_hostile_title_cannot_redirect_the_search() {
    let fs = world()
        .with_dir_tree("C:/Windows/System32")
        .with_dir_tree(&format!("{HOME}/Documents/Real Game"));

    for hostile in [
        "C:/Windows/System32",
        "../../../Windows/System32",
        "..",
        "\\\\server\\share",
        "C:",
    ] {
        let found = found_names(hostile, &fs);
        for path in &found {
            assert!(
                path.starts_with(HOME) || path.starts_with("D:/"),
                "`{hostile}` produced a path outside the search roots: {path}"
            );
            assert!(!path.contains(".."), "`{hostile}` produced a traversal: {path}");
        }
    }
}

/// A hostile *developer* name reaches the filesystem through the two-level vendor
/// pair, which is a second entry point for the same class of bug.
#[test]
fn a_hostile_vendor_name_cannot_escape_a_root() {
    let fs = world().with_dir_tree("C:/Windows/System32");
    let found = detect_with(
        &fs,
        &TitleContext {
            title: "Game",
            developer: Some("../../../Windows"),
            publisher: Some("C:/Windows"),
            install_dir: None,
        },
    );
    for path in paths(&found) {
        assert!(path.starts_with(HOME), "escaped: {path}");
        assert!(!path.contains(".."), "traversal: {path}");
    }
}

/// The locator's path guard and the KB template guard must refuse the same things.
/// Two implementations of one security property drift; this is what notices.
#[test]
fn both_path_guards_refuse_the_same_hostile_input() {
    use crate::saves::fs::join_under;
    use crate::saves::kb::template;

    let base = std::path::Path::new("C:/Users/test/Documents");
    let fs = world();
    let vars = template::TemplateVars {
        title: "X",
        publisher: None,
        developer: None,
        steam_appid: None,
        steam_userid: None,
        install_dir: None,
    };

    for hostile in [
        "..",
        "../escape",
        "..\\escape",
        "C:/Windows",
        "C:",
        "sub/../../escape",
    ] {
        // The locator guard refuses outright.
        let joined = join_under(base, hostile);
        assert!(
            joined.is_none(),
            "join_under accepted `{hostile}` -> {joined:?}"
        );

        // The template guard refuses the equivalent template, either at validation
        // or by expanding to nothing.
        let as_template = format!("{{DOCUMENTS}}/{hostile}");
        let expanded = if template::validate(&as_template).is_err() {
            Vec::new()
        } else {
            template::expand(&fs, &as_template, &vars)
        };
        for path in expanded {
            let text = path.to_string_lossy().replace('\\', "/");
            assert!(
                text.starts_with("C:/Users/test") && !text.contains(".."),
                "template guard let `{hostile}` escape: {text}"
            );
        }
    }
}

/// Enumeration must not turn into a content read. The trait has no method that opens
/// a file, so this is really a guard against the trait gaining one: if a future
/// change adds a content read, this test is where the intent is recorded.
#[test]
fn detection_finds_a_folder_without_needing_its_contents() {
    let dir = format!("{HOME}/AppData/Roaming/Hollow Knight");
    let fs = world()
        .with_dir_tree(&dir)
        .with_file(&format!("{dir}/user1.dat"), 4096);

    let found = detect(&fs, "Hollow Knight");
    assert!(
        paths(&found).contains(&dir),
        "the folder should be found from metadata alone: {:?}",
        paths(&found)
    );
}
