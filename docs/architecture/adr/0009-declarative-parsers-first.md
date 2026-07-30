# ADR-0009: Declarative manifests are the default parser tier

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Parsers
- **Supersedes:** — · **Superseded by:** —

## Context

Reading achievement and progress data out of save files is inherently per-game work,
and the target is 10,000 games over time. If each game costs a Rust module, a review
and a release, the corpus grows at the rate of the maintainers' attention.

Surveying real formats shows most are structurally trivial: a JSON, INI or XML file
containing a list of unlocked identifiers. Emulated-Steam layers in particular
converged on a small number of shapes, so one generic description plausibly covers
hundreds of games.

## Decision

Extraction has two tiers, and the declarative tier is the default.

**Tier 1 — declarative manifests.** Data describing *where* values live, interpreted by
a fixed engine. Supported formats: `json`, `ini`, `xml`, `kv`, `csv`. Ships and updates
as data, independent of releases.

**Tier 2 — compiled in-tree extractors.** A Rust trait impl, for binary formats,
checksums and anything needing real logic.

A tier-2 contribution must justify why tier 1 was insufficient.

## Alternatives considered

| Option | Why not |
|---|---|
| Rust extractor per game | Cost per game is a PR, review, tests and a release. Does not reach 10,000 games, and a format change requires shipping a new binary |
| An embedded scripting language (Lua, Rhai) | Solves expressiveness and creates a security surface: arbitrary code reading user files. Also an interpreter to maintain and sandbox |
| Regex/offset-based binary descriptions in the manifest | Turns the manifest into a programming language by increments. This is the failure mode the fixed format list exists to prevent |
| One universal heuristic parser (find any list of booleans) | Produces confident nonsense. Achievement data must be exact or it is worse than absent |
| Only tier 2, no manifests | Guarantees the corpus stalls; every game becomes a code change |

## Consequences

- Common formats cost **zero Rust** and can be fixed without a release — parser rot
  becomes a data update.
- Contributors can add game support without writing Rust, which widens the pool
  substantially.
- The manifest engine is a fixed interpreter, so its security surface is reviewed once
  rather than per contribution.
- Expressiveness is deliberately capped. Some games will need tier 2, and that is the
  intended escape hatch rather than a failure.
- Ongoing discipline required: every request to "just add a small expression feature"
  to the manifest format must be refused or it becomes a scripting language.
- Two execution paths to maintain and test.

Design: [`PARSER_ARCHITECTURE.md`](../PARSER_ARCHITECTURE.md) §2–4.
