# GameVault — Development Roadmap

**App:** Local-first game library & progress tracker (Tauri 2 + React 18 + SQLite)  
**Version:** 0.1.0  
**Current Phase:** Phase 2 (Mods & Additional Scanners)  
**Overall Completion:** ~83%

---

## Completed Work

All items below are fully implemented, tested, and passing `cargo check` + `cargo clippy` + `npm run build` as of 2026-05-29.

### Backend (Rust / Tauri)

| Component | Location |
|-----------|----------|
| SQLite database + WAL mode + FK enforcement + connection pool | `src-tauri/src/db/mod.rs` |
| Schema migration (single migration, all 14 tables) | `src-tauri/migrations/0001_init.sql` |
| Steam library scanner — multi-library VDF + ACF parsing | `src-tauri/src/scanner/steam.rs` |
| Manual folder scanner — depth-3 walk, exe ranking, size | `src-tauri/src/scanner/manual.rs` |
| Scanner orchestrator — parallel execution, upsert dedup, scan audit log | `src-tauri/src/scanner/mod.rs` |
| Save backup — custom `.gvbk` deterministic archive | `src-tauri/src/save_mgr/mod.rs` |
| Save restore — atomic pre-restore backup → rename → restore | `src-tauri/src/save_mgr/mod.rs` |
| Save profiles CRUD — multiple profiles per game | `src-tauri/src/db/saves.rs` |
| Playtime tracking — explicit (`start`/`stop`) | `src-tauri/src/playtime/mod.rs` |
| Playtime tracking — passive background watcher (sysinfo, 5 s poll) | `src-tauri/src/playtime/mod.rs` |
| Idle detection — frontend-reported idle seconds tracked per session | `src-tauri/src/playtime/mod.rs` |
| Achievement CRUD — create, toggle unlock, delete, auto-completion % | `src-tauri/src/db/achievements.rs` |
| Game library CRUD — upsert, list, favorite, completion state, notes | `src-tauri/src/db/games.rs` |
| Game launch — spawns exe directly; opens `steam://run/<id>` URI for Steam games | `src-tauri/src/commands/games.rs` |
| Duplicate game merge — reparents sessions/saves/achievements to survivor | `src-tauri/src/db/games.rs` |
| Event bus — `tokio::broadcast`, 9 event variants, forwarded to frontend | `src-tauri/src/events.rs` |
| Settings store — JSON key/value, upsert-safe | `src-tauri/src/db/settings.rs` |
| 28 Tauri IPC command handlers across all subsystems | `src-tauri/src/commands/` |
| Analytics — dashboard stats (total, completed, playtime, favorites, genres) | `src-tauri/src/commands/analytics.rs` |
| Analytics — daily activity heatmap aggregation (configurable window) | `src-tauri/src/commands/analytics.rs` |
| `MetadataProvider` async trait + offline no-op fallback | `src-tauri/src/metadata/` |
| `AppState` — Arc'd DB pool, event bus, scanner, saves, playtime tracker | `src-tauri/src/state.rs` |

### Frontend (React / TypeScript)

| Component | Location |
|-----------|----------|
| TypeScript types — all Rust models mirrored exactly | `src/types/index.ts` |
| IPC wrapper — 28 typed commands + event listener helper | `src/lib/ipc.ts` |
| Zustand library store — optimistic updates with rollback, search + filter | `src/stores/library.ts` |
| Dashboard — stats cards, 90-day activity chart, recently played | `src/pages/Dashboard.tsx` |
| Library — game grid, tab filters (All/Favorites/Playing/Backlog/Completed) | `src/pages/Library.tsx` |
| Game details — state mutations, playtime/rating display, sub-page links | `src/pages/GameDetails.tsx` |
| Achievements — unlock toggle, create form, delete, unlock % | `src/pages/Achievements.tsx` |
| Save manager — profile creation, backup, list, restore | `src/pages/SaveManager.tsx` |
| Analytics — 365-day SVG heatmap with color intensity | `src/pages/Analytics.tsx` |
| Timeline — session history list (200 sessions max) | `src/pages/Timeline.tsx` |
| Settings — scan path add/remove, folder picker, preferences, app info | `src/pages/Settings.tsx` |
| Sidebar, TopBar, GameCard (cover/initials fallback, badges) | `src/components/` |
| Toast notification system — auto-dismiss, stacks up to 5 | `src/components/ToastContainer.tsx` |
| Play button — disabled when no launchable installation | `src/pages/GameDetails.tsx` |

---

## Phase 1 — MVP Completion ✅ COMPLETE (2026-05-29)

**Goal:** Close the remaining gaps that block a usable daily-driver experience. Every feature listed here has partial infrastructure already in place; this phase wires it to the user.

### Features

| Feature | What's needed | Infrastructure already present |
|---------|---------------|-------------------------------|
| Game launch ("Play" button) | `GameDetails.tsx` Play button → resolve `game_installations.executable` → Tauri `shell` plugin launch | `tauri-plugin-shell` in `Cargo.toml`; `executable` column in `game_installations`; passive playtime watcher already detects running processes |
| Toast / in-app notifications | Lightweight toast component subscribing to `onEvent()` in `App.tsx` | Event bus fires `AchievementUnlocked`, `BackupCreated`, `ScanCompleted`, `PlaytimeUpdated`, etc.; `onEvent()` already wired in `App.tsx` |
| Vdf deprecation fix | Replace `Vdf::parse()` with `Vdf::from()` at lines 107 and 132 | `steam.rs` already working; this is a one-line fix per call site |

### Dependencies

- `tauri-plugin-shell` (already in `Cargo.toml` and `capabilities/default.json`)
- No new Rust crates required
- No schema changes

### Estimated Effort

| Task | Effort |
|------|--------|
| Game launch button + command handler | ~100 lines Rust + ~60 lines TypeScript |
| Toast component + event subscription | ~120 lines TypeScript |
| `Vdf::parse()` deprecation fix | ~5 lines Rust |
| **Total** | **~285 lines** |

### Success Criteria — ALL MET ✅

- ✅ Clicking "Play" on a game with a known executable launches the process.
- ✅ Passive playtime session is created automatically when the process is detected.
- ✅ A toast appears when a backup is created, a scan completes, or an achievement is unlocked.
- ✅ `cargo clippy` produces 0 warnings (Vdf deprecations + unit-struct lints fixed).

### Validation Commands — Results

```powershell
# Zero deprecation warnings ✅
cargo clippy  # → Finished with 0 warnings

# TypeScript clean ✅
npm run typecheck  # → no output (pass)
```

---

## Phase 2 — Mods Management & Additional Scanners ← **CURRENT PHASE**

**Goal:** Expand source coverage beyond Steam/Manual and deliver the mods workflow that is already stubbed in the UI and schema.

### Features

| Feature | What's needed |
|---------|---------------|
| Mods backend — CRUD commands | Tauri commands: add mod, toggle enabled, set load_order, delete; maps to `mods` table |
| Mods page — list UI | Replace placeholder text with mod list, enable/disable toggle, load-order drag or up/down arrows |
| Epic Games Store scanner | Parse `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item` JSON; extract `InstallLocation`, `DisplayName`, `CatalogItemId` |
| GOG Galaxy scanner | Parse `C:\ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db` (SQLite) or manifest JSON under `%PROGRAMDATA%\GOG.com` |
| Bundle size reduction | Vite `manualChunks` splitting Recharts and React Router into separate chunks |

### Dependencies

- `serde_json` already present (Epic JSON parsing)
- `sqlx` already present (GOG SQLite parsing)
- No new Rust crates required for Epic; GOG may require reading an SQLite file at an arbitrary path (already supported by `sqlx` + `tauri-plugin-fs`)

### Estimated Effort

| Task | Effort |
|------|--------|
| Mods backend commands | ~200 lines Rust |
| Mods page UI | ~150 lines TypeScript |
| Epic Games scanner | ~120 lines Rust |
| GOG scanner | ~150 lines Rust |
| Bundle split (Vite config) | ~15 lines config |
| **Total** | **~635 lines** |

### Success Criteria

- Mods page shows a real list; mods can be toggled enabled/disabled.
- Scanning with Epic Games installed discovers titles and populates the library.
- Scanning with GOG Galaxy installed discovers titles.
- JS bundle main chunk is below the Vite 500 kB soft limit.
- `cargo check` passes with 0 errors; `npm run build` passes with 0 warnings.

### Validation Commands

```powershell
# Cargo clean check
cargo check

# Bundle chunk sizes
npm run build 2>&1 | Select-String "kB"

# Epic scan (requires Epic installed)
npm run tauri:dev
# Trigger scan from Settings page, verify Epic games appear in Library
```

---

## Phase 3 — Live Metadata & Media

**Goal:** Enrich the library with cover art, release years, and developer info from a live metadata provider, and surface the `media` table for screenshots.

### Features

| Feature | What's needed |
|---------|---------------|
| Live metadata provider | Implement `MetadataProvider` trait for IGDB or RAWG; fetch cover art, release year, developer, publisher, genres; store result in `games.metadata_json` and `games.cover_path` |
| API key setting | Add metadata provider key field to Settings page; persist in `settings` table under `metadata_api_key` |
| Genre assignment | After metadata fetch, populate `genres` + `game_genres` tables; expose genre filter tab in Library |
| Media / screenshots | Tauri commands for `media` table CRUD; UI on GameDetails to browse screenshots and set cover/hero images |
| Cover art display | Replace initials fallback in `GameCard` with fetched cover image when `cover_path` is populated |

### Dependencies

- `reqwest` already in `Cargo.toml` (feature-gated behind `http`)
- IGDB requires a Twitch Client ID + Secret (OAuth 2 machine-to-machine); RAWG requires a free API key
- No schema changes required (columns and tables already present)

### Estimated Effort

| Task | Effort |
|------|--------|
| IGDB or RAWG provider implementation | ~250 lines Rust |
| API key settings UI | ~60 lines TypeScript |
| Genre population + Library filter | ~80 lines Rust + ~50 lines TypeScript |
| Media CRUD commands | ~150 lines Rust |
| Media gallery UI on GameDetails | ~180 lines TypeScript |
| **Total** | **~770 lines** |

### Success Criteria

- Running "Fetch metadata" on a game with an API key configured populates cover art, release year, and genres.
- `GameCard` shows cover images instead of initials for enriched games.
- Genre filter tab in Library filters correctly.
- Screenshots can be added, viewed, and deleted from GameDetails.

### Validation Commands

```powershell
# Cargo check with reqwest feature active
cargo check --features http

npm run typecheck

# Manual: configure API key in Settings, trigger metadata fetch on one game,
# verify cover image appears in Library grid
```

---

## Phase 4 — Multi-User Profiles & Achievement Templates

**Goal:** Activate the multi-user profile system and the achievement template/community sharing workflow, both of which have complete schema support but no backend or UI.

### Features

| Feature | What's needed |
|---------|---------------|
| Profile CRUD backend | Tauri commands: create profile, list profiles, switch active profile, delete profile; scope library queries by `profile_id` |
| Profile switcher UI | Profile selector in TopBar or Settings; persists active profile across restarts |
| Achievement templates | Tauri commands for `achievement_templates` CRUD: import template JSON, apply template to game (creates achievements from template rows) |
| Achievement template UI | Import button on Achievements page; template browser list |
| Steam achievement sync | On scan or on-demand, fetch Steam achievement data via Steam Web API; upsert into `achievements` using `template_id` for deduplication |

### Dependencies

- Steam Web API requires a Steam API key (free, user-supplied) — add to Settings
- `reqwest` already present
- No schema changes

### Estimated Effort

| Task | Effort |
|------|--------|
| Profile CRUD backend + query scoping | ~200 lines Rust |
| Profile switcher UI | ~80 lines TypeScript |
| Achievement template backend | ~150 lines Rust |
| Achievement template UI | ~120 lines TypeScript |
| Steam achievement sync | ~180 lines Rust |
| **Total** | **~730 lines** |

### Success Criteria

- Two profiles can be created; switching profiles shows a different (isolated) library.
- A community achievement template JSON file can be imported and applied to a matching game.
- Steam achievements for a game are fetched and appear pre-populated in the Achievements page.

### Validation Commands

```powershell
cargo check
npm run typecheck

# Manual: create second profile, verify Library is empty for new profile,
# switch back and verify original library is intact
```

---

## Phase 5 — Production Readiness & Release

**Goal:** Validate the full packaging pipeline, replace placeholder assets, harden edge cases, and produce a signed distributable.

### Features

| Feature | What's needed |
|---------|---------------|
| Real application icons | Replace placeholder PNGs in `src-tauri/icons/` with final artwork at all required resolutions (32×32, 128×128, 128×128@2×, .ico, .icns) |
| `tauri build` validation | Run full packaging build; verify installer is produced without errors; smoke-test the installed binary |
| Xbox / Microsoft Store scanner | Parse `%LOCALAPPDATA%\Packages\` directory for Xbox Game Pass titles |
| Scan audit log UI | Surface `scan_runs` table in Settings or a dedicated scan history view |
| Error boundary UI | React error boundary wrapping each page; surface Tauri `AppError` variants as user-readable messages instead of console logs |
| Offline mode enforcement | Respect `settings.offline_mode = true`; disable all network calls (metadata, Steam achievement sync) |
| Auto-backup scheduler | Optional scheduled save backup (e.g., on game close detected via playtime tracker); configurable interval in Settings |

### Dependencies

- Production icon artwork (external asset, user-supplied)
- `tauri build` requires platform signing certificates for distributable builds
- Xbox scanner requires no new crates; JSON parsing of `appxmanifest.xml` may use `quick-xml` (new dependency) or regex

### Estimated Effort

| Task | Effort |
|------|--------|
| Icon replacement | Asset work only |
| `tauri build` validation + fix any issues | ~2–4 hours investigation |
| Xbox scanner | ~150 lines Rust |
| Scan audit log UI | ~80 lines TypeScript |
| Error boundary + error message mapping | ~100 lines TypeScript |
| Offline mode enforcement | ~40 lines Rust + ~20 lines TypeScript |
| Auto-backup scheduler | ~120 lines Rust + ~60 lines TypeScript |
| **Total** | **~570 lines + asset work** |

### Success Criteria

- `npm run tauri:build` completes without errors and produces an installer.
- Installed application launches, scans, and backs up saves on a clean machine.
- All placeholder icons replaced with final artwork.
- `cargo check` and `npm run build` produce 0 warnings.
- The bundle passes Windows Defender SmartScreen (signed certificate or test bypass documented).

### Validation Commands

```powershell
# Full release build
npm run tauri:build

# Verify installer artifact exists
Get-ChildItem "src-tauri\target\release\bundle" -Recurse | Select-Object Name, Length

# Cargo release profile check
cargo check --release

# Zero warnings
npm run build 2>&1 | Select-String "warning|warn"
cargo check 2>&1 | Select-String "warning|warn"
```

---

## Future Enhancements

Items below are out of scope for the current roadmap but are architecturally supported by the existing schema and event system.

| Enhancement | Notes |
|-------------|-------|
| Cloud sync for saves | Upload `.gvbk` archives to a user-configured storage backend (S3-compatible, Google Drive); `ring` crate already present for optional encryption |
| Emulator ROM tracking | `sources` table already seeds `emulator` source code; scanner would index ROM directories and parse `.cue`/`.m3u` playlists |
| Ubisoft Connect scanner | Parse `%LOCALAPPDATA%\Ubisoft Game Launcher\games\` manifest files |
| Battle.net scanner | Parse `%PROGRAMDATA%\Battle.net\Agent\product.db` (protobuf) |
| Playnite / Steam library import | Bulk import from Playnite database or Steam `sharedconfig.vdf` for completion states and ratings |
| Mobile companion (read-only) | Expose a local REST API (Tauri `http` plugin) for a mobile app to read library stats and playtime |
| Custom themes | Extend `src/styles/theme.css` CSS variables; add theme selector to Settings; persist in `settings` table |
| Keyboard shortcuts | Global hotkey for scan, backup now, quick search; Tauri global shortcut plugin |
| Duplicate detection improvements | Fuzzy title matching (`strsim` crate) to catch near-duplicates across scanner sources |
| Achievement points leaderboard | Profile-level aggregate of `achievements.points` for unlocked achievements; visible on Dashboard |
