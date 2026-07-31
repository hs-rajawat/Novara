//! The scenario runner.
//!
//! One test walks every `.toml` under `scenarios/`, builds a [`VirtualFs`] from the
//! fixture, runs the detection pipeline, and asserts the fixture's `[expect]`
//! block. Adding a case is adding a file (ADR-0013).
//!
//! Directory names are categories, so `cargo test scenarios::negative` is a
//! meaningful selector — each category gets its own `#[test]` below.
//!
//! Failure output names the scenario file and prints what was found, because a
//! table-driven failure that only says "assertion failed" is unactionable when
//! there are hundreds of cases.

use std::path::{Path, PathBuf};

use super::{Scenario, ScenarioError, SYNTHETIC_HOME};
use crate::saves::fs::RootKind;
use crate::saves::pipeline::{self, GameContext};
use crate::test_support::VirtualFs;

/// Where fixtures live, relative to the crate root.
fn scenario_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios")
}

/// Every `.toml` beneath `dir`, sorted so failures are reported deterministically.
fn fixtures_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if !dir.is_dir() {
        return found;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "toml") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Build the world a fixture declares.
///
/// The six well-known roots are always registered, so a scenario only has to
/// declare the directories that matter to it rather than restate the OS layout.
fn world(scenario: &Scenario) -> VirtualFs {
    let mut fs = VirtualFs::new()
        .with_root(RootKind::AppDataRoaming, &format!("{SYNTHETIC_HOME}/AppData/Roaming"))
        .with_root(RootKind::AppDataLocalLow, &format!("{SYNTHETIC_HOME}/AppData/LocalLow"))
        .with_root(RootKind::AppDataLocal, &format!("{SYNTHETIC_HOME}/AppData/Local"))
        .with_root(RootKind::DocumentsMyGames, &format!("{SYNTHETIC_HOME}/Documents/My Games"))
        .with_root(RootKind::Documents, &format!("{SYNTHETIC_HOME}/Documents"))
        .with_root(RootKind::SavedGames, &format!("{SYNTHETIC_HOME}/Saved Games"));

    for entry in &scenario.fs {
        let Some(dir) = scenario.expand(&entry.path) else {
            panic!(
                "{}: [[fs]] path `{}` uses a variable the [game] block does not declare",
                scenario.meta.id, entry.path
            );
        };
        fs = fs.with_dir(&dir);
        for file in &entry.files {
            fs = fs.with_file(&format!("{dir}/{}", file.name), file.size.max(1));
        }
    }
    fs
}

fn context(scenario: &Scenario) -> GameContext {
    GameContext {
        title: scenario.game.title.clone(),
        steam_appid: scenario.game.steam_appid.clone(),
        gog_id: scenario.game.gog_id.clone(),
        epic_id: scenario.game.epic_id.clone(),
        exe_name: scenario.game.exe_name.clone(),
        install_dir: scenario
            .game
            .install_dir
            .as_deref()
            .and_then(|d| scenario.expand(d)),
        developer: scenario.game.developer.clone(),
        publisher: scenario.game.publisher.clone(),
        last_played_at: scenario.game.last_played_at.clone(),
    }
}

/// Turn a fixture's `[[kb]]` blocks into stored entries.
///
/// Task 1.22 activated these. Until then the blocks parsed but were inert: the runner
/// held no database and `pipeline::detect` did not consult the knowledge base, so a KB
/// fixture would have looked like coverage while asserting nothing.
///
/// Written through `Db::replace_kb_layer` per layer rather than by direct insert, so the
/// fixtures exercise the same path a real KB load takes — including the layer-scoped
/// delete that protects invariant I7.
async fn seed_kb(db: &crate::db::Db, scenario: &Scenario) -> Result<(), String> {
    use crate::db::save_kb::NewKbEntry;
    use std::collections::BTreeMap;

    let mut by_layer: BTreeMap<String, Vec<NewKbEntry>> = BTreeMap::new();
    for (index, k) in scenario.kb.iter().enumerate() {
        let entry = NewKbEntry {
            id: format!("{}:{index}", scenario.meta.id),
            match_kind: k.match_kind.clone(),
            match_value: k.match_value.clone(),
            platform: "windows".into(),
            role: k.role.clone(),
            path_template: k.path_template.clone(),
            glob: None,
            // Fixture entries are curated by definition; a fixture that wants to test a
            // convention rule says so with `match_kind = "any"`.
            priority: 10,
            note: k.note.clone(),
            source_ref: Some("scenario".into()),
        };
        // The same gate the shipped corpus passes through, so a fixture cannot smuggle
        // in a template the real KB would refuse.
        crate::saves::kb::validate::validate_entry(&k.layer, &entry)
            .map_err(|e| format!("[[kb]] entry {index} is invalid: {e}"))?;
        by_layer.entry(k.layer.clone()).or_default().push(entry);
    }

    for (layer, entries) in by_layer {
        db.replace_kb_layer(&layer, "scenario", &format!("scenario-{layer}"), None, &entries)
            .await
            .map_err(|e| format!("could not seed the `{layer}` KB layer: {e}"))?;
    }
    Ok(())
}

/// Normalise for comparison: fixtures are written with `/`, `Path::join` produces
/// `\` on Windows.
fn norm(p: &str) -> String {
    p.replace('\\', "/")
}

/// Run one fixture, returning a description of the failure if it did not hold.
async fn check(file: &Path, raw: &str) -> Result<(), String> {
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let scenario = match Scenario::parse(&name, raw) {
        Ok(s) => s,
        Err(ScenarioError::NotYetSupported { .. }) => return Ok(()), // skipped, see below
        Err(e) => return Err(e.to_string()),
    };

    let outcome = assess(&scenario, &name).await;

    // A pending fixture encodes intended behaviour that is not built yet. Skipping
    // it is only safe if we also notice when it starts passing — otherwise the
    // marker outlives the work and the case silently stops being a test.
    match (&scenario.meta.pending, &outcome) {
        (Some(_), Err(_)) => Ok(()),
        (Some(waiting_on), Ok(())) => Err(format!(
            "{name} is marked `pending = \"{waiting_on}\"` but now passes.\n  \
             Remove the marker — the behaviour it was waiting for exists."
        )),
        (None, result) => result.clone(),
    }
}

/// Assert a fixture's `[expect]` block against a real detection run.
async fn assess(scenario: &Scenario, name: &str) -> Result<(), String> {
    let fs = world(scenario);
    let db = crate::test_support::test_db().await;
    seed_kb(&db, scenario).await?;

    let outcome = match pipeline::detect_with_kb(&db, &fs, &context(scenario)).await {
        Ok(o) => o,
        Err(e) => return Err(format!("{name}: detection failed: {e}")),
    };
    let found: Vec<String> = outcome.candidates.iter().map(|c| norm(&c.path)).collect();

    let report = |msg: String| -> String {
        let decisions: Vec<String> = outcome
            .assessed
            .iter()
            .map(|a| {
                format!(
                    "{} -> rule {} {} [{}]",
                    norm(&a.path),
                    a.decision.rule,
                    a.decision.outcome.status(),
                    a.evidence.explain().join(", ")
                )
            })
            .collect();
        format!(
            "{name} ({})\n  {msg}\n  found: {found:#?}\n  decisions:\n    {}",
            scenario.meta.title,
            decisions.join("\n    ")
        )
    };

    // Paths that must not appear. Checked first: most detection bugs are extra
    // candidates, not missing ones.
    for forbidden in &scenario.expect.must_not_include {
        let Some(expanded) = scenario.expand(forbidden) else {
            return Err(report(format!(
                "must_not_include `{forbidden}` uses an undeclared variable"
            )));
        };
        if found.contains(&expanded) {
            return Err(report(format!("`{expanded}` should not have been offered")));
        }
    }

    // Suggested candidates, in order.
    if !scenario.expect.suggested.is_empty() {
        let mut expected = Vec::new();
        for s in &scenario.expect.suggested {
            match scenario.expand(s) {
                Some(e) => expected.push(e),
                None => {
                    return Err(report(format!("suggested `{s}` uses an undeclared variable")))
                }
            }
        }
        if found != expected {
            return Err(report(format!("expected suggestions {expected:#?}")));
        }
    } else if scenario.expect.bind_eligible.is_none()
        && scenario.expect.must_not_include.is_empty()
        && !found.is_empty()
    {
        // An `[expect]` with nothing in it means "find nothing".
        return Err(report("expected no candidates".to_string()));
    }

    // `bind_eligible` is now asserted against the decision table's actual outcome
    // rather than against "is the leading candidate". Task 1.20 made the status
    // assertable; before that this could only check ordering.
    if let Some(expected) = &scenario.expect.bind_eligible {
        let Some(expanded) = scenario.expand(expected) else {
            return Err(report(format!(
                "bind_eligible `{expected}` uses an undeclared variable"
            )));
        };
        let eligible: Vec<String> = outcome
            .bind_eligible()
            .map(|a| norm(&a.path))
            .collect();
        if !eligible.contains(&expanded) {
            return Err(report(format!(
                "expected `{expanded}` to be bind-eligible, but only {eligible:#?} were"
            )));
        }
    }

    // Which row fired. Asserted because two rules reaching the same outcome is a
    // behaviour change worth catching — the outcome alone would hide it.
    if let Some(expected_rule) = scenario.expect.rule {
        let leader = outcome
            .leader()
            .ok_or_else(|| report(format!("expected rule {expected_rule}, but nothing was offered")))?;
        if leader.decision.rule != expected_rule {
            return Err(report(format!(
                "expected rule {expected_rule}, got rule {}",
                leader.decision.rule
            )));
        }
    }

    if let Some(fragment) = &scenario.expect.explanation_contains {
        let explanations: Vec<&str> = outcome
            .assessed
            .iter()
            .map(|a| a.decision.explanation.as_str())
            .collect();
        if !explanations.iter().any(|e| e.contains(fragment.as_str())) {
            return Err(report(format!(
                "no explanation contained `{fragment}`; got {explanations:#?}"
            )));
        }
    }

    // Invariant I9: every decision carries a sentence.
    for a in &outcome.assessed {
        if a.decision.explanation.trim().is_empty() {
            return Err(report(format!(
                "`{}` was decided by rule {} with no explanation (invariant I9)",
                norm(&a.path),
                a.decision.rule
            )));
        }
    }

    Ok(())
}

/// Run every fixture in one category and report all failures together, so a
/// single run surfaces the whole picture rather than the first broken case.
async fn run_category(category: &str) {
    let dir = scenario_root().join(category);
    let fixtures = fixtures_in(&dir);
    assert!(
        !fixtures.is_empty(),
        "no scenarios found in {} — the category exists in the test plan, so either \
         add fixtures or remove the test",
        dir.display()
    );

    let mut failures = Vec::new();
    for file in &fixtures {
        let raw = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("could not read {}: {e}", file.display()));
        if let Err(msg) = check(file, &raw).await {
            failures.push(msg);
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} scenarios failed in `{category}`:\n\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n\n")
    );
}

#[tokio::test]
async fn official() {
    run_category("official").await;
}

#[tokio::test]
async fn portable() {
    run_category("portable").await;
}

#[tokio::test]
async fn negative() {
    run_category("negative").await;
}

#[tokio::test]
async fn knowledge_base() {
    run_category("kb").await;
}

/// The format itself must reject a fixture it cannot honour. A Phase 2 fixture is
/// *skipped* by [`check`] rather than failed, so the corpus can carry cases ahead
/// of the code — but the skip must be visible, not silent.
#[tokio::test]
async fn phase_2_fixtures_are_skipped_not_silently_passed() {
    let raw = r#"
version = 1
[scenario]
id = "future"
title = "Needs the Write Witness"
[game]
title = "X"
[[sessions]]
started_at = "2026-01-01T20:00:00Z"
ended_at = "2026-01-01T22:00:00Z"
writes = ["{APPDATA}/X"]
[expect]
bind_eligible = "{APPDATA}/X"
"#;
    // Parsing refuses it...
    let err = Scenario::parse("future.toml", raw).expect_err("must refuse");
    assert!(err.to_string().contains("Phase 2"));
    // ...and the runner treats that refusal as a skip rather than a pass of the
    // assertions, which it could never satisfy.
    assert!(check(Path::new("future.toml"), raw).await.is_ok());
}
