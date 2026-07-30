# Testing Strategy

**Status:** design. Establishes how the save, detection, parser, achievement and
progress subsystems are tested, and what constraints testing imposes on the
architecture.

Owns: test levels, the filesystem abstraction requirement, fixture architecture,
invariant tests, CI budget.

Per-subsystem case matrices live in `docs/testing/`:

| Plan | Covers |
|---|---|
| [`SAVE_DETECTION_TEST_PLAN.md`](../testing/SAVE_DETECTION_TEST_PLAN.md) | The detection scenario corpus — the largest suite by far |
| _(planned)_ `VAULT_TEST_PLAN.md` | Snapshot, restore, retention, hostile archives |
| _(planned)_ `PARSER_TEST_PLAN.md` | Manifest engine, compiled extractors, fuzz corpus |

---

## 1. Testing dictates one architectural requirement

Detection currently calls `dirs::config_dir()`, `dirs::document_dir()` and
`std::path` directly. **That makes it untestable**: a test would read the developer's
own `%APPDATA%`, results would differ per machine, and CI would be meaningless.

Therefore, as a **Phase 0 deliverable**, all filesystem access in the save system
goes through an injected abstraction:

```rust
pub trait FileSystem {
    fn roots(&self) -> Vec<(PathBuf, RootKind)>;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryMeta>>;
    fn metadata(&self, path: &Path) -> io::Result<FileMeta>;   // size, mtime, is_dir
    fn exists(&self, path: &Path) -> bool;
}
```

Two implementations: `RealFs` in production, `VirtualFs` in tests. The trait is
deliberately **metadata-only** for detection — it has no `read` method, which makes
"detection never reads file contents" ([ADR-0003](./adr/0003-detection-is-read-only.md)) a property of the
type system rather than a rule someone must remember.

This is the single highest-leverage item in this document. Without it there is no
detection test suite, only manual verification on one machine.

## 2. Test levels

| Level | Runs against | Count | Runtime each | Purpose |
|---|---|---|---|---|
| **Unit** | Pure functions, no I/O | ~100 | µs | Alias generation, template expansion, similarity, score ordering |
| **Scenario** | `VirtualFs` + synthetic sessions + synthetic KB | **hundreds** | < 1 ms | The bulk of the suite. A declared world in, a decision out |
| **Invariant** | `VirtualFs`, asserted across whole categories | ~20 | ms | Properties that must never break (§4) |
| **Property** | Generated input | ~15 | ms | "never panics", "never escapes a root", "always terminates" |
| **Fuzz** | Malformed bytes | corpus | seconds | Archive extraction, manifest parsing |
| **Integration** | Real temp directories, real SQLite | ~20 | tens of ms | Migrations, transactions, end-to-end wiring |
| **Manual** | Real machines | checklist | — | What cannot be automated (§7) |

The shape matters: **scenario tests are the suite.** They are pure, so they cost
microseconds and can number in the hundreds without slowing anything down.
Integration tests are deliberately few — they verify wiring, not behaviour.

## 3. Scenarios are data, not Rust

You suggested `tests/test_rdr2.rs`, `test_skyrim.rs`, `test_elden_ring.rs`. That is
the natural instinct and I want to argue against it, for the same reason parsers are
manifests rather than code
([`PARSER_ARCHITECTURE.md`](./PARSER_ARCHITECTURE.md) §2).

A Rust file per game means, per game: boilerplate to construct a fake tree, a
handwritten KB stub, a session builder, and an assertion block. Fifty games is
5,000 lines of near-identical setup. The suite then grows slowly, contributors
avoid it, and the boilerplate drifts so that two tests build subtly different
worlds.

Instead: **one table-driven runner over a directory of declarative scenario
files.** Adding a game is adding a data file. Recorded in
[ADR-0013](./adr/0013-scenario-driven-tests.md).

```
src-tauri/tests/
  scenarios.rs                    ← the only runner
  scenarios/
    official/
      rdr2-steam.toml
      skyrim-steam-mygames.toml
      cp2077-gog.toml
    portable/
      generic-savedat-beside-exe.toml
    emulated/
      goldberg-remote-dir.toml
      codex-appdata-ini.toml
    multi-path/
      witcher3-saves-plus-profiles.toml
    negative/
      logs-written-during-session.toml
      documents-photos-name-collision.toml
    ambiguity/
      two-games-concurrent-session.toml
    safety/
      symlink-escape.toml
      depth-bomb.toml
```

Directory names are the test categories, so `cargo test scenarios::negative` is a
meaningful selector. The format is specified in
[`SAVE_DETECTION_TEST_PLAN.md`](../testing/SAVE_DETECTION_TEST_PLAN.md) §3.

Where a genuinely game-specific behaviour needs real code, a `tests/games/` Rust
module remains available — but it should be the exception, and needing one is a
signal the scenario format is missing an expressive feature.

## 4. Invariant tests

The highest-value tests in the codebase. Each asserts a property across a whole
category of inputs rather than one case, and **none may ever be deleted** — if one
becomes inconvenient, the architecture changed and that needs a decision entry.

| # | Invariant | Why it exists |
|---|---|---|
| I1 | A binding with `is_locked = 1` is byte-identical after: rescan, KB refresh of every layer, scoring-algorithm change, app upgrade migration | The worst bug this system can have is silently repointing a confirmed folder and restoring into it |
| I2 | No detection code path calls a write, create, remove or open-for-read on any candidate path | Detection is read-only by construction; enforced by the `FileSystem` trait having no such methods, and asserted with a recording mock |
| I3 | No candidate path is outside the declared root set, at any depth | Bounds are the defence against disk-walking and traversal |
| I4 | Re-scoring persisted evidence yields the same decision as scoring at observation time | Determinism; this is what makes the "re-decide offline after a KB update" design safe |
| I5 | Extraction never writes to a row with `source = 'user'` | Parsed data must never destroy hand-entered data |
| I6 | Restore always creates a `reason = 'pre_restore'` snapshot first, and leaves the original intact on any failure | Recovery from a wrong restore |
| I7 | A KB refresh never modifies the `user` layer | User KB entries are permanent |
| I8 | Retention never prunes `manual` or `pre_restore` snapshots | Deleting a backup silently is unacceptable |
| I9 | Every decision-table outcome carries a non-empty explanation string | Explainability is a design goal, so it is a tested property |
| I10 | Detection completes within its time budget on a pathological tree, or yields partial results with backoff — never hangs | An unbounded scan on a network mount is a hang the user blames on NOVARA |

I1, I5 and I6 protect user data. If effort has to be rationed, those three come
first.

## 5. Fixture principles

**Synthetic content, real structure.** A detection fixture declares a tree with file
names, sizes and mtimes — never real bytes. Detection only ever reads metadata
(§1), so real content would be dead weight and a licensing/privacy hazard.

**Parser fixtures do need real bytes**, and must be scrubbed: no usernames, no
Steam IDs, no machine names, no paths outside the fixture. A scrubbing checklist
belongs in the parser test plan, and review of a parser PR includes checking it.

**Fixtures are versioned with the scenario format.** A format change bumps a
version field and the runner rejects unknown versions loudly rather than silently
misinterpreting old fixtures.

**Every shipped KB entry and every shipped manifest needs at least one fixture.**
Not merged without one. This is what makes the corpus self-verifying: a scenario
asserting "RDR2 on Steam binds to X" is simultaneously a detection test and a KB
correctness test.

## 6. CI budget and shape

| Gate | Runs | Budget |
|---|---|---|
| Pre-commit / `cargo test` | unit + scenario + invariant | **< 30 s total** |
| CI on PR | above + integration + property | < 3 min |
| Nightly | above + fuzz corpus | unbounded |

The 30-second figure is a design constraint, not an aspiration. A suite that takes
five minutes stops being run before committing, and a suite that is not run before
committing does not prevent regressions. Scenario tests are pure and in-memory
precisely so that hundreds of them fit inside that budget.

Frontend tests continue as they are (`vitest`, currently 135 passing) and are
unaffected by this document.

## 7. What cannot be automated

Recorded as a manual checklist, run before a release that touches the save system:

- Real antivirus interference with filesystem watchers
- A network mount that blocks on `stat`
- OneDrive / Dropbox redirecting `Documents` (a genuine, common source of wrong
  detection)
- Case-insensitive vs case-sensitive volumes
- Watcher descriptor limits under a library of thousands of games
- Actual restore of an actual save, verified by launching the game

The last one is the only true end-to-end test that exists, and it should be done by
hand before every release that changes the vault. Nothing in CI can prove a
restored save loads.

## 8. Anti-patterns

| Don't | Because |
|---|---|
| Assert on a confidence *number* | Numbers are ordering-only; asserting them locks in arbitrary values and makes the scoring untunable |
| Write a test that reads the developer's real `%APPDATA%` | Non-deterministic, machine-dependent, and it will pass locally and fail in CI |
| Add a game-specific Rust test where a scenario file would do | Boilerplate growth is what kills large suites |
| Chase a coverage percentage | Meaningful target: every decision-table row has ≥ 1 positive and ≥ 1 negative case. That is a real statement about behaviour; 85% line coverage is not |
| Delete an invariant test to make a change pass | The invariant is the requirement. Changing it needs a superseding [ADR](./adr/README.md) |
