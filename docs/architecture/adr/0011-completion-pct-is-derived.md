# ADR-0011: `completion_pct` is a derived cache with one writer

- **Status:** Accepted
- **Date:** 2026-07-30
- **Affects:** Progress
- **Supersedes:** — · **Superseded by:** —

## Context

`games.completion_pct` is a single `REAL` column that exists in the schema and is
displayed prominently in the Game Details hero badge. Nothing computes it meaningfully
today.

Three planned subsystems will each have a legitimate claim to it: achievement
percentage, story progress, and collectible counts. A column written by three
subsystems produces a number that visibly changes depending on which ran last, and that
nobody can explain.

The column's semantics were never defined, which is the actual defect — the multiple
writers are a symptom.

## Decision

Progress is modelled as **many named metrics per game** (`progress_metrics`), with one
designated headline. `games.completion_pct` becomes a documented derived cache of the
headline metric, with exactly one writer: `progress::aggregate()`.

No parser, command or UI path writes it.

## Alternatives considered

| Option | Why not |
|---|---|
| Keep one column, agree on a priority order between writers | An informal convention across three subsystems and years of contributors. It will be violated, and the violation will be a flickering number nobody can trace |
| Drop the column; compute on read | Every list view would need a join and an aggregation. The cache exists for a reason |
| One column per metric on `games` | Schema churn for every new metric, and no way to add a metric as data |
| Let the user pick the number's meaning | Reasonable eventually, but it does not remove the need for a single writer, and it is not needed in Phase 5 |
| Weighted blend of all metrics into one score | Invents an opinion and makes the number incomparable between games. Explicitly a non-goal |

## Consequences

- One writer, so the value is always explicable: it is the headline metric.
- More than one metric can be displayed, which the single column could never support.
- Headline selection must be deterministic, or the displayed number flaps between
  metrics. Chosen order: explicit user choice, then `achievements`, then largest
  `value_max`, then none.
- The metric key vocabulary needs curation, or every parser invents its own key and the
  UI fills with noise.
- Requires a comment at the schema and in `models.rs`, because a bare
  `completion_pct REAL` invites exactly the write this forbids.
- Games with no metrics keep whatever value the column holds and fall back to
  displaying `completion_state`. That fallback must not read as a computed figure.

Design: [`PROGRESS_TRACKING.md`](../PROGRESS_TRACKING.md) §2–4.
