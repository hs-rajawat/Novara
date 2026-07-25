# NOVARA — Project Handoff

> Living handoff for a fresh Claude session to continue development without losing context.
> Last updated: 2026-07-21. Branch: `main`. HEAD: `ce27088`.
> NOTE: This file is documentation-only. Nothing in the app reads it; delete or update freely.

---

## 1. What NOVARA Is

NOVARA is a **local-first desktop game library manager** (think a privacy-respecting Playnite/GOG Galaxy). It scans installed games from multiple launchers, tracks playtime, manages saves/achievements/mods, and presents an artwork-rich UI. Privacy-first: **no network by default**; any online metadata is user-opt-in.

- **Stack:** Tauri 2 (Rust core) + React 18 + TypeScript + Vite. SQLite via `sqlx` (runtime-tokio-rustls).
- **Primary target OS:** Windows 11. Rust uses `#[cfg(windows)]` / `#[cfg(not(windows))]` splits; non-Windows compiles and tests pass but some features (Epic, drive-mount detection) are Windows-authoritative.
- **Product name:** NOVARA (renamed from "Game Vault" in commit `60a1b7b`). See §7 for the branding constraint.

---

## 2. Architecture Overview

### Rust core (`src-tauri/src/`)
Crate layout is documented at the top of `lib.rs`. Dependency graph is a strict DAG.

| Module | Responsibility |
|---|---|
| `error` | Single `AppError`/`AppResult` used across all services. |
| `events` | Tokio **broadcast** bus (`EventBus`, capacity 256). Services emit `AppEvent`; lagging subscribers drop, never block. |
| `db` | `sqlx` SQLite pool (max 4 conns, WAL, `foreign_keys=ON`). Typed repositories per table in `db/*.rs`. Migrations in `src-tauri/migrations/` run on startup. |
| `integrity` | **Library Integrity System** — install-status resolution + background verifier. Pure leaf checks in `mod.rs`; stateful service in `service.rs`. |
| `models` | Domain structs crossing IPC (snake_case for sqlx+serde; TS adapts). |
| `scanner` | Pluggable `Scanner` trait + `ScannerOrchestrator`. Sources: `steam`, `epic`, `manual`. |
| `metadata` | Pluggable `MetadataProvider` trait. **Currently only a no-op `offline` provider** — the next milestone builds this out. |
| `save_mgr` / `save_detect` | Save-folder backup/restore + auto-detection. |
| `playtime` | Process watcher + idle detection (5s poll). |
| `launcher` / `sources` | Launch dispatch (Steam/Epic URIs via `ShellExecuteW`). |
| `commands` | Tauri IPC handlers — the ONLY place that touches `AppHandle`. |
| `state` | `AppState` holds `Db`, `EventBus`, and all long-lived services; each field is internally `Arc`'d so cloning is cheap. |

### Startup sequence (`lib.rs` `setup`)
1. Resolve `app_data_dir`, `create_dir_all`.
2. `block_on(AppState::initialize)` — opens DB `gamevault.db` (filename intentionally unchanged, see §7), runs migrations, constructs services, spawns playtime watcher.
3. `start_event_forwarder` — subscribes to the bus and forwards every `AppEvent` to the frontend over Tauri event `gv://event`.
4. **Only after the forwarder subscribes:** spawn integrity startup sweep + periodic sweep (300s). Ordering is load-bearing — sweeps emit bus events that would be dropped if spawned before the forwarder subscribes.

### Frontend (`src/`)
- Pages: `Dashboard`, `Library`, `GameDetails`, `Achievements`, `SaveManager`, `Mods`, `Timeline`, `Analytics`, `Settings`.
- State: `src/stores/library.ts` (Zustand) — single source of truth for the games list; `selectVisibleGames` does filtering/sorting.
- IPC in `src/lib/ipc.ts` (`api.*` wrappers + `onEvent`). Images resolved via `src/lib/image.ts` `toImgSrc` (passes through `http`/`data:`, else `convertFileSrc` for local paths).
- Backend events drive UI refresh (e.g. `GameDetails` re-fetches on `game_updated` matching its id).

### Event flow pattern
Service does work → `bus.emit(AppEvent::...)` → forwarder → frontend `onEvent` → store reload / component refetch. Follow this for any new background work (metadata/artwork included).

---

## 3. Data Model (current schema)

Migrations applied (`src-tauri/migrations/`):
- `0001_init.sql` — full base schema.
- `0002_executable_override.sql` — manual executable override flag.
- `0003_save_detection.sql` — save auto-detection fields.
- `0004_install_status.sql` — `game_installations.status` column.
- `0005_install_verified_at.sql` — `last_verified_at` column.

**Key tables:** `sources` (seeded: steam, epic, gog, xbox, ubisoft, battle, emulator, manual), `games`, `genres`/`game_genres`, `game_installations`, `play_sessions`, `achievements`/`achievement_templates`, `save_profiles`/`save_backups`, `media`, `mods`, `profiles`, `settings`, `scan_runs`.

**Artwork columns already exist on `games`** (unused-until-populated by user today): `cover_path`, `hero_path`, `icon_path`, `metadata_json`, `metadata_source`. There is NO `logo_path` column yet, and no per-artwork provenance/state table — the metadata milestone will need schema additions (see §5).

**Artwork storage convention (established, reuse it):** user-set artwork is copied into `<app_data>/artwork/<game_id>/<kind>.<ext>` by `copy_artwork` in `commands/games.rs`. The DB stores the absolute path; the frontend converts via `toImgSrc`. Any downloaded artwork should follow the same on-disk layout.

---

## 4. Recently Completed Milestones

Newest first (see `git log`):

- **`ce27088` — Installation Integrity infrastructure (Phase 1).** The most recent milestone. Details in §6.
- **`60a1b7b` — Rebrand "Game Vault" → "NOVARA"** (branding/strings/comments only; compatibility identifiers preserved — see §7).
- **`3d0c0fd` — Epic launcher reliability with pre-warm support.**
- **`c021c0b` — Launcher URIs via `ShellExecuteW`** instead of `cmd /c start`.
- **`2586c00` — Epic Games scanner** added as a library source.
- **`7660574` — Library integrity + missing-game detection** (original, pre-Phase-1 version).
- Earlier: a UI overhaul series (artwork-first Dashboard, cinematic GameDetails, sliding sidebar, GameArtwork primitive with progressive loading), and `8ef6a45` exposing primary platform source on `list_games`/`get_game`.

---

## 5. Planned Roadmap — NEXT MILESTONE: Automatic Metadata & Artwork

**This is the active priority.** The user requested a plan (analysis + provider abstraction, cache structure, schema changes, download/update strategy, refresh policy, error handling) and **approval is required before any coding.** No implementation has started; the previous session entered plan mode and was interrupted before producing the plan.

### Goals (verbatim intent)
- Auto-fetch game metadata after a scan.
- Download + cache cover art, hero art, logos, icons.
- Support Steam and Epic first.
- Store artwork locally for offline use.
- Provider-agnostic so IGDB / SteamGridDB / others can be added later.
- Background downloads must never block scans or launching.
- Missing artwork falls back gracefully to placeholders.

### Existing scaffolding to build on (do NOT reinvent)
- `metadata/mod.rs` already defines `GameMetadata` struct + `MetadataProvider` async trait (`code()`, `lookup(title)`). `offline.rs` is a no-op provider. Commented stubs for `igdb`/`rawg` exist. **This trait likely needs extending** to return/download artwork (currently only returns URLs in `GameMetadata.cover_url`/`hero_url`), and to look up by source app-id (Steam appid / Epic catalog id), not just fuzzy title.
- Artwork on-disk convention + `copy_artwork` helper (§3) — reuse the `<app_data>/artwork/<game_id>/` layout; add a download-to-that-path path.
- `games` columns `metadata_json` / `metadata_source` are ready to hold raw blobs + provenance.
- Event bus for progress + completion (`GameUpdated` already re-renders GameDetails/Library). Consider adding metadata-specific events if granular progress UI is wanted.
- `ScannerOrchestrator::run` is the natural trigger point (post-scan hook, mirroring how `scan_paths_now` already runs a post-scan integrity sweep in `commands/scan.rs`).

### Design areas the plan must cover (per user's explicit ask)
1. **Provider abstraction** — extend `MetadataProvider` for artwork + id-based lookup; a registry/orchestrator picking providers by availability + priority; Steam/Epic as first "providers" (Steam can source artwork from its CDN/appid; Epic from catalog).
2. **Local cache structure** — extend `<app_data>/artwork/<game_id>/` to include `logo`/`icon` kinds; decide filename/provenance scheme; dedupe/eviction not required v1 but note it.
3. **Schema changes** — likely: a `logo_path` column (migration `0006_*`), and possibly an artwork/asset table tracking (kind, source, url, local_path, state, fetched_at, etag) so refresh + per-asset fallback work cleanly. Decide table-vs-columns in the plan.
4. **Download/update strategy** — background task pool, concurrency cap, never block scan/launch, retry/backoff, HTTP client choice (respect privacy — only reach network when the feature is enabled).
5. **Refresh policy** — when to re-fetch (new game, user request, stale after N days, missing asset only), and how to avoid clobbering user-set artwork (mirror the `executable_override` pattern — never overwrite a manual asset).
6. **Error handling** — partial success (some assets fetched, some not), offline/timeout, placeholder fallback already handled by `toImgSrc` returning `undefined` → component placeholder.

### Follow-on roadmap (not yet scheduled)
- Additional scanners: GOG, Xbox, Ubisoft, Battle.net (stubs noted in `scanner/mod.rs`; `sources` table already seeded).
- IGDB / SteamGridDB / RAWG providers behind the same abstraction.

---

## 6. Installation Integrity — Phase 1 (just shipped, `ce27088`)

Context: distinguishes uninstalled games from temporarily-unavailable ones (unplugged drive) and auto-relinks launcher games that moved on disk, preserving play history.

**Install health states** (`InstallStatus` in `integrity/mod.rs`), priority order **`installed > offline > missing > deleted`**:
- `installed` — verified present & launchable.
- `offline` — the install's drive/volume is unmounted; presence can't be checked (temporary; auto-heals on reconnect). Healthier than missing.
- `missing` — install dir present but executable gone (repairable in place via "Locate Executable").
- `deleted` — folder/manifest confirmed gone while its drive is online (real uninstall).

**Move is NOT a persisted state** — it's an automatic reconciliation event: relink the row in place (preserve `game_id` + history), return to `installed`. Carried by `Resolution { status, relink_to }`.

Key pieces:
- `volume_online(path)` — drive-mount detection (Windows: probe reconstructed volume root; Unix: best-effort ancestor walk). This is what separates `deleted` from `offline`.
- `resolve_status` (generic, volume-first) and `resolve_installation_status` (launcher-aware, returns `Resolution`); `resolve_launcher` shared launcher logic; `paths_equal` (trailing-sep + case tolerant).
- Scanners: `steam.rs` / `epic.rs` gained `locate()` returning the launcher-authoritative install path; `is_installed()` derives from it. Epic context stores only *complete* installs mapped to `InstallLocation`.
- `db/games.rs`: `STATUS_PRIORITY_ORDER` CASE fragment shared by `list_primary_sources` / `get_primary_source` (health-first tiebreak so a stale ghost can't mislabel a live install); move-aware `upsert_game` (step "3a" relinks + deletes destination ghosts); `relink_installation(id, new_dir)` (transactional, preserves identity, deletes ghosts, forces `installed`).
- `integrity/service.rs`: verifier relinks on move; combined "no longer available" Warning notice for newly missing+deleted; **offline transitions are silent** (unplugged drive is not an alarm).
- `commands/games.rs` `launch_game`: launch-time relink + re-read so launch uses the new dir; blocks launch when `!= Installed` with a drive-disconnect-aware message.
- Frontend: `deleted` / "Drive offline" badges in `GameDetails.tsx`; `.save-badge.deleted` (danger red) + `.save-badge.offline` (neutral slate) in `app.css`; `library.ts` `selectVisibleGames` hides launcher-managed `missing`/`deleted` games but keeps `offline` visible.

`is_auto_managed(status)` gates the verifier/upsert self-heal to the four disk-derived states; any future **user-asserted** state (ignored, archived, …) must NOT be added to it, so manual states survive automatic sweeps.

**Status column is free-form TEXT** (from `0004_install_status.sql`) — adding new disk-derived states needed no migration, only enum + `as_str`/`FromStr` + `resolve_status` + `is_auto_managed` updates.

Validation at ship: `cargo check`, `cargo clippy --all-targets -- -D warnings`, `npm run build`, `cargo test` (15 tests) all green.

---

## 7. Hard Constraints Future Work MUST Follow

1. **Migrations are immutable.** Never edit an existing migration; only add new `000N_*.sql` files. The comment "GameVault schema v1" in `0001_init.sql` and any `GameVault`/`gamevault` identifier inside migrations must stay.
2. **Do NOT rename compatibility-sensitive identifiers** during any rebrand/refactor: crate names, package identifiers, environment variable names (e.g. `GAMEVAULT_EPIC_MANIFESTS_DIR`), and the **database filename `gamevault.db`** (`state.rs`). These are load-bearing for existing installs. Branding changes are user-facing strings/comments only.
3. **Privacy-first / offline by default.** No network access unless the user opts in. The metadata milestone must gate all network calls behind an explicit enabled setting (there's already an `offline_mode` setting seeded in `0001`).
4. **Background work must never block scans or launches.** Spawn via `tauri::async_runtime::spawn`; communicate via the event bus. Mirror the existing post-scan integrity sweep pattern (best-effort, failures logged, never fail the user's action).
5. **Event-forwarder ordering.** Any new startup background task that emits bus events must be spawned AFTER `start_event_forwarder` in `lib.rs`, or its early events are dropped.
6. **Never overwrite user-set data on rescans.** Follow the `executable_override` / `is_manual_override` precedent — auto-fetched artwork/metadata must not clobber a user's manual choice.
7. **DAG discipline.** `scanner::{steam,epic}` are pure filesystem leaves (no `Db`, no bus) so `integrity` and `db` can depend on them without a cycle. Keep new leaf logic pure; keep stateful orchestration in services/commands.
8. **Commit hygiene.** One milestone per commit, message-first-line `type: summary`, co-author trailer `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>`. Keep unrelated changes out. Confirm nothing is pushed before amending.

---

## 8. Deferred Work

### Installation Integrity — Phase 2 (DEFERRED, user confirmed keep-deferred 2026-07-21)
**Manual move reconciliation and heuristics.** Detecting that a *manually-added* game (no launcher app-id) moved on disk, and relinking it. Deferred to its own isolated milestone because it's inherently heuristic.

Non-negotiable constraint for whenever it's picked up: **extremely conservative — never risk merging two different games. False negatives are acceptable; false positives are NOT. Any ambiguity → do not relink automatically.** Phase 1's launcher-based reconciliation is safe because the launcher's app-id is authoritative; manual moves have no such anchor, so any heuristic (folder name, executable hash, size match) must clear a very high bar or defer to the user.

### Other deferred/stubbed
- Scanners for GOG, Xbox, Ubisoft, Battle.net — `sources` seeded, `scanner/mod.rs` has commented stubs.
- Real metadata providers (IGDB, RAWG, SteamGridDB) — trait exists, only `offline` no-op implemented.

---

## 9. Current State of the Codebase

- **Working tree:** clean (as of writing). Branch `main`, upstream `origin/main`. HEAD `ce27088` is **not yet pushed**.
- **Build/test health at last check:** Rust `cargo check` + `clippy -D warnings` clean; `cargo test` green (15 tests incl. integrity unit tests in `integrity/mod.rs` and scanner tests in `epic.rs`); `npm run build` clean.
- **No outstanding known bugs** from Phase 1. The next action is **producing the Metadata/Artwork plan for approval** (§5) — implementation is blocked on user sign-off.
- Line endings: repo is LF; Git warns about LF→CRLF on Windows checkout (cosmetic, expected).

### Immediate next step for a new session
Resume the **Metadata & Artwork milestone planning** (§5): analyze the current metadata module + artwork storage, then propose the provider abstraction, cache structure, schema changes, download/update strategy, refresh policy, and error handling. **Wait for user approval before writing any code.**
