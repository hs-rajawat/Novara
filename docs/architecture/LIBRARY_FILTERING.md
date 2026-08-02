# Library filtering

**Status: implemented.** `src-tauri/src/scanner/filter.rs`, `db/skipped_items.rs`,
migration `0013`.

## 1. The problem

Steam installs its own system components as ordinary apps: same `steamapps/common`
directory, same `appmanifest_*.acf` shape, same everything a game has. *Steamworks Common
Redistributables*, *Steam Linux Runtime*, Proton builds and SDKs all arrive looking exactly
like library entries.

This was found by real-library validation during task 1.22, where
*Steamworks Common Redistributables* was sitting in the library and being scanned for save
files like any other game.

## 2. Where it belongs

**In the scanner, before import.** It is an import question, not a detection question.

A component that never enters the library costs nothing downstream: no save scan, no
artwork lookup, no playtime row, no UI clutter. Filtering later would mean every subsystem
carrying its own idea of what counts as a game, and they would drift.

## 3. An honest correction about "prefer Steam metadata"

The obvious design is to read Steam's **app type** and drop anything that is not a game.
That field is not available locally.

`appmanifest_*.acf` carries only `appid`, `name`, `installdir`, `SizeOnDisk` and
`StateFlags` — verified against the parser, not assumed. App type lives in:

* `appcache/appinfo.vdf` — binary, undocumented, and its layout changes across client
  releases. Reading it means a reverse-engineered parser on the scan path.
* The Steam Web API — network, and NOVARA treats metadata networking as optional.

So the metadata-first signal that *is* available is the **app id**, and rule 1 uses it.
That is categorically different from matching display names: app id `228980` is Steamworks
Common Redistributables permanently, whereas its name is localised and can be renamed.
`an_app_id_match_survives_a_renamed_or_localised_title` pins that difference.

`appinfo.vdf` remains an option if the id table ever proves insufficient. It is not worth a
binary parser today for a list that changes a few times a year.

## 4. Rules, in order, first match wins

| # | Rule | Signal | Why it is at this position |
|---|---|---|---|
| 1 | `steam_system_app_id` | A known Steam app id | Strongest: an id is assigned by the storefront and never changes |
| 2 | `no_launchable_executable` | The scanner found nothing to run | Structural, source-independent, but only meaningful where the scanner resolves executables |
| 3 | `system_name_pattern` | An anchored name phrase | Last resort — a name is the weakest identity there is |

### 4.1 Rule 2's trap

Steam launches through `steam://` and never resolves an executable, so `has_executable` is
`None` for every Steam entry. Treating `None` as "no executable" would reject the entire
Steam library. The field is deliberately `Option<bool>`: `None` means *the scanner does not
answer this question*, which is not evidence of anything.

### 4.2 Rule 3 is deliberately narrow

The asymmetry that governs this list: **wrongly hiding a game is far worse than wrongly
importing a component.** A component is a row of clutter the user can hide; a game the
scanner refuses to import is a feature that looks broken and offers no clue why.

So bare words that could appear in a real title — `Runtime`, `Tools`, `Benchmark`,
`SDK` on their own — are **excluded on purpose**. `Sanctum 2 Tools`,
`Final Fantasy XV Benchmark` and `Runtime Terror` are shipped games.
`real_games_with_systemy_titles_are_still_imported` holds that line, and it is longer than
the list of things the filter catches.

`Proton` needs a prefix test rather than a substring one, because `Protonwar` is a game.

## 5. Nothing is silently dropped

A skip is recorded in `skipped_library_items` with the rule that fired and a sentence, and
`ScanReport.skipped` reports the count so the gap between `found` and `added` is never
unexplained.

This is the same principle detection follows for its rejections: an item that disappears
with no explanation is indistinguishable from a bug.

## 6. User overrides

`skipped_library_items.override_import` exists now so that adding the UI needs no migration.
The scanner already honours it — an overridden item is imported despite the filter — and
`Db::set_import_override` is the command surface a future "Import anyway" action will call.

The override is checked in the **scanner**, not inside `classify`, so the filter stays a pure
function of the candidate and remains unit-testable without a database. A rescan never clears
an override (`a_rescan_does_not_clear_an_override`).

Not built yet: the UI, and a "Show system components" library view. Both read the table
above.

## 7. Extending it

Adding a system component means adding a row to `STEAM_SYSTEM_APP_IDS` — an id and what it
is. Adding a *rule* means adding a branch to `classify` plus its rule name, and the reason
string is the contract with the user.

Every addition to rule 3 should come with entries in
`real_games_with_systemy_titles_are_still_imported` demonstrating what it does *not* catch.
A filter list that grows without that pressure eventually eats somebody's library.

## 8. Other storefronts

Only Steam ships system components into the library today, so only rule 1 has a table. Epic,
GOG and manual installs pass through rules 2 and 3. If another storefront starts doing the
same, it gets its own id table keyed by `source_code` —
`the_steam_app_id_table_only_applies_to_steam` already asserts that ids are storefront-scoped,
because an id is only unique within a store.
