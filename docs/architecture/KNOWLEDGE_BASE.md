# Save Location Knowledge Base

**Status:** design. Not implemented. Target: Phase 1 (built-in + user layers),
Phase 8 (community layer).

The KB is a **long-lived data asset**, not a lookup table. It will outlive
individual releases, accumulate value slowly, and eventually be the thing that
makes NOVARA hard to replace. It is designed accordingly: versioned, layered,
replaceable, and structurally incapable of overwriting a user's decisions.

Owns these tables: `save_kb_entries`, `save_kb_versions`.

Consumed by [`GAME_SAVE_DETECTION.md`](./GAME_SAVE_DETECTION.md) §4.2 as one of
four evidence producers. This document covers the asset; that one covers its use.

---

## 1. Why this is an asset and not a table

Code depreciates; curated data appreciates. A path template for a 2011 game is
still correct in 2035. Every entry added is permanent value, and the corpus is the
only part of this system that cannot be rebuilt from first principles by a
competitor — they would have to gather it again.

Three consequences for the design:

1. **The KB must be separable from the binary.** Data that can only ship in a
   release is data that updates six times a year instead of continuously.
2. **The KB must be versioned as data.** "Which KB do you have" must be a question
   with an answer, independent of the app version.
3. **The KB must never be the authority on a specific machine.** It describes the
   *typical* installation. The machine in front of us is the authority, and the
   user is the authority above that. This is why the KB writes *candidates*, never
   *bindings*.

Point 3 is the one most likely to be violated by a well-intentioned future change.

## 2. Three layers

```
┌──────────────────────────────────────────────────┐
│ USER KB            highest priority, permanent   │  never overwritten
├──────────────────────────────────────────────────┤
│ COMMUNITY KB       optional, independently ver.  │  replaceable wholesale
├──────────────────────────────────────────────────┤
│ BUILT-IN KB        ships with NOVARA, offline    │  replaced on app update
└──────────────────────────────────────────────────┘
```

Layers are **additive for candidate generation** and **ordered for conflict
resolution**. All three may contribute candidates for the same game; when two
entries describe the same path, the higher layer's metadata wins.

Critically, layer priority affects only *how strong the resulting evidence is*
(see detection §6, rules 5 and 7) — it never decides a binding on its own.

### 2.1 Built-in KB

| Property | Value |
|---|---|
| Ships | Compiled into the binary (`include_str!` of a versioned JSON) |
| Availability | Always, offline, from first launch |
| Content | Curated entries for launcher-managed titles and high-population manual titles |
| Trust | Highest of the non-user layers; reviewed in-tree via PR |
| Update cadence | With app releases |
| Size target | < 2 MB compressed; a few thousand entries |

Being compiled in is deliberate. A fresh install with no network must detect saves
for popular titles immediately — a KB that requires a download is a KB that is
absent exactly when first impressions are formed.

### 2.2 Community KB

| Property | Value |
|---|---|
| Ships | Fetched on demand, cached in app data |
| Availability | Only when the metadata-networking setting permits it |
| Content | Long tail, unusual releases, corrections |
| Trust | Moderate — reviewed, but at volume |
| Update cadence | Continuous, independent of app releases |
| Versioning | Its own version line, unrelated to the app version |

Optional by construction. Disabling networking must degrade breadth, never
function.

### 2.3 User KB

| Property | Value |
|---|---|
| Ships | Created locally by the user |
| Availability | Always |
| Content | Paths the user added or corrected for their own machine |
| Trust | Absolute |
| Lifetime | **Permanent.** Survives rescans, app updates, KB refreshes, schema migrations |

The user KB and the locked binding (detection §11) are related but distinct: a
locked binding says "this game's saves are here"; a user KB entry says "for games
matching this pattern, look here" — reusable across games, e.g. a custom
`D:/Games/Saves/{Title}` convention. The second is a generalisation the user can
express once instead of correcting fifty games individually.

That reuse is the entire reason the user layer is a KB layer and not just a flag on
a binding.

## 3. Schema

```sql
CREATE TABLE save_kb_entries (
  id            TEXT PRIMARY KEY,        -- stable, layer-prefixed: 'builtin:1234'
  layer         TEXT NOT NULL,           -- 'builtin' | 'community' | 'user'
  match_kind    TEXT NOT NULL,           -- 'steam_appid'|'gog_id'|'epic_id'
                                         -- |'exe_name'|'title_norm'|'any'
  match_value   TEXT NOT NULL,
  platform      TEXT NOT NULL,           -- 'windows'|'linux'|'macos'
  role          TEXT NOT NULL,           -- 'saves'|'config'|'screenshots'
  path_template TEXT NOT NULL,           -- '{APPDATA}/{Publisher}/{Title}/Saves'
  glob          TEXT,                    -- optional include filter
  priority      INTEGER NOT NULL DEFAULT 100,
  note          TEXT,                    -- e.g. 'Goldberg builds only'
  source_ref    TEXT,                    -- provenance: PR, wiki page, user
  kb_version    TEXT NOT NULL,
  created_at    TEXT NOT NULL
);

CREATE INDEX idx_kb_lookup ON save_kb_entries(match_kind, match_value, platform);
CREATE INDEX idx_kb_layer  ON save_kb_entries(layer);

CREATE TABLE save_kb_versions (
  layer       TEXT PRIMARY KEY,          -- one row per layer
  version     TEXT NOT NULL,
  checksum    TEXT NOT NULL,             -- sha256 of the source payload
  entry_count INTEGER NOT NULL,
  applied_at  TEXT NOT NULL,
  source_url  TEXT
);
```

Design notes:

- **`layer` is a column, not three tables.** Matching queries want all layers at
  once; splitting them would mean a three-way union on every lookup. Layer
  ordering is a `CASE` in the query, which is cheaper and keeps the schema flat.
- **`match_kind = 'any'`** exists for user rules that apply library-wide (the
  `D:/Games/Saves/{Title}` case in §2.3).
- **`source_ref`** is what makes a wrong entry fixable a year later. Without
  provenance the corpus becomes unauditable.
- **Refresh is destructive per layer.** Replacing the community layer deletes and
  reinserts `WHERE layer = 'community'`. The user layer is never touched by a
  refresh — enforced in the migration/refresh code, and worth an explicit test.

## 4. Match resolution order

Most specific to least:

```
1. steam_appid / gog_id / epic_id     exact, authoritative identity
2. exe_name                            survives retitling and repacks
3. title_norm                          normalised title (case/punctuation folded)
4. any                                 user-defined library-wide rules
```

`exe_name` above `title_norm` is deliberate: a repack frequently renames the game
but ships the original executable. The executable name is the most stable identity
a manually installed game has, and NOVARA already records it per installation.

### 4.0 A game has many save layouts, not one

A single game can legitimately keep saves in several places, and the entries describing
them are **not equivalent claims**. `save_kb_entries` has no `UNIQUE(match_kind,
match_value)`, so N entries may match one game and `kb::candidates` expands all of them —
that capability existed from the start. What was missing was *what kind of location* each
entry describes.

| `layout` | The claim it makes | Authority |
|---|---|---|
| `official` | The game as shipped writes here | **Curated** |
| `user_defined` | The user told us | **Curated** |
| `engine` | Games on this engine write here | Advisory |
| `os` | This is a conventional Windows location | Advisory |
| `launcher` | This storefront keeps save data here | Advisory |
| `portable` | Self-contained installs write beside the executable | Advisory |
| `community` | Installs using a save-redirection layer write here | Advisory |

**Advisory means the entry describes a *class* of installs.** Whether *this* install
belongs to that class is exactly what is unknown, so an advisory layout suggests and is
promoted by content, mtime correlation or a write witness. Only a curated layout can bind
alone.

#### Why this is a second axis and not a variant of `layer`

`layer` records **who authored an entry** — provenance. `layout` records **what sort of
location it names**. They are orthogonal: a shipped built-in entry can perfectly well
describe a community layout.

Before the distinction existed, the only proxy for layout was `match_kind != 'any'`
("keyed"), which is a *matching mechanism*. The consequence was concrete: a community
layout entered as a keyed built-in entry satisfied decision table row 5 and bound with
exactly the authority of the official path. The original schema anticipated this and parked
it in free text — the `note` column's example comment is `'Goldberg builds only'`.

Making `layout` a fourth *layer* would have been worse: a shipped entry describing a
community layout would have to misreport who wrote it, and layer-scoped replacement
(invariant I7) would break.

#### Adding a layout is data; granting it authority is code

`layout` has **no CHECK constraint**. A corpus update or a community contribution may
introduce a new layout without a migration and without a Rust change. `saves::kb::layout`
classifies known values and returns `Advisory` for everything else, so an unrecognised
layout is usable immediately and safe by construction — the failure mode is
under-trusting, never over-trusting.

That asymmetry is the security boundary:

* `layer` is set by the **loader**, never by the payload (`replace_kb_layer` takes it as a
  parameter; `add_user_entry` hardcodes it), so an entry cannot choose its own provenance.
* `layout` is chosen freely by the data but only *classified* in code.
* Promoting a layout to `Curated` is one line in `layout::CURATED_LAYOUTS` and a deliberate
  review, which is the right weight for a privilege decision.

`resolver` is expressed over the authority tier, never over a layout name, so a new layout
flows through the existing decision rows with no new row and no code change. That is what
makes "support another save layout" a data task.

#### Community layouts are conventions, not per-game entries

Some installs route saves through a redirection layer that keeps them under a shared root.
Those roots are **directories** — filesystem facts. Nothing in the corpus identifies or
attributes whoever produced a layer, and NOVARA has no business doing so; the only question
is where a user's saves are so they can be backed up.

They are `match_kind = 'any'`, because a shared root is a convention *of the layer* rather
than a fact about one game. **One entry covers every game that layer wraps** — the
difference between a corpus that scales and one needing a row per game per layer.

Each is keyed *inside* the path by `{TITLE}` or `{STEAM_APPID}`. This is not optional: a
shared multi-game root keyed by `{WILDCARD}` would fan out across every subfolder, and
every game would then claim every other game's saves. The path variable is what keeps a
library-wide rule game-specific.

### 4.1 How the corpus is organised on disk

The built-in corpus is **many small files**, not one. `build.rs` merges
`data/kb/**/*.json` into a single document in `OUT_DIR`, which `saves::kb::builtin`
embeds with `include_str!`. Runtime cost is unchanged: one embedded string, one parse,
no directory walking at startup.

```
data/kb/
  manifest.json                       corpus version
  README.md                           the contributor guide
  official/<letter>.json              one game's real save location
  engine/conventions.json             engine defaults
  os/windows.json                     Windows known folders
  launcher/                           storefront locations (empty — see §9)
  community/redirection-roots.json    shared redirection roots
  portable/install-dir.json           beside the executable
```

**A directory is a third concept, and it is deliberately the weakest of the three:**

| Concept | Meaning | Reaches the database? | Affects behaviour? |
|---|---|---|---|
| `layer` | who authored the entry | **yes** | yes — provenance |
| `layout` | what kind of location | **yes** | yes — authority |
| directory | how the corpus is organised | **no** | **no** |

The directory has no effect on matching, confidence, authority, evidence or the decision
table, and it has no representation in the schema. `the_category_directory_has_no_runtime_effect`
asserts that no stored column can carry a corpus path, and
`identical_entries_from_different_files_decide_identically` asserts the behavioural half.

**Layout is declared per file, not inferred from the path.** If the directory determined it,
moving a file would silently change whether its entries can bind.
`the_declared_layout_matches_the_directory` catches misfiling without the path ever becoming
load-bearing.

Three properties worth knowing:

* **Adding an entry is dropping a file.** No Rust change, no registration list — a shared
  list would reintroduce the single edit point this layout exists to remove.
* **Granularity is a data decision.** The build walk is recursive, so splitting
  `official/h.json` into `official/h/hollow-knight.json` later needs no code change.
* **Merge order is sorted.** Load-bearing: startup idempotence compares a SHA-256 over the
  merged bytes, so unstable ordering would reload the corpus on every launch.

The build fails on unparseable JSON, a missing `layout`, a missing required field or a
duplicate id — the last checked across the whole corpus rather than per file. Deep validation
(template anchoring, traversal refusal, key normalisation) stays in `saves::kb::validate`,
because it needs the crate the build script is building. Build time catches "the corpus is
malformed"; test time catches "an entry is wrong".

See [`data/kb/README.md`](../../src-tauri/data/kb/README.md) for the contributor guide.

### 4.1 Normalisation rules for match values

`match_value` must be stored **pre-normalised** for the two derived kinds, or a
lookup will never hit. Both are applied by `saves::kb`:

| Kind | Rule | Example |
|---|---|---|
| `title_norm` | Every non-alphanumeric character removed, including spaces; lowercased | `Marvel's Spider-Man` → `marvelsspiderman` |
| `exe_name` | Directory and `.exe` extension stripped; lowercased | `D:/Games/Foo/Bin/Launcher.EXE` → `launcher` |

Removing separators rather than collapsing them to a space is a deliberate
correction: collapsing split words at apostrophes, so `Marvel's` became
`marvel s` and could never match `Marvels`. Removing them makes hyphenated,
spaced and compacted spellings all collide — `Half-Life 2`, `Half Life 2` and
`HalfLife2` share one key — which is the direction that helps. Two genuinely
different games almost never differ only by punctuation, and the value is a
matching key, not a display string.

## 5. Template variables

A closed set. Anything outside it is rejected at import (security §7).

| Variable | Expands to |
|---|---|
| `{APPDATA}` | `%APPDATA%` / `$XDG_CONFIG_HOME` / `~/Library/Application Support` |
| `{LOCALAPPDATA}` | `%LOCALAPPDATA%` |
| `{LOCALLOW}` | `%LOCALAPPDATA%Low` |
| `{DOCUMENTS}` | Documents |
| `{MYGAMES}` | `Documents/My Games` |
| `{SAVEDGAMES}` | `%USERPROFILE%/Saved Games` |
| `{USERPROFILE}` | Home |
| `{INSTALL}` | This installation's directory |
| `{PUBLIC}` | `%PUBLIC%` |
| `{TITLE}` | Game title as stored |
| `{PUBLISHER}` / `{DEVELOPER}` | From metadata, when known |
| `{STEAM_APPID}` | When known |
| `{STEAM_USERID}` | Resolved from the local Steam install, when present |
| `{WILDCARD}` | Single path segment wildcard — for user-id directories |

Absolute paths, drive letters and `..` are **not** expressible. A template that
needs one is a template we will not accept, because it cannot be portable across
machines and is the obvious traversal vector.

`{WILDCARD}` deserves a note: Steam's `userdata/<id>/<appid>/remote` and many
emulated layers include an account id. A single-segment wildcard handles this
without opening the door to arbitrary globbing.

## 6. Update strategy

| Layer | Mechanism | Trigger |
|---|---|---|
| Built-in | Replaced on app update; migration reinserts `WHERE layer='builtin'` | App launch after upgrade |
| Community | HTTPS fetch of a versioned, checksummed payload | Manual check, or periodic if networking is enabled |
| User | Never auto-updated | — |

Update is **transactional and atomic per layer**: download → verify checksum →
validate every entry → replace the layer in one transaction → record the new
version. A partially applied KB is worse than an old one, because the failure is
silent and the symptom (a missing game) looks like a detection bug.

**Re-scoring after an update.** A KB refresh may introduce a `KbMatch` for a game
that previously had none. This does not require filesystem work: existing
candidates are re-decided from persisted evidence plus the new KB match, and only
genuinely new template paths need a `stat`. Games with locked bindings are skipped
entirely. This is the payoff for storing evidence rather than conclusions
(detection §5.1).

## 7. Trust and verification

The KB is data that steers filesystem access, so its integrity matters more than
its confidentiality.

| Control | Status | Rationale |
|---|---|---|
| HTTPS transport | Required | Baseline |
| SHA-256 checksum of payload, verified before apply | Required | Detects corruption and truncation |
| Per-entry validation at import | Required | Template variables from the closed set only; no absolute paths; no `..`; known `role`; known `platform` |
| Entry-count and size sanity bounds | Required | A 500 MB "KB" is an attack, not an update |
| Detached signature over the payload | **Deferred** | See below |

**On signing, honestly:** a detached signature is straightforward to verify and
hard to *operate*. It needs a keypair, a signing step in the release pipeline, a
key-rotation story, and a decision about what to do when verification fails on a
user's machine (fail closed and lose updates, or fail open and gain nothing). It
is worth doing when the community layer is genuinely third-party-writable; it is
theatre before then. Meanwhile the meaningful controls are per-entry validation
and the layer separation that stops any KB from touching a user's bindings —
those bound the damage a malicious KB can do to "suggests useless paths", which
is an annoyance rather than a breach.

Recorded in [ADR-0008](./adr/0008-kb-validation-over-signing.md).

## 8. Community contribution workflow

```
user corrects a binding in NOVARA
        │
        │  (explicit action — never automatic)
        ▼
"Contribute this location?"  ──► preview screen
        │                         shows the exact templated string
        │                         that would be submitted
        ▼
local path → template
   C:/Users/harsh/AppData/Roaming/CDPR/Witcher3
   → {APPDATA}/{PUBLISHER}/{TITLE}
        │
        ├─ reject if it cannot be templated (contains an
        │  unmappable absolute path)
        ▼
submission: { match_kind, match_value, platform, role,
              path_template, note, app_version }
        │
        ▼
review queue (GitHub PR or equivalent)
        │
        ├─ automated: schema valid? template variables closed-set?
        │             duplicate of an existing entry?
        ├─ human: plausible? correct layer? correct match_kind?
        ▼
merged → community KB version bump → clients fetch
```

Two rules, both non-negotiable:

1. **Nothing leaves the machine without an explicit per-submission action and a
   preview of the exact payload.** Local paths contain usernames, drive layouts
   and evidence of which games someone owns. Bulk-harvesting confirmed bindings
   would be the single most valuable thing NOVARA could do for the corpus and is
   forbidden.
2. **Templating happens before submission, on-device.** The server never sees a
   raw local path.

## 9. Seeding the corpus

Cold-starting a KB is the practical risk (§11). Options, in order of preference:

| Source | Licence care | Value |
|---|---|---|
| Hand-curated top titles | none | High per entry; makes the feature feel real on day one |
| **Engine and OS convention rules** (Unreal's `Saved/SaveGames`, Unity's `LocalLow/<company>/<product>`, the Windows `Saved Games` and `My Games` known folders) | none | Broad coverage from a handful of *rules* rather than entries |
| Existing open datasets (PCGamingWiki, Ludusavi manifest) | **Must check licence and attribute** | Very large; would be transformative if compatible |
| User contributions | consent (§8) | Slow start, compounding |

The convention row is the highest leverage, and it is worth being precise about
*why* — an earlier draft of this document called these "launcher conventions", which
was wrong in a way that mattered.

**They are engine and operating-system conventions, not launcher conventions.**
Steam, GOG and Epic impose almost no save-location convention on the games they
distribute; a game's save path is chosen by its engine or its developer, and the
storefront is irrelevant to it. What *is* conventional:

| Convention | Origin |
|---|---|
| `{LOCALAPPDATA}/<Game>/Saved/SaveGames` | Unreal Engine 4/5 default |
| `{LOCALLOW}/<Company>/<Product>` | Unity `Application.persistentDataPath` |
| `{SAVEDGAMES}/<Game>` | Windows Vista+ `FOLDERID_SavedGames` known folder |
| `{MYGAMES}/<Game>` | Games for Windows guidance, widely adopted since |
| `{APPDATA}/<Vendor>/<Game>` | General Windows roaming-profile practice |

These rules work because a convention entry uses `match_kind = 'any'` and a
candidate is only produced when the expanded path **actually exists**. One Unreal
rule therefore covers every Unreal game in a library without NOVARA knowing which
games use Unreal — and costs one `is_dir` check per game for the ones that do not.

The one genuine *launcher* convention, Steam Cloud's
`<Steam install>/userdata/<account id>/<appid>/remote`, is **not currently
expressible**: the template variable set has no anchor for the Steam installation
directory, and inventing one that resolved to a guessed path would be worse than
omitting the rule. Adding a `{STEAM_INSTALL}` anchor is a small, well-defined future
task — the Steam library locations are already discovered by
`scanner::steam`.

Any use of a third-party dataset requires a licence review before a line of import
code is written. This is a legal precondition, not an implementation detail.

## 10. Performance

| Operation | Budget | Notes |
|---|---|---|
| Match one game | < 1 ms | Indexed lookup on `(match_kind, match_value, platform)` |
| Load built-in KB at first launch | < 200 ms | One-time parse and bulk insert |
| Community refresh apply | < 2 s for ~50k entries | Single transaction, prepared statements |
| Re-decide library after refresh | No filesystem I/O for unchanged templates | Evidence is persisted |

The KB is never queried in a hot loop: detection consults it once per game per
scan, and locked bindings skip it entirely.

## 11. Risks

| Risk | Severity | Mitigation |
|---|---|---|
| **Empty corpus makes the feature look broken** | High | Seed with engine/OS convention rules + curated titles before shipping Phase 1; the Write Witness carries games the KB misses |
| A KB update overwrites user decisions | Critical | Layer separation; KB writes candidates only; `is_locked` bindings untouchable; explicit test |
| Wrong entries erode trust | Medium | `source_ref` provenance; corrections are just higher-priority entries; per-entry rejection recorded locally |
| Licence contamination from an imported dataset | High | Legal review before import; attribution recorded in `source_ref` |
| Community layer unmaintained | Medium | Built-in layer alone must be sufficient; community is additive |
| Corpus growth slows matching | Low | Indexed; tens of thousands of rows is nothing for SQLite |
| Privacy leak via contributions | High | Explicit consent, on-device templating, preview (§8) |

## 12. Future expansion

- **Per-release-group entries** (`note = 'Goldberg'`) selected by evidence rather
  than guessed — requires detecting the release layer, which the install directory
  often reveals.
- **Negative entries**: "this path is *never* a save location for this game",
  useful for suppressing a persistent false positive globally.
- **Confidence hints in the KB**: an entry marked "usually correct" vs "one known
  variant", feeding rule selection in detection §6.
- **KB coverage reporting** in Settings: "the KB knows 812 of your 1,204 games" —
  honest, and it makes contribution feel worthwhile.
- **Deriving `config` and `screenshots` roles** once a consumer exists.
