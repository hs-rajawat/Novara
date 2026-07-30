# ADR-0013: Detection tests are declarative scenarios, not per-game Rust

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Testing
- **Supersedes:** — · **Superseded by:** —

## Context

Detection is a heuristic system that regresses silently: a change improving ten games
can break fifty, and nobody notices until a user restores a backup into the wrong
folder. It needs a large regression corpus — several hundred cases at maturity.

The natural instinct is a Rust test module per game: `test_rdr2.rs`, `test_skyrim.rs`,
`test_elden_ring.rs`.

Each such file needs the same setup: construct a fake directory tree, stub KB entries,
build synthetic sessions, assert an outcome. At fifty games that is thousands of lines
of near-identical boilerplate, and boilerplate copied fifty times drifts — two tests end
up building subtly different worlds and nobody can say which is canonical.

## Decision

The detection suite is a directory of **declarative scenario files** driven by a single
table-driven runner. A scenario declares the world (`[[fs]]`, `[[kb]]`, `[[sessions]]`)
and the expectation (`[expect]`, including which decision-table rule fired and what must
*not* appear).

Adding a game means adding a data file. Directory names are test categories.

A game-specific Rust test remains possible, but needing one signals a missing feature in
the scenario format — extend the format first.

## Alternatives considered

| Option | Why not |
|---|---|
| One Rust module per game | Per-game boilerplate; the suite grows slowly, contributors avoid it, and setup drifts between cases |
| A shared Rust test-builder DSL | Better than raw boilerplate, but still a code change per case, still needs review, and non-Rust contributors are excluded |
| Snapshot/golden tests over a real directory tree checked into the repo | Repository bloat, platform-dependent mtimes and permissions, and unable to express synthetic sessions |
| Property-based testing only | Excellent for "never panics, never escapes a root", useless for "RDR2 on Steam binds to this exact path" |
| Record real detection runs from real machines and replay | Privacy hazard (real usernames and paths) and non-reproducible |

## Consequences

- Hundreds of cases are realistic, and each costs microseconds because nothing touches
  a real filesystem.
- A scenario doubles as a KB correctness test: "RDR2 on Steam binds to X" verifies both
  the engine and the entry.
- A mis-detection bug report can *be* a scenario file, merged failing before the fix —
  which turns a one-off patch into a permanent guarantee.
- Contributors need no Rust to add a regression case.
- The scenario format becomes an interface: versioned, and the runner must reject
  unknown versions loudly rather than misreading old fixtures.
- Pressure will build to add expressiveness to the format. Same discipline as
  [ADR-0009](./0009-declarative-parsers-first.md): extend deliberately, or the format
  becomes a language.
- Depends entirely on [ADR-0012](./0012-filesystem-behind-a-trait.md).

Design: [`SAVE_DETECTION_TEST_PLAN.md`](../../testing/SAVE_DETECTION_TEST_PLAN.md) §3.
