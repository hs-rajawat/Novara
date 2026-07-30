# ADR-0006: Bindings allow multiple paths per role

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection, Vault
- **Supersedes:** — · **Superseded by:** —

## Context

The first schema draft keyed bindings `UNIQUE(game_id, role)` — one path per role per
game.

Review against real layouts showed this is wrong. Split saves are common:

```
Documents/My Games/The Witcher 3/gamesaves/      ← save files
%APPDATA%/CD Projekt Red/The Witcher 3/          ← user settings, profile
```

Per-character or per-profile directory trees are also common, and some games write
saves to one location and cloud-sync staging to another.

## Decision

`save_bindings` is keyed `UNIQUE(game_id, role, path)` with an `is_primary` flag. A
game may have several bindings for the same role; exactly one is primary.

The vault operates on the primary binding. Extraction may read all bindings for a role.

## Alternatives considered

| Option | Why not |
|---|---|
| Keep one path per role | Forces a wrong binding or a fake extra role. Both corrupt the data model to satisfy a constraint that was never justified |
| Invent more roles (`saves`, `saves2`, `profile`) | Roles are semantic categories, not slots. Numbered roles are a schema smell and would proliferate |
| One binding whose path is the common ancestor | The common ancestor of `Documents` and `%APPDATA%` is the user profile. Snapshotting that is absurd |
| A `binding_paths` child table | Cleaner in the abstract, but adds a join to the hottest query for a cardinality that is almost always one. The flag is the pragmatic choice |
| Store multiple paths as JSON in one row | Unqueryable, and per-path provenance and verification state have nowhere to live |

## Consequences

- Split saves are representable, so they can be backed up correctly rather than half
  backed up.
- Per-playthrough save branching becomes possible later with no migration.
- `is_primary` needs a deterministic selection rule, or scenario tests flake. Chosen:
  greatest written byte volume across sessions, ties broken by earliest
  `first_seen_at`.
- The vault must decide whether restore covers all bindings for a role or only the
  primary. Restoring only the primary can leave a game with mismatched saves and
  settings — an open question for the vault design.
- Every consumer must handle "more than one" rather than assuming a single row.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §11.
