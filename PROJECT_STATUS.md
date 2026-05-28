# Project Status

**Date:** 2026-05-28

## Build Status

| Step | Status |
|------|--------|
| `cargo check` | PASS (2 deprecation warnings, 0 errors) |
| `npm run build` | PASS (1 chunk-size warning, 0 errors) |

---

## Fixed Issues

### 1. Missing icons directory (`icons/icon.ico` not found)
- **Root cause:** `src-tauri/icons/` directory did not exist; `tauri-build` requires it for Windows resource file generation.
- **Fix:** Created `src-tauri/icons/` with placeholder icons:
  - `icon.ico` (multi-size: 16×16, 32×32, 48×48)
  - `32x32.png`
  - `128x128.png`
  - `128x128@2x.png` (256×256)
  - `icon.icns` (macOS placeholder)

### 2. `tauri::generate_context!()` panic — `frontendDist` path missing
- **Root cause:** `tauri.conf.json` references `"../dist"` but the `dist/` directory did not exist; the proc macro validates this at compile time.
- **Fix:** Created `dist/index.html` (minimal placeholder). This directory is overwritten by `npm run build`, so it is always present going forward.

### 3. `Default` derive on `ActiveSession` (`src/playtime/mod.rs:32`)
- **Root cause:** `std::time::Instant` does not implement `Default`, so `#[derive(Default)]` failed.
- **Fix:** Removed `#[derive(Default)]` from `ActiveSession`; it was unused — all instantiation sites supply explicit field values.

### 4. Extra argument to `sysinfo::System::refresh_processes` (`src/playtime/mod.rs:123`)
- **Root cause:** `sysinfo` 0.31 API changed — `refresh_processes` takes only one argument (`ProcessesToUpdate`); the second `bool` argument was removed.
- **Fix:** Removed the extra `true` argument.

### 5. TypeScript errors in `vite.config.ts`
- **Root cause 1:** `node:path` import requires `@types/node`, which was not installed.
- **Root cause 2:** `__dirname` is not defined in ESM modules.
- **Root cause 3:** `vite.config.ts` was included in the browser-targeted `tsconfig.json`, which uses `"moduleResolution": "bundler"` without Node.js types.
- **Fix:**
  - Installed `@types/node` as a dev dependency.
  - Replaced `path.resolve(__dirname, "src")` with `fileURLToPath(new URL("./src", import.meta.url))`.
  - Replaced `import path from "node:path"` with `import { fileURLToPath, URL } from "node:url"`.
  - Removed `vite.config.ts` from `tsconfig.json`'s `include` array.
  - Created `tsconfig.node.json` covering `vite.config.ts` with proper Node.js type settings.

---

## Remaining Issues / Warnings

### Deprecation warnings (non-blocking)
- `keyvalues_parser::Vdf::parse` is deprecated in `src/scanner/steam.rs` (lines 107, 132).
- Recommended migration: `parse().map(Vdf::from)`. Not blocking compilation.

### Chunk size warning (non-blocking)
- The main JS bundle is ~577 kB (169 kB gzipped), exceeding Vite's 500 kB soft warning.
- No action required for build stabilization; code-splitting can be addressed as a future optimization.

### Placeholder icons
- The icons in `src-tauri/icons/` are solid-blue placeholders (74, 144, 226 — a neutral blue).
- They should be replaced with real artwork before release.

### `tauri:build` not yet run
- `npm run build` (frontend only) passes. Full Tauri packaging (`npm run tauri:build`) has not been run and may surface additional bundler-level issues unrelated to compilation correctness.
