# Project Status

**Date:** 2026-07-15
**Build:** `cargo check` PASS · `cargo clippy` PASS (0 warnings) · `npm run build` PASS · `npm run tauri:dev` PASS (no runtime panic)

---

## Build Status

| Step | Status | Notes |
|------|--------|-------|
| `cargo check` | ✅ PASS | 0 warnings, 0 errors |
| `cargo clippy` | ✅ PASS | 0 warnings, 0 errors |
| `npm run build` | ✅ PASS | 1 chunk-size warning (pre-existing, ~586 kB), 0 errors |
| `npm run tauri build` | Not yet run | Full packaging not validated |

---

## Feature Implementation Status

### Implemented Features (fully working)

| Feature | Location | Notes |
|---------|----------|-------|
| SQLite database + migrations | `src-tauri/src/db/` | WAL mode, FK enforcement, connection pool; 5 migrations |
| Steam library scanner | `src-tauri/src/scanner/steam.rs` | Reads `libraryfolders.vdf` + ACF manifests; multi-library |
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
| Game metadata | Backend trait + offline stub | `MetadataProvider` async trait defined; `OfflineProvider` returns `None` | No live provider (IGDB/RAWG); auto-fetch not wired |
| Genre tracking | Schema + aggregation | `genres` and `game_genres` tables; `top_genres` surfaced in dashboard and rendered | No UI to assign genres; scanners do not populate genre data |
| Multi-user profiles | Schema only | `profiles` table seeded with "Default" profile | No UI, no profile switching |
| Save glob filter | Schema only | `glob` column in `save_profiles` | `write_archive` does not apply the filter |
| Auto-backup trigger | Schema only | `auto_backup` flag in save_profiles | No background task fires automatic backups |

---

### Placeholder Features (stub / not started)

| Feature | Location | Notes |
|---------|----------|-------|
| Mods page | `src/pages/Mods.tsx` | Renders "Mod tracking — coming next"; `mods` table schema ready |
| Mod backend commands | — | No Tauri commands for mod CRUD |
| Epic Games scanner | `src-tauri/src/scanner/` (commented) | Stub present; not implemented |
| GOG scanner | `src-tauri/src/scanner/` (commented) | Stub present; not implemented |
| Xbox / MS Store scanner | `src-tauri/src/scanner/` (commented) | Stub present; not implemented |
| Live metadata provider | `src-tauri/src/metadata/` | IGDB/RAWG stubs; only offline no-op exists |
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
| `scan_runs` | ✅ | ✅ (write) | — |

---

## Estimated Completion

| Layer | % Complete |
|-------|-----------|
| Database schema | 100% |
| Rust backend — core infrastructure | 100% |
| Rust backend — scanners | 40% (Steam + Manual done; Epic/GOG/Xbox missing) |
| Rust backend — save manager | 100% |
| Rust backend — save detection | 100% |
| Rust backend — cover art | 100% |
| Rust backend — playtime | 95% |
| Rust backend — achievements | 85% |
| Rust backend — metadata | 5% |
| Rust backend — mods | 0% |
| Frontend — core / layout | 100% |
| Frontend — library (search + sort + cover) | 100% |
| Frontend — game details (hero + artwork pickers) | 100% |
| Frontend — dashboard (cover art + genres + refresh) | 100% |
| Frontend — save manager (detection + badges + delete) | 100% |
| Frontend — mods page | 0% |
| **Overall** | **~88%** |

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

## Recommended Next (Phase 2)

1. **Mods CRUD** — Schema + stub are ready. Add ~200 lines Rust + ~150 lines TypeScript.
2. **Epic Games Store scanner** — Parse `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\*.item` JSON.
3. **Bundle size reduction** — Vite `manualChunks` to split Recharts below 500 kB.
4. **GOG Galaxy scanner** — Read `%PROGRAMDATA%\GOG.com\Galaxy\storage\galaxy.db`.
