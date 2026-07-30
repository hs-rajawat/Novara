# ADR-0005: Ship the Write Witness as an mtime sweep before watchers

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection
- **Supersedes:** — · **Superseded by:** —

## Context

[ADR-0001](./0001-write-witness-as-primary-signal.md) makes observed writes the primary
detection signal. The obvious implementation is filesystem watchers (`notify`) armed
for the duration of a session.

Watchers are the riskiest component in the design: descriptor exhaustion on large root
sets, blocking on network mounts, antivirus interference, platform inconsistency, and
event storms from unrelated applications writing to `%APPDATA%`.

That risk sits directly in front of the feature that differentiates the product.

## Decision

The Write Witness ships in two tiers.

**Tier 1 — mtime sweep.** Record directory mtimes across the bounded root set at
session start; compare at session end. No watchers, no live event stream.

**Tier 2 — watchers.** Finer-grained per-file counts and byte volumes, armed on
session start.

Tier 1 ships first and remains a permanent fallback whenever arming a watcher fails.
Tier 2 is an optimisation *of* tier 1, never a replacement.

## Alternatives considered

| Option | Why not |
|---|---|
| Watchers only | Puts the highest-risk component in front of the highest-value feature. A watcher failure would mean no detection improvement at all |
| Watchers with a "try again next session" fallback | Still no signal when watching persistently fails, which is exactly the antivirus-heavy environment where users need help most |
| Poll during the session | Wasteful and no better than comparing endpoints — nothing needs the intermediate states |
| Full recursive mtime walk at session end only | Loses the baseline: a directory whose mtime is old still cannot be distinguished from one that was never touched without a "before" reading |
| Use the OS journal (USN on NTFS) | Genuinely interesting and precise, but Windows-specific, needs elevation, and is a large amount of machinery for a signal the sweep already approximates |

## Consequences

- The differentiating feature lands early, in Phase 2a, at low risk.
- Tier 1 is coarser: it sees "this directory changed", not which files or how many
  bytes. Decision-table rule 4 (witness + content shape) still works; rule
  strengthening by byte volume waits for tier 2.
- Tier 1 misses a write that was later reverted within the same session. Acceptable.
- Two code paths to maintain, with tier 1 permanently exercised as the fallback — so
  it cannot rot unnoticed.
- Primary-binding selection among multiple paths uses written byte volume, which is
  tier-2 data; tier 1 must fall back to a deterministic tie-break.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §10.2.
