//! Validation against a real, installed library.
//!
//! Task 1.22's last obligation, and the only thing in the suite that reads the machine
//! it runs on. Ignored by default for three reasons: it is not reproducible, it is not an
//! assertion, and it touches the user's own directories.
//!
//! ```text
//! cargo test --lib real_library -- --ignored --nocapture
//! ```
//!
//! It opens the **live NOVARA database read-only-in-effect** (detection persists nothing
//! here — a scratch database receives the writes) and runs the real pipeline against
//! `RealFs` for every game, reporting where each candidate came from and what the
//! decision table concluded.
//!
//! ## Privacy
//!
//! Output is deliberately shaped to be pasteable. Absolute paths are shortened so the
//! user profile name never appears: `C:/Users/<name>/AppData/...` prints as
//! `~/AppData/...`. Game titles are printed because they are the point of the exercise.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::saves::evidence::{Evidence, KbLayer};
use crate::saves::fs::RealFs;
use crate::saves::resolver::Outcome;
use crate::saves::{kb, pipeline, service};

/// Where the shipped application keeps its database.
fn live_db_path() -> Option<std::path::PathBuf> {
    // Mirrors `lib.rs`'s app-data resolution: `%APPDATA%/<identifier>/gamevault.db`.
    let base = dirs::config_dir()?;
    for candidate in ["com.novara.app", "NOVARA", "novara", "com.gamevault.app"] {
        let p = base.join(candidate).join("gamevault.db");
        if p.is_file() {
            return Some(p);
        }
    }
    // Fall back to a search one level down, since the identifier may differ per build.
    let entries = std::fs::read_dir(&base).ok()?;
    for entry in entries.filter_map(Result::ok) {
        let p = entry.path().join("gamevault.db");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Replace the user profile prefix so output can be pasted safely.
fn redact(path: &str) -> String {
    let p = path.replace('\\', "/");
    if let Some(home) = dirs::home_dir() {
        let home = home.to_string_lossy().replace('\\', "/");
        if let Some(rest) = p.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    p
}

#[derive(Default)]
struct Tally {
    games: usize,
    scanned: usize,
    with_candidates: usize,
    bind_eligible: usize,
    suggested_only: usize,
    nothing: usize,
    curated_kb: usize,
    convention_kb: usize,
    install_local: usize,
    name_only: usize,
    verifier_rejections: usize,
    rejections_by_rule: BTreeMap<u8, usize>,
}

/// Probe every curated entry's template against this machine's filesystem, independent of
/// what is in the games library.
///
/// This is the more informative half of the validation. A library of four games can only
/// exercise four entries, so "0 of 30 matched" says nothing about the other 26 — it says
/// those games are not installed. Expanding each template with its own title and asking
/// whether the directory exists tests the part that is actually uncertain: **the paths**.
///
/// A `MISS` here is not automatically a wrong entry. It means one of: the game is not
/// installed, it is installed but never launched, or the path is wrong. Only the third is
/// a defect, and distinguishing them needs the parent directory — which is why the probe
/// reports the nearest existing ancestor.
#[tokio::test]
#[ignore = "reads the developer's own filesystem; run explicitly"]
async fn curated_entry_paths_on_this_machine() {
    use crate::saves::fs::FileSystem;
    use crate::saves::kb::template::{self, TemplateVars};

    let (_, entries) = kb::builtin::parsed().expect("valid corpus");
    let fs = RealFs;
    let curated: Vec<_> = entries
        .iter()
        .filter(|e| e.match_kind == "title_norm")
        .collect();

    println!("\n=== curated entry paths, probed directly ===");
    let mut hits = 0usize;
    let mut partial = 0usize;

    for entry in &curated {
        // The template's own title, so the entry is tested on its own terms.
        let vars = TemplateVars {
            title: &entry.match_value,
            publisher: None,
            developer: None,
            steam_appid: None,
            steam_userid: None,
            install_dir: None,
        };
        // `expand` produces candidate paths without judging existence — that check is
        // `kb::candidates`' job and is exactly what this probe is standing in for.
        // A `{WILDCARD}` template returns nothing when its parent is absent, so those
        // entries show as MISS with no ancestor detail.
        let expanded = template::expand(&fs, &entry.path_template, &vars);

        let mut status = "MISS";
        let mut detail = String::new();
        for path in &expanded {
            if fs.is_dir(path) {
                status = "HIT";
                detail = redact(&path.display().to_string());
                hits += 1;
                break;
            }
            // Walk up to the nearest ancestor that does exist. A parent that is present
            // while the leaf is absent is the interesting case: it means the game is
            // installed and the entry names the wrong subfolder.
            let mut cursor = path.parent();
            while let Some(p) = cursor {
                if fs.is_dir(p) {
                    status = "PARTIAL";
                    detail = format!(
                        "template {} | nearest existing ancestor {}",
                        entry.path_template,
                        redact(&p.display().to_string())
                    );
                    partial += 1;
                    break;
                }
                cursor = p.parent();
            }
            if status != "MISS" {
                break;
            }
        }
        if status == "MISS" && detail.is_empty() {
            detail = entry.path_template.clone();
        }
        println!("  {status:<8} {:<34} {detail}", entry.id);
    }

    println!(
        "\n  {hits} confirmed, {partial} with an existing ancestor but a missing leaf, {} not present at all",
        curated.len() - hits - partial
    );
    println!("  PARTIAL rows are the ones worth acting on: the game is here, the subfolder is not.");
}

/// Run the library filter over the games already in the real library.
///
/// Retrospective: these rows were imported before the filter existed, so this answers
/// "what would the filter do now" — both which system components it removes and, more
/// importantly, whether it would wrongly remove anything the user actually plays.
#[tokio::test]
#[ignore = "reads the developer's own library; run explicitly"]
async fn library_filter_against_the_real_library() {
    use crate::scanner::filter;

    let Some(db_path) = live_db_path() else {
        println!("\nNo NOVARA database found; run the app once first.");
        return;
    };
    let scratch = crate::test_support::TempDir::new("filter-validation");
    let copy = scratch.path().join("gamevault.db");
    std::fs::copy(&db_path, &copy).expect("copy the live database");
    let db = crate::db::Db::open(&copy).await.expect("open the copy");

    let games = db.list_games(true).await.expect("list games");
    println!("\n=== library filter, applied retrospectively ===");
    let mut skipped = 0usize;

    for game in &games {
        let installs = db.list_installations(&game.id).await.unwrap();
        let install = installs.iter().find(|i| i.is_primary == 1).or(installs.first());
        let source = match install {
            Some(i) => db.source_code_for(i.source_id).await.unwrap(),
            None => "manual".to_string(),
        };
        let dir = install
            .map(|i| std::path::PathBuf::from(&i.install_dir))
            .unwrap_or_default();

        let verdict = filter::classify(&filter::Candidate {
            source_code: &source,
            source_app_id: install.and_then(|i| i.source_app_id.as_deref()),
            title: &game.title,
            install_dir: &dir,
            // Steam leaves this unset; the scanner passes `Some(true)` only when it
            // actually resolved a binary.
            has_executable: install.and_then(|i| i.executable.as_ref().map(|_| true)),
        });

        match verdict.skip() {
            Some(s) => {
                skipped += 1;
                println!("  SKIP    {:<44} [{}] {}", game.title, s.rule, s.reason);
            }
            None => println!("  import  {}", game.title),
        }
    }

    println!(
        "\n  {} of {} would be kept out of the library",
        skipped,
        games.len()
    );
    println!("  Check the import lines: a real game appearing as SKIP is a defect.");
}

/// The Track G validation report.
///
/// Prints, for every game in the real library: which KB entry matched, which decision-table
/// row fired, the final outcome, the full evidence set and the matched path — then lists the
/// games where nothing was found, for manual classification.
///
/// Deliberately verbose. The other harnesses summarise; this one exists to be read.
#[tokio::test]
#[ignore = "reads the developer's own library; run explicitly"]
async fn track_g_validation_report() {
    let Some(db_path) = live_db_path() else {
        println!("\nNo NOVARA database found; run the app once first.");
        return;
    };
    let scratch = crate::test_support::TempDir::new("track-g");
    let copy = scratch.path().join("gamevault.db");
    std::fs::copy(&db_path, &copy).expect("copy the live database");
    let db = crate::db::Db::open(&copy).await.expect("open the copy");

    match crate::saves::kb::builtin::load(&db).await {
        Ok(Ok(o)) => println!("\nbuilt-in KB: {o:?}"),
        Ok(Err(e)) => println!("\nbuilt-in KB INVALID: {e}"),
        Err(e) => println!("\nbuilt-in KB load failed: {e}"),
    }

    let games = db.list_games(true).await.expect("list games");
    let fs = RealFs;
    let mut nothing: Vec<String> = Vec::new();
    let mut timings: Vec<(u128, String)> = Vec::new();

    println!("\n════════ per-game detection ════════");
    for game in &games {
        let Some(ctx) = service::context_for(&db, &game.id).await.unwrap() else {
            continue;
        };

        let started = Instant::now();
        let outcome = pipeline::detect_with_kb(&db, &fs, &ctx).await.unwrap();
        timings.push((started.elapsed().as_micros(), game.title.clone()));

        println!(
            "\n▸ {}  [steam:{}]",
            game.title,
            ctx.steam_appid.as_deref().unwrap_or("-")
        );
        if outcome.assessed.is_empty() {
            println!("    (no candidate paths at all)");
            nothing.push(game.title.clone());
            continue;
        }

        for a in &outcome.assessed {
            println!(
                "    {:<13} rule {:<2}  {}",
                a.decision.outcome.status(),
                a.decision.rule,
                redact(&a.path)
            );
            println!("        why      : {}", a.decision.explanation);
            for e in &a.evidence.items {
                let detail = match e {
                    Evidence::KbMatch { entry_id, layer, layout, keyed, priority } => format!(
                        "KbMatch      {entry_id}  layer={layer:?} layout={layout} keyed={keyed} priority={priority}"
                    ),
                    Evidence::NameMatch { alias, similarity } => {
                        format!("NameMatch    alias=`{alias}` similarity={similarity:.2}")
                    }
                    Evidence::InstallLocal { subdir } => format!("InstallLocal {subdir}"),
                    Evidence::ContentShape { save_like, total, newest_mtime, .. } => format!(
                        "ContentShape save_like={save_like} of {total} newest={}",
                        newest_mtime.as_deref().unwrap_or("-")
                    ),
                    Evidence::ContentMismatch { reason } => format!("ContentMismatch {reason}"),
                    other => format!("{other:?}"),
                };
                println!("        evidence : {detail}");
            }
        }

        if outcome
            .assessed
            .iter()
            .all(|a| a.decision.outcome == Outcome::Rejected)
        {
            nothing.push(format!("{} (all candidates rejected)", game.title));
        }
    }

    println!("\n════════ nothing offered ════════");
    if nothing.is_empty() {
        println!("  (none)");
    }
    for t in &nothing {
        println!("  {t}");
    }

    timings.sort_unstable_by_key(|t| std::cmp::Reverse(t.0));
    let total: u128 = timings.iter().map(|t| t.0).sum();
    println!(
        "\n════════ timing ════════\n  average {:.2} ms over {} games",
        if timings.is_empty() { 0.0 } else { total as f64 / timings.len() as f64 / 1000.0 },
        timings.len()
    );
    for (micros, title) in timings.iter().take(3) {
        println!("  worst   {:.2} ms  {title}", *micros as f64 / 1000.0);
    }
}

#[tokio::test]
#[ignore = "reads the developer's own library; run explicitly"]
async fn real_library_validation() {
    let Some(db_path) = live_db_path() else {
        println!(
            "\nNo NOVARA database found under {:?}. Run the app once, then re-run this.",
            dirs::config_dir()
        );
        return;
    };
    println!("\nlive database: {}", redact(&db_path.display().to_string()));

    // Copy the live database to a scratch file. Detection writes candidates and scan
    // attempts, and a measurement run must not mutate the user's real state.
    let scratch = crate::test_support::TempDir::new("real-library");
    let copy = scratch.path().join("gamevault.db");
    std::fs::copy(&db_path, &copy).expect("copy the live database");

    let db = crate::db::Db::open(&copy).await.expect("open the copy");
    // Ensure the shipped knowledge base is present, exactly as startup would.
    match crate::saves::kb::builtin::load(&db).await {
        Ok(Ok(o)) => println!("built-in KB: {o:?}"),
        Ok(Err(e)) => println!("built-in KB INVALID: {e}"),
        Err(e) => println!("built-in KB load failed: {e}"),
    }

    // Hidden games included: they are still installed and still have saves.
    let games = db.list_games(true).await.expect("list games");
    let fs = RealFs;
    let mut tally = Tally {
        games: games.len(),
        ..Default::default()
    };
    let mut timings: Vec<(u128, String)> = Vec::new();
    let mut detected: Vec<String> = Vec::new();
    let mut curated_hits: Vec<String> = Vec::new();

    for game in &games {
        let Some(ctx) = service::context_for(&db, &game.id).await.unwrap() else {
            continue;
        };

        let started = Instant::now();
        let outcome = pipeline::detect_with_kb(&db, &fs, &ctx).await.unwrap();
        let micros = started.elapsed().as_micros();
        tally.scanned += 1;
        timings.push((micros, game.title.clone()));

        let offered: Vec<_> = outcome
            .assessed
            .iter()
            .filter(|a| a.decision.outcome != Outcome::Rejected)
            .collect();

        for a in &outcome.assessed {
            if a.decision.outcome == Outcome::Rejected {
                *tally.rejections_by_rule.entry(a.decision.rule).or_insert(0) += 1;
                // Rule 6 is the content-mismatch row: the verifier's contribution.
                if a.decision.rule == 6 {
                    tally.verifier_rejections += 1;
                }
            }
        }

        for a in &offered {
            let mut kinds = Vec::new();
            for e in &a.evidence.items {
                match e {
                    Evidence::KbMatch { layer, keyed, entry_id, .. } => {
                        if *keyed {
                            tally.curated_kb += 1;
                            kinds.push(format!("kb-curated({entry_id})"));
                            if *layer == KbLayer::Builtin {
                                curated_hits.push(format!(
                                    "  {:<40} {}  ->  {}",
                                    game.title,
                                    entry_id,
                                    redact(&a.path)
                                ));
                            }
                        } else {
                            tally.convention_kb += 1;
                            kinds.push("kb-convention".into());
                        }
                    }
                    Evidence::InstallLocal { .. } => {
                        tally.install_local += 1;
                        kinds.push("install-local".into());
                    }
                    Evidence::NameMatch { .. } => kinds.push("name".into()),
                    _ => {}
                }
            }
            if kinds.iter().all(|k| k == "name") {
                tally.name_only += 1;
            }
            detected.push(format!(
                "  {:<40} rule {:>2} {:<14} {}\n      {}",
                game.title,
                a.decision.rule,
                a.decision.outcome.status(),
                redact(&a.path),
                a.decision.explanation
            ));
        }

        if offered.is_empty() {
            tally.nothing += 1;
        } else {
            tally.with_candidates += 1;
            if offered
                .iter()
                .any(|a| a.decision.outcome == Outcome::BindEligible)
            {
                tally.bind_eligible += 1;
            } else {
                tally.suggested_only += 1;
            }
        }
    }

    // ── Report ───────────────────────────────────────────────────────────
    println!("\n=== detected candidates ===");
    for line in &detected {
        println!("{line}");
    }

    println!("\n=== curated built-in KB matches ===");
    if curated_hits.is_empty() {
        println!("  (none)");
    }
    for line in &curated_hits {
        println!("{line}");
    }

    println!("\n=== which curated entries never matched ===");
    let (_, entries) = kb::builtin::parsed().expect("valid corpus");
    let matched: std::collections::HashSet<&str> = curated_hits
        .iter()
        .filter_map(|l| l.split_whitespace().find(|w| w.starts_with("builtin:")))
        .collect();
    let mut unmatched: Vec<&str> = entries
        .iter()
        .filter(|e| e.match_kind == "title_norm" && !matched.contains(e.id.as_str()))
        .map(|e| e.id.as_str())
        .collect();
    unmatched.sort_unstable();
    println!("  {} of {} curated entries unmatched", unmatched.len(), 
        entries.iter().filter(|e| e.match_kind == "title_norm").count());
    for id in &unmatched {
        println!("    {id}");
    }

    println!("\n=== rejections by decision-table rule ===");
    for (rule, count) in &tally.rejections_by_rule {
        println!("  rule {rule:>2}: {count}");
    }

    // Descending, so the worst case is first.
    timings.sort_unstable_by_key(|t| std::cmp::Reverse(t.0));
    let total: u128 = timings.iter().map(|t| t.0).sum();
    let average = if timings.is_empty() { 0 } else { total / timings.len() as u128 };

    println!("\n=== summary ===");
    println!("  games in library        {}", tally.games);
    println!("  games scanned           {}", tally.scanned);
    println!("  games with candidates   {}", tally.with_candidates);
    println!("    bind_eligible         {}", tally.bind_eligible);
    println!("    suggested only        {}", tally.suggested_only);
    println!("  games with nothing      {}", tally.nothing);
    println!("  evidence occurrences:");
    println!("    curated KB            {}", tally.curated_kb);
    println!("    convention KB         {}", tally.convention_kb);
    println!("    install-local         {}", tally.install_local);
    println!("    name-only candidates  {}", tally.name_only);
    println!("  verifier rejections     {}", tally.verifier_rejections);
    println!("\n  average detection time  {:.2} ms", average as f64 / 1000.0);
    println!("  worst case:");
    for (micros, title) in timings.iter().take(5) {
        println!("    {:>8.2} ms  {title}", *micros as f64 / 1000.0);
    }
}
