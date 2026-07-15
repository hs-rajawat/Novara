# GameVault

Local-first game library & progress tracker for any PC game — Steam, Epic, GOG, Xbox, Ubisoft Connect, Battle.net, emulators, or manually installed copies. **No piracy features, no DRM circumvention, no downloads. GameVault never distributes games — it only tracks ones you already own and have installed.**

> **Privacy first:** zero telemetry, no network calls by default, encrypted-ready local SQLite, offline mode.

---

## Status

This repo is the **foundational MVP**. The bones are production-quality and the data model is complete; some surfaces are wired end-to-end, others are scaffolded behind clean interfaces so they can be filled in incrementally.

### Wired end-to-end (works today)

- Tauri 2 desktop shell (Windows-first; macOS/Linux paths included)
- SQLite + sqlx with full migration (`migrations/0001_init.sql`)
- Event-driven core (tokio broadcast → Tauri event bus → React)
- **Scanner engine** with two scanners:
  - **Steam** — parses `libraryfolders.vdf` + `appmanifest_*.acf`
  - **Manual** — walks user-configured folders, picks best executable
- **Save Manager** — versioned backups (custom `.gvbk` archive), restore with pre-restore safety snapshot
- **Library Integrity** — detects games uninstalled/moved/deleted (Steam manifest verification + manual executable checks) at startup, on a periodic background sweep, after scans, and at launch; shows a Missing badge with non-destructive Locate Executable and Remove/Restore recovery
- **Playtime tracker** — explicit start/stop *and* passive process watcher (`sysinfo`, 5s poll)
- **Achievement system** — custom achievements, toggle unlock, auto-computed completion %
- Dashboard, Library, Game Details, Achievements, Save Manager, Analytics (heatmap), Timeline, Mods (stub page), Settings — all routed
- Steam-Deck / Discord / Playnite-inspired dark UI

### Scaffolded (clear extension point, ready to implement)

- Epic / GOG / Xbox / Ubisoft / Battle.net scanners — add a `Scanner` impl under `src-tauri/src/scanner/<source>.rs` and register it in `scanner::mod.rs::ScannerOrchestrator::new`
- IGDB / RAWG metadata providers — implement the `MetadataProvider` trait in `src-tauri/src/metadata/`
- RetroAchievements adapter — drop in as a `Scanner` for ROMs + a `MetadataProvider` for achievement templates
- Mod tracking — schema is in place; needs filesystem watcher + UI list
- Multi-user profiles — table exists; UI surface to switch active profile
- Encrypted DB — `ring` is wired into deps; either swap to `sqlcipher` or encrypt at the file layer
- Playnite/Lutris import — read their JSON/SQLite stores, map to `UpsertGame`
- AI features (recap, summaries, recommendations) — strictly opt-in; recommend a local LLM via a `summary_provider` trait
- Cloud sync — only enabled by user; export `gamevault.db` + media archive to user-chosen destination

### Explicitly out of scope (and will stay that way)

No downloaders, torrenters, key generators, crack distribution, DRM bypass, license circumvention, account hijacking, paid-game unlocking. GameVault tracks games you've already installed legally — that's it.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  React UI (TS)  ──  zustand store  ──  ipc layer            │
└──────────────────────────────┬──────────────────────────────┘
                       Tauri IPC (typed)
┌──────────────────────────────┴──────────────────────────────┐
│  Rust core (tokio)                                          │
│                                                             │
│  commands::*  ──  AppState  ──  EventBus (broadcast)        │
│                                                             │
│  Services (each owns a domain):                             │
│    • db         (sqlx + SQLite, migrations, repos)          │
│    • scanner    (pluggable Scanner trait)                   │
│    • metadata   (pluggable MetadataProvider trait)          │
│    • save_mgr   (custom .gvbk archive, snapshot+restore)    │
│    • playtime   (explicit + sysinfo process watcher)        │
└─────────────────────────────────────────────────────────────┘
```

### Why these choices

- **Tauri 2** over Electron — startup time, RAM footprint (~80 MB vs ~250 MB), tiny binary, native menus.
- **sqlx + SQLite (WAL)** — single-file, fast, transactional, no server. WAL = concurrent reads while a write is in flight.
- **Broadcast bus instead of direct calls** — services don't import each other. Save manager can react to `SessionEnded` without `playtime` knowing it exists.
- **Trait-based scanners/providers** — the `Scanner` trait is the only contract a new source needs to satisfy. Adding Epic = one file.
- **Custom `.gvbk` archive** — zero extra deps, deterministic format, easy to verify byte-for-byte. Swap to `zip` if you want external compatibility.

### Folder structure

```
gamevault/
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs               # entrypoint (release: no console)
│   │   ├── lib.rs                # Tauri builder + command registration
│   │   ├── state.rs              # AppState (DB + bus + services)
│   │   ├── error.rs              # AppError (single error type)
│   │   ├── events.rs             # broadcast bus
│   │   ├── models.rs             # serializable domain models
│   │   ├── db/                   # sqlx pool + per-table repos
│   │   ├── scanner/              # game-source scanners
│   │   ├── metadata/             # metadata providers (offline stub today)
│   │   ├── save_mgr/             # backup engine
│   │   ├── playtime/             # session tracker + process watcher
│   │   └── commands/             # Tauri IPC handlers
│   ├── migrations/0001_init.sql  # full schema
│   ├── capabilities/default.json # Tauri 2 capability set
│   ├── tauri.conf.json
│   └── Cargo.toml
├── src/                          # React frontend
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/               # Sidebar, TopBar, GameCard
│   ├── pages/                    # Dashboard, Library, GameDetails, ...
│   ├── stores/library.ts         # zustand store
│   ├── lib/ipc.ts                # typed Tauri invoke wrapper
│   ├── lib/format.ts             # display helpers
│   ├── types/                    # mirrors src-tauri/src/models.rs
│   └── styles/                   # theme.css + app.css
├── package.json
├── tsconfig.json
├── vite.config.ts
└── index.html
```

### Database schema highlights

See `src-tauri/migrations/0001_init.sql` for the full DDL.

- `games` is the canonical title row. **Duplicate detection** is keyed off `(source, source_app_id)` first, falling back to `install_dir`. Two installs of the same Steam title across two libraries collapse to one `games` row with two `game_installations` rows.
- `play_sessions` records raw events with separate `duration_seconds` and `idle_seconds` so analytics can report active vs total time.
- `achievements` recomputes parent `games.completion_pct` on every toggle, in one transaction.
- `settings` is a JSON-valued key/value store — additions don't require migrations.
- `scan_runs` is an audit log so you can see what each scanner found and when.

---

## Run it

### Prerequisites

- Node 20+
- Rust 1.77+ (stable)
- On Windows: WebView2 (preinstalled on Win11)
- On Linux: `libwebkit2gtk-4.1-dev`, `libssl-dev`, `librsvg2-dev`, `libayatana-appindicator3-dev`

### Install & dev

```bash
npm install
npm run tauri:dev
```

First run will:
1. Compile the Rust crate (~1–2 min cold)
2. Open a window
3. Create `gamevault.db` in your OS app-data dir (shown in Settings → About)
4. Run migrations to schema v1

### Try it

1. **Settings → Scan paths → Add folder…** — point at something like `D:\Games` or your portable installs folder.
2. **Scan now** (top bar). Steam is discovered automatically. Manual folders get walked.
3. **Library** — your games appear. Click one.
4. **Achievements** — add some custom ones, click to toggle. Completion % updates.
5. **Saves** — register a save folder, click *Backup now*, restore later.

To verify the Steam scanner without installing Steam, point `GAMEVAULT_STEAM_DIR` at a test fixture directory containing `steamapps/libraryfolders.vdf`.

---

## Extending

### Add a new source (e.g. Epic)

1. Create `src-tauri/src/scanner/epic.rs`:
   ```rust
   #[derive(Default)] pub struct EpicScanner;
   #[async_trait::async_trait]
   impl crate::scanner::Scanner for EpicScanner {
       fn code(&self) -> &'static str { "epic" }
       async fn scan(&self, _: &[std::path::PathBuf])
         -> crate::error::AppResult<Vec<crate::scanner::DetectedGame>>
       {
           // Epic stores manifests under
           //   %ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item
           // Parse JSON, return DetectedGame { source_code: "epic", .. }
           Ok(vec![])
       }
   }
   ```
2. Register it in `scanner::mod.rs`:
   ```rust
   Box::new(epic::EpicScanner::default()),
   ```
3. Add `pub mod epic;` to `scanner/mod.rs`. Done — the orchestrator picks it up.

### Add a metadata provider

Implement `MetadataProvider` in `src-tauri/src/metadata/`. Provider gets a title, returns a `GameMetadata`. Call it from the scanner orchestrator after upsert (a tiny addition — left out of the MVP to avoid a network dep without user consent).

### Add a plugin

The plugin architecture is just *traits + a registry*. There is no `dlopen`-style dynamic loader in the MVP (security: third-party native code is risky). To add a "plugin" today, drop in a Rust module behind a Cargo feature flag and register at startup.

---

## Performance & footprint

- ~12 MB release binary (Tauri's `Cargo.toml` profile uses `opt-level = "s"`, LTO, strip).
- ~80 MB RAM idle (WebView + Rust). Compare Electron at ~250 MB.
- SQLite in WAL mode handles ~10k games comfortably — schema indexes cover the hot reads.
- Scanner runs are bounded depth (3) on manual roots to keep walks predictable.

## License

MIT (or whatever you choose). The data model and scanners are your own; nothing here ships third-party game content.
