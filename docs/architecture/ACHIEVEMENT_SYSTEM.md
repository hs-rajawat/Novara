# Achievement System

**Status:** design. The `achievements` and `achievement_templates` tables exist and
hold user-authored data; the UI (Game Details → Achievements) is built and renders
correctly with zero data. Definitions, state and parsing are planned for Phase 6.

Owns: `achievement_definitions`, `achievement_state`, and the existing
`achievements` table.

Input: facts from [`PARSER_ARCHITECTURE.md`](./PARSER_ARCHITECTURE.md).
Output: metrics to [`PROGRESS_TRACKING.md`](./PROGRESS_TRACKING.md).

---

## 1. Two features, one word

"Achievements" in NOVARA means two genuinely different things:

| | User goals | Imported achievements |
|---|---|---|
| Created by | The user typing "Beat the final boss" | Parsed from game files or a catalogue |
| Identity | None — a local row | External (`appid` + `apiname`) |
| Truth | True by definition | An observation that may be wrong |
| Lifetime | **Permanent** — irreplaceable | **Disposable** — rebuildable by re-parsing |
| Table | `achievements` (existing) | `achievement_definitions` + `achievement_state` |

**They must not share storage.** If they do, a parser bug, a re-parse or a
catalogue refresh can destroy hand-entered data that exists nowhere else. The UI
merges them for display; the database keeps them apart.

This was the single largest correction to the original architecture proposal.

## 2. Schema

```sql
-- Catalogue: what achievements exist for this game. Replaceable.
CREATE TABLE achievement_definitions (
  id          TEXT PRIMARY KEY,
  game_id     TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL,        -- 'steam' | 'goldberg' | 'manifest:<id>'
  external_id TEXT NOT NULL,        -- the game's own id (apiname)
  name        TEXT NOT NULL,
  description TEXT,
  icon_path   TEXT,
  points      INTEGER,
  is_hidden   INTEGER NOT NULL DEFAULT 0,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  UNIQUE(game_id, provider, external_id)
);

-- Observations of unlock state. One row per (definition, source).
CREATE TABLE achievement_state (
  definition_id TEXT NOT NULL REFERENCES achievement_definitions(id) ON DELETE CASCADE,
  source        TEXT NOT NULL,      -- 'parsed' | 'user'
  unlocked      INTEGER NOT NULL DEFAULT 0,
  unlocked_at   TEXT,
  inferred_time INTEGER NOT NULL DEFAULT 0,
  observed_at   TEXT NOT NULL,
  PRIMARY KEY(definition_id, source)
);
```

The composite key `(definition_id, source)` is the important design choice: a user
override and a parsed observation **coexist** rather than one overwriting the
other. Re-parsing rewrites only the `parsed` row.

## 3. Deriving display state

Unlock state is **derived, not stored as truth**:

```
effective_unlocked(definition) =
    if exists state(source='user')   → that value        (user wins, always)
    else if exists state(source='parsed') → that value
    else → false
```

Consequences that fall out of this and are worth stating:

- A user can mark something unlocked that the parser cannot see (a pirated build
  with no achievement layer) and re-parsing will not undo it.
- A user can un-mark a false positive and it stays un-marked.
- Re-parsing is always safe. That is what makes parsers replaceable.

## 4. Sources of definitions

| Source | Availability | Notes |
|---|---|---|
| Steam appdetails / schema | Requires network + appid | Names, descriptions, icons. Icons need caching through the existing artwork store |
| Goldberg `steam_settings/achievements.json` | Local, offline | Often ships the *full* catalogue — the best offline source for emulated titles |
| Manifest-declared | Local | Where a save file enumerates its own achievements |
| None | — | A game may have unlock state with no catalogue: definitions are synthesised from observed ids with `name = external_id` |

That last row matters for the UI: a tile strip must render when all we know is
"seven ids exist and three are set". The Achievements card was built to handle it.

## 5. Unlock detection

Facts arrive from extraction after a cold snapshot
([Parsers §7](./PARSER_ARCHITECTURE.md)):

```
AchievementUnlocked { external_id, at: Option<String>, inferred_time }
        │
        ├─ definition exists?  no → synthesise one (provider = extractor code)
        │
        ├─ upsert achievement_state(definition_id, source='parsed')
        │
        └─ newly unlocked?  → emit `achievement_unlocked` event
                              (the event already exists in events.rs)
```

**Timestamps.** Most formats do not record unlock time. When absent, infer the
session end and set `inferred_time = 1`. The UI must not present an inferred time
as precise — "unlocked during this session" rather than "unlocked at 22:47".

**Timestamp regression.** If a parsed timestamp is *earlier* than one already
recorded, keep the earlier one: a re-parse of a full catalogue should not reset
history to today.

## 6. Merge for display

The UI shows one list. Ordering: imported definitions by `sort_order`, then user
goals. Both render in the same tile grid; user goals are visually distinguishable
(they are the user's own, and conflating them is confusing rather than clean).

Progress uses **imported definitions only** for the achievement percentage — user
goals are a personal checklist, and folding them into a completion figure makes the
number incomparable between games. If user goals should count, that is a separate
named metric ([Progress](./PROGRESS_TRACKING.md)).

## 7. UI contract (already built)

The Achievements card in Game Details expects, in this exact order: headline
percentage + the word "Complete", progress bar, `N / M unlocked`, a strip of up to
eight tiles with a `+N` overflow, and a `View All` outlined button. It renders with
zero data as `0%` / `0 / 0 unlocked` / eight locked tiles. See
`/GAME_DETAILS_REDESIGN.md` and `/DESIGN.md` §21.12.

Nothing in this document requires a UI change — which was the point of building
that card against a defined empty state.

## 8. IPC

```
achievements_get(game_id) → { definitions[], state[], goals[] }
achievements_set_state(definition_id, unlocked) → void   -- writes source='user'
achievements_create_goal(input) → Goal                    -- existing command
achievements_toggle_goal(id)    → bool                    -- existing command
```

Existing goal commands are unchanged. New commands only touch imported data.

## 9. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| Parsed data destroys user goals | **Critical** | Separate tables; nothing writes to `achievements` from extraction |
| Re-parse resets unlock history | High | Keep earliest timestamp; user rows untouched |
| False positives from a bad manifest | Medium | User can override; overrides are permanent |
| Fabricated totals ("0 / 51" with no data) | Medium | Never invent a denominator — `0 / 0` is honest, an invented total is not |
| Icon fetching leaks activity | Low | Behind the existing network toggle; cached locally |
| `achievement_templates` orphaned | Low | Existing table for shareable goal sets; unrelated to imports. Decide in Phase 6 whether to keep or fold into the KB pattern |

## 10. Future

- Community goal templates matched by title (`achievement_templates` already
  anticipates this).
- Rarity data, where a catalogue provides it.
- Unlock timeline merged into the existing Timeline view.
- Cross-game achievement statistics — deferred until there is data worth
  aggregating.
