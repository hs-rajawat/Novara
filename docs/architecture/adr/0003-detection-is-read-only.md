# ADR-0003: Detection is read-only; the verifier reads metadata only

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection, Security
- **Supersedes:** — · **Superseded by:** —

## Context

Detection inspects directories that may contain a user's only copy of a save. It also
inspects directories chosen by heuristics, which means it will sometimes inspect the
wrong thing entirely.

Two temptations exist. One is to write a probe file to test whether a directory is
writable, or to touch a file to test permissions. The other is to open save files and
look inside them, since content inspection would obviously improve accuracy.

## Decision

No component in the detection pipeline writes to, creates in, removes from, or opens
for reading any candidate path.

The verifier judges plausibility from **metadata only**: extension histogram, file
count, size distribution, mtimes, depth. It never reads file contents.

This is enforced structurally: the `FileSystem` trait (see
[ADR-0012](./0012-filesystem-behind-a-trait.md)) exposes directory listing and
metadata and has **no read method**.

## Alternatives considered

| Option | Why not |
|---|---|
| Write a probe file to test writability | Risks corrupting a game's state in a directory we may have identified incorrectly. Indefensible for the benefit |
| Read file headers to identify save formats | Blurs detection into parsing, which belongs a layer up and carries a hostile-input security posture detection should not need to adopt |
| Read contents only for small files | The same posture problem with an arbitrary threshold; "small" files can still be malicious or huge after decompression |
| Rely on code review to prevent writes | Reviews miss things over years and many contributors. A type that cannot express a write is a stronger guarantee |
| Touch mtimes to test access | Modifies user data to gather evidence about user data. No |

## Consequences

- Detection cannot be blamed for save corruption, because it is structurally
  incapable of causing it.
- The verifier's signals are weaker than content inspection would give. Accepted —
  metadata is enough for the decision table's needs, and it is cheap.
- Detection stays fast: no file opens, no decompression, no size limits to enforce.
- Content inspection is available a layer up, after a **snapshot** has been taken, so
  parsers read a copy rather than the live save.
- Adding a read method to `FileSystem` later would silently remove this guarantee.
  That change requires a superseding ADR.

Design: [`GAME_SAVE_DETECTION.md`](../GAME_SAVE_DETECTION.md) §9, §13.
