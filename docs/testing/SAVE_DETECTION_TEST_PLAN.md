# Save Detection Test Plan

**Status:** design. No tests exist yet; the detection engine is unbuilt and today's
`save_detect.rs` is untestable as written (see
[`TESTING.md`](../architecture/TESTING.md) §1).

The scenario corpus for save detection. Strategy, levels and invariants live in
[`../architecture/TESTING.md`](../architecture/TESTING.md); this document owns the
**cases**.

Reference for expected behaviour:
[`GAME_SAVE_DETECTION.md`](../architecture/GAME_SAVE_DETECTION.md), particularly §6
(the decision table) and §7 (bounds).

---

## 1. What this suite is for

Detection is a system of heuristics reaching a conservative decision. Heuristics
regress silently: a change that improves ten games can break fifty, and nobody
notices until a user restores a backup into the wrong folder.

The corpus exists so that every improvement is measured against every previously
handled case. Its value compounds — like the KB, it is an asset that outlives any
individual release.

**The negative corpus matters more than the positive one.** A missed detection is
recoverable: the user picks the folder. A wrong binding is not: it can destroy a
save. So the cases that assert "do *not* bind this" carry more weight than the ones
that assert "bind this."

## 2. Categories

Directory names under `tests/scenarios/` are the categories.

| Category | Asserts | Target count |
|---|---|---|
| `official/` | Launcher-managed titles resolve via the KB | ~80 |
| `portable/` | Install-local saves are found | ~30 |
| `emulated/` | Goldberg / CODEX / RUNE / FLT layouts | ~40 |
| `repack/` | FitGirl / DODI variants, including redirected paths | ~20 |
| `multi-path/` | Split saves bind primary, suggest secondary | ~15 |
| `override/` | User decisions win and persist | ~15 |
| `kb-refresh/` | Updates never disturb locked bindings | ~10 |
| `lifecycle/` | Moved, deleted, restored save folders | ~15 |
| `negative/` | **False positives are rejected** | ~60 |
| `ambiguity/` | Concurrent sessions, duplicate titles | ~15 |
| `safety/` | Bounds, symlinks, depth, pathological trees | ~20 |
| `determinism/` | Re-scoring stored evidence is stable | ~10 |
| `backoff/` | Failed scans retry correctly, not constantly | ~8 |
| `migration/` | `save_profiles` → `save_bindings` | ~8 |

Roughly 350 scenarios at maturity, and it should keep growing — every user-reported
mis-detection becomes a scenario before it becomes a fix.

## 3. Scenario format

Declarative, one file per case. Version-gated so the runner rejects rather than
misreads old fixtures.

```toml
version = 1

[scenario]
id    = "rdr2-steam-official"
title = "Red Dead Redemption 2, Steam install, KB hit"
tags  = ["steam", "kb", "official"]

[game]
title       = "Red Dead Redemption 2"
steam_appid = "1174180"
install_dir = "{DRIVE}/Steam/steamapps/common/Red Dead Redemption 2"
developer   = "Rockstar Games"

# The world. Metadata only — no file contents anywhere.
[[fs]]
path  = "{DOCUMENTS}/Rockstar Games/Red Dead Redemption 2/Profiles/A1B2C3D4"
files = [
  { name = "sfl0001", size = 8_400_000, mtime = "2026-01-02T22:14:00Z" },
  { name = "prof0",   size = 240_000,   mtime = "2026-01-02T22:14:00Z" },
]

# A decoy that must not win.
[[fs]]
path  = "{LOCALAPPDATA}/Rockstar Games/Launcher/Logs"
files = [{ name = "launcher.log", size = 12_000, mtime = "2026-01-02T22:15:00Z" }]

[[kb]]
layer         = "builtin"
match_kind    = "steam_appid"
match_value   = "1174180"
role          = "saves"
path_template = "{DOCUMENTS}/Rockstar Games/Red Dead Redemption 2/Profiles"

[expect]
binding          = "{DOCUMENTS}/Rockstar Games/Red Dead Redemption 2/Profiles"
origin           = "kb"
rule             = 5            # which decision-table row fired
locked           = false
suggestions      = []
must_not_include = ["{LOCALAPPDATA}/Rockstar Games/Launcher/Logs"]
explanation_contains = "Known save location"
```

Sessions, for witness cases:

```toml
[[sessions]]
started_at = "2026-01-02T20:00:00Z"
ended_at   = "2026-01-02T22:30:00Z"
writes     = ["{APPDATA}/Goldberg SteamEmu Saves/1174180/remote"]
```

Notes on the format's deliberate choices:

- **`rule` is asserted, not just the outcome.** Two different rules reaching the
  same binding is a behaviour change worth catching — it usually means precedence
  shifted.
- **`must_not_include` is first-class.** Most detection bugs are extra candidates,
  not missing ones.
- **No confidence numbers.** Asserting them would lock in arbitrary values
  ([`TESTING.md`](../architecture/TESTING.md) §8).
- **`{DRIVE}`, `{DOCUMENTS}` etc. are the same closed variable set the KB uses**, so
  a scenario is portable and cannot express a real absolute path. They expand
  beneath a synthetic home (`C:/Users/test`), never the machine's own.
- **`pending` marks a fixture that leads the implementation.** §7 requires a
  mis-detection case to be merged *failing*, before the fix; this is that mechanism.
  It is deliberately two-sided — the runner skips a pending fixture's assertions,
  **but fails if a pending fixture starts passing**, so the marker cannot outlive the
  work it was waiting for. The value names what it waits on
  (`pending = "task 1.17 (verifier)"`).
- **`[[sessions]]` parses but is refused** with a "Phase 2" message rather than
  ignored, so a Write Witness fixture cannot appear to pass while testing nothing.

**Phase 1 `[expect]` vocabulary.** Automatic binding is Phase 3, so Phase 1 records
a decision without acting on it. The strongest outcome a fixture can assert is
`bind_eligible` — the candidate the decision table *would* bind. `binding`,
`origin` and `locked` arrive with the binding store.

## 4. Core cases

The cases you listed, specified. Each row is at least one scenario file; most are
several.

### 4.1 Official launcher

| Case | World | Expect |
|---|---|---|
| Steam + KB hit | `{DOCUMENTS}/My Games/<T>` exists, KB entry present | bind, `origin=kb`, rule 5 |
| Steam, KB path absent on disk | KB entry points at a folder that does not exist | **no binding**; rule 5 requires the path to exist. Fall through to aliases |
| Steam userdata with account id | `{WILDCARD}` in template | bind, wildcard resolved to the one present id |
| Steam, two accounts present | two id directories | suggest both, bind neither — ambiguity is not resolvable without more evidence |
| GOG / Epic / Xbox equivalents | per-launcher templates | bind via KB |
| Launcher-managed, cloud-only game | no local save dir at all | no binding, no suggestion, `outcome='nothing'`, backoff set |

That second row is the one most likely to be got wrong in implementation: a KB
entry is a *claim*, not a fact, and binding to a non-existent path would produce a
binding that fails on first snapshot.

### 4.2 Portable / install-local

| Case | World | Expect |
|---|---|---|
| `save.dat` beside the exe | `{INSTALL}/save.dat` | bind `{INSTALL}`, rule 8 (`InstallLocal` + content shape) |
| `{INSTALL}/saves/` | subdirectory | bind the subdirectory, not the install root — more specific wins |
| `{INSTALL}/Profiles/<name>/` | nested | bind `Profiles`, the common parent |
| Install dir also contains the game's binaries | `.exe`, `.dll`, `.pak` present at the root | **must not** bind the install root itself; the verifier's executable signal excludes it |
| Portable game never launched | no witness, no KB | suggest only. Never auto-bind from install-local alone |

Row four is a specific trap: install-local detection and "this is an install folder,
not a save folder" pull in opposite directions, and the verifier is what
distinguishes them.

### 4.3 Emulated (the case that motivated the design)

| Case | World | Expect |
|---|---|---|
| Goldberg, one session | `{APPDATA}/Goldberg SteamEmu Saves/<appid>/remote` written during the session | bind, rule 4 (witness + content), `origin=witness` |
| Goldberg, no session yet | folder exists, never observed | suggest via KB/name only — not bound |
| Goldberg vs alias decoy | a name-matching folder exists elsewhere, but the witness fired on the Goldberg path | **the witness path wins.** This is the headline assertion of the whole design |
| CODEX ini in `{APPDATA}` | written during session | bind via witness |
| Emulator layer plus real Steam saves both present | two candidates, one witnessed | witnessed path is primary, other is suggested |

The third row is worth stating as a named test — `emulated/witness-beats-alias.toml`
— because it encodes the central claim: observation outranks name similarity.

### 4.4 Multiple save folders

| Case | Expect |
|---|---|
| Saves in `{MYGAMES}/<T>`, profiles in `{APPDATA}/<T>`, both witnessed | two bindings, role `saves`, one `is_primary=1` |
| Which is primary? | the one with more written bytes across sessions; ties broken by earlier `first_seen_at` for determinism |
| Vault behaviour | snapshots the primary only |
| Extraction behaviour | may read all bindings for the role |

Tie-breaking must be deterministic or `determinism/` scenarios will flake.

### 4.5 User override

| Case | Expect |
|---|---|
| User binds a path detection never suggested | binding stored, `origin=user`, `is_locked=1` |
| Rescan afterwards | binding **byte-identical**; candidates may be re-scored but the binding is untouched (invariant I1) |
| User rejects a candidate | `UserRejected` recorded; that path never suggested again for this game, even if the KB later adds it |
| User overrides, then a KB update adds a higher-priority entry | user binding survives; KB path appears as a suggestion at most |
| User binds a non-existent path | rejected at the command boundary with a clear error — not stored as a broken binding |

### 4.6 KB refresh

| Case | Expect |
|---|---|
| Community layer replaced | user layer untouched (I7); locked bindings untouched (I1) |
| A refresh adds a match for a previously unmatched game | re-decided from stored evidence; **filesystem touched only for genuinely new template paths** |
| A refresh removes an entry that produced a current binding | binding remains (it is a system of record, not a cache) |
| Malformed payload | rejected atomically; previous KB version still active; version row unchanged |
| Payload with an absolute path or `..` | rejected per-entry at import, with the rest of the payload still applied |

### 4.7 Lifecycle

| Case | Expect |
|---|---|
| Bound folder deleted | binding marked unverified, **not** deleted; UI offers rescan; no snapshot attempted |
| Bound folder deleted, then recreated | binding re-verifies on next check; no user action needed |
| Game moved to another drive | install-local binding invalid; rescan finds the new location; the old binding is replaced only if unlocked |
| Locked binding whose path vanished | stays locked, stays unverified, never auto-repointed |

### 4.8 Negative corpus

The most valuable category. Each of these has burned someone's tool.

| Case | Must not |
|---|---|
| Game titled "Photos"; `{DOCUMENTS}/Photos` exists with images | bind. Name match alone is rule 9 → suggest at most, and content shape should exclude it |
| `Logs/` written during a session | bind. Ignore-list excludes it before evidence is recorded |
| `Crashes/`, `crashpad/`, `GPUCache/` written during a session | bind or even appear as candidates |
| A cache folder containing hundreds of small files | bind — the file-count signal excludes it |
| An empty folder whose name matches exactly | bind. Zero files means not a save directory |
| `{APPDATA}/Microsoft` (matched by a game called "Microsoft Flight Simulator" after subtitle stripping) | bind. Over-aggressive alias stripping must not reach generic vendor folders |
| Two-character alias from a short title | generate candidates at all — minimum alias length applies |
| A folder matching another game in the library | bind for the wrong game |

The Flight Simulator row generalises to a rule worth encoding: **alias stripping
must not produce a term that matches a known vendor or system directory.** A
denylist of generic segment names (`Microsoft`, `Google`, `Packages`, `Programs`)
belongs alongside the ignore list.

### 4.9 Ambiguity

| Case | Expect |
|---|---|
| Two games ran with overlapping sessions | evidence recorded against both; rule 4 disabled; rule 3 (two sessions) required |
| Same title installed twice (two installs) | candidates attributed per installation via `{INSTALL}` |
| Two library entries with identical titles | no cross-contamination of bindings |
| A write observed at a path already bound to another game | recorded, flagged, bound to neither without user input |

### 4.10 Safety and bounds

| Case | Expect |
|---|---|
| Symlink from a root to `C:/` | not followed; no candidate outside the root set (I3) |
| Directory nested 40 deep | walk stops at depth 4 |
| 50,000 sibling directories under a root | candidate cap enforced; partial result; backoff set; no hang (I10) |
| Junction loop | terminates |
| Path with unicode / trailing spaces / reserved Windows names | handled without panic |
| Permission denied mid-walk | skipped, recorded, scan continues |

### 4.11 Determinism

| Case | Expect |
|---|---|
| Score at observation vs re-score from stored evidence | identical decision (I4) |
| Same scenario run twice | identical binding, identical suggestion order |
| Evidence arriving in a different order | identical outcome — the decision table must be order-insensitive with respect to evidence arrival |
| Unknown evidence variant in `evidence_json` (from a newer version) | ignored gracefully, not a parse failure |

That last row is what makes downgrade survivable.

### 4.12 Migration

| Case | Expect |
|---|---|
| `save_profiles` rows migrate | one `save_bindings` row each, `is_locked=1`, `origin=user` |
| `save_backups` backfill | `binding_id` populated for every row that had a `profile_id` |
| Migration run twice | idempotent |
| Empty `save_profiles` | no-op |
| Profile with a path that no longer exists | still migrated, still locked, marked unverified |

## 5. Per-game regression cases

Your `test_rdr2.rs` instinct, expressed as data. A named-title scenario is worth
adding when:

- a user reports a mis-detection (**always** — the scenario lands before the fix), or
- the game has an unusual layout worth pinning, or
- it is popular enough that a regression would be widely felt.

Each doubles as a KB correctness test (§1 of [`TESTING.md`](../architecture/TESTING.md)
— fixture principles). Starter set:

```
official/rdr2-steam.toml                Documents/Rockstar Games, profile subdir
official/skyrim-se-steam.toml           Documents/My Games/Skyrim Special Edition
official/cp2077-gog.toml                Saved Games/CD Projekt Red
official/witcher3-steam.toml            split: gamesaves + user settings
official/elden-ring-steam.toml          AppData/Roaming/EldenRing/<steamid>
official/eldenring-wildcard.toml        {WILDCARD} account-id resolution
emulated/goldberg-remote.toml           witness beats alias
emulated/codex-appdata.toml             ini-based
portable/generic-save-beside-exe.toml   install-local
multi-path/witcher3-two-roots.toml      primary selection
negative/photos-name-collision.toml     the classic false positive
negative/flight-sim-vendor-folder.toml  alias over-stripping
```

Twelve files, no Rust, covering the majority of the behaviours that matter.

### 5.1 Corpus coverage is data-driven, not one file per title

The built-in KB carries ~40 entries. Forty near-identical fixtures would rot as a
set and would not test the thing worth testing, so coverage of the *corpus* lives in
`saves::kb::corpus_tests` and is **derived from the corpus itself**: every entry is
walked, the world in which it should fire is synthesised, and the entry is required
to produce a candidate. Adding a KB entry therefore adds a case automatically, and
coverage cannot fall behind the data.

What that catches is the failure a growing corpus actually develops: an entry that
is well-formed, validates cleanly, and can never match anything — an unreachable
key, an anchor no context supplies, a variable no game carries. Such an entry passes
review and does nothing in production.

Named-title `.toml` scenarios remain the vehicle for *behaviour* — ordering, false
positives, alias handling — where the value is in the specific case rather than in
the breadth.

**Prerequisite for KB-driven scenarios.** `[[kb]]` blocks parse but are currently
inert: `scenario::runner` is filesystem-only and holds no `Db`, and `pipeline::detect`
does not yet consult the KB. Writing KB fixtures before the runner has a database
would produce files that look like coverage while asserting nothing — worse than
having none. Giving the runner a `Db` and seeding `[[kb]]` belongs with task 1.22,
where the resolver is wired into the pipeline.

### 5.2 One game, several real save locations

`multi-path/` holds cases where a game legitimately has **more than one** save directory —
the normal outcome of changing store or edition. Both real libraries validated so far showed
it (Dying Light and Red Dead Redemption 2 each had a current location plus one left behind by
an earlier install).

These fixtures carry `[[sessions]]` and are therefore **skipped until Phase 2**, because
Phase 1 can find the locations but cannot tell which is live — every signal available to it
describes them equally well, and an mtime comparison would be a guess that a cloud sync or
antivirus scan can defeat. See [`GAME_SAVE_DETECTION.md` §10.1.1](../architecture/GAME_SAVE_DETECTION.md).

Two rules for anything added here:

* **Never assert that a historical location is absent.** It may hold the only copy of a long
  save, and a user migrating stores is exactly who needs it. Reducing detections loses
  information that cannot be recovered.
* **Assert the *rule*, not just the path.** The point of these cases is that the witnessed
  location binds via row 3 or 4 while the others remain suggestions — an outcome-only
  assertion would pass for the wrong reason.

## 6. Definition of done for Phase 1 and Phase 2

| Phase | Gate |
|---|---|
| **Phase 1** | Every **Phase-1-reachable** decision-table row has ≥ 1 positive and ≥ 1 negative scenario. `official/`, `portable/`, `negative/` and `safety/` populated to at least half their target counts. Invariants I2, I3, I9, I10 passing |
| **Phase 2** | `emulated/witness-beats-alias.toml` passing, plus a positive and negative case for rows 3, 4 and 6. `ambiguity/` populated. Invariants I1 and I4 passing |
| **Phase 3** | `override/`, `kb-refresh/`, `migration/` populated. Invariants I1 and I7 passing |
| **Phase 4** | Vault plan written and hostile-archive corpus passing. Invariants I6 and I8 passing |

**On "Phase-1-reachable".** Rows 3, 4 and 6 of the decision table depend on
`WriteWitness` evidence, which no subsystem produces until Phase 2. They are written
into the table from the start — so precedence is correct and Phase 2 only has to begin
emitting the evidence — but they are unreachable and therefore untestable in Phase 1.
Their scenarios are a Phase 2 gate instead. Rows 1, 5, 7, 8, 9 and 10 are reachable in
Phase 1 and must each be covered both ways.

Phase gates expressed as tests rather than features is the point: "detection works"
is unfalsifiable, "these 180 scenarios pass" is not.

## 7. Contributor workflow

Adding a case, once the runner exists:

1. Copy the nearest existing scenario in the right category.
2. Describe the world: `[[fs]]` entries, `[[kb]]` entries, `[[sessions]]` if
   relevant. Metadata only — never real file contents, never a real absolute path.
3. State `[expect]`, including `rule` and `must_not_include`.
4. `cargo test scenarios` — a new scenario that passes immediately is suspicious;
   confirm it fails when the relevant logic is disabled.
5. For a reported mis-detection, the scenario is part of the bug report, and it
   should be merged **failing** (marked `#[ignore]` with the issue number) before
   the fix lands.

That last step is what turns a bug report into a permanent guarantee rather than a
one-off patch.
