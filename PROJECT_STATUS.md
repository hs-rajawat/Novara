# Project Status

**Date:** 2026-05-28
**Build:** `cargo check` PASS · `npm run build` PASS

---

## Build Status

| Step | Status | Notes |
|------|--------|-------|
| `cargo check` | ✅ PASS | 2 deprecation warnings, 0 errors |
| `npm run build` | ✅ PASS | 1 chunk-size warning, 0 errors |
| `npm run tauri build` | Not yet run | Full packaging not validated |

---

## Feature Implementation Status

### Implemented Features (fully working)

| Feature | Location | Notes |
|---------|----------|-------|
| SQLite database + migrations | `src-tauri/src/db/` | WAL mode, FK enforcement, connection pool |
| Steam library scanner | `src-tauri/src/scanner/steam.rs` | Reads `libraryfolders.vdf` + ACF manifests; multi-library |
| Manual folder scanner | `src-tauri/src/scanner/manual.rs` | Depth-3 walk, exe detection, size computation |
| Scanner orchestrator | `src-tauri/src/scanner/mod.rs` | Parallel execution, dedup via upsert, scan audit log |
| Save backup (create) | `src-tauri/src/save_mgr/mod.rs` | Custom `.gvbk` archive, deterministic format |
| Save restore (atomic) | `src-tauri/src/save_mgr/mod.rs` | Pre-restore auto-backup, atomic directory rename |
| Save profiles CRUD | `src-tauri/src/db/saves.rs` | Multiple profiles per game |
| Playtime tracking — explicit | `src-tauri/src/playtime/mod.rs` | `start()`/`stop()` called from frontend |
| Playtime tracking — passive | `src-tauri/src/playtime/mod.rs` | Background watcher via `sysinfo`, 5 s poll |
| Idle detection | `src-tauri/src/playtime/mod.rs` | Frontend reports idle; tracked separately |
| Achievement CRUD | `src-tauri/src/db/achievements.rs` | Create, toggle unlock, delete; completion % auto-computed |
| Game library CRUD | `src-tauri/src/db/games.rs` | Upsert, list, get, favorite, completion state, notes |
| Duplicate game merge | `src-tauri/src/db/games.rs` | Reparents sessions/saves/achievements to survivor |
| Event bus | `src-tauri/src/events.rs` | `tokio::broadcast`; 9 event variants; forwarded to frontend |
| Settings store | `src-tauri/src/db/settings.rs` | JSON key/value; upsert-safe |
| All Tauri IPC commands | `src-tauri/src/commands/` | 28 handlers covering all subsystems |
| Analytics — dashboard stats | `src-tauri/src/commands/analytics.rs` | Total games, completed, playtime, favorites, recent, genres |
| Analytics — activity heatmap | `src-tauri/src/commands/analytics.rs` | Daily playtime aggregation over configurable window |
| Frontend type system | `src/types/index.ts` | All models mirror Rust exactly |
| Frontend IPC wrapper | `src/lib/ipc.ts` | 28 typed commands, event listener helper |
| Zustand library store | `src/stores/library.ts` | Optimistic updates with rollback, search + filter |
| Dashboard page | `src/pages/Dashboard.tsx` | Stats cards, 90-day activity chart, recently played |
| Library page | `src/pages/Library.tsx` | Game grid, tab filters (All/Favorites/Playing/Backlog/Completed) |
| Game details page | `src/pages/GameDetails.tsx` | State mutations, playtime/rating display, sub-page links |
| Achievements page | `src/pages/Achievements.tsx` | Unlock toggle, create form, delete, unlock % |
| Save manager page | `src/pages/SaveManager.tsx` | Profile creation, backup now, backup list, restore |
| Analytics page | `src/pages/Analytics.tsx` | 365-day SVG heatmap with color intensity |
| Timeline page | `src/pages/Timeline.tsx` | Session history list (200 sessions max) |
| Settings page | `src/pages/Settings.tsx` | Scan path add/remove, folder picker, preferences, app info |
| Sidebar + TopBar | `src/components/` | Navigation, search, scan-now button |
| GameCard component | `src/components/GameCard.tsx` | Cover/initials fallback, favorite/completion badges |

---

### Partially Implemented Features

| Feature | Status | What works | What's missing |
|---------|--------|-----------|----------------|
| Game metadata | Backend trait + offline stub | `MetadataProvider` async trait defined; `OfflineProvider` returns `None` safely | No live provider (IGDB/RAWG); cover art and rich metadata not fetched |
| Genre tracking | Schema + aggregation | `genres` and `game_genres` tables created; `top_genres` surfaced in dashboard stats | No UI to assign genres; scanners do not populate genre data |
| Multi-user profiles | Schema only | `profiles` table seeded with "Default" profile | No UI, no profile switching, no backend profile-scoped queries |
| Game launch ("Play" button) | Passive side only | Passive watcher detects running processes and creates sessions | No explicit launch button that executes the game binary |
| In-app notifications | Events only | Backend emits all events; frontend `onEvent()` listener wired in `App.tsx` | No toast/notification UI; events are consumed silently |

---

### Placeholder Features (stub / not started)

| Feature | Location | Notes |
|---------|----------|-------|
| Mods page | `src/pages/Mods.tsx` | Renders "Mod tracking — coming next"; `mods` table schema ready |
| Mod backend commands | — | No Tauri commands for mod CRUD |
| Epic Games scanner | `src-tauri/src/scanner/` (commented) | Stub comment present; not implemented |
| GOG scanner | `src-tauri/src/scanner/` (commented) | Stub comment present; not implemented |
| Xbox / MS Store scanner | `src-tauri/src/scanner/` (commented) | Stub comment present; not implemented |
| Live metadata provider | `src-tauri/src/metadata/` | IGDB/RAWG stubs commented out; only offline no-op exists |
| Media/screenshots | `db schema only` | `media` table in schema; no commands, no UI |
| Achievement templates | `db schema only` | `achievement_templates` table in schema; no commands, no UI |

---

## Subsystem Deep-Dive

### Scanner Subsystem

| Component | Status |
|-----------|--------|
| `Scanner` async trait | Complete |
| `ScannerOrchestrator` | Complete — runs scanners in parallel, deduplicates results |
| `SteamScanner` | Complete — multi-library VDF parsing, ACF manifest extraction |
| `ManualScanner` | Complete — depth-3 walk, exe ranking, directory sizing |
| `EpicScanner` | Not implemented (commented stub) |
| `GogScanner` | Not implemented (commented stub) |
| `XboxScanner` | Not implemented (commented stub) |
| Scan audit log | Complete — `scan_runs` table records each execution |
| Deprecation warning | `Vdf::parse()` deprecated; replace with `Vdf::from()` (non-blocking) |

### Save Manager Subsystem

| Component | Status |
|-----------|--------|
| `.gvbk` archive write | Complete — deterministic, no external compression deps |
| `.gvbk` archive read / restore | Complete — atomic: pre-restore backup → rename old → restore new |
| Save profile CRUD | Complete |
| Backup listing | Complete |
| Event emission on backup | Complete |
| Frontend UI | Complete — profile creation, backup, list, restore all wired |

### Achievement Subsystem

| Component | Status |
|-----------|--------|
| Achievement CRUD | Complete |
| Unlock toggle | Complete |
| Completion % auto-compute | Complete — computed from unlocked/total on each mutation |
| Sort order management | Complete |
| Achievement templates | Schema only — no commands or UI |
| Steam achievement sync | Not implemented |

### Frontend Status

| Area | Status |
|------|--------|
| TypeScript types | Complete — all Rust models mirrored |
| IPC layer | Complete — typed wrappers for all 28 commands |
| State management (Zustand) | Complete — optimistic updates + rollback |
| Routing | Complete — all pages reachable |
| Dashboard | Complete |
| Library (grid + filters) | Complete |
| Game details | Complete |
| Achievements | Complete |
| Save manager | Complete |
| Analytics heatmap | Complete |
| Timeline (session log) | Complete |
| Settings (scan paths) | Complete |
| Mods | Placeholder only |
| Toast / notification UI | Missing |
| Game launch button | Missing |
| Bundle size | 577 kB (~169 kB gzip); exceeds Vite 500 kB soft limit |

### Database Status

| Table | Schema | Backend CRUD | Frontend |
|-------|--------|-------------|----------|
| `games` | ✅ | ✅ | ✅ |
| `game_installations` | ✅ | ✅ | ✅ (read-only) |
| `play_sessions` | ✅ | ✅ | ✅ (timeline) |
| `achievements` | ✅ | ✅ | ✅ |
| `achievement_templates` | ✅ | — | — |
| `save_profiles` | ✅ | ✅ | ✅ |
| `save_backups` | ✅ | ✅ | ✅ |
| `sources` | ✅ | ✅ (seeded) | — |
| `genres` / `game_genres` | ✅ | partial | — |
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
| Rust backend — playtime | 90% (passive + explicit done; no launch integration) |
| Rust backend — achievements | 85% (CRUD done; templates + Steam sync missing) |
| Rust backend — metadata | 5% (trait defined; no live provider) |
| Rust backend — mods | 0% |
| Frontend — core / layout | 100% |
| Frontend — all pages except mods | 100% |
| Frontend — mods page | 0% |
| Frontend — notifications UI | 0% |
| **Overall** | **~78%** |

---

## Remaining Build Warnings

| Warning | File | Severity | Fix |
|---------|------|----------|-----|
| `Vdf::parse()` deprecated | `src-tauri/src/scanner/steam.rs:107,132` | Low | Replace with `Vdf::from(Vdf::parse(...))` |
| JS bundle > 500 kB | Vite output | Low | Code-split with `React.lazy` + dynamic imports |
| Placeholder icons | `src-tauri/icons/` | Pre-release | Replace with real artwork |
| `tauri build` not validated | — | Medium | Run full packaging before first release |

---

## Recommended Next Development Phase

Priority order based on user-facing impact:

1. **Game launch integration** — Add a "Play" button to `GameDetails` that resolves the installation executable and launches it via Tauri's `shell` plugin. This closes the loop between library and actual play and makes passive playtime tracking activate naturally.

2. **Mods page** — The schema (`mods` table with `enabled`, `load_order`, `source_url`) and page stub are already in place. Wire up backend commands and a simple list UI with enable/disable toggle. Estimated scope: ~200 lines Rust + ~150 lines TypeScript.

3. **Toast / event notifications** — The event bus already fires `AchievementUnlocked`, `BackupCreated`, `ScanCompleted`, etc. Add a lightweight toast component that subscribes to `onEvent()` and surfaces these to the user.

4. **Metadata provider (IGDB or RAWG)** — Implement one live `MetadataProvider` to populate cover art and release year. This dramatically improves library visual quality. Requires an API key setting in the Settings page.

5. **Additional scanners** — Epic Games Store (`C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item` JSON) is the next highest install-base target. GOG Galaxy follows.

6. **Bundle optimization** — Split vendor chunks (Recharts, React Router) with Vite's `manualChunks` to drop below the 500 kB soft limit and improve initial load time.

7. **Production icons + `tauri build` validation** — Replace placeholder icons and run a full packaging build before any release candidate.
