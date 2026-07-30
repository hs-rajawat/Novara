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
| Hand-curated top ~500 titles | none | High per entry; makes the feature feel real on day one |
| Launcher conventions (Steam `userdata`, Epic, GOG Galaxy defaults) | none | Broad coverage from a handful of *rules* rather than entries |
| Existing open datasets (PCGamingWiki, Ludusavi manifest) | **Must check licence and attribute** | Very large; would be transformative if compatible |
| User contributions | consent (§8) | Slow start, compounding |

The launcher-conventions row is the highest leverage: a dozen template rules cover
thousands of games because launcher-managed titles are conventional by definition.
Manual installs are where per-entry curation is unavoidable.

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
| **Empty corpus makes the feature look broken** | High | Seed with launcher conventions + top titles before shipping Phase 1; the Write Witness carries games the KB misses |
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
