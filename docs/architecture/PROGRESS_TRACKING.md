# Progress Tracking

**Status:** design. `games.completion_pct` and `games.completion_state` exist and
are displayed; nothing writes `completion_pct` meaningfully today. Planned: Phase 5.

Owns: `progress_metrics`, and the contract governing `games.completion_pct`.

Input: facts from [`PARSER_ARCHITECTURE.md`](./PARSER_ARCHITECTURE.md) and counts
from [`ACHIEVEMENT_SYSTEM.md`](./ACHIEVEMENT_SYSTEM.md).

---

## 1. The problem with one number

`games.completion_pct` is a single float, displayed prominently in the Game Details
hero badge. Today nothing computes it.

The moment achievement parsing, story-progress parsing and collectible counting all
exist, all three will want to write it. Two subsystems alternately writing one
column produces a number that visibly flickers and that nobody can explain — the
classic version of this bug.

**Decision: many named metrics, one designated headline.** `completion_pct` becomes
a documented derived cache with exactly one writer. Recorded in
[ADR-0011](./adr/0011-completion-pct-is-derived.md).

## 2. Schema

```sql
CREATE TABLE progress_metrics (
  game_id     TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
  key         TEXT NOT NULL,        -- 'achievements'|'story'|'collectibles'|...
  label       TEXT NOT NULL,        -- display label
  value_num   REAL NOT NULL,
  value_max   REAL,                 -- NULL for open-ended counters
  kind        TEXT NOT NULL,        -- 'percent'|'count'|'enum'
  source      TEXT NOT NULL,        -- 'achievements'|'extract:<code>'|'user'
  is_headline INTEGER NOT NULL DEFAULT 0,
  updated_at  TEXT NOT NULL,
  PRIMARY KEY(game_id, key)
);
```

A game may have several metrics. At most one is the headline, enforced by the
aggregator rather than by a constraint (SQLite cannot express "at most one true per
group" cleanly, and the aggregator is the only writer anyway).

## 3. Headline selection

Deterministic, in order:

```
1. explicit user choice, if ever offered      (not in Phase 5)
2. metric key 'achievements', if it exists and value_max > 0
3. the metric with the largest value_max
4. none  → the hero badge falls back to completion_state
```

Rule 2 encodes a judgement: achievement percentage is the figure players recognise
and compare, so it wins by default when available.

Rule 4 matters more than it looks — most games will have no metrics for a long time,
and the page must look finished anyway. The hero badge showing `42%` today comes
from `completion_pct`; when no metric exists that value stays whatever it was, and
the honest display is the completion state.

## 4. The `completion_pct` contract

```
games.completion_pct
  - DERIVED. A cache of the headline metric's percentage.
  - Exactly one writer: progress::aggregate().
  - Never written by a parser, a command, or the UI.
  - Recomputed when facts change; unchanged otherwise.
  - If no headline metric exists, retains its previous value and is not displayed
    as a computed figure.
```

This needs a comment in the migration and in `models.rs`, because a bare
`completion_pct REAL` on a table invites exactly the write that breaks it.

Note the existing `set_completion` command writes `completion_state` and passes
`completion_pct` through unchanged — consistent with this contract. As of the
current UI pass that command has no caller (see
[Detection roadmap open decisions](./IMPLEMENTATION_ROADMAP.md#open-decisions)).

## 5. Aggregation

```
facts arrive (post-snapshot, post-extraction)
      │
      ├─ Counter/Flag/Enum facts → upsert progress_metrics rows
      │     key/label/kind from the fact; source = extractor code
      │
      ├─ achievement counts → upsert metric key='achievements'
      │     value_num = unlocked, value_max = total, kind='percent'
      │
      ├─ select headline (§3)
      │
      ├─ recompute games.completion_pct from the headline
      │
      └─ emit `facts_updated`  → UI refreshes
```

Aggregation is pure: same facts in, same metrics out. That makes it testable
without a filesystem and safe to re-run.

## 6. Deliberate non-goals

| Not doing | Why |
|---|---|
| Cross-game "true completion" score | An opinion dressed as data. Needs real usage to inform, and would be wrong on first attempt |
| Normalising progress between games | 100% in one game is not comparable to 100% in another; pretending otherwise is misleading |
| Estimating "time to complete" | Requires a corpus we do not have and will not collect |
| Inferring story progress from playtime | Plausible-looking and frequently wrong |
| Folding user goals into the headline | Makes the number incomparable; if wanted, it is a separate named metric |

Each of these is easy to add later and hard to remove once users have seen it.

## 7. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Two writers on `completion_pct` | High | Single-writer contract, documented at the schema |
| Metric explosion (one per parser whim) | Medium | Keys are a curated vocabulary; a new key needs justification, like a new tier-2 parser |
| Headline flapping between metrics | Medium | Deterministic ordering (§3), no ties broken randomly |
| Percentage regressing after a re-parse | Medium | Aggregation is pure and idempotent; achievement state keeps earliest timestamps |
| Metric with `value_max = 0` | Low | Excluded from headline selection; displayed as a count, not a percentage |

## 8. Future

- User-selected headline metric.
- Per-playthrough progress, once bindings support multiple save paths per role
  (already possible — see [Detection §11](./GAME_SAVE_DETECTION.md)).
- Progress history over time, reusing the `play_sessions` timeline.
- Milestones ("50% complete") as timeline events.
