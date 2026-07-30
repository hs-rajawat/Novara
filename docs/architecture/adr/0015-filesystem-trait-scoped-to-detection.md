# ADR-0015: The `FileSystem` trait is scoped to detection, not the whole save system

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Detection, Testing, Vault
- **Supersedes:** — · **Superseded by:** —
- **Amends:** [ADR-0012](./0012-filesystem-behind-a-trait.md)

## Context

[ADR-0012](./0012-filesystem-behind-a-trait.md) states that *all* save-system
filesystem access goes through an injected `FileSystem` trait, and that the trait is
metadata-only — no content-read method — so that
[ADR-0003](./0003-detection-is-read-only.md) is enforced by the type system.

Implementing Phase 0 exposed a conflict between those two clauses. The vault
(`save_mgr/`, becoming `saves/vault/`) exists to copy save files into archives and
extract them back. It cannot function through a trait with no read or write. Taken
literally, ADR-0012 would require either adding read/write to the trait — which voids
the ADR-0003 guarantee for detection — or virtualising the vault's I/O as well.

Two facts make the second option unattractive. The vault is already well tested
against real temp directories (`save_mgr/tests.rs`, ~29 KB, using
`test_support::TempDir`), and archive round-tripping is precisely the behaviour where a
virtual filesystem would prove least: a fake filesystem that "successfully" writes a
zip tells you nothing about whether a real zip round-trips.

## Decision

The `FileSystem` trait governs **detection only** — the locator, the verifier, and the
Write Witness. It stays metadata-only: root enumeration, directory listing, metadata,
existence. No content read, no write.

The vault continues to use `std::fs` and archive libraries directly, and continues to
be tested against real temporary directories.

## Alternatives considered

| Option | Why not |
|---|---|
| One trait with read and write, used everywhere | Voids ADR-0003's structural guarantee: the verifier would be able to open files, and the only thing stopping it would be a convention |
| Two traits — `MetadataFs` for detection, `FullFs` for the vault | Defensible, and rejected as premature. A second trait earns its cost when a second consumer needs virtualising; today the vault has one implementation and real-directory tests that are better evidence than a mock |
| Virtualise the vault too, with an in-memory filesystem | Loses the property the vault's tests exist to prove. Archive round-tripping, file locking, partial writes and atomic rename are exactly the behaviours a fake filesystem models incorrectly |
| Leave ADR-0012 as written and interpret it loosely at implementation time | The documents are the source of truth. An ADR that is quietly not followed is worse than one that is amended |

## Consequences

- ADR-0003's read-only guarantee for detection stays structural: the verifier has no
  API through which to open a file.
- The detection scenario corpus (hundreds of cases, in-memory, microseconds) remains
  possible, which was ADR-0012's purpose.
- Vault tests keep touching a real filesystem, so they stay slower and are correctly
  classified as *integration* tests in [`TESTING.md`](../TESTING.md) §2 rather than
  scenario tests.
- The vault's filesystem access is not injectable, so vault failure modes (disk full,
  permission denied mid-write, a file locked by a running game) cannot be simulated
  cheaply. Accepted for now; if Phase 4 needs those cases, a narrow seam for
  fault injection is a smaller change than virtualising the whole vault.
- "All save-system filesystem access" in ADR-0012 should be read as "all *detection*
  filesystem access". That clause is amended by this record rather than deleted from
  it, so the original reasoning stays legible.

## Reopen when

Phase 4 needs to simulate vault I/O failures that a real temp directory cannot
produce, or a second component needs the same virtualisation the locator has.

Design: [`TESTING.md`](../TESTING.md) §1.
