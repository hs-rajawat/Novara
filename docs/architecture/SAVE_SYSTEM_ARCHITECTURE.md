# Save System Architecture

**Status:** design. `save_mgr/` exists and is production code (backup, restore,
archive). Everything else here is planned.

The umbrella document: layer model, subsystem boundaries, the save vault, module
layout and IPC. Subsystem detail lives in the sibling documents linked below.

Owns: the layer model, the vault, `save_backups` (and its new columns), the Rust
module layout, the IPC surface.

---

## 1. Four layers

Each layer depends only on the one below it.

```
┌─────────────────────────────────────────────────────────────┐
│ L4  PRESENTATION      progress aggregation · UI             │
│                       PROGRESS_TRACKING · ACHIEVEMENT_SYSTEM │
└─────────────────────────────────────────────────────────────┘
                          ▲ facts, metrics
┌─────────────────────────────────────────────────────────────┐
│ L3  CONTENT           snapshot · diff · extract             │
│                       this doc (vault) · PARSER_ARCHITECTURE │
└─────────────────────────────────────────────────────────────┘
                          ▲ binding: game + role → path
┌─────────────────────────────────────────────────────────────┐
│ L2  RESOLUTION        score · bind · override · backoff     │
│                       GAME_SAVE_DETECTION                    │
└─────────────────────────────────────────────────────────────┘
                          ▲ candidates + evidence
┌─────────────────────────────────────────────────────────────┐
│ L1  EVIDENCE          KB · locator · verifier · witness     │
│                       GAME_SAVE_DETECTION · KNOWLEDGE_BASE   │
└─────────────────────────────────────────────────────────────┘
```

**The single most important rule in this architecture:** detection knows nothing
about achievements, and parsing knows nothing about detection. They meet at the
binding and nowhere else.

Why it matters concretely — three things this buys:

- Detection can ship and be useful with zero parsers written.
- A parser bug cannot corrupt a binding, and a detection bug cannot corrupt
  achievement data.
- Either side can be rewritten without touching the other, which over years is
  the difference between a system that can be maintained and one that must be
  replaced.

The temptation to violate it will take the form of "the Goldberg parser could tell
us where the saves are, so let's have it feed detection". Resist: express that as
a KB entry or an evidence producer, not as a dependency from L3 back into L1.

## 2. Subsystems

| Subsystem | Layer | Responsibility | Must not | Detail |
|---|---|---|---|---|
| `kb` | L1 | Match games to save-location entries | Touch the filesystem | [KB](./KNOWLEDGE_BASE.md) |
| `locator` | L1 | Expand templates, generate aliases → candidate paths | Score or decide | [Detection §7–8](./GAME_SAVE_DETECTION.md) |
| `verifier` | L1 | Judge whether contents look like saves | Consider names; read file contents | [Detection §9](./GAME_SAVE_DETECTION.md) |
| `witness` | L1 | Observe writes during a session | Interpret them | [Detection §10](./GAME_SAVE_DETECTION.md) |
| `resolver` | L2 | Decide bindings; own overrides and backoff | Read save content | [Detection §6, §11–12](./GAME_SAVE_DETECTION.md) |
| `vault` | L3 | Snapshot, restore, retention, integrity | Understand save formats | §3 below |
| `extract` | L3 | Bound files → typed facts | Write to the filesystem | [Parsers](./PARSER_ARCHITECTURE.md) |
| `progress` | L4 | Facts → named metrics | Parse anything | [Progress](./PROGRESS_TRACKING.md) |
| `achievements` | L4 | Definitions, state, user goals | Be overwritten by parsers | [Achievements](./ACHIEVEMENT_SYSTEM.md) |

## 3. The vault

The only component that writes to disk on the user's behalf. It is therefore held
to a higher standard than the rest of the system: every operation must be safe to
interrupt, and no operation may leave a save folder in a partial state.

### 3.1 Snapshots

A snapshot is a content-addressed archive of a binding's files at a point in time.

**Cold vs hot.** A snapshot taken while the game is running can be torn — half the
files from before a write, half after. Restoring a torn save can corrupt a
playthrough.

- **Cold** (`is_hot = 0`): taken at `session_ended`, when the process has exited.
  The default and the only kind that is offered for restore without a warning.
- **Hot** (`is_hot = 1`): taken on user request while a game runs. Permitted,
  because a user asking for a backup right now usually has a reason, but **labelled
  in the UI and in the database** so it is never silently presented as trustworthy.

The distinction is recorded rather than prevented. Refusing hot snapshots would be
safer and more annoying; labelling them is the better trade, provided the label
actually reaches the restore confirmation dialog.

### 3.2 Schema additions

`save_backups` exists. New columns:

```sql
ALTER TABLE save_backups ADD COLUMN binding_id   TEXT REFERENCES save_bindings(id);
ALTER TABLE save_backups ADD COLUMN content_hash TEXT;
ALTER TABLE save_backups ADD COLUMN is_hot       INTEGER NOT NULL DEFAULT 0;
ALTER TABLE save_backups ADD COLUMN reason       TEXT;
   -- 'session_end' | 'manual' | 'pre_restore' | 'pre_migration'
```

- `content_hash` — hash of the archive's logical content. Two consecutive sessions
  that changed nothing produce one stored archive, not two. On a large library with
  automatic snapshots this is the difference between bounded and unbounded growth.
  It is also the diff key that lets extraction ask "what changed?" cheaply.
- `reason` — `pre_restore` rows are the safety net (§3.3) and must be exempt from
  retention pruning.
- `binding_id` — snapshots follow the binding, not a free-text label. `profile_id`
  is retained during migration; see §6.

### 3.3 Restore

Restore overwrites live data and is the most dangerous operation in the
application. Required sequence:

```
1. refuse if a session for this game is active
2. take a snapshot of the current state  (reason = 'pre_restore')
3. extract the archive to a temp directory on the same volume
4. validate: entry count, total size, no traversal, no symlinks
5. swap into place (rename where the platform allows it)
6. on any failure: leave the original untouched; the temp dir is discarded
```

Step 2 is not optional and is not a setting. A user who restores the wrong
snapshot must be able to get back, and "are you sure?" is not a recovery
mechanism.

### 3.4 Retention

Unbounded automatic snapshots will fill a disk. Proposed default:

| Class | Policy |
|---|---|
| `session_end` | Keep the most recent N per game (default 10) |
| `manual` | Keep all |
| `pre_restore` | Keep all — never pruned |
| Deduplicated | Identical `content_hash` collapses to one stored archive |

Retention must be a documented default the user can change, and pruning must be
logged, because silently deleting a backup is the second-worst thing this system
could do.

### 3.5 Hostile input

Every archive is untrusted — it may have come from another machine, an older
version, or a user's manual edit. Caps on entry count, uncompressed size and
compression ratio; rejection of absolute paths, `..` and symlinks; extraction to a
temp directory before any swap. Detail in
[Detection §13](./GAME_SAVE_DETECTION.md).

## 4. Rust module layout

```
src-tauri/src/
  resolve/              ← lifted out of metadata/ (§5)
    mod.rs              Lookup<T>, TemporaryReason, PermanentReason
    breaker.rs          circuit breaker      (moved)
    throttle.rs         shared rate limiter  (moved)
    offline.rs          network gate         (moved)
  saves/
    mod.rs              SaveService — the only type commands touch
    kb/
      mod.rs            matching, template expansion
      builtin.rs        embedded dataset
      import.rs         community/user import + validation
    locator.rs          candidate generation
    verifier.rs         content plausibility
    witness.rs          session-scoped observation
    resolver.rs         decision table, bindings, backoff
    vault/
      mod.rs            ← today's save_mgr/mod.rs
      archive.rs        ← today's save_mgr/archive.rs
      snapshot.rs       cold/hot capture, hashing, dedup
      restore.rs        pre-restore safety, atomic swap
      retention.rs      pruning
  facts/
    mod.rs              Fact enum, FactSink
    extract/            see PARSER_ARCHITECTURE.md
    progress.rs         see PROGRESS_TRACKING.md
  achievements/
    mod.rs              see ACHIEVEMENT_SYSTEM.md
```

`SaveService` as a single façade is deliberate: commands should not be able to
call `resolver` or `verifier` directly, because that is how layer violations get
introduced one convenient shortcut at a time.

## 5. Reusing the metadata stack

`metadata/` already solved problems this system will hit:

| Existing | Reuse for |
|---|---|
| `Lookup<T>` (`Found`/`Unsupported`/`Temporary`/`Permanent`) and its `TemporaryReason` / `PermanentReason` | Every provider-style trait here — the four-way distinction between "no data", "try later" and "never try again" is exactly right for KB fetch and extraction |
| `breaker.rs` | Community KB fetch |
| `throttle.rs` | Shared outbound rate limit — must be the *same* limiter, not a second one |
| `title_resolver.rs` + migration `0007_artwork_backoff` | The per-game persisted-attempt-with-backoff pattern → `save_scan_attempts` |
| `capability.rs` | Declaring what a provider can do before calling it |
| `integrity/` sweep + backoff | Periodic binding re-verification |
| `events.rs` | Progress reporting for long scans |

**Recommendation: promote `Lookup<T>` + its reason enums, `throttle.rs` and
`breaker.rs` into `resolve/` before Phase 1.** They are generic, already
battle-tested, and copying them is how a codebase ends up with three circuit breakers
that behave differently under load. This is the whole content of Phase 0's first track.

Two things deliberately **stay** in `metadata/`, contrary to an earlier draft of this
document:

- **`offline.rs`** is not a network gate. It is a null `MetadataTextProvider` that
  returns `Lookup::Unsupported`, keeping `MetadataService` functional with no network
  configured. It is metadata-specific and does not move.
- **`LookupContext`** holds `&GameIdentity` and is therefore metadata-specific. Only
  the `Lookup` result type is generic. The save system will need its own context type.

The actual network gate is `Db::allow_metadata_network()` (`metadata_enabled &&
!offline_mode`), surfaced to providers as `LookupContext::allow_network`. It is also
metadata-specific; the save system's only networked component is the community KB
fetch, which will gate on the same setting but through its own path.

## 6. Migrating `save_profiles`

`save_profiles` is live user data. `save_bindings` supersedes it.

```
for each save_profiles row:
    insert save_bindings {
        game_id, role='saves', path=source_dir, glob,
        origin='user', confidence=1.0,
        is_locked=1,                    ← existing choices are sacred
        auto_backup, created_at
    }
save_backups.binding_id ← backfilled from profile_id
```

`is_locked = 1` for every migrated row. These paths were chosen by a human, and no
future KB or scoring change may move them. `save_profiles` is retained read-only
for one release cycle, then dropped once the backfill is verified.

## 7. IPC surface

Style follows `lib/ipc.ts`: thin typed wrappers, logic in services, every command
idempotent, long work reported through events.

```
saves_get_state(game_id)   → { binding?, candidates[], last_scan, kb_matched }
saves_rescan(game_id)      → void          (emits progress; cancellable)
saves_bind(game_id, path, role)  → void    (sets is_locked = 1)
saves_reject(candidate_id) → void
saves_snapshot(game_id, note?)   → SnapshotRef
saves_restore(snapshot_id) → void          (pre-restore snapshot first)
saves_list_snapshots(game_id)    → Snapshot[]

kb_status()                → { layers: [{layer, version, entry_count}] }
kb_refresh()               → RefreshReport  (network-gated)
kb_add_user_entry(entry)   → void
```

Achievement and progress commands are defined in their own documents.

Two rules worth stating because they are easy to erode:

1. **Every mutating command emits an event.** The UI never polls and never guesses
   whether something changed.
2. **No command blocks on a long filesystem or network operation.** It starts work
   and returns; progress arrives on `novara://event`. The metadata fetcher already
   works this way.

New events: `save_candidate_found`, `save_binding_changed`, `snapshot_created`,
`kb_updated`.

## 8. Cross-cutting concerns

| Concern | Position |
|---|---|
| **Offline-first** | Full function with no network. Only the community KB layer needs it. |
| **Multi-profile** | A `profiles` table exists and is unused. Whether saves are per-profile must be decided before Phase 3 — retrofitting is a painful migration, adding a nullable column now is trivial. See [roadmap open decisions](./IMPLEMENTATION_ROADMAP.md#open-decisions). |
| **Cancellation** | Every scan and snapshot is cancellable; a cancelled operation leaves no partial rows. |
| **Idempotence** | Re-running any operation is safe. Detection is naturally idempotent because evidence is append-only and candidates are keyed on `(game_id, path, role)`. |
| **Observability** | Scan outcomes recorded in `save_scan_attempts`; `scan_runs` precedent already exists for auditing. |
| **Testing** | Filesystem work goes through a trait so tests use temp directories; `test_support.rs` already establishes this pattern. |
