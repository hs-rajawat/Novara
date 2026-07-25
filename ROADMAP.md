# NOVARA — Development Roadmap

**App:** Local-first game library & progress tracker (Tauri 2 + React 18 + SQLite)  
**Version:** 0.1.0  
**Current Phase:** Phase 2 (Mods & Additional Scanners)  
**Overall Completion:** ~88%

---

## Completed Work

All items below are fully implemented, tested, and passing `cargo check` + `cargo clippy` + `npm run build` as of 2026-05-29.

### Backend (Rust / Tauri)

| Component | Location |
|-----------|----------|
| SQLite database + WAL mode + FK enforcement + connection pool | `src-tauri/src/db/mod.rs` |
| Schema migrations (5 migrations) | `src-tauri/migrations/` |
| Library Integrity System — source-aware install-status resolution (Steam manifest `StateFlags` + generic exe/dir check), reused `SteamContext` per sweep | `src-tauri/src/integrity/` |
| Library Integrity System — startup + periodic (5 min) background verifier, post-scan reconciliation, launch-time recheck & missing-state launch guard | `src-tauri/src/integrity/service.rs`, `src-tauri/src/commands/games.rs` |
| Steam library scanner — multi-library VDF + ACF parsing | `src-tauri/src/scanner/steam.rs` |
| Manual folder scanner — depth-3 walk, exe ranking, size | `src-tauri/src/scanner/manual.rs` |
| Scanner orchestrator — parallel execution, upsert dedup, scan audit log | `src-tauri/src/scanner/mod.rs` |
| Save backup — custom `.gvbk` deterministic archive | `src-tauri/src/save_mgr/mod.rs` |
| Save restore — atomic pre-restore backup → rename → restore | `src-tauri/src/save_mgr/mod.rs` |
| Save profiles CRUD — create (with `is_manual_override`), delete, list | `src-tauri/src/db/saves.rs` |
| Save path detection — heuristic search of 6 OS locations | `src-tauri/src/save_detect.rs` |
| Playtime tracking — explicit (`start`/`stop`) | `src-tauri/src/playtime/mod.rs` |
| Playtime tracking — passive background watcher (sysinfo, 5 s poll) | `src-tauri/src/playtime/mod.rs` |
| Idle detection — frontend-reported idle seconds tracked per session | `src-tauri/src/playtime/mod.rs` |
| Achievement CRUD — create, toggle unlock, delete, auto-completion % | `src-tauri/src/db/achievements.rs` |
| Game library CRUD — upsert, list, favorite, completion state, notes, cover/hero | `src-tauri/src/db/games.rs` |
| Cover art management — `set_cover_path`/`set_hero_path`; copies to `<app_data>/artwork/` | `src-tauri/src/commands/games.rs` |
| Game launch — spawns exe directly; opens `steam://run/<id>` URI for Steam games | `src-tauri/src/commands/games.rs` |
| Executable override — user-chosen exe survives rescans; import-by-exe flow | `src-tauri/src/commands/games.rs` |
| Duplicate game merge — reparents sessions/saves/achievements to survivor | `src-tauri/src/db/games.rs` |
| Event bus — `tokio::broadcast`, 9 event variants, forwarded to frontend | `src-tauri/src/events.rs` |
| Settings store — JSON key/value, upsert-safe | `src-tauri/src/db/settings.rs` |
| 35+ Tauri IPC command handlers across all subsystems | `src-tauri/src/commands/` |
| Analytics — dashboard stats (total, completed, playtime, favorites, genres) | `src-tauri/src/commands/analytics.rs` |
| Analytics — daily activity heatmap aggregation (configurable window) | `src-tauri/src/commands/analytics.rs` |
| `MetadataProvider` async trait + offline no-op fallback | `src-tauri/src/metadata/` |
| `AppState` — Arc'd DB pool, event bus, scanner, saves, playtime tracker | `src-tauri/src/state.rs` |

### Frontend (React / TypeScript)

| Component | Location |
|-----------|----------|
| TypeScript types — all Rust models mirrored exactly | `src/types/index.ts` |
| IPC wrapper — typed commands for all subsystems + event listener helper | `src/lib/ipc.ts` |
| Image utility — `toImgSrc()` using `convertFileSrc` for local paths | `src/lib/image.ts` |
| Zustand library store — optimistic updates, search, filter, sort (5 modes, persisted) | `src/stores/library.ts` |
| Dashboard — stats cards, 90-day chart, recently played with cover art, top genres; event refresh | `src/pages/Dashboard.tsx` |
| Library — game grid, tab filters, 5 sort options (title/playtime/last played/added) | `src/pages/Library.tsx` |
| Game details — hero banner, Artwork section (cover+hero pickers), state, installations, notes, Missing badge, Locate Executable, Remove/Restore from Library | `src/pages/GameDetails.tsx` |
| Achievements — unlock toggle, create form, delete, unlock % | `src/pages/Achievements.tsx` |
| Save manager — detection panel, Auto/Manual badges, delete, backup/restore | `src/pages/SaveManager.tsx` |
| Analytics — 365-day SVG heatmap with color intensity | `src/pages/Analytics.tsx` |
| Timeline — session history list (200 sessions max) | `src/pages/Timeline.tsx` |
| Settings — scan path add/remove, import executable, folder picker, preferences | `src/pages/Settings.tsx` |
| Sidebar, TopBar (search wired to library store), GameCard (cover via `convertFileSrc`) | `src/components/` |
| Toast notification system — auto-dismiss, stacks up to 5 | `src/components/ToastContainer.tsx` |

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

## Library Integrity System — Missing Game Detection ✅ COMPLETE (2026-07-15)

**Goal:** Keep the library honest about what is actually installed — detect when a game has been uninstalled, moved, or deleted (via Steam or manually), surface it clearly, and offer non-destructive recovery, without ever blocking startup or the UI.

### Features — ALL DELIVERED ✅

- ✅ Steam uninstall detection via manifest-based verification (`appmanifest_<id>.acf` presence across all libraries).
- ✅ Conservative Steam `StateFlags` handling — absent manifest is the only unconditional "missing"; only bit 1 (Uninstalling) is treated as not-installed; transitional states stay Installed.
- ✅ Efficient `SteamContext` reuse — Steam library discovery happens once per sweep, not per game (cheap at 500–1000+ libraries).
- ✅ Manual missing-executable detection via the generic install-dir/executable filesystem check.
- ✅ Shared, source-aware installation status resolution (`resolve_installation_status`) — the single point Steam, manual, and future sources feed into.
- ✅ Startup integrity verification (one-shot sweep, catches uninstalls that happened while NOVARA was closed).
- ✅ Periodic background integrity verification (~5 min recurring sweep).
- ✅ Post-scan best-effort reconciliation.
- ✅ Launch-time installation recheck + missing-state launch guard (blocks Play on a stale row deterministically).
- ✅ Missing status surfaced to the frontend (`primary_install_status`, `Installation.status`).
- ✅ Missing badge and status-aware Play behavior in the Library grid and Game Details.
- ✅ "Locate Executable" recovery flow — repoint a Missing installation and restore it to Installed.
- ✅ Non-destructive "Remove from Library" / "Restore to Library" reusing the existing `is_hidden` state — no row deleted, all history preserved.
- ✅ "Show hidden" toggle in the Library view.
- ✅ Historical data preservation (playtime, sessions, achievements, saves, mods, artwork) across every state change.
- ✅ `GameUpdated` event propagation with live UI refresh.
- ✅ Tauri runtime panic fix — background sweeps spawned via `tauri::async_runtime::spawn` (the sync `setup` closure has no ambient Tokio runtime, matching the `start_event_forwarder` pattern).

### Schema Changes

- `0004_install_status.sql` — `game_installations.status TEXT NOT NULL DEFAULT 'installed'`.
- `0005_install_verified_at.sql` — `game_installations.last_verified_at TEXT` (nullable).

### Validation — ALL MET ✅

- ✅ `cargo check` — passed.
- ✅ `cargo clippy -- -D warnings` — passed, zero warnings.
- ✅ `npm run build` — passed.
- ✅ `npm run tauri:dev` — app launched and ran without the prior runtime panic; startup sweep confirmed in logs.
- ✅ Manual runtime testing — feature confirmed working.

---

## Metadata & Artwork ✅ COMPLETE (2026-07-25, `26795b9`)

**Automatic metadata and artwork fetching, privacy-gated and provider-agnostic.**

- ✅ Provider abstraction with identity resolution (source app-id first, title fallback)
- ✅ `steam_local` provider — copies from Steam's own `librarycache`, zero network
- ✅ `steam_cdn` and `epic_catalog` providers
- ✅ `artwork_assets` provenance/refresh ledger (migration `0006`) with a
      SQL-level ownership guard: no provider may overwrite another
      provider's asset or a user-locked one
- ✅ `logo_path` as a fourth artwork kind alongside cover/hero/icon
- ✅ Local artwork store under `<app_data>/artwork/<game_id>/`, reusing the
      convention established by manual artwork picking
- ✅ Opt-in `metadata_enabled` setting, off by default, checked alongside
      the global `offline_mode` kill-switch
- ✅ Per-provider circuit breakers in both fill services

**Known issues at ship, tracked in the remediation plan:** the Tauri asset
protocol is not enabled so fetched artwork does not render (Batch 1);
`ArtworkKind::Icon` has no provider, leaving the fill loop
non-terminating and re-issuing CDN requests every scan (Batch 5); the
post-scan fill runs inline and blocks the scan IPC (Batch 6).

---

## Pre-Release Remediation ← **CURRENT PHASE**

Audit-driven hardening before first release. Sequenced in 13 batches,
dependency-ordered rather than severity-ordered. Batch 0 (repository
truth + test harness) is complete; the release gate is Batch 10
(`tauri build`, which has never been run, plus real application icons).

Phases 2 and 3 below resume after that gate.

---

## Phase 2 — Mods Management & Additional Scanners

**Goal:** Expand source coverage beyond Steam/Manual and deliver the mods workflow that is already stubbed in the UI and schema.

### Features

| Feature | What's needed |
|---------|---------------|
| Mods backend — CRUD commands | Tauri commands: add mod, toggle enabled, set load_order, delete; maps to `mods` table |
| Mods page — list UI | Replace placeholder text with mod list, enable/disable toggle, load-order drag or up/down arrows |
| GOG Galaxy scanner | Parse `C:\ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db` (SQLite) or manifest JSON under `%PROGRAMDATA%\GOG.com` |
| Bundle size reduction | Vite `manualChunks` splitting Recharts and React Router into separate chunks |

> The **Epic Games Store scanner shipped in `2586c00`** (with launcher
> reliability follow-ups in `c021c0b` and `3d0c0fd`) and is no longer part of
> this phase. Use `scanner/epic.rs` as the reference implementation for GOG:
> it is a pure filesystem leaf with no `Db` or bus dependency, which is what
> lets `integrity` depend on it without a cycle.

### Dependencies

- `serde_json` already present
- `sqlx` already present (GOG SQLite parsing)
- No new Rust crates required; GOG needs to read an SQLite file at an
  arbitrary path, already supported by `sqlx` + `tauri-plugin-fs`

### Estimated Effort

| Task | Effort |
|------|--------|
| Mods backend commands | ~200 lines Rust |
| Mods page UI | ~150 lines TypeScript |
| GOG scanner | ~150 lines Rust |
| Bundle split (Vite config) | ~15 lines config |
| **Total** | **~515 lines** |

### Success Criteria

- Mods page shows a real list; mods can be toggled enabled/disabled.
- Scanning with GOG Galaxy installed discovers titles.
- JS bundle main chunk is below the Vite 500 kB soft limit.
- `cargo check` passes with 0 errors; `npm run build` passes with 0 warnings.

### Validation Commands

```powershell
# Cargo clean check
cargo check

# Bundle chunk sizes
npm run build 2>&1 | Select-String "kB"

# GOG scan (requires GOG Galaxy installed)
npm run tauri:dev
# Trigger scan from Settings page, verify GOG games appear in Library
```

---

## Phase 3 — Additional Providers & Media

**Goal:** Extend the metadata/artwork subsystem that shipped in
`26795b9` with third-party providers, surface genres as a filter, and
expose the `media` table for screenshots.

> **What already shipped** (`26795b9`, migration `0006`): the provider
> abstraction, identity resolution, per-asset provenance ledger
> (`artwork_assets`), local artwork store, and three working providers —
> `steam_local` (zero-network, copies from Steam's own `librarycache`),
> `steam_cdn`, and `epic_catalog`. Text and artwork fill run as separate
> services with per-provider circuit breakers.
>
> Plan against that abstraction, **not** the superseded design this section
> used to describe. Two specifics that changed:
> - There is no `metadata_api_key` setting. The gate is `metadata_enabled`
>   (seeded `false` in `0006`), and it is checked **in addition to** the
>   global `offline_mode` kill-switch from `0001`. A provider fetches only
>   when both allow it.
> - Artwork is not written straight to `games.cover_path` by providers.
>   They go through `Db::upsert_artwork_ready`, whose SQL-level ownership
>   guard prevents one provider clobbering another's asset or any
>   `user_locked` asset. New providers must use it rather than writing the
>   `games.*_path` columns directly.

### Features

| Feature | What's needed |
|---------|---------------|
| SteamGridDB provider | Highest-value addition: purpose-built for covers/heroes/logos, and fills the gap for non-Steam titles. Implement the shipped artwork provider trait; needs a user-supplied API key |
| IGDB or RAWG provider | Text metadata beyond what the Steam store API returns — release year, developer, publisher, genres for non-Steam titles. IGDB needs a Twitch Client ID + Secret (OAuth 2 M2M); RAWG needs a free key |
| Per-provider API key settings | Settings surface for the above. Store per provider (e.g. `provider_key_steamgriddb`), not one shared `metadata_api_key` |
| Genre assignment UI + filter | `set_game_metadata` already populates `genres`/`game_genres` for Steam titles; needs a manual assignment surface and a Library genre filter |
| Media / screenshots | Tauri commands for `media` table CRUD; gallery UI on GameDetails |

### Dependencies

- `reqwest` already wired and used by the shipped providers
- Batch 5 of the remediation plan should land first: it settles the fill
  loop's termination condition (`skipped` state) and adds the concurrency
  cap, delay and persisted backoff. Adding providers before that
  multiplies the existing repeat-traffic problem across more endpoints.
- A key-bearing provider needs a decision on key storage. Keys in the
  plain `settings` table are readable by anything with filesystem access;
  this is the first feature that would justify the encryption work
  currently claimed but unimplemented in the README.

### Estimated Effort

| Task | Effort |
|------|--------|
| SteamGridDB provider | ~200 lines Rust |
| IGDB or RAWG provider | ~250 lines Rust |
| Per-provider key settings UI | ~90 lines TypeScript |
| Genre assignment UI + Library filter | ~80 lines Rust + ~70 lines TypeScript |
| Media CRUD commands | ~150 lines Rust |
| Media gallery UI on GameDetails | ~180 lines TypeScript |
| **Total** | **~1,020 lines** |

### Success Criteria

- With `metadata_enabled` on and a key configured, a non-Steam title
  receives cover, hero and logo artwork.
- With `metadata_enabled` off **or** `offline_mode` on, no provider issues
  a network request (asserted by test, not by inspection).
- A new provider cannot overwrite artwork a user set manually, or artwork
  another provider already supplied.
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
