# The built-in knowledge base

Save-location data compiled into the binary. This directory is the corpus; `build.rs`
merges it into a single blob at build time and `saves::kb::builtin` loads that blob at
startup.

## Adding an entry

**Drop it in the directory that matches what kind of location it is.** Nothing else is
required — no Rust change, no registration, no build-file edit.

```
data/kb/
  manifest.json                       corpus version
  official/<letter>.json              one game's real save location
  engine/conventions.json             "games on this engine write here"
  os/windows.json                     Windows known folders and profile practice
  launcher/                           storefront-specific locations
  community/redirection-roots.json    shared roots used by save-redirection layers
  portable/install-dir.json           beside the executable
```

For `official/`, the file is the **first character of `match_value`**. `hollowknight` goes
in `official/h.json`. That is mechanical on purpose: there is no judgement call about where
an entry belongs, and `entries_live_in_the_shard_their_key_implies` enforces it.

Copy an adjacent entry as a template. The file already declares the right `layout`.

## Directories are organisation, not behaviour

The directory a file sits in has **no effect on matching, confidence, authority, evidence
or the decision table**. It exists so the corpus stays reviewable as it grows and so two
contributors adding different games never touch the same file.

What *does* affect behaviour is the `layout` field, declared **once per file** at the top:

```json
{
  "layout": "official",
  "entries": [ ... ]
}
```

Layout is declared rather than inferred from the path deliberately. If the directory
determined it, moving a file would silently change whether its entries can bind. A test
asserts the declared layout matches the directory name, so a misfiled file is caught — but
the path is never load-bearing.

## What each layout means

| Layout | The claim | Can it bind alone? |
|---|---|---|
| `official` | The game as shipped writes here | **Yes** |
| `user_defined` | The user told us (never authored here) | **Yes** |
| `engine` | Games on this engine write here | No — advisory |
| `os` | A conventional Windows location | No — advisory |
| `launcher` | This storefront keeps save data here | No — advisory |
| `portable` | Self-contained installs write beside the executable | No — advisory |
| `community` | Installs using a redirection layer write here | No — advisory |

Advisory means the entry describes a **class** of installs. Whether *this* install belongs
to that class is exactly what is unknown, so it suggests and content or mtime evidence
promotes it. See `docs/architecture/KNOWLEDGE_BASE.md` §4.0.

Layout is free-form: a new one can be introduced as data. Anything the build does not
recognise is treated as advisory, so an unknown layout is safe by construction. Granting a
layout binding authority is a reviewed change in `saves::kb::layout`.

## Two kinds of entry

**Keyed** (`match_kind` is `title_norm`, `steam_appid`, `gog_id`, `epic_id` or `exe_name`)
— a claim about one game. `priority` 10.

**Convention** (`match_kind` is `any`) — a claim about a path shape, applied to every game
in the library. `priority` 100+. Safe because a candidate is only produced when the expanded
path actually **exists**, so one Unreal rule covers every Unreal game without NOVARA
knowing which games use Unreal.

Lower `priority` wins, so a keyed entry outranks a convention for the same path.

`match_value` for `title_norm` must be **pre-normalised**: lowercase, all non-alphanumerics
removed. `Marvel's Spider-Man` → `marvelsspiderman`. Validation rejects anything else rather
than repairing it, because a near-miss is a well-formed entry that can never match.

## Rules for writing entries

**Never guess a path.** A wrong entry is worse than a missing one: the Write Witness will
eventually find what the corpus misses, but a wrong path can be bound and restored into.

**When the exact subfolder is uncertain, name the parent.** Terraria keeps saves in both
`Players/` and `Worlds/`, so its entry names `Terraria/`. A parent that always exists beats
a subfolder that sometimes does — and you can add both, at different priorities, which is
what `red-dead-redemption-2` and `red-dead-redemption-2-parent` do.

**A game may have several entries.** There is no uniqueness constraint on
`(match_kind, match_value)`; every matching entry is expanded and the evidence model decides
between them. Multiple layouts per game is the normal case, not a workaround.

**Templates cannot escape their anchor.** Absolute paths, drive letters, UNC prefixes and
`..` segments are rejected at load. A template must start with a directory variable.

## Provenance

Every entry carries `source_ref`, which is what makes a wrong entry fixable a year later:

| `source_ref` | Meaning |
|---|---|
| `curated:phase1` | Authored from familiarity. **Unverified.** |
| `curated:phase1-validated-1.22` | Confirmed against a real installation |
| `engine-convention:<engine>` | Documented engine default |
| `os-convention:<what>` | Windows known folder or documented practice |
| `community-layout:observed` | A directory observed in the wild |

**No third-party dataset has been imported.** PCGamingWiki, Ludusavi and similar were not
consulted, and using one requires a licence review first — see
`docs/architecture/KNOWLEDGE_BASE.md` §9. Do not paste entries from them.

### Validation status

Probed against a real machine during task 1.22:

- **Confirmed:** `builtin:elden-ring` resolved a real `{WILDCARD}` account directory,
  validating wildcard expansion end to end.
- **Corrected:** `red-dead-redemption-2`. The game folder existed but `Profiles/` did not, so
  the entry contributed nothing on an installed game. A parent-level fallback was **added**
  at lower precedence rather than replacing the precise entry — both are correct, and the
  precise one still wins when it resolves.
- **Inconclusive:** the remaining entries. Their games are not installed on the test machine,
  so a miss says nothing about the path. Nothing was removed: a miss with no evidence is not
  grounds for deletion.

## Not expressible yet

Steam Cloud's `<Steam install>/userdata/<account id>/<appid>/remote` is the one genuine
*launcher* convention, and the template variable set has no anchor for the Steam
installation directory. Inventing one that resolved to a guessed path would be worse than
omitting the rule, so `launcher/` is currently empty. Adding a `{STEAM_INSTALL}` anchor is
tracked as roadmap decision 8.

## How the build works

`build.rs` walks this directory recursively, merges every `entries` array into one JSON
document in `OUT_DIR`, and fails the build on unparseable JSON, a missing `layout`, a missing
required field, or a duplicate id.

Because the walk is recursive, **granularity is a data decision**. Letter shards suit the
current size; splitting `official/h.json` into `official/h/hollow-knight.json` later needs no
code change.

Merge order is sorted by path, then by position within a file. That is load-bearing: startup
idempotence compares a SHA-256 of the merged bytes, and unstable ordering would make the
corpus reload on every launch.
