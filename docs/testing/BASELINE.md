# Phase 0 Baseline

Captured **2026-07-30**, immediately before Phase 0 began. Every Phase 0 task
asserts "unchanged relative to this", so it exists to make that claim measurable
rather than asserted.

Phase 0 is behaviour-neutral. Any deviation from these numbers other than
*additional* tests is a defect in the task that caused it.

## Rust — `src-tauri`

| Measure | Value |
|---|---|
| `cargo test` | **250 passed**, 0 failed, 0 ignored |
| Doc-tests | 0 passed (none exist) |
| `cargo clippy --all-targets` | **0 warnings**, 0 errors |

Note: `cargo test` exits non-zero under PowerShell because cargo writes progress to
stderr and PowerShell surfaces that as `NativeCommandError`. The test results
themselves are clean. Read the `test result:` lines, not the exit code.

## Frontend

| Measure | Value |
|---|---|
| `npm test` | **135 passed**, 12 files |
| `npm run typecheck` | clean |
| `npm run build` | clean (chunk-size warning is pre-existing) |

## Pre-existing observations, deliberately not addressed in Phase 0

Recorded so they are not mistaken for regressions, and so they are not lost.

| Observation | Note |
|---|---|
| `metadata/tests.rs` defines its own `TempDir`, duplicating `test_support::TempDir` | Real duplication. Out of scope — folding it in would mix an unrelated refactor into a behaviour-neutral phase |
| `save_mgr::tests::restore_refuses_when_the_safety_backup_cannot_be_taken` exists | The vault **already** takes a pre-restore safety backup. `SAVE_SYSTEM_ARCHITECTURE.md` §3.3 presents this as Phase 4 work; part of it is already built. Phase 4 scope should be re-read against the code before it starts |
| `save_detect.rs` has **zero** tests | The specific gap Tasks 0.9–0.11 close |
| `notify` is a declared dependency | Present for the Phase 2b watcher; not used by Phase 0 |

## How to re-verify

```
cd src-tauri
cargo test              # expect: 250 passed (+ any new Phase 0 tests)
cargo clippy --all-targets   # expect: 0 warnings
cd ..
npm test                # expect: 135 passed
npm run typecheck && npm run build
```

---

## Outcome — Tasks 0.0–0.11 (2026-07-30)

| Measure | Baseline | After | Delta |
|---|---|---|---|
| `cargo test` | 250 | **276** | +26 new tests, **0 pre-existing tests changed** |
| `cargo clippy --all-targets` | 0 warnings | **0 warnings** | — |
| `npm test` | 135 | **135** | — |
| `npm run build` | clean | clean | — |

The +26: 6 for `RealFs`, 20 for the locator (its first tests ever, including two
invariant seeds). No existing assertion was modified — only `use` paths — which is
the evidence that Phase 0 was behaviour-neutral.

Task 0.12 (`SaveService` façade) and 0.13 (exit gate) were deliberately deferred.

### Two things worth remembering from the execution

**A test that passes immediately is suspicious.** The first version of the dedup
regression test used two roots pointing at one path. Both candidates scored 1.0, so
they were *adjacent* after sorting and the historical `dedup_by` bug deduplicated
them correctly — the test was vacuous. Verified by reintroducing the bug and watching
it pass. The replacement uses `"the witcher 3"`, where the same directory matches at
0.75 and 0.68 with a third path scoring 0.72 *between* them, so the duplicates are
non-adjacent. That version fails on the buggy implementation and passes on the correct
one, both confirmed.

**Restoring a file with `Move-Item` can produce a stale build.** `Copy-Item`/`Move-Item`
preserve timestamps, so restoring a backup gave the source an *older* mtime than the
compiled artifact and cargo reused the buggy build — presenting as a test failure
against correct code. If a result contradicts the source, touch the file and rebuild
before believing it.
