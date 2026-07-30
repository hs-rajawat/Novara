# ADR-0012: Filesystem access behind an injected trait

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection, Testing
- **Supersedes:** — · **Superseded by:** —
- **Amended by:** [ADR-0015](./0015-filesystem-trait-scoped-to-detection.md) — the trait
  governs *detection* filesystem access, not the vault's

## Context

`save_detect.rs` calls `dirs::config_dir()`, `dirs::document_dir()` and `std::path`
directly. A test therefore reads the developer's own `%APPDATA%`: results differ per
machine, CI is meaningless, and no scenario can be described in a fixture.

The detection test plan targets several hundred scenarios. None of them can exist while
the filesystem is reached directly.

Separately, [ADR-0003](./0003-detection-is-read-only.md) requires that detection never
reads file contents — a property currently guaranteed only by everyone remembering it.

## Decision

All save-system filesystem access goes through an injected `FileSystem` trait, with
`RealFs` in production and `VirtualFs` in tests.

For detection the trait is **metadata-only**: it exposes root enumeration, directory
listing, metadata (size, mtime, is_dir) and existence. It has **no method that reads
file contents**.

This is a Phase 0 deliverable, because everything in the test plan depends on it.

## Alternatives considered

| Option | Why not |
|---|---|
| Test against real temp directories | Works for a handful of integration tests; unusable for hundreds of scenarios needing `%APPDATA%`-shaped roots, controlled mtimes and synthetic sessions. Also slow |
| Override root paths via environment variables in tests | Fixes roots but not enumeration, mtimes or permission errors, and leaves global mutable state across parallel tests |
| `chroot`/container per test | Platform-specific, heavy, and unavailable on the primary target OS |
| Mock only at the `dirs::` boundary | Leaves `std::fs` calls unmocked, so the interesting behaviour — walking, metadata, errors — stays untestable |
| Accept manual testing only | A heuristic system without regression tests degrades silently. Non-viable for a subsystem that can destroy saves |

## Consequences

- The detection scenario corpus becomes possible: pure, in-memory, microsecond-scale,
  hundreds of cases inside a 30-second suite.
- "Detection never reads file contents" becomes a property of the type system rather
  than a convention. The verifier physically cannot open a file.
- Error paths become testable: permission denied, symlink loops, vanishing directories
  mid-walk are all expressible in `VirtualFs`.
- A refactor of working code in Phase 0 with no user-visible benefit. The existing test
  suite is the safety net.
- Every future filesystem call in this subsystem must go through the trait; a direct
  `std::fs` call is a review failure, not a style preference.
- Adding a content-read method later would silently void ADR-0003 and requires a
  superseding ADR.

Design: [`TESTING.md`](../TESTING.md) §1.
