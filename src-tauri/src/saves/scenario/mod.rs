//! The declarative scenario format for detection tests.
//!
//! Per ADR-0013, detection tests are **data, not Rust**: a fixture declares a world
//! and an expectation, and one runner walks them all. Adding a game means adding a
//! `.toml` file, which is what makes a corpus of hundreds realistic where a Rust
//! module per game would stall at dozens.
//!
//! The format is versioned from the first commit. An unknown `version` is a loud
//! error rather than a silent misread — a fixture written for a later format must
//! not be quietly reinterpreted by an earlier runner.
//!
//! Paths use a closed set of variables (§3 of the test plan) so a fixture is
//! portable and cannot express a real absolute path on the developer's machine.

use serde::Deserialize;

/// The only format version this runner understands.
pub const FORMAT_VERSION: u32 = 1;

mod runner;

/// Synthetic home directory every fixture variable expands beneath.
///
/// Deliberately not a real path: a fixture must not be able to name something on
/// the machine running the test.
pub const SYNTHETIC_HOME: &str = "C:/Users/test";

#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub version: u32,
    #[serde(rename = "scenario")]
    pub meta: Meta,
    pub game: GameFixture,
    /// The filesystem as it exists in this world.
    #[serde(default)]
    pub fs: Vec<FsFixture>,
    /// Knowledge-base entries available to detection.
    #[serde(default)]
    pub kb: Vec<KbFixture>,
    /// Play sessions. **Phase 2** — parsed so a Phase 2 fixture is recognised and
    /// rejected with a clear message rather than silently ignored.
    #[serde(default)]
    pub sessions: Vec<SessionFixture>,
    pub expect: Expect,
}

#[derive(Debug, Deserialize)]
pub struct Meta {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Marks a fixture that encodes *intended* behaviour not yet implemented.
    ///
    /// The test plan's workflow is to merge a mis-detection case **failing**, before
    /// the fix, so the bug becomes a permanent guarantee rather than a one-off patch.
    /// This is that mechanism, and it is deliberately two-sided: the runner skips the
    /// assertions, but **fails if a pending scenario starts passing**, so the marker
    /// cannot quietly outlive the work it was waiting for.
    ///
    /// The value names what it is waiting on, e.g. `"task 1.17 (verifier)"`.
    #[serde(default)]
    pub pending: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GameFixture {
    pub title: String,
    #[serde(default)]
    pub steam_appid: Option<String>,
    #[serde(default)]
    pub gog_id: Option<String>,
    #[serde(default)]
    pub epic_id: Option<String>,
    #[serde(default)]
    pub exe_name: Option<String>,
    #[serde(default)]
    pub install_dir: Option<String>,
    #[serde(default)]
    pub developer: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    /// RFC3339. Feeds the verifier's mtime-correlation signal.
    #[serde(default)]
    pub last_played_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FsFixture {
    /// A directory. Declared explicitly — ancestors are not implied, so a fixture
    /// says exactly what exists.
    pub path: String,
    #[serde(default)]
    pub files: Vec<FileFixture>,
}

#[derive(Debug, Deserialize)]
pub struct FileFixture {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    /// RFC3339. Optional: most cases do not care.
    #[serde(default)]
    pub mtime: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct KbFixture {
    #[serde(default = "default_layer")]
    pub layer: String,
    pub match_kind: String,
    #[serde(default)]
    pub match_value: String,
    #[serde(default = "default_role")]
    pub role: String,
    /// What kind of location this describes. Defaults to official, because a fixture
    /// naming a specific game is asserting that game's real save location; a fixture
    /// testing a convention or community layout states it explicitly.
    #[serde(default = "default_layout")]
    pub layout: String,
    pub path_template: String,
    #[serde(default)]
    pub note: Option<String>,
}

fn default_layer() -> String {
    "builtin".to_string()
}
fn default_role() -> String {
    "saves".to_string()
}
fn default_layout() -> String {
    crate::saves::kb::layout::OFFICIAL.to_string()
}

#[derive(Debug, Deserialize)]
pub struct SessionFixture {
    pub started_at: String,
    pub ended_at: String,
    #[serde(default)]
    pub writes: Vec<String>,
}

/// What detection is expected to conclude.
///
/// Phase 1 records decisions without acting on them, so the strongest outcome
/// expressible here is `bind_eligible` — the candidate the decision table *would*
/// bind. `binding` and `locked` arrive with the binding store in Phase 3.
#[derive(Debug, Deserialize)]
pub struct Expect {
    /// The path the decision table marks bind-eligible, if any.
    #[serde(default)]
    pub bind_eligible: Option<String>,
    /// Which decision-table row fired for `bind_eligible`. Asserted because two
    /// rules reaching the same outcome is a behaviour change worth catching.
    #[serde(default)]
    pub rule: Option<u8>,
    /// Suggested candidates, in the order they should be offered.
    #[serde(default)]
    pub suggested: Vec<String>,
    /// Paths that must not appear as candidates at all. Most detection bugs are
    /// extra candidates, not missing ones, so this is first-class.
    #[serde(default)]
    pub must_not_include: Vec<String>,
    /// Substring the explanation must contain (invariant I9 asserts non-empty).
    #[serde(default)]
    pub explanation_contains: Option<String>,
}

/// Why a fixture could not be used.
#[derive(Debug)]
pub enum ScenarioError {
    Parse { file: String, message: String },
    UnsupportedVersion { file: String, found: u32 },
    NotYetSupported { file: String, feature: &'static str, phase: &'static str },
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioError::Parse { file, message } => {
                write!(f, "{file}: could not parse scenario: {message}")
            }
            ScenarioError::UnsupportedVersion { file, found } => write!(
                f,
                "{file}: scenario format version {found} is not supported by this runner \
                 (expected {FORMAT_VERSION}). Refusing to guess at its meaning."
            ),
            ScenarioError::NotYetSupported { file, feature, phase } => write!(
                f,
                "{file}: uses `{feature}`, which is {phase} work. The fixture is valid \
                 but nothing produces that evidence yet, so it cannot pass."
            ),
        }
    }
}

impl Scenario {
    /// Parse and validate a fixture. `file` is used only for error messages.
    pub fn parse(file: &str, raw: &str) -> Result<Self, ScenarioError> {
        let scenario: Scenario = toml::from_str(raw).map_err(|e| ScenarioError::Parse {
            file: file.to_string(),
            message: e.to_string(),
        })?;

        if scenario.version != FORMAT_VERSION {
            return Err(ScenarioError::UnsupportedVersion {
                file: file.to_string(),
                found: scenario.version,
            });
        }
        if !scenario.sessions.is_empty() {
            return Err(ScenarioError::NotYetSupported {
                file: file.to_string(),
                feature: "[[sessions]]",
                phase: "Phase 2 (Write Witness)",
            });
        }
        Ok(scenario)
    }

    /// Expand the closed variable set in a fixture path.
    ///
    /// `{INSTALL}` resolves from `[game].install_dir`, which may itself contain
    /// variables (`install_dir = "{DRIVE}/Games/HK"` is the common form), so it is
    /// expanded first. If a variable a path needs is not declared, the path is
    /// unresolvable and the caller should skip it rather than emit a literal brace
    /// into a filesystem path.
    pub fn expand(&self, path: &str) -> Option<String> {
        let install = self
            .game
            .install_dir
            .as_deref()
            .map(|d| self.expand_fixed(d));

        let mut out = self.expand_fixed(path);

        // Optional substitutions: an unresolvable variable makes the whole path
        // unusable rather than producing a literal brace in a filesystem path.
        for (var, value) in [
            ("{INSTALL}", install.as_deref()),
            ("{DEVELOPER}", self.game.developer.as_deref()),
            ("{PUBLISHER}", self.game.publisher.as_deref()),
            ("{STEAM_APPID}", self.game.steam_appid.as_deref()),
        ] {
            if out.contains(var) {
                match value {
                    Some(v) => out = out.replace(var, v),
                    None => return None,
                }
            }
        }

        // Anything still braced is an unknown variable — refuse rather than guess.
        if out.contains('{') {
            return None;
        }
        Some(out.replace('\\', "/"))
    }

    /// Substitute the variables that are always resolvable.
    ///
    /// `{MYGAMES}` is replaced before `{DOCUMENTS}` even though one is a prefix of
    /// the other's expansion — the ordering here is what makes that safe.
    fn expand_fixed(&self, path: &str) -> String {
        let mut out = path.to_string();
        for (var, value) in [
            ("{APPDATA}", format!("{SYNTHETIC_HOME}/AppData/Roaming")),
            ("{LOCALAPPDATA}", format!("{SYNTHETIC_HOME}/AppData/Local")),
            ("{LOCALLOW}", format!("{SYNTHETIC_HOME}/AppData/LocalLow")),
            ("{MYGAMES}", format!("{SYNTHETIC_HOME}/Documents/My Games")),
            ("{DOCUMENTS}", format!("{SYNTHETIC_HOME}/Documents")),
            ("{SAVEDGAMES}", format!("{SYNTHETIC_HOME}/Saved Games")),
            ("{USERPROFILE}", SYNTHETIC_HOME.to_string()),
            ("{PUBLIC}", "C:/Users/Public".to_string()),
            ("{DRIVE}", "C:".to_string()),
            ("{TITLE}", self.game.title.clone()),
        ] {
            out = out.replace(var, &value);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
version = 1
[scenario]
id = "t"
title = "T"
[game]
title = "Hollow Knight"
[expect]
suggested = []
"#;

    #[test]
    fn a_minimal_scenario_parses() {
        let s = Scenario::parse("t.toml", MINIMAL).expect("should parse");
        assert_eq!(s.meta.id, "t");
        assert_eq!(s.game.title, "Hollow Knight");
        assert!(s.fs.is_empty());
        assert!(s.kb.is_empty());
    }

    #[test]
    fn an_unknown_version_is_refused_loudly() {
        let raw = MINIMAL.replace("version = 1", "version = 99");
        let err = Scenario::parse("future.toml", &raw).expect_err("should refuse");
        let msg = err.to_string();
        assert!(msg.contains("future.toml"), "error must name the file: {msg}");
        assert!(msg.contains("99"), "error must name the version: {msg}");
    }

    /// A Phase 2 fixture must be recognised and rejected, not silently ignored —
    /// otherwise it would appear to pass while testing nothing.
    #[test]
    fn a_sessions_block_is_rejected_as_phase_2() {
        let raw = format!(
            "{MINIMAL}\n[[sessions]]\nstarted_at = \"2026-01-01T20:00:00Z\"\nended_at = \"2026-01-01T22:00:00Z\"\nwrites = [\"{{APPDATA}}/X\"]\n"
        );
        let err = Scenario::parse("witness.toml", &raw).expect_err("should refuse");
        let msg = err.to_string();
        assert!(msg.contains("sessions"), "{msg}");
        assert!(msg.contains("Phase 2"), "{msg}");
    }

    #[test]
    fn a_malformed_fixture_names_the_file() {
        let err = Scenario::parse("broken.toml", "version = ").expect_err("should refuse");
        assert!(err.to_string().contains("broken.toml"));
    }

    #[test]
    fn root_variables_expand_beneath_the_synthetic_home() {
        let s = Scenario::parse("t.toml", MINIMAL).unwrap();
        assert_eq!(
            s.expand("{MYGAMES}/Hollow Knight").as_deref(),
            Some("C:/Users/test/Documents/My Games/Hollow Knight")
        );
        assert_eq!(
            s.expand("{APPDATA}/Team Cherry").as_deref(),
            Some("C:/Users/test/AppData/Roaming/Team Cherry")
        );
        // MYGAMES must win over DOCUMENTS despite sharing a prefix.
        assert_eq!(
            s.expand("{DOCUMENTS}/Other").as_deref(),
            Some("C:/Users/test/Documents/Other")
        );
    }

    #[test]
    fn title_expands_from_the_game_block() {
        let s = Scenario::parse("t.toml", MINIMAL).unwrap();
        assert_eq!(
            s.expand("{MYGAMES}/{TITLE}").as_deref(),
            Some("C:/Users/test/Documents/My Games/Hollow Knight")
        );
    }

    #[test]
    fn an_unresolvable_variable_yields_no_path() {
        let s = Scenario::parse("t.toml", MINIMAL).unwrap();
        // No publisher declared, so a publisher-shaped template is unusable.
        assert_eq!(s.expand("{APPDATA}/{PUBLISHER}/{TITLE}"), None);
        // No install_dir declared.
        assert_eq!(s.expand("{INSTALL}/saves"), None);
    }

    #[test]
    fn an_unknown_variable_yields_no_path() {
        let s = Scenario::parse("t.toml", MINIMAL).unwrap();
        assert_eq!(s.expand("{NONSENSE}/x"), None);
    }

    #[test]
    fn optional_variables_expand_when_declared() {
        let raw = MINIMAL.replace(
            "title = \"Hollow Knight\"",
            "title = \"Hollow Knight\"\npublisher = \"Team Cherry\"\ninstall_dir = \"{DRIVE}/Games/HK\"",
        );
        let s = Scenario::parse("t.toml", &raw).unwrap();
        assert_eq!(
            s.expand("{APPDATA}/{PUBLISHER}/{TITLE}").as_deref(),
            Some("C:/Users/test/AppData/Roaming/Team Cherry/Hollow Knight")
        );
        // install_dir itself contains a variable, so it expands in two steps.
        assert_eq!(s.expand("{INSTALL}/saves").as_deref(), Some("C:/Games/HK/saves"));
    }

    #[test]
    fn kb_fixtures_default_to_the_builtin_saves_layer() {
        let raw = format!(
            "{MINIMAL}\n[[kb]]\nmatch_kind = \"steam_appid\"\nmatch_value = \"367520\"\npath_template = \"{{MYGAMES}}/{{TITLE}}\"\n"
        );
        let s = Scenario::parse("t.toml", &raw).unwrap();
        assert_eq!(s.kb.len(), 1);
        assert_eq!(s.kb[0].layer, "builtin");
        assert_eq!(s.kb[0].role, "saves");
    }
}
