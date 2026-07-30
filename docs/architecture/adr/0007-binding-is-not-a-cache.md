# ADR-0007: The binding store is a system of record, not a cache

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection
- **Supersedes:** — · **Superseded by:** —

## Context

Once a save location is confirmed, NOVARA should reuse it rather than rediscovering
it. The natural name for that is a "binding cache", and the natural question is whether
it should be a subsystem of its own or part of the resolver.

The naming turns out to matter more than the placement. "Cache" carries an implicit
contract: derived data, evictable, rebuildable, invalidated on a heuristic. Applied to
bindings, that contract produces the worst failure this system can have — deciding a
binding looks stale, discarding it, re-detecting, and silently repointing a folder the
user confirmed. The next restore then writes into the wrong directory.

## Decision

Bindings are a **system of record**: the persisted answer, with provenance, owned by
the resolver as its own state. Not a separate subsystem.

Positive results are permanent. Only *negative* results expire — "we looked and found
nothing" lives in `save_scan_attempts` with backoff and is genuinely cache-like.

A binding with `is_locked = 1` is immutable except by explicit user action.

## Alternatives considered

| Option | Why not |
|---|---|
| A standalone binding-cache subsystem | Would need its own invalidation policy. Two components with opinions about when a binding is valid is precisely the race that loses user data |
| A cache with a long TTL | A TTL on a user's confirmed choice is a countdown to silently discarding it. There is no correct TTL for "the user told us" |
| Re-verify and evict when the path disappears | Deleting the binding loses the user's decision over a temporarily unplugged drive. Mark unverified instead; keep the record |
| Cache with an "authoritative" flag | This *is* the design, minus the misleading word. Calling it a cache would keep inviting eviction logic |
| No persistence; detect on demand | Thousands of filesystem probes on library load, and the user re-confirms forever |

## Consequences

- One owner, one table, one invariant. No cross-component invalidation to get wrong.
- A locked binding survives rescans, KB refreshes, scoring changes and upgrades. This
  is invariant I1 in the test plan and it may never be deleted.
- Detection cost is paid once per game rather than repeatedly: a locked binding
  short-circuits before any I/O, so library load performs zero filesystem calls for
  detection.
- A binding may point at a path that no longer exists. It is marked unverified and
  retained — the UI must handle "bound but missing" as a normal state, not an error.
- Stale bindings can outlive reality (game moved, drive re-lettered). The remedy is
  user-initiated rescan, not automatic eviction. Accepted deliberately.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §12.
