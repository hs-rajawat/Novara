# Game Save Detection

**Status:** design. Today's implementation is `src-tauri/src/save_detect.rs`, which
covers only a fraction of this document (see §17).

The definitive reference for how NOVARA determines where a game keeps its saves.

Owns these tables: `save_candidates`, `save_witness_events`, `save_bindings`,
`save_scan_attempts`.

Related: [`KNOWLEDGE_BASE.md`](./KNOWLEDGE_BASE.md) for the KB as an asset;
[`SAVE_SYSTEM_ARCHITECTURE.md`](./SAVE_SYSTEM_ARCHITECTURE.md) for what happens to
a save folder once it is found.

---

## 1. The problem

Launcher-managed games are tractable. Steam, GOG, Epic, EA, Ubisoft and Xbox
either document their save conventions or expose enough identity (an app id) to
look them up in a shared dataset.

Manually installed games are the real problem, and they are a first-class citizen
in NOVARA rather than an edge case. A single title may store saves in any of:

```
%APPDATA%/<Publisher>/<Title>/
%LOCALAPPDATA%/<Title>/Saved/SaveGames/
%LOCALAPPDATA%Low/<Publisher>/<Title>/
Documents/My Games/<Title>/
Documents/<Title>/saves/
%USERPROFILE%/Saved Games/<Title>/
<install dir>/saves/            ← portable and scene releases
<install dir>/Profiles/
```

The variation is not random but it is not predictable either. Emulated-Steam
layers (Goldberg and similar), repacks, portable builds and older titles each
have their own conventions, and the same game may behave differently depending
on how it was installed. Asking a search engine typically returns three
plausible answers, one of which is right for one release.

**We will not solve this by guessing harder.** The design instead treats
detection as an evidence-gathering problem with a conservative decision rule, and
leans on one signal nobody else has bothered to use.

## 2. Design goals

| Goal | Consequence |
|---|---|
| **Correctness over coverage** | A path we are unsure about is *suggested*, never bound. A wrong binding leads to a wrong restore, and a wrong restore destroys a save. Being unhelpful is recoverable; being wrong is not. |
| **Observation over inference** | Prefer "this folder changed while the game ran" to "this folder's name resembles the game's title". |
| **Never surprise the user** | A confirmed binding is permanent. No update, rescan or algorithm change may silently move it. |
| **Bounded work** | Never walk a disk. Fixed roots, capped depth, capped candidates. Detection must be invisible in cost. |
| **Read-only** | Detection never writes to, creates in, or probes a candidate directory. |
| **Offline by default** | Full-strength detection with no network. The network only ever refreshes the KB. |
| **Explainable** | Every decision can be rendered as a sentence a user understands. If we cannot explain why we picked a folder, we should not have picked it. |
| **Re-scorable** | Store evidence, not just conclusions, so a better algorithm can re-decide without touching the filesystem again. |

That last goal is worth more than it looks. Scoring will be wrong at first. If the
database records *what was observed* rather than only *what was concluded*, the
whole corpus can be re-evaluated on upgrade, offline, in milliseconds.

## 3. Architecture

Detection is layer 1 and 2 of the save system (see
[`SAVE_SYSTEM_ARCHITECTURE.md`](./SAVE_SYSTEM_ARCHITECTURE.md) §1). It ends at a
**binding** and knows nothing about what reads the files afterwards.

```
                    ┌──────────────┐
                    │   BINDING    │  game + role → path   (the contract)
                    └──────▲───────┘
                           │ decide (rule table, §6)
                    ┌──────┴───────┐
                    │   RESOLVER   │  owns bindings, overrides, backoff
                    └──────▲───────┘
                           │ evidence
        ┌──────────────┬───┴────────┬──────────────┐
        │              │            │              │
   ┌────┴────┐   ┌─────┴─────┐ ┌────┴─────┐  ┌─────┴──────┐
   │   KB    │   │  LOCATOR  │ │ VERIFIER │  │  WITNESS   │
   │ match   │   │ candidate │ │ content  │  │ writes during│
   │ entries │   │ generation│ │ plausib. │  │ a session   │
   └─────────┘   └───────────┘ └──────────┘  └────────────┘
        │              │            │              │
        └──────────────┴────────────┴──────────────┘
                   bounded filesystem access
```

Four evidence producers, one decider. The producers do not know about each other
and do not know the decision rule; the resolver does not touch the filesystem.
This is what makes each piece testable in isolation, and it is the main structural
departure from today's `save_detect.rs`, which fuses generation, inspection and
scoring into one function.

## 4. Detection pipeline

### 4.1 Short-circuit first

> The fastest search is the one that never happens.

Before any filesystem work:

```
resolve(game, role):
  binding = bindings.get(game, role)
  if binding exists:
      if binding.is_locked:            → return binding          (no I/O at all)
      if path still exists:            → return binding          (one stat call)
      else:                            → mark unverified, fall through
  if scan_attempts.backoff_active(game):
                                       → return none             (no I/O)
  … full pipeline …
```

A locked binding is returned without even a `stat`. This matters at scale: a
library of several thousand games must not perform thousands of filesystem probes
on load. Detection is a cold-start cost paid once per game, not a recurring one.

### 4.2 Cold start

```
game added / user requests rescan
   │
   ├── KB match ─────────────────► template candidates
   │     steam_appid → gog_id → exe_name → normalised title
   │
   ├── alias generation ─────────► heuristic candidates
   │     title variants × bounded root set
   │
   ├── install-dir probe ────────► portable candidates
   │     <install>/{saves,save,Saved,Profiles,userdata,...}
   │
   ▼
   candidate set (deduplicated, capped, §7)
   │
   ├── verifier.inspect(each)  ── read-only content plausibility (§9)
   │
   ▼
   candidates + evidence persisted
   │
   └── resolver.decide()  ────────► bind │ suggest │ discard   (§6)
```

### 4.3 Warm path (the important one)

```
session_started(game)                    [existing playtime event]
   └── witness.arm(game)

        … game runs …

session_ended(game, t0..t1)              [existing playtime event]
   ├── witness.harvest()  → paths written within t0..t1
   ├── resolver.ingest(WriteWitness evidence)
   ├── resolver.decide()  → may promote a suggestion to a binding
   └── hand off to the vault for a cold snapshot
```

Cold start is a guess. The warm path is a measurement. Over a couple of sessions
the warm path converges on the truth for games the KB has never heard of, which is
precisely the population that motivated the design.

## 5. Evidence model

**This is the conceptual core of the system.** Most other constraints follow from
it.

### 5.1 Evidence is typed and stored

```rust
enum Evidence {
    KbMatch      { entry_id: String, layer: KbLayer, priority: u16 },
    NameMatch    { alias: String, similarity: f32 },
    InstallLocal { subdir: String },
    ContentShape { save_like: u32, total: u32, max_depth: u8, newest_mtime: String },
    WriteWitness { session_id: i64, file_count: u32, bytes: u64 },
    UserConfirmed{ at: String },
    UserRejected { at: String },
}
```

Persisted per candidate as a versioned JSON array (`evidence_json`, with a
`schema` discriminator so old rows remain readable). Append-only: new observations
add entries, they never rewrite history. A `WriteWitness` from three sessions ago
is still evidence.

### 5.2 Why not a single confidence float

Today's detector emits one `confidence` derived from title similarity. That number
answers "how much does this folder's name look like the game's name", which is
**not the question**. The question is "is this the save folder", and name
similarity is one of the weakest available signals for it.

Worse, a single float collapses independent observations into something that can
only be tuned, not reasoned about. `0.72` cannot be explained to a user and cannot
be unit-tested meaningfully.

### 5.3 Signal strength, honestly stated

Ranked by how much they actually tell you:

| Signal | Strength | Why |
|---|---|---|
| `UserConfirmed` | Terminal | The user knows. Nothing outranks it. |
| `WriteWitness` ≥ 2 sessions | Very strong | Two independent correlations with a running process. Near-conclusive. |
| `WriteWitness` × 1 | Strong | Could be a log or cache directory; ignore-list mitigates but does not eliminate. |
| `KbMatch` (built-in) | Strong | Curated, but describes the *typical* install, not this one. |
| `KbMatch` (community) | Moderate | Same, with less review. |
| `ContentShape` | Moderate | Discriminates saves from logs well, but many folders look save-shaped. |
| `InstallLocal` | Moderate | Strong for portable releases, weak elsewhere. |
| `NameMatch` | Weak | Confirms almost nothing alone. Many false positives (`Documents/Photos` for a game called *Photos*). |

Note `KbMatch` and `NameMatch` are **correlated**, not independent — a KB entry's
template frequently contains the title. Any scoring maths that multiplies them as
independent probabilities overstates confidence. This is the specific reason §6
does not use a probabilistic formula.

## 6. The decision rule

**Revised from the original architecture** — see
[ADR-0002](./adr/0002-evidence-tiers-over-weighted-scoring.md).
The first draft proposed a noisy-OR combination with per-signal weights. That is
rejected: the weights were invented without data, the independence assumption is
false (§5.3), and the output is neither explainable nor testable.

Instead, a **decision table** evaluated top to bottom, first match wins:

| # | Condition | Outcome | Explanation shown to user |
|---|---|---|---|
| 1 | `UserRejected` | discard | — |
| 2 | `UserConfirmed` | **bind** (locked) | "You chose this folder." |
| 3 | `WriteWitness` in ≥ 2 distinct sessions | **bind** | "Changed while you were playing, twice." |
| 4 | `WriteWitness` × 1 **and** `ContentShape.save_like > 0` | **bind** | "Changed while you were playing, and contains save files." |
| 5 | `KbMatch(builtin)` **and** path exists | **bind** | "Known save location for this game." |
| 6 | `WriteWitness` × 1 | suggest (high) | "Changed while you were playing." |
| 7 | `KbMatch(community)` **and** `ContentShape.save_like > 0` | suggest (high) | "Community-reported location, contains save files." |
| 8 | `ContentShape.save_like ≥ 2` **and** (`NameMatch ≥ 0.8` or `InstallLocal`) | suggest (medium) | "Contains save files in a folder matching this game." |
| 9 | `NameMatch ≥ 0.9` only | suggest (low) | "Folder name matches this game." |
| 10 | otherwise | discard | — |

A numeric score is still computed, but its **only** job is ordering the suggestion
list. It never decides.

Why this is better: each row is a test case; each row has a sentence; adding a
signal means adding rows rather than retuning weights; and the conservative bias
is explicit rather than emergent. Rules 3–5 are the only paths to an automatic
binding, and every one of them requires either observation or curated data — never
name similarity alone.

## 7. Candidate generation

### 7.1 Root set

```
%APPDATA%              (dirs::config_dir)
%LOCALAPPDATA%         (dirs::data_local_dir)
%LOCALAPPDATA%Low      (sibling of Local — no stdlib constant)
Documents
Documents/My Games
%USERPROFILE%/Saved Games
<install_dir>          ← per-game, from installations
```

The install directory is **new** relative to today's detector, which omits it
entirely. Portable and scene releases commonly save beside the executable, so this
omission blanks out a large slice of exactly the population this system exists to
serve.

Linux/macOS roots (`XDG_DATA_HOME`, `~/Library/Application Support`, Proton
`compatdata/<appid>/pfx/...`) are modelled in the KB's `platform` column but are
not a near-term target; NOVARA is Windows-first today. The Proton path shape is
worth noting now because it is *deterministic* — a rare case where a launcher
gives us an exact answer.

### 7.2 Bounds

| Bound | Value | Reason |
|---|---|---|
| Max depth below a root | 4 | Deeper than any real convention; unbounded recursion on `Documents` is pathological |
| Max candidates per game | 200 | Beyond this the alias generator is malfunctioning |
| Max verifier reads per candidate | 64 files | Enough to characterise a directory |
| Max file size read by verifier | 0 bytes | The verifier reads *metadata only* — never contents (§13) |
| Per-game scan time budget | soft 2s | Exceeded → persist partial results, back off, continue next time |

### 7.3 Ignore list

Directories that generate false `WriteWitness` and `ContentShape` evidence:

```
Crashes  crashpad  CrashDumps  Logs  Log  Cache  Caches  GPUCache
Shaders  ShaderCache  Temp  tmp  CrashReportClient  Backup(ours)
.git  node_modules  DXCache  MediaCache  webcache
```

Maintained as data, not code — it will grow, and it is the difference between the
Write Witness being useful and being noise.

## 8. Alias generation

Aliases only *propose*; they never decide (§6 rule 9 is the weakest row for a
reason).

Retained from today's implementation: exact title, lowercase, trailing
number/roman-numeral stripped, spaces→underscores, spaces removed, first word if
≥ 5 characters.

To add:

| Transform | Example | Why |
|---|---|---|
| Strip subtitle after `:` or `–` | `Nier: Automata` → `Nier` | Very common folder convention |
| Strip edition suffix | `... GOTY / Definitive / Remastered / Complete` | Editions rarely appear in paths |
| Strip punctuation & articles | `S.T.A.L.K.E.R.` → `STALKER` | Filesystem-hostile characters get dropped |
| Initialism | `The Witcher 3` → `TW3` | Used by a minority, cheap to try |
| `{Developer}/{Title}` | `Documents/My Games/CDPR/Witcher3` | Needs metadata we already fetch — a real advantage over generic tools |
| `{Publisher}/{Title}` | as above | ditto |

The developer/publisher pairing is worth calling out: because NOVARA already
resolves metadata, it can generate two-level candidates that a title-only matcher
cannot. That is free accuracy from data we already hold.

**Similarity metric.** Aliases are matched against real directory names with a
normalised edit distance (case-folded, punctuation-stripped) rather than equality,
so `Witcher3` matches `witcher 3`. Threshold for evidence: 0.75. Below that, no
`NameMatch` is recorded at all.

## 9. Verification

The verifier answers one question: **do this directory's contents look like
saves?** It never considers the name — that is the locator's job, and mixing them
is how a single confidence number ends up meaning nothing.

Metadata-only inspection:

| Check | Signal |
|---|---|
| Extension histogram | `.sav .save .dat .slot .profile .bin .json .xml .ini` → save-like |
| Executable/library presence | `.exe .dll .pak .asset` → this is an install dir, not a save dir |
| File count | 0 files → not a save dir. Hundreds of files → probably a cache |
| Size distribution | Save files cluster in KB–MB; a folder of 4-byte files is a marker directory |
| mtime clustering | Files written within minutes of each other → a save event |
| Newest mtime vs `games.last_played_at` | Correlation is weak evidence even without a Witness record — usable retroactively for games played before monitoring existed |
| Depth | Saves are usually ≤ 2 levels below the folder |

That second-to-last row is quietly valuable: it lets a library that predates the
Write Witness get retroactive evidence from mtimes alone, on first scan, offline.

Explicit non-goals: no content parsing (that is
[`PARSER_ARCHITECTURE.md`](./PARSER_ARCHITECTURE.md)), no hashing, no opening of
files.

## 10. Write Witness

The subsystem that makes this design different. **Status: PLANNED (Phase 2).**

### 10.1 Principle

NOVARA already knows the exact interval a game was running (`play_sessions`). A
directory written during that interval, in a plausible root, not on the ignore
list, is almost certainly the game's save directory — regardless of launcher,
release group, era or naming.

### 10.2 Two implementations, ship the boring one first

**Tier 1 — mtime sweep (Phase 2a).** Record directory mtimes across the bounded
root set at `session_started`; compare at `session_ended`. No watchers, no
descriptors, no live event stream, no failure modes beyond a stat storm that is
already bounded. Coarser than watching (misses a write that was later reverted)
but captures the signal that matters.

**Tier 2 — filesystem watchers (Phase 2b).** `notify`-based recursive watchers
armed on session start. Finer-grained: per-file counts and byte volumes, which
strengthens rule 4 and helps discriminate saves from config writes.

Watchers are the risky component — descriptor exhaustion, network mounts that
block, antivirus interference, platform inconsistency. Therefore: tier 2 is an
*optimisation of* tier 1, never a replacement, and tier 1 remains the fallback
whenever arming fails. Shipping tier 1 first means the differentiating feature
lands early and cheaply.

### 10.3 Safeguards

- Coalesce in memory; write aggregated rows at session end, not per event.
- Cap distinct observed paths per session (~500); beyond that, keep the busiest.
- Debounce; ignore-list applied before recording.
- Never watch outside the bounded root set.
- A game running for < 60 s produces low-trust evidence (crash-on-launch writes
  logs, not saves).
- If two games ran concurrently, evidence is ambiguous — record against both, and
  require rule 3 (two sessions) rather than rule 4 for either.

That last point is a real correctness trap: sessions overlap more often than you
would expect, and attributing a write to the wrong game is how a binding ends up
pointing at another game's saves.

### 10.4 Storage and pruning

`save_witness_events` holds raw observations. It is aggregated into candidate
evidence at session end and pruned after 90 days, keeping per-candidate session
*counts* permanently (rule 3 only needs the count). Without this the table grows
without limit for an active library.

## 11. Bindings

The output of detection and the input to everything downstream.

```sql
CREATE TABLE save_bindings (
  id          TEXT PRIMARY KEY,
  game_id     TEXT NOT NULL REFERENCES games(id) ON DELETE CASCADE,
  role        TEXT NOT NULL,           -- 'saves' | 'config' | 'screenshots'
  path        TEXT NOT NULL,
  glob        TEXT,
  origin      TEXT NOT NULL,           -- 'kb' | 'witness' | 'heuristic' | 'user'
  confidence  REAL NOT NULL,           -- ordering/display only, never decides
  is_locked   INTEGER NOT NULL DEFAULT 0,
  is_primary  INTEGER NOT NULL DEFAULT 1,
  auto_backup INTEGER NOT NULL DEFAULT 1,
  verified_at TEXT,
  created_at  TEXT NOT NULL,
  UNIQUE(game_id, role, path)
);
```

**Schema correction from the original architecture.** The first draft used
`UNIQUE(game_id, role)`, which forbids a game having saves in two places. That is
wrong — split saves are common (`Documents/My Games/X` for saves plus
`%APPDATA%/X` for profiles, or per-character directories). The key is now
`(game_id, role, path)` with `is_primary` marking the one the vault treats as
canonical. Recorded in [ADR-0006](./adr/0006-multiple-bindings-per-role.md).

**`is_locked` is the most important column in the schema.** When set, no KB
update, rescan, algorithm change or version upgrade may alter or remove the
binding. It is set whenever a user confirms or edits a binding. Every future
change to this system must preserve that invariant; violating it means silently
repointing a binding and restoring a backup into the wrong folder.

**Roles.** The column exists from day one to avoid a migration, but only `saves`
is populated until a consumer for the others exists — shipping three roles with
one user is surface without value.

## 12. The binding cache question

The brief asks whether the binding cache should live in the resolver or be its own
subsystem. **Neither, as framed — and the framing is the problem.**

A binding is not a cache. Caches are derived data that may be evicted, rebuilt and
invalidated on a heuristic; that mental model applied to bindings produces exactly
the bug §11 forbids: "the entry looks stale, discard and re-detect", silently
repointing a user's confirmed folder.

A binding is a **system of record**. It is the answer, persisted, with provenance.
It is evicted only by explicit user action or by the game being removed.

So: **the resolver owns bindings as its persistent state.** Not a separate
subsystem — a separate subsystem would need its own invalidation policy, and two
components with opinions about when a binding is valid is precisely the race that
loses user data. One owner, one table, one invariant.

What *is* genuinely cache-like, and should be modelled as such:

```sql
CREATE TABLE save_scan_attempts (
  game_id      TEXT PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
  last_attempt TEXT NOT NULL,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  outcome      TEXT NOT NULL,          -- 'bound'|'suggested'|'nothing'|'error'
  next_retry_at TEXT
);
```

Negative results expire. "We looked and found nothing" should be retried after new
information arrives (a KB update, a new session, a metadata refresh) but must not
be retried on every library load. This mirrors the backoff already proven in
`metadata/title_resolver.rs` and migration `0007_artwork_backoff`, and it should
reuse that machinery rather than reinvent it.

The distinction to hold onto: **positive results are permanent, negative results
expire.**

## 13. Security

Save directories and any imported archive are **untrusted input**.

| Threat | Mitigation |
|---|---|
| Path traversal via KB/manifest templates | Templates expand only to known root variables; reject absolute paths and `..` at import |
| Symlink escape out of a root | Do not follow symlinks during candidate walking or snapshotting |
| Resource exhaustion (huge trees) | Depth, count and time caps (§7.2) |
| Malicious archive on restore | Size and entry-count caps, traversal rejection, extract to temp then swap |
| Destructive restore | Mandatory pre-restore snapshot; never restore during a live session |
| KB supply chain | Checksums, review, layer separation — see [`KNOWLEDGE_BASE.md`](./KNOWLEDGE_BASE.md) §7 |
| Accidental writes during detection | Detection is read-only by construction; the verifier reads metadata only, never contents |
| Leaking paths off-device | Detection never transmits anything. KB *fetch* is one-way; nothing about the local library is uploaded |

That last row is a privacy commitment, not just a security one: a system that
"reports which paths it found" to improve a shared dataset would be genuinely
useful and is explicitly out of scope unless a user opts in per-submission (§16).

## 14. Performance

Targets for a 5,000-game library on a spinning disk:

| Operation | Budget | How |
|---|---|---|
| Library load | **0 filesystem calls** for detection | Locked bindings short-circuit before I/O (§4.1) |
| Unlocked binding validation | 1 `stat` per game, lazily on view | Not on load |
| Cold scan, one game | < 500 ms typical, 2 s soft cap | Bounded roots, capped candidates, metadata-only verification |
| Session-end witness (tier 1) | < 300 ms | Directory mtimes only, no recursion into unchanged trees |
| Full library rescan | Background, throttled, cancellable | Reuse the throttle/breaker from `metadata/` |
| Re-scoring after an algorithm change | Zero I/O | Evidence is persisted (§5.1) |

Two consequences worth stating: detection cost is **per game, once**, not per
launch; and no detection work happens on the UI thread or blocks a command —
progress is reported through the existing `novara://event` bus.

## 15. Offline-first

Everything in §4 through §12 works with no network, ever. The built-in KB is
compiled into the binary; aliases, verification and the Write Witness are purely
local; and the Write Witness — the *strongest* signal — is inherently local.

The network is used for exactly one thing: refreshing the community KB layer, and
only when the existing metadata-networking setting permits it. An air-gapped
install is not a degraded install; it loses curated breadth, not capability.

## 16. Community contribution

Detection *consumes* community data; it does not produce it. The submission
workflow, review process and trust model belong to
[`KNOWLEDGE_BASE.md`](./KNOWLEDGE_BASE.md) §8.

One constraint originates here, though: a user-confirmed binding is the highest
quality save-location data in existence, and NOVARA will have thousands of them.
Harvesting them automatically would be a privacy violation (local paths contain
usernames, drive layouts, and the fact that you own a particular game). Any
contribution flow must therefore be **explicit, per-entry, previewed, and
path-templated before it leaves the machine** — never a background upload.

## 17. Current implementation gap

`src-tauri/src/save_detect.rs` today:

| This document | Today | Gap |
|---|---|---|
| KB layer | none | Entire subsystem |
| Candidate roots | 6 | Missing install dir |
| Aliases | 6 transforms | Missing subtitle/edition/publisher forms |
| Similarity | exact match on variants | No fuzzy comparison against real dir names |
| Verification | none | Entire subsystem — no content inspection at all |
| Evidence | single float | Typed, persisted evidence |
| Decision | caller picks from a sorted list | Rule table, auto-binding |
| Bindings | `save_profiles`, manual only | Bindings with provenance and locking |
| Write Witness | none | Entire subsystem |
| Backoff | none | `save_scan_attempts` |
| Persistence | none — recomputed each call | Candidates and evidence persisted |

The existing detector's title-variant generator is sound and should be lifted into
`locator.rs` rather than rewritten. Its ordering-and-dedup logic is also correct
(and has a documented past bug worth not reintroducing). Everything else is new
construction.

## 18. Future expansion

| Idea | Prerequisite |
|---|---|
| Proton/Wine `compatdata` resolution | Linux support; deterministic, so high value per effort |
| Cloud-save conflict detection (local vs launcher cloud) | Snapshot hashing (Phase 4) |
| Per-playthrough save branching | Bindings supporting multiple paths per role (now possible, §11) |
| Detecting *deleted* save folders as an integrity signal | Reuse `integrity/` sweep pattern |
| Suggesting a binding for a game the user has not launched, from a sibling game's publisher directory | Publisher aliases (§8) |
| Confidence calibration from real outcomes | Enough opt-in telemetry to be honest — currently forbidden by design, and that is the right trade |
