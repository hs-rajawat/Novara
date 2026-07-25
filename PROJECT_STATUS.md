# Project Status

**Date:** 2026-07-25
**HEAD:** `26795b9` (Metadata & Artwork subsystem)
**Build:** `cargo check` PASS · `npm run build` PASS · `npm run tauri build` **NEVER RUN**

> **Read this first.** An audit on 2026-07-25 found that this file
> materially overstated completeness. The "Estimated Completion" figures
> below have been revised, and a Known Issues section has been added. A
> feature being *implemented* is no longer recorded as *working* unless it
> has been verified end to end at runtime.

---

## Build Status

| Step | Status | Notes |
|------|--------|-------|
| `cargo check` | ✅ PASS | 0 warnings, 0 errors |
| `cargo clippy` | ✅ PASS | 0 warnings, 0 errors |
| `npm run build` | ✅ PASS | 1 chunk-size warning (pre-existing, ~586 kB), 0 errors |
| `npm run tauri build` | ❌ **NEVER RUN** | Release packaging has never been validated. `panic = "abort"`, LTO and `strip` are all unexercised, and the release CSP behaves differently from dev. Release-gate item (remediation Batch 10) |

---

## Feature Implementation Status

### Implemented Features (fully working)

| Feature | Location | Notes |
|---------|----------|-------|
| SQLite database + migrations | `src-tauri/src/db/` | WAL mode, FK enforcement, connection pool; **6 migrations** |
| Steam library scanner | `src-tauri/src/scanner/steam.rs` | Reads `libraryfolders.vdf` + ACF manifests; multi-library |
| Epic Games scanner | `src-tauri/src/scanner/epic.rs` | Parses `%PROGRAMDATA%\Epic\...\Manifests\*.item`; `locate()` is launcher-authoritative; Windows-only |
| Metadata & artwork subsystem | `src-tauri/src/metadata/` | Provider abstraction, identity resolution (app-id first), `steam_local` / `steam_cdn` / `epic_catalog` providers, `artwork_assets` provenance ledger, per-provider circuit breakers, opt-in `metadata_enabled` gate. **Fetching works; rendering does not — see Known Issues** |
| Manual folder scanner | `src-tauri/src/scanner/manual.rs` | Depth-3 walk, exe detection, size computation |
| Scanner orchestrator | `src-tauri/src/scanner/mod.rs` | Parallel execution, dedup via upsert, scan audit log |
| Library Integrity System | `src-tauri/src/integrity/` | Source-aware install-status resolution (Steam manifest `StateFlags` + generic exe/dir check); reused `SteamContext` per sweep; startup + periodic (5 min) verifier; post-scan reconciliation; launch-time recheck + missing-state launch guard; non-destructive Remove/Restore + Locate Executable recovery; background tasks spawned via `tauri::async_runtime::spawn` |
| Save backup (create) | `src-tauri/src/save_mgr/mod.rs` | Custom `.gvbk` archive, deterministic format |
| Save restore (atomic) | `src-tauri/src/save_mgr/mod.rs` | Pre-restore auto-backup, atomic directory rename |
| Save profiles CRUD | `src-tauri/src/db/saves.rs` | Multiple profiles per game; create + delete |
| Save path detection | `src-tauri/src/save_detect.rs` | Heuristic search of AppData/Roaming, Local, LocalLow, Documents, Saved Games |
| Playtime tracking — explicit | `src-tauri/src/playtime/mod.rs` | `start()`/`stop()` called from frontend |
| Playtime tracking — passive | `src-tauri/src/playtime/mod.rs` | Background watcher via `sysinfo`, 5 s poll |
| Idle detection | `src-tauri/src/playtime/mod.rs` | Frontend reports idle; tracked separately |
| Achievement CRUD | `src-tauri/src/db/achievements.rs` | Create, toggle unlock, delete; completion % auto-computed |
| Game library CRUD | `src-tauri/src/db/games.rs` | Upsert, list, get, favorite, completion state, notes, cover/hero |
| Game launch | `src-tauri/src/commands/games.rs` | Spawns exe directly; opens `steam://run/<id>` URI for Steam games |
| Cover art management | `src-tauri/src/commands/games.rs` | set_cover_path / set_hero_path; copies to `<app_data>/artwork/`; emits GameUpdated |
| Executable override | `src-tauri/src/commands/games.rs` | User-chosen exe survives rescans; import-by-exe flow |
| Duplicate game merge | `src-tauri/src/db/games.rs` | Reparents sessions/saves/achievements to survivor |
| Event bus | `src-tauri/src/events.rs` | `tokio::broadcast`; 9 event variants; forwarded to frontend |
| Settings store | `src-tauri/src/db/settings.rs` | JSON key/value; upsert-safe |
| All Tauri IPC commands | `src-tauri/src/commands/` | 35+ handlers covering all subsystems |
| Analytics — dashboard stats | `src-tauri/src/commands/analytics.rs` | Total games, completed, playtime, favorites, recent, genres |
| Analytics — activity heatmap | `src-tauri/src/commands/analytics.rs` | Daily playtime aggregation over configurable window |
| Frontend type system | `src/types/index.ts` | All models mirror Rust exactly |
| Frontend IPC wrapper | `src/lib/ipc.ts` | Typed commands for all subsystems, event listener helper |
| Image path helper | `src/lib/image.ts` | `toImgSrc()` converts local paths via `convertFileSrc` |
| Zustand library store | `src/stores/library.ts` | Optimistic updates, search, filter, sort (5 modes, persisted) |
| Dashboard page | `src/pages/Dashboard.tsx` | Stats cards, 90-day activity chart, recently played with cover art, top genres; event-driven refresh |
| Library page | `src/pages/Library.tsx` | Game grid, tab filters, 5 sort options (title/playtime/last played/added) |
| Game details page | `src/pages/GameDetails.tsx` | Hero banner, Artwork section (cover+hero pickers), state, installations, notes |
| Achievements page | `src/pages/Achievements.tsx` | Unlock toggle, create form, delete, unlock % |
| Save manager page | `src/pages/SaveManager.tsx` | Detection panel, Auto/Manual badges, profile deletion, backup/restore |
| Analytics page | `src/pages/Analytics.tsx` | 365-day SVG heatmap with color intensity |
| Timeline page | `src/pages/Timeline.tsx` | Session history list (200 sessions max) |
| Settings page | `src/pages/Settings.tsx` | Scan path add/remove, import executable, folder picker, preferences |
| Sidebar + TopBar | `src/components/` | Navigation, search (wired to library store), scan-now button |
| GameCard component | `src/components/GameCard.tsx` | Cover via `convertFileSrc`; initials fallback; favorite/completion badges |
| Toast notifications | `src/components/ToastContainer.tsx` | Auto-dismiss toasts for scan, backup, achievement, notice events |

---

### Partially Implemented Features

| Feature | Status | What works | What's missing |
|---------|--------|-----------|----------------|
| Game metadata | Implemented, rendering blocked | Provider abstraction, Steam/Epic providers, artwork ledger, opt-in gate | Artwork does not render (asset protocol disabled); fill loop never terminates; no rate limiting; no tests |
| Idle tracking | Backend only, unreachable | `report_idle` implemented; `idle_threshold_seconds` seeded | `report_idle` is **not registered** as a Tauri command and the threshold has no reader, so `idle_seconds` is always 0 — the "active vs total time" schema goal does not exist yet |
| Genre tracking | Schema + aggregation | `genres` and `game_genres` tables; `top_genres` surfaced in dashboard and rendered | No UI to assign genres; scanners do not populate genre data |
| Multi-user profiles | Schema only | `profiles` table seeded with "Default" profile | No UI, no profile switching |
| Save glob filter | Schema only | `glob` column in `save_profiles` | `write_archive` does not apply the filter |
| Auto-backup trigger | Schema only | `auto_backup` flag in save_profiles | No background task fires automatic backups |

---

### Placeholder Features (stub / not started)

| Feature | Location | Notes |
|---------|----------|-------|
| Mods page | `src/pages/Mods.tsx` | Renders developer-facing placeholder copy (should be user-facing — remediation 8.11); `mods` table schema ready |
| Mod backend commands | — | No Tauri commands for mod CRUD |
| GOG scanner | `src-tauri/src/scanner/` (commented) | Stub present; not implemented |
| Xbox / MS Store scanner | `src-tauri/src/scanner/` (commented) | Stub present; not implemented |
| Third-party metadata providers | `src-tauri/src/metadata/providers/` | IGDB / RAWG / SteamGridDB not implemented. The abstraction is ready for them; the shipped providers are Steam and Epic only |
| Media/screenshots | DB schema only | `media` table in schema; no commands, no UI |
| Achievement templates | DB schema only | `achievement_templates` table; no commands, no UI |

---

## Database Status

| Table | Schema | Backend CRUD | Frontend |
|-------|--------|-------------|----------|
| `games` | ✅ | ✅ (+ cover/hero setters) | ✅ |
| `game_installations` | ✅ | ✅ (+ executable override, + integrity status) | ✅ (Missing badge, Locate Executable) |
| `play_sessions` | ✅ | ✅ | ✅ (timeline) |
| `achievements` | ✅ | ✅ | ✅ |
| `achievement_templates` | ✅ | — | — |
| `save_profiles` | ✅ (+ `is_manual_override`) | ✅ (+ delete + detection) | ✅ |
| `save_backups` | ✅ | ✅ | ✅ |
| `sources` | ✅ | ✅ (seeded) | — |
| `genres` / `game_genres` | ✅ | partial | dashboard display |
| `media` | ✅ | — | — |
| `mods` | ✅ | — | — |
| `profiles` | ✅ | — (seeded) | — |
| `settings` | ✅ | ✅ | ✅ |
| `scan_runs` | ✅ | ✅ (write) | — (never read; no scan history UI) |
| `artwork_assets` | ✅ (0006) | ✅ (ledger + ownership guard) | — (read indirectly via `games.*_path`) |

---

## Estimated Completion

> Revised 2026-07-25. Previous figures counted code written rather than
> behaviour verified, which is how "cover art 100%" coexisted with artwork
> that never rendered. A layer is only 100% if it has been exercised at
> runtime and has tests.

| Layer | % Complete | Note |
|-------|-----------|------|
| Database schema | 100% | 6 migrations, CHECK-constrained where sets are closed |
| Rust backend — core infrastructure | 95% | No graceful shutdown; open play sessions are never closed |
| Rust backend — scanners | 55% | Steam + Epic + Manual done; GOG/Xbox/Ubisoft/Battle.net/emulator missing |
| Rust backend — save manager | 80% | Restore has a path-construction bug on dotted folder names; displaced saves are never cleaned up; `glob` filter accepted and ignored |
| Rust backend — save detection | 95% | `dedup_by` only removes adjacent duplicates |
| Rust backend — artwork & metadata | 70% | Fetching works; rendering blocked, loop non-terminating, unthrottled, untested |
| Rust backend — playtime | 60% | Steam/launcher sessions record as 0–5 s; no graceful shutdown; process matching by bare basename |
| Rust backend — achievements | 85% | No template import, no Steam sync |
| Rust backend — mods | 0% | Schema only |
| Frontend — core / layout | 90% | No error boundary, no modal system, listener leaks |
| Frontend — library | 95% | `<button>` nested in `<Link>` on the primary control |
| Frontend — game details | 90% | No icon artwork slot despite complete backend |
| Frontend — dashboard | 85% | `dashboard_stats` fails outright on an empty library |
| Frontend — save manager | 90% | Silent no-op when a profile label is empty |
| Frontend — analytics | 85% | Heatmap is UTC-keyed against local grid bounds — off by one day east of UTC |
| Frontend — mods page | 0% | Placeholder |
| **Test coverage** | **~10%** | 15 Rust tests, all in `integrity/` and `scanner/epic.rs`. Zero for `db/*`, zero for `metadata/*`, zero frontend tests |
| **Release readiness** | **0%** | `tauri build` never run; placeholder icons |
| **Overall** | **~78%** | Feature-complete-looking, not release-ready |

---

## Known Issues

Full classification lives in the remediation plan. Highest-severity items:

| # | Issue | Batch |
|---|---|---|
| 1 | Tauri asset protocol disabled + CSP scheme mismatch — **no artwork renders at all**, silently | 1 |
| 2 | `dashboard_stats` decodes `NULL` into `i64` — command **fails on every fresh install** | 3 |
| 3 | `merge_games` violates `game_genres` PK when both games share a genre — whole merge rolls back | 3 |
| 4 | `merge_games` does not reparent `artwork_assets` — cascade destroys the losing game's ledger | 3 |
| 5 | Steam sessions die within 5 s of launch — corrupts all playtime, analytics and "last played" | 4 |
| 6 | `ArtworkKind::Icon` unsatisfiable — fill loop re-runs the full provider chain every scan, unthrottled | 5 |
| 7 | Save restore mis-constructs sibling paths for folders containing dots | 7 |
| 8 | Displaced save directories accumulate forever after every restore | 7 |
| 9 | Pre-restore safety backup failure is swallowed — restore proceeds without an undo path | 7 |
| 10 | Failures are invisible app-wide: `catch {}`, `console.error`, `unwrap_or_default()` on query errors | 2 |

---

## Remaining Build Warnings

| Warning | File | Severity | Fix |
|---------|------|----------|-----|
| JS bundle > 500 kB | Vite output | Low | Code-split with `manualChunks` (Phase 2) |
| Placeholder icons | `src-tauri/icons/` | Pre-release | Replace with real artwork (Phase 5) |
| `tauri build` not validated | — | Medium | Run full packaging before first release (Phase 5) |

---

## Milestones Completed

### Library Integrity System — Missing Game Detection (2026-07-15)
- ✅ Steam uninstall detection via manifest-based verification; conservative `StateFlags` handling (absent manifest = missing; only bit 1 Uninstalling treated as not-installed)
- ✅ Efficient `SteamContext` reuse — Steam discovery once per sweep, not per game
- ✅ Manual missing-executable detection; shared source-aware `resolve_installation_status`
- ✅ Startup + periodic (~5 min) background verification; post-scan reconciliation; launch-time recheck + missing-state launch guard
- ✅ Missing status surfaced to frontend; Missing badge + status-aware Play behavior
- ✅ Locate Executable recovery; non-destructive Remove/Restore via existing `is_hidden`; Show hidden toggle
- ✅ Historical data (playtime, sessions, achievements, saves, mods, artwork) preserved across all state changes
- ✅ `GameUpdated` event propagation + live UI refresh
- ✅ Tauri runtime panic fixed — background sweeps spawned via `tauri::async_runtime::spawn`
- ✅ Migrations `0004_install_status.sql`, `0005_install_verified_at.sql`

### Phase 1 (2026-05-29)
- ✅ Game launch (`launch_game` + Play button)
- ✅ Toast notifications (`ToastContainer`)
- ✅ Zero clippy warnings

### UX Milestone (2026-05-29)
- ✅ Cover art: `set_cover_path` / `set_hero_path` commands; artwork copied to `<app_data>/artwork/`; `convertFileSrc` rendering in all views
- ✅ Hero banner in Game Details; Artwork section with cover + hero pickers
- ✅ Cover thumbnails in library cards and Dashboard recently-played
- ✅ Library sort: 5 modes (title A-Z/Z-A, playtime, last played, added); persisted in localStorage
- ✅ Dashboard: top genres section; event-driven refresh on scan/session/game events
- ✅ Save detection: heuristic search across 6 OS locations; results panel with "Use this path"
- ✅ Save profiles: Auto/Manual badge; delete button; `is_manual_override` column

## Recommended Next

Pre-release remediation, in dependency order (not severity order). Batch 0
(repository truth + test harness) is complete.

1. **Batch 1 — asset protocol.** One config change; unblocks all visual verification.
2. **Batch 2 — error surface.** Error boundary, error→message mapping, stop swallowing IPC failures.
3. **Batch 3 — data-correctness bugs.** `dashboard_stats` NULL decode, `merge_games` genre PK and artwork reparenting, unscoped installation DELETE, unified primary-installation rule.
4. **Batch 4 — playtime integrity.** Watch processes under the install directory; graceful shutdown; repair orphaned sessions.
5. **Batch 5 — artwork pipeline.** Terminal `skipped` state, concurrency cap, delay, persisted backoff, icon UI slot.
6. **Batch 6 — async hygiene.** Background the scan, coalesce events, move blocking I/O off the async threads.
7. **Batch 7 — save manager safety.**
8. **Batch 8 — DESIGN.md conformance.**
9. **Batch 9 — test backfill + a test proving zero network calls when metadata is off.**
10. **Batch 10 — release gate.** First-ever `tauri build`, real icons.

Feature work (mods CRUD, GOG scanner, bundle split, third-party providers)
resumes after the release gate.
