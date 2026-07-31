//! Path template validation and expansion.
//!
//! A KB entry stores a *template* (`{APPDATA}/{PUBLISHER}/{TITLE}`) rather than a
//! path, so one entry works on every machine. Expansion turns it into concrete
//! paths for a specific game on a specific filesystem.
//!
//! # This module is a security boundary
//!
//! Templates come from data — shipped, contributed, or typed by a user — and they
//! steer filesystem access. Two rules make that safe, and both are enforced at
//! *import* time as well as at expansion time:
//!
//!   1. **Closed variable set.** Only the variables in [`Anchor`] plus the
//!      game-derived ones below are recognised. An unknown variable is a rejection,
//!      never a passthrough — a literal `{NONSENSE}` must never reach the
//!      filesystem, and a silently-dropped variable could collapse a template onto
//!      an unintended directory.
//!   2. **No way to escape an anchor.** Absolute paths, drive letters, UNC prefixes
//!      and `..` segments are rejected. A template can only ever address something
//!      *beneath* a directory NOVARA already searches.
//!
//! Together these bound what a malicious or malformed KB can do to "suggests a
//! useless path", which is the guarantee ADR-0008 relies on when it defers payload
//! signing in favour of per-entry validation.
//!
//! See `docs/architecture/KNOWLEDGE_BASE.md` §5 for the variable set and
//! `GAME_SAVE_DETECTION.md` §13 for the threat model.

use std::path::{Path, PathBuf};

use crate::saves::fs::{Anchor, FileSystem};

/// A single path segment wildcard, for account-id directories like Steam's
/// `userdata/<id>/<appid>/remote`. Deliberately one segment, not a glob: `**`
/// would reopen the disk-walking problem bounds exist to prevent.
pub const WILDCARD: &str = "{WILDCARD}";

/// Game-derived template variables, longest first so no name is a prefix of
/// another during substitution.
const GAME_VARS: [&str; 4] = ["{STEAM_USERID}", "{STEAM_APPID}", "{PUBLISHER}", "{DEVELOPER}"];

/// Why a template was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    /// A variable outside the closed set.
    UnknownVariable(String),
    /// Anchored somewhere other than a known directory — an absolute path, a drive
    /// letter, or a UNC share.
    NotAnchored,
    /// Contains a `..` segment.
    ParentTraversal,
    /// Empty, or nothing but a variable.
    Empty,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::UnknownVariable(v) => write!(
                f,
                "unknown template variable `{v}` — only the documented set is allowed"
            ),
            TemplateError::NotAnchored => write!(
                f,
                "template must start with a directory variable such as {{APPDATA}}, \
                 {{MYGAMES}} or {{INSTALL}}; absolute paths and drive letters are not \
                 portable and are refused"
            ),
            TemplateError::ParentTraversal => {
                write!(f, "template contains a `..` segment, which could escape its anchor")
            }
            TemplateError::Empty => write!(f, "template is empty"),
        }
    }
}

/// Everything a template can be expanded against, beyond the filesystem anchors.
#[derive(Debug, Clone, Default)]
pub struct TemplateVars<'a> {
    pub title: &'a str,
    pub publisher: Option<&'a str>,
    pub developer: Option<&'a str>,
    pub steam_appid: Option<&'a str>,
    /// The local Steam account id, when one could be resolved.
    pub steam_userid: Option<&'a str>,
    /// This installation's directory, for `{INSTALL}` templates.
    pub install_dir: Option<&'a str>,
}

/// Validate a template without expanding it.
///
/// Called at KB import so a bad entry is rejected before storage rather than
/// failing quietly on every scan afterwards.
pub fn validate(template: &str) -> Result<(), TemplateError> {
    let t = template.trim();
    if t.is_empty() {
        return Err(TemplateError::Empty);
    }

    // Every `{...}` must be a known variable. Checked first, because an unknown
    // variable makes every later judgement meaningless.
    let mut rest = t;
    while let Some(open) = rest.find('{') {
        let after = &rest[open..];
        let Some(close) = after.find('}') else {
            return Err(TemplateError::UnknownVariable(after.to_string()));
        };
        let var = &after[..=close];
        if !is_known_variable(var) {
            return Err(TemplateError::UnknownVariable(var.to_string()));
        }
        rest = &after[close + 1..];
    }

    // `..` as a whole segment. A literal `..` inside a game title is fine — this
    // looks at segments, not substrings.
    if t.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(TemplateError::ParentTraversal);
    }

    // A drive prefix or NTFS alternate-data-stream marker in *any* segment, not
    // only the first.
    //
    // `{DOCUMENTS}/C:/Windows` passes the anchor check below — it genuinely does
    // start with a directory variable — but `PathBuf::push("C:")` *replaces* the
    // whole path on Windows rather than extending it, so the anchor was silently
    // discarded and the template expanded to `C:Windows`. A colon cannot appear in
    // a legitimate Windows folder name, so rejecting the whole class costs nothing.
    //
    // Found by `locator::track_d_tests::both_path_guards_refuse_the_same_hostile_input`,
    // which feeds identical hostile input to this guard and to `fs::join_under` and
    // requires neither to escape. Neither guard had noticed on its own.
    if t.split(['/', '\\']).any(|seg| seg.contains(':')) {
        return Err(TemplateError::NotAnchored);
    }

    // Must be anchored on a variable. This is what forbids `C:/...`, `/etc/...`
    // and `\\server\share`: all of them start with something other than `{`.
    if !t.starts_with('{') {
        return Err(TemplateError::NotAnchored);
    }
    // ...and specifically on a *directory* variable. `{TITLE}/saves` names no
    // anchor and would be interpreted relative to the process's working directory.
    let anchored = Anchor::ALL_LONGEST_FIRST
        .iter()
        .any(|a| t.starts_with(&format!("{{{}}}", a.variable())))
        || t.starts_with("{INSTALL}");
    if !anchored {
        return Err(TemplateError::NotAnchored);
    }

    Ok(())
}

fn is_known_variable(var: &str) -> bool {
    if var == WILDCARD || var == "{INSTALL}" || var == "{TITLE}" {
        return true;
    }
    if GAME_VARS.contains(&var) {
        return true;
    }
    Anchor::ALL_LONGEST_FIRST
        .iter()
        .any(|a| var == format!("{{{}}}", a.variable()))
}

/// Expand a template into every concrete path it names on this filesystem.
///
/// Returns an empty vector — never an error — when the template cannot apply here:
/// an anchor this machine lacks, a variable the game does not supply (no publisher
/// known), or a wildcard with nothing to match. "Does not apply" is the normal case
/// for most entries and is not a failure.
///
/// Assumes `template` has already passed [`validate`]; an invalid template yields
/// nothing rather than an unsafe path.
pub fn expand(fs: &dyn FileSystem, template: &str, vars: &TemplateVars) -> Vec<PathBuf> {
    if validate(template).is_err() {
        return Vec::new();
    }

    // Anchor first: the leading variable becomes a real directory, everything after
    // it stays relative. This ordering is what makes escape impossible — the
    // remainder is only ever joined onto an anchor.
    let Some((anchor_path, remainder)) = split_anchor(fs, template, vars) else {
        return Vec::new();
    };

    let mut out = remainder;
    for (var, value) in [
        ("{TITLE}", Some(vars.title)),
        ("{STEAM_USERID}", vars.steam_userid),
        ("{STEAM_APPID}", vars.steam_appid),
        ("{PUBLISHER}", vars.publisher),
        ("{DEVELOPER}", vars.developer),
    ] {
        if out.contains(var) {
            match value {
                // A value containing a separator would smuggle extra segments into
                // the path — a game titled `../../x` must not become traversal.
                Some(v) if !v.contains('/') && !v.contains('\\') && v != ".." => {
                    out = out.replace(var, v)
                }
                _ => return Vec::new(),
            }
        }
    }

    // Fan out on the wildcard, then join. Nothing after this point can introduce a
    // new variable, so the result is final.
    expand_wildcard(fs, &anchor_path, &out)
}

/// Resolve the leading anchor variable to a real directory, returning it with the
/// still-relative remainder.
fn split_anchor(
    fs: &dyn FileSystem,
    template: &str,
    vars: &TemplateVars,
) -> Option<(PathBuf, String)> {
    if let Some(rest) = template.strip_prefix("{INSTALL}") {
        let dir = vars.install_dir?;
        return Some((PathBuf::from(dir), trim_separator(rest)));
    }
    for anchor in Anchor::ALL_LONGEST_FIRST {
        let token = format!("{{{}}}", anchor.variable());
        if let Some(rest) = template.strip_prefix(&token) {
            return Some((fs.anchor(anchor)?, trim_separator(rest)));
        }
    }
    None
}

fn trim_separator(s: &str) -> String {
    s.trim_start_matches(['/', '\\']).to_string()
}

/// Join `relative` onto `base`, fanning out one level where a wildcard appears.
fn expand_wildcard(fs: &dyn FileSystem, base: &Path, relative: &str) -> Vec<PathBuf> {
    let Some(index) = relative.find(WILDCARD) else {
        return vec![join_segments(base, relative)];
    };

    let before = &relative[..index];
    let after = trim_separator(&relative[index + WILDCARD.len()..]);
    let parent = join_segments(base, before);

    // One segment, one listing. Directories only: a wildcard stands in for an
    // account-id folder, never a file.
    let Ok(entries) = fs.read_dir(&parent) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|e| e.is_dir)
        .flat_map(|e| expand_wildcard(fs, &parent.join(e.name), &after))
        .collect()
}

/// Join a relative remainder segment-by-segment.
///
/// Deliberately not `PathBuf::join(str)`: joining a string that happens to be
/// absolute would *replace* the base rather than extend it. Validation already
/// rejects that shape, but this makes escape impossible by construction rather
/// than by an earlier check having run.
fn join_segments(base: &Path, relative: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for segment in relative.split(['/', '\\']).filter(|s| !s.is_empty()) {
        // Unreachable after validation; belt and braces at the point of use.
        // A colon is a drive prefix or an alternate-data-stream marker, and
        // `push` on a segment carrying one discards everything before it.
        if segment == ".." || segment.contains(':') {
            return out;
        }
        out.push(segment);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::fs::RootKind;
    use crate::test_support::VirtualFs;

    const HOME: &str = "C:/Users/test";

    fn world() -> VirtualFs {
        VirtualFs::new()
            .with_root(RootKind::AppDataRoaming, &format!("{HOME}/AppData/Roaming"))
            .with_root(RootKind::AppDataLocal, &format!("{HOME}/AppData/Local"))
            .with_root(RootKind::DocumentsMyGames, &format!("{HOME}/Documents/My Games"))
            .with_root(RootKind::Documents, &format!("{HOME}/Documents"))
            .with_root(RootKind::SavedGames, &format!("{HOME}/Saved Games"))
            .with_anchor(Anchor::UserProfile, HOME)
    }

    fn vars<'a>(title: &'a str) -> TemplateVars<'a> {
        TemplateVars {
            title,
            ..Default::default()
        }
    }

    // ── Validation: the security boundary ─────────────────────────────────

    #[test]
    fn a_documented_template_is_accepted() {
        assert!(validate("{APPDATA}/{PUBLISHER}/{TITLE}").is_ok());
        assert!(validate("{MYGAMES}/{TITLE}/Saves").is_ok());
        assert!(validate("{INSTALL}/saves").is_ok());
        assert!(validate("{LOCALAPPDATA}/Foo/Saved/SaveGames").is_ok());
        assert!(validate("{USERPROFILE}/Saved Games/Foo").is_ok());
    }

    #[test]
    fn an_absolute_path_is_refused() {
        for bad in [
            "C:/Users/harsh/AppData/Roaming/Foo",
            "/home/user/.local/share/Foo",
            "D:\\Games\\Saves",
        ] {
            assert_eq!(
                validate(bad),
                Err(TemplateError::NotAnchored),
                "should refuse absolute path: {bad}"
            );
        }
    }

    /// Regression: a drive prefix in a *later* segment used to slip through.
    ///
    /// `{DOCUMENTS}/C:/Windows` satisfies the anchor rule — it really does start with
    /// a directory variable — but `PathBuf::push("C:")` replaces the whole path on
    /// Windows instead of extending it, so this expanded to `C:Windows` and the
    /// anchor was gone. Both `validate` and `join_segments` now refuse a colon in any
    /// segment.
    #[test]
    fn a_drive_prefix_in_a_later_segment_is_refused() {
        for bad in [
            "{DOCUMENTS}/C:/Windows",
            "{APPDATA}/sub/D:/other",
            "{MYGAMES}/{TITLE}/C:",
            // Alternate data stream on an otherwise innocent name.
            "{APPDATA}/saves:hidden",
        ] {
            assert_eq!(
                validate(bad),
                Err(TemplateError::NotAnchored),
                "should refuse `{bad}`"
            );
        }
    }

    /// Even if such a template reached expansion, it must not escape.
    #[test]
    fn expansion_refuses_a_drive_prefix_segment() {
        let fs = VirtualFs::new()
            .with_root(RootKind::Documents, "C:/Users/test/Documents")
            .with_dir("C:/Users/test/Documents");

        let vars = TemplateVars {
            title: "X",
            publisher: None,
            developer: None,
            steam_appid: None,
            steam_userid: None,
            install_dir: None,
        };
        for path in expand(&fs, "{DOCUMENTS}/C:/Windows", &vars) {
            let text = path.to_string_lossy().replace('\\', "/");
            assert!(
                text.starts_with("C:/Users/test/Documents"),
                "expansion escaped the anchor: {text}"
            );
        }
    }

    #[test]
    fn a_unc_path_is_refused() {
        assert_eq!(
            validate("\\\\server\\share\\saves"),
            Err(TemplateError::NotAnchored)
        );
    }

    #[test]
    fn parent_traversal_is_refused() {
        for bad in [
            "{APPDATA}/../../Windows/System32",
            "{MYGAMES}/{TITLE}/../../..",
            "{APPDATA}/..",
        ] {
            assert_eq!(
                validate(bad),
                Err(TemplateError::ParentTraversal),
                "should refuse traversal: {bad}"
            );
        }
    }

    #[test]
    fn a_double_dot_inside_a_segment_is_not_traversal() {
        // `..` only matters as a whole segment. A directory legitimately named
        // `Game..Extra` is not an escape attempt.
        assert!(validate("{APPDATA}/Game..Extra/Saves").is_ok());
    }

    #[test]
    fn an_unknown_variable_is_refused() {
        assert_eq!(
            validate("{NONSENSE}/x"),
            Err(TemplateError::UnknownVariable("{NONSENSE}".into()))
        );
        assert_eq!(
            validate("{APPDATA}/{EXPLOIT}/x"),
            Err(TemplateError::UnknownVariable("{EXPLOIT}".into()))
        );
    }

    #[test]
    fn an_unterminated_variable_is_refused() {
        assert!(matches!(
            validate("{APPDATA}/{TITLE"),
            Err(TemplateError::UnknownVariable(_))
        ));
    }

    #[test]
    fn a_template_must_be_anchored_on_a_directory() {
        // A game-derived variable is not an anchor: this would resolve relative to
        // the process working directory.
        assert_eq!(validate("{TITLE}/saves"), Err(TemplateError::NotAnchored));
        assert_eq!(validate("{PUBLISHER}/{TITLE}"), Err(TemplateError::NotAnchored));
    }

    #[test]
    fn an_empty_template_is_refused() {
        assert_eq!(validate(""), Err(TemplateError::Empty));
        assert_eq!(validate("   "), Err(TemplateError::Empty));
    }

    // ── Expansion ─────────────────────────────────────────────────────────

    #[test]
    fn a_simple_template_expands_to_one_path() {
        let fs = world();
        let got = expand(&fs, "{MYGAMES}/{TITLE}", &vars("Hollow Knight"));
        assert_eq!(
            got,
            vec![PathBuf::from(format!("{HOME}/Documents/My Games")).join("Hollow Knight")]
        );
    }

    #[test]
    fn nested_segments_are_joined_one_at_a_time() {
        let fs = world();
        let got = expand(&fs, "{LOCALAPPDATA}/Foo/Saved/SaveGames", &vars("Foo"));
        let expected = PathBuf::from(format!("{HOME}/AppData/Local"))
            .join("Foo")
            .join("Saved")
            .join("SaveGames");
        assert_eq!(got, vec![expected]);
    }

    #[test]
    fn a_publisher_template_needs_a_publisher() {
        let fs = world();
        assert!(
            expand(&fs, "{APPDATA}/{PUBLISHER}/{TITLE}", &vars("Hollow Knight")).is_empty(),
            "no publisher known, so the entry does not apply"
        );

        let with_publisher = TemplateVars {
            title: "Hollow Knight",
            publisher: Some("Team Cherry"),
            ..Default::default()
        };
        let got = expand(&fs, "{APPDATA}/{PUBLISHER}/{TITLE}", &with_publisher);
        assert_eq!(
            got,
            vec![PathBuf::from(format!("{HOME}/AppData/Roaming"))
                .join("Team Cherry")
                .join("Hollow Knight")]
        );
    }

    #[test]
    fn an_anchor_this_machine_lacks_yields_nothing() {
        // No LocalLow root declared in `world()`.
        let fs = world();
        assert!(expand(&fs, "{LOCALLOW}/Foo", &vars("Foo")).is_empty());
    }

    #[test]
    fn an_install_template_needs_an_install_dir() {
        let fs = world();
        assert!(expand(&fs, "{INSTALL}/saves", &vars("Foo")).is_empty());

        let installed = TemplateVars {
            title: "Foo",
            install_dir: Some("D:/Games/Foo"),
            ..Default::default()
        };
        assert_eq!(
            expand(&fs, "{INSTALL}/saves", &installed),
            vec![PathBuf::from("D:/Games/Foo").join("saves")]
        );
    }

    /// A game title is data too. One containing a separator must not smuggle extra
    /// path segments into an otherwise-safe template.
    #[test]
    fn a_title_containing_a_separator_is_refused_at_expansion() {
        let fs = world();
        assert!(expand(&fs, "{MYGAMES}/{TITLE}", &vars("../../Windows")).is_empty());
        assert!(expand(&fs, "{MYGAMES}/{TITLE}", &vars("a/b")).is_empty());
        assert!(expand(&fs, "{MYGAMES}/{TITLE}", &vars("..")).is_empty());
    }

    #[test]
    fn an_invalid_template_expands_to_nothing_rather_than_an_unsafe_path() {
        let fs = world();
        // Even if validation were somehow skipped upstream, expansion refuses.
        assert!(expand(&fs, "C:/Windows/System32", &vars("Foo")).is_empty());
        assert!(expand(&fs, "{APPDATA}/../../../Windows", &vars("Foo")).is_empty());
    }

    // ── Wildcards ─────────────────────────────────────────────────────────

    #[test]
    fn a_wildcard_fans_out_over_one_directory_level() {
        let base = format!("{HOME}/AppData/Roaming/EldenRing");
        let fs = world()
            .with_dir(&base)
            .with_dir(&format!("{base}/76561198000000001"))
            .with_dir(&format!("{base}/76561198000000002"));

        let mut got = expand(&fs, "{APPDATA}/EldenRing/{WILDCARD}", &vars("Elden Ring"));
        got.sort();
        assert_eq!(
            got,
            vec![
                PathBuf::from(&base).join("76561198000000001"),
                PathBuf::from(&base).join("76561198000000002"),
            ]
        );
    }

    #[test]
    fn a_wildcard_continues_into_the_rest_of_the_template() {
        let base = format!("{HOME}/AppData/Roaming/Steam/userdata");
        let fs = world()
            .with_dir(&base)
            .with_dir(&format!("{base}/12345"))
            .with_dir(&format!("{base}/12345/367520"))
            .with_dir(&format!("{base}/12345/367520/remote"));

        let got = expand(
            &fs,
            "{APPDATA}/Steam/userdata/{WILDCARD}/367520/remote",
            &vars("Hollow Knight"),
        );
        assert_eq!(got, vec![PathBuf::from(&base).join("12345").join("367520").join("remote")]);
    }

    #[test]
    fn a_wildcard_ignores_files_and_missing_parents() {
        let base = format!("{HOME}/AppData/Roaming/Game");
        let fs = world()
            .with_dir(&base)
            .with_file(&format!("{base}/readme.txt"), 100)
            .with_dir(&format!("{base}/profile1"));

        let got = expand(&fs, "{APPDATA}/Game/{WILDCARD}", &vars("Game"));
        assert_eq!(got, vec![PathBuf::from(&base).join("profile1")]);

        // Nothing to list — not an error, just no candidates.
        assert!(expand(&fs, "{APPDATA}/Absent/{WILDCARD}", &vars("Game")).is_empty());
    }

    #[test]
    fn expansion_never_leaves_its_anchor() {
        // The property that matters most: whatever the template says, every result
        // is beneath the anchor it named.
        //
        // Counts what it checked, because a loop over results passes vacuously if
        // expansion returns nothing — which is exactly how a security test stops
        // testing anything.
        let fs = world();
        let anchor = format!("{HOME}/Documents/My Games");
        let mut examined = 0;
        for template in [
            "{MYGAMES}/{TITLE}",
            "{MYGAMES}/{TITLE}/Saves/Profile",
            "{MYGAMES}/Game..Extra/x",
        ] {
            for path in expand(&fs, template, &vars("Some Game")) {
                let p = path.to_string_lossy().replace('\\', "/");
                assert!(p.starts_with(&anchor), "escaped anchor: {p} from {template}");
                examined += 1;
            }
        }
        assert_eq!(examined, 3, "expected one path per template — the test must not pass vacuously");
    }
}
