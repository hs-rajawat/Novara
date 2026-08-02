# Implementation Roadmap

Sequences the save, detection, parser, achievement and progress architecture. This
is narrower than `/ROADMAP.md`, which is product-level.

Estimates assume one developer familiar with the codebase.
**S** ≈ days · **M** ≈ 1–2 weeks · **L** ≈ 2–4 weeks · **XL** ≈ 1–2 months.

---

## Sequencing principle

Phases are ordered by **dependency, not by visible value**. Two rules produced the
order below:

1. **Nothing that reads save content ships before the binding layer.** Achievement
   parsing and progress parsing both need to know which files are the save files.
2. **Monitoring precedes parsing.** Diffing snapshots is how you find the bytes
   that changed when something was unlocked.

This is why Achievements — the most visible feature — is Phase 6 rather than Phase 1.
Shipping it earlier would mean hard-coding paths per game, which is the collection
of one-off solutions this architecture exists to avoid.

---

## Phase 0 — Foundations

**Purpose:** make the layer boundaries real before building on them. No
user-visible change.

| Aspect | Detail |
|---|---|
| Architecture | Promote `Lookup<T>` + `TemporaryReason` / `PermanentReason`, `throttle` and `breaker` from `metadata/` into `resolve/`. Move `save_mgr/` → `saves/vault/` and `save_detect.rs` → `saves/locator.rs`. **Introduce the `FileSystem` trait** (`RealFs` / `VirtualFs`) and route detection through it. `offline.rs` and `LookupContext` stay in `metadata/` — see [SAVE_SYSTEM_ARCHITECTURE §5](./SAVE_SYSTEM_ARCHITECTURE.md#5-reusing-the-metadata-stack). |
| Database | None |
| Backend | Refactor only. `metadata/` consumes `resolve/`. `saves/locator.rs` no longer calls `dirs::` or `std::fs` directly. |
| Frontend | None |
| Risks | Touching working metadata code — mitigated by the existing suite ([250 tests](../testing/BASELINE.md)). Routing detection through the trait is the one real regression risk, because `save_detect.rs` had no tests; manual verification plus new tests land in the same phase. |
| Testing | The existing suite is the specification: no assertion may change, only `use` paths. **The `FileSystem` trait is the precondition for every test in [`SAVE_DETECTION_TEST_PLAN.md`](../testing/SAVE_DETECTION_TEST_PLAN.md)** — without it the detection corpus cannot exist. Phase 0 adds the first locator tests and seeds invariants I2 and I3. |
| Complexity | **M** |

Doing this first is the difference between one circuit breaker and three — and
between a detection suite of hundreds of cases and no detection suite at all.

**Deferred out of Phase 0:** the `SaveService` façade. It is a wrapper with no
behaviour of its own, and the layer-violation risk it guards against does not
materialise until Phase 1 introduces `resolver` and `verifier` for a command to reach
into. Revisit at the start of Phase 1.

## Phase 1 — Knowledge Base + candidates

**Purpose:** replace guess-only detection with KB matching, persisted candidates
and content verification. Ships as "NOVARA finds your saves properly."

| Aspect | Detail |
|---|---|
| Architecture | `kb/` (built-in + user layers), `locator.rs` (lifting today's title variants), `verifier.rs`, decision table v1 |
| Database | `save_kb_entries`, `save_kb_versions`, `save_candidates`, `save_scan_attempts` |
| Backend | `saves_get_state`, `saves_rescan`, `saves_reject`; `kb_status`, `kb_add_user_entry` |
| Frontend | Saves tab lists candidates with human-readable reasons |
| Risks | **Empty corpus makes it look broken** — seed engine/OS convention rules + curated titles first. Scan cost on large libraries — bounded per §7.2 and backoff-gated. |
| Testing | Locator/verifier are pure over a temp-dir fixture tree. Decision table is table-driven, one case per rule. KB matching tested per `match_kind` precedence. |
| Complexity | **L** |

## Phase 2 — Write Witness

**Purpose:** the differentiator. Detect saves by observing writes during a session.

**Primary use case, confirmed by real-library validation:** telling apart several *real*
save locations for one game. A user who changes store or edition accumulates directories —
both libraries validated in Phase 1 showed it (Dying Light and Red Dead Redemption 2 each
had a current location plus one left behind by an earlier install). Phase 1 correctly reports
all of them and **must not be changed to suppress any**; it simply cannot tell which is live,
because name, KB and content shape describe them equally well and an mtime comparison is a
guess a cloud sync can defeat. Observation is the only thing that settles it, which makes
this the clearest argument for the whole subsystem. See
[§10.1.1](./GAME_SAVE_DETECTION.md) and `scenarios/multi-path/`.

**2a — mtime sweep.** Directory mtimes at session start and end. No watchers.

**2b — filesystem watchers.** `notify`-based, finer-grained, falls back to 2a.

| Aspect | Detail |
|---|---|
| Architecture | `witness.rs`, hooked to existing `session_started`/`session_ended`. Decision table v2 adds rules 3, 4, 6. |
| Database | `save_witness_events` (+ pruning) |
| Backend | Session hooks; evidence ingestion. Classify locations **active / historical / unused** — derived from evidence, never from a heuristic, and presentation only: a historical location stays bind-eligible if its evidence says so. |
| Frontend | Evidence copy: "changed while you were playing". Group a game's locations by class so a migrating user can see which is current *and* still reach the old one. |
| Extension point | User states (`Imported`, `Pinned`, …) are a **second, orthogonal axis** above the evidence model, not more values of the observation class — a location can be `Historical` *and* `Pinned`. Deferred to Phase 2/3; see [§10.1.2](./GAME_SAVE_DETECTION.md) for the test that decides whether a proposed state is evidence or presentation. |
| Risks | Overlapping sessions mis-attributing writes (require two sessions, §10.3). Watcher instability in 2b — 2a remains the fallback. Descriptor exhaustion. **Classification becoming authority** — if recency starts deciding outcomes, Phase 1's decisions stop being reproducible from evidence. **User state leaking into `save_candidates.status`** — that column belongs to the decision table alone. |
| Open | `Evidence::WriteWitness` may need an `at` field. Distinguishing recent from earlier sessions otherwise requires joining `play_sessions`, breaking §6's "reproducible from the evidence set alone". `UserConfirmed { at }` sets the precedent; the enum is version-tolerant so adding it later is safe. |
| Testing | Simulated session windows over a temp tree; assert correct candidate promotion. Concurrent-session ambiguity is an explicit test case. `scenarios/multi-path/` already holds a store-migration fixture, skipped until this lands. |
| Complexity | **L** (2a is **M** alone) |

Ship 2a early. It is most of the value at a fraction of the risk.

## Phase 3 — Binding lifecycle

**Purpose:** a durable, user-correctable answer.

| Aspect | Detail |
|---|---|
| Architecture | `resolver.rs` owns bindings. `is_locked` invariant enforced. |
| Database | `save_bindings`; migrate `save_profiles` with `is_locked = 1`; backfill `save_backups.binding_id` |
| Backend | `saves_bind` |
| Frontend | Binding display with origin and confidence; correction flow |
| Risks | **Migrating live user data.** Retain `save_profiles` read-only for one release; verify backfill before dropping. |
| Testing | Migration test with realistic prior data. Explicit test: a KB refresh must not alter a locked binding. |
| Complexity | **M** |

## Phase 4 — Vault hardening

**Purpose:** make automatic snapshots safe and bounded.

| Aspect | Detail |
|---|---|
| Architecture | `snapshot.rs` (cold/hot, hashing, dedup), `restore.rs` (pre-restore snapshot, atomic swap), `retention.rs` |
| Database | `save_backups`: `binding_id`, `content_hash`, `is_hot`, `reason` |
| Backend | Session-end auto-snapshot; hardened restore |
| Frontend | Snapshot list with hot/cold labelling; restore confirmation naming the pre-restore snapshot |
| Risks | **Restore is the most dangerous operation in the app.** Pre-restore snapshot mandatory; refuse during a live session. Disk growth — retention from day one, not later. |
| Testing | Hostile archive fixtures (traversal, bomb, symlink, truncated). Interrupted-restore test: original must survive. Dedup test: unchanged saves produce one archive. |
| Complexity | **M** |

## Phase 5 — Facts and progress

**Purpose:** read meaning out of saves; give `completion_pct` a definition.

| Aspect | Detail |
|---|---|
| Architecture | `facts/extract/declarative.rs`, `facts/progress.rs` |
| Database | `progress_metrics`, `parser_manifests` |
| Backend | `progress_get`; extraction on session end, diff-gated |
| Frontend | Progress metric display |
| Risks | Manifest engine scope creep into a scripting language — fixed format list. Parser rot — failures are `Permanent` with backoff. |
| Testing | Manifest engine is table-driven over fixtures. Every shipped manifest needs a fixture. Unchanged-snapshot test must yield zero facts. |
| Complexity | **L** |

## Phase 6 — Achievements

**Purpose:** the flagship feature, on foundations that are already proven.

| Aspect | Detail |
|---|---|
| Architecture | `achievements/`, `extract/steam_stats.rs`, `extract/goldberg.rs` |
| Database | `achievement_definitions`, `achievement_state` |
| Backend | `achievements_get`, `achievements_set_state`; emit existing `achievement_unlocked` |
| Frontend | **None.** The Achievements card is already built and renders an honest empty state. |
| Risks | Parsed data destroying user goals — separate tables, enforced by test. Inferred timestamps presented as precise — `inferred_time` must reach the UI. |
| Testing | Real anonymised save fixtures. Explicit test: re-parsing never modifies a `source='user'` row. |
| Complexity | **L** |

That the frontend row reads "none" is the payoff for having built the card against a
defined empty state two passes earlier.

## Phase 7 — Compiled extractors

Ongoing, per format. Each is a small PR with fixtures. No schema or UI change. A
tier-2 extractor must justify why a manifest was insufficient.
**Complexity: S each, ongoing.**

## Phase 8 — Community KB

| Aspect | Detail |
|---|---|
| Architecture | `kb/import.rs`, fetch behind the network gate, checksum verification, transactional per-layer replace |
| Database | `save_kb_versions` rows for the community layer |
| Backend | `kb_refresh`; contribution export (templated, previewed) |
| Frontend | KB settings: version, coverage, refresh, contribute |
| Risks | Supply chain (§7 of KB doc). Privacy in contributions — explicit consent, on-device templating. Licence review if seeding from a third-party dataset. |
| Testing | Malformed/oversized payload rejection. Refresh must not touch the user layer or any locked binding. |
| Complexity | **M** |

## Deferred indefinitely

| Item | Why | Reopen when |
|---|---|---|
| Dynamic plugins | Permanent ABI, sandboxing and security cost; benefits obtainable via tiers 1–2 | A contributor community exists that cannot ship in-tree *and* someone owns security review |
| KB payload signing | Key management and failure-mode complexity outweigh the benefit while the layer is first-party | The community layer becomes genuinely third-party-writable |
| Cross-game completion scoring | An opinion dressed as data | Real usage informs what players actually want compared |
| Confidence calibration from telemetry | Requires collection we have committed not to do | Never, unless opt-in and clearly worth it |

---

## Phase gates are test outcomes

"Detection works" is unfalsifiable. Each phase is considered done when a named set
of scenarios and invariants passes — the gates are specified in
[`SAVE_DETECTION_TEST_PLAN.md`](../testing/SAVE_DETECTION_TEST_PLAN.md) §6 and the
invariants in [`TESTING.md`](./TESTING.md) §4. In summary:

| Phase | Gate |
|---|---|
| 0 | Existing suites pass unchanged; `VirtualFs` exists and a trivial scenario runs |
| 1 | Every Phase-1-reachable decision row has a positive and a negative case; invariants I2, I3, I9, I10 |
| 2 | `emulated/witness-beats-alias.toml` passes, rows 3/4/6 covered; invariants I1, I4 |
| 3 | `override/`, `kb-refresh/`, `migration/` populated; invariants I1, I7 |
| 4 | Hostile-archive corpus passes; invariants I6, I8 |
| 6 | Re-parse never modifies a `source='user'` row; invariant I5 |

### Phase 1 scope, as agreed

Narrower than the phase table above, deliberately:

- **In:** knowledge base (built-in + user layers), candidate generation, verifier,
  resolver, scenario corpus.
- **Out:** the Write Witness and file watchers (Phase 2); `save_bindings`,
  `is_locked` and **automatic binding** (Phase 3); the community KB layer and any
  network access (Phase 8); progress and achievement extraction (Phases 5–6); all
  frontend work.
- **Consequence of excluding automatic binding:** the resolver *computes and records*
  a decision without acting on it. `save_candidates.status` carries
  `bind_eligible` where the table says "bind", and Phase 3 converts those into real
  bindings once there is a store and a correction UI. Auto-binding before a user can
  correct it would be the "wrong binding destroys a save" risk with no escape hatch.
- **Phase 1 is not behaviour-neutral.** Unlike Phase 0, `detect_save_paths` will
  return better candidates and better hints. Its IPC *shape* is fixed so no UI change
  is required, but its content changes and needs verification against a real library.
- **KB seeding:** hand-curated only, ~25–50 representative titles plus launcher
  convention rules. No third-party dataset import until the licence review in open
  decision 6 is complete.

---

## Open decisions

These block the phases named. Deciding them late is expensive.

| # | Decision | Blocks | Recommendation |
|---|---|---|---|
| 1 | **Multi-profile saves.** A `profiles` table exists and is unused. Are bindings per-profile? | Phase 3 | Add a nullable `profile_id` to `save_bindings` now. Trivial today, a painful migration later. |
| 2 | **`save_profiles` migration vs coexistence.** | Phase 3 | Migrate with `is_locked = 1`; retain read-only one release. |
| 3 | **Retention default.** | Phase 4 | Keep last 10 `session_end` per game; keep all `manual` and `pre_restore`. |
| 4 | **Vocabulary: "achievements" for both imported and user-authored?** | Phase 6 UI copy | Use "Achievements" for imported and "Goals" for user-authored. Conflating them is the confusion, not the fix. |
| 5 | **Where does the library completion state live?** It was removed from Game Details, so `set_completion` currently has no UI caller and the state is user-unassignable app-wide. | Not blocking, but user-visible now | Decide as part of library-management UI, not this architecture. Recorded so it is not forgotten. |
| 6 | **KB seeding source.** Hand-curated only, or import an existing open dataset? | Phase 1 | Requires a **licence review** before any import code. Engine and OS convention rules give broad coverage licence-free — start there. See [KB §9](./KNOWLEDGE_BASE.md#9-seeding-the-corpus) for why these are engine/OS rather than launcher conventions. |
| 7 | **Verifying the curated seed.** Every `curated:phase1` entry is authored from familiarity, not from a dataset, and is therefore unverified. | Phase 1, task 1.22 | Check against a real library during 1.22. Entries that do not resolve on a machine that owns the game should be corrected or removed, not left hopeful. |
| 8 | **`{STEAM_INSTALL}` anchor.** Steam Cloud's `userdata/<id>/<appid>/remote` is the one genuine launcher convention and is not expressible today. | after Phase 1 | Needs an anchor for the Steam install directory; `scanner::steam` already discovers library locations. Omitted rather than guessed. |

Decision 6 is the one with a real external dependency; the rest are ours to make.
