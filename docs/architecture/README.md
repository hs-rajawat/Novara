# NOVARA Architecture Documentation

Design documentation for NOVARA's save, achievement and progress subsystems.

These documents describe **intended** architecture. Where the code has not caught
up yet, that is stated explicitly rather than implied. A document that quietly
describes an aspiration as if it were built is worse than no document.

---

## Where to look

| Document | Owns | Read it when |
|---|---|---|
| [`SAVE_SYSTEM_ARCHITECTURE.md`](./SAVE_SYSTEM_ARCHITECTURE.md) | The four-layer model, subsystem boundaries, the save vault (snapshot / restore / retention), IPC surface, Rust module layout | You need the shape of the whole thing, or you are touching snapshots and restore |
| [`GAME_SAVE_DETECTION.md`](./GAME_SAVE_DETECTION.md) | The detection pipeline: candidate generation, aliases, verification, the evidence model, the Write Witness, bindings, filesystem boundaries | You are working on *finding* a game's save location |
| [`KNOWLEDGE_BASE.md`](./KNOWLEDGE_BASE.md) | The KB as a long-lived asset: the three layers, schema, versioning, trust and distribution, contribution workflow | You are adding save-location data, or building KB import/update |
| [`PARSER_ARCHITECTURE.md`](./PARSER_ARCHITECTURE.md) | Extractor tiers, the declarative manifest format, the compiled-provider registry, sandboxing rules | You are reading data *out of* save files |
| [`ACHIEVEMENT_SYSTEM.md`](./ACHIEVEMENT_SYSTEM.md) | Definitions vs state vs user goals, unlock derivation, merge rules | You are working on achievements |
| [`PROGRESS_TRACKING.md`](./PROGRESS_TRACKING.md) | The progress metric model, headline selection, the `completion_pct` contract | You are working on completion or progress display |
| [`IMPLEMENTATION_ROADMAP.md`](./IMPLEMENTATION_ROADMAP.md) | Phase sequencing, per-phase scope, open decisions blocking each phase | You are planning work, or wondering why something is deferred |
| [`TESTING.md`](./TESTING.md) | Test strategy: levels, the filesystem abstraction, fixture rules, invariants, CI budget | You are writing tests, or your change makes something untestable |
| [`adr/`](./adr/README.md) | **Architecture Decision Records** — one file per decision, with alternatives considered and consequences | You want to know *why*, or you are about to reverse something |

## Test plans

Case matrices live outside `architecture/`, because they grow with the corpus
rather than with the design.

| Plan | Covers |
|---|---|
| [`../testing/SAVE_DETECTION_TEST_PLAN.md`](../testing/SAVE_DETECTION_TEST_PLAN.md) | The detection scenario corpus — ~350 cases at maturity |
| _(planned)_ `../testing/VAULT_TEST_PLAN.md` | Snapshot, restore, retention, hostile archives |
| _(planned)_ `../testing/PARSER_TEST_PLAN.md` | Manifest engine, compiled extractors, fuzz corpus |

## Related documents outside this directory

| Document | Scope |
|---|---|
| `/DESIGN.md` | The **UI** design system — tokens, components, spacing, page composition. Not concerned with backend architecture. |
| `/GAME_DETAILS_REDESIGN.md` | Visual specification for the Game Details page. |
| `/ROADMAP.md`, `/PROJECT_STATUS.md` | Product-level roadmap and status. `IMPLEMENTATION_ROADMAP.md` here is narrower: it sequences *this* architecture only. |

---

## Rules these documents follow

**One owner per table.** Every database table is defined in exactly one document —
the one that owns the subsystem writing to it. Other documents reference it by
name and link, and never restate its DDL. Ownership:

| Tables | Owner document |
|---|---|
| `save_kb_entries`, `save_kb_versions` | `KNOWLEDGE_BASE.md` |
| `save_candidates`, `save_witness_events`, `save_bindings`, `save_scan_attempts` | `GAME_SAVE_DETECTION.md` |
| `save_backups` (+ new columns), retention state | `SAVE_SYSTEM_ARCHITECTURE.md` |
| `parser_manifests` | `PARSER_ARCHITECTURE.md` |
| `achievement_definitions`, `achievement_state`, `achievements` (existing) | `ACHIEVEMENT_SYSTEM.md` |
| `progress_metrics` | `PROGRESS_TRACKING.md` |

**No duplicated prose.** If a concept needs explaining twice, it belongs in one
document and a one-line summary plus a link in the other. Duplicated design text
drifts, and the copy that drifts is always the one someone reads.

**Status labels.** Every major section carries one:

- `IMPLEMENTED` — exists in the codebase today
- `PLANNED (Phase N)` — designed, not built
- `DEFERRED` — deliberately not being built, with the conditions that would reopen it
- `SUPERSEDED` — replaced; kept because the reasoning is still useful

**Decisions get their own record.** Anything that would be expensive to reverse, or
that a reasonable contributor might propose the opposite of, gets an
[ADR](./adr/README.md) — numbered, with alternatives considered and consequences
stated. Accepted ADRs are immutable: a changed decision is a *new* ADR superseding the
old one, never an edit. The architecture will be wrong about things; the record of
*how* it was wrong is what stops the same mistake twice.

`DECISIONS.md` is a redirect from the former running log and is not maintained.

---

## Reading order for a new contributor

1. `SAVE_SYSTEM_ARCHITECTURE.md` §1–2 — the layer model and why detection and
   parsing are separated.
2. `GAME_SAVE_DETECTION.md` §3–5 — the pipeline and the evidence model. This is
   the conceptual core of the system.
3. `IMPLEMENTATION_ROADMAP.md` — what is actually being built now.
4. Whichever subsystem doc matches your task.

If you only read one section, read `GAME_SAVE_DETECTION.md` §5 (Evidence model).
Nearly every design constraint in the rest of the system follows from it.
