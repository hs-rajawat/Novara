// Source-behavior semantics for the frontend — the single place that
// encodes what a source *code* means for library display. Mirrors the Rust
// counterpart in src-tauri/src/sources.rs; keep the two in sync.

// Sources whose installs are owned by an external launcher (Steam, Epic, …).
// A "missing" launcher-managed game means the launcher uninstalled it, so it
// is hidden from the active grid rather than shown with a Locate/Remove flow.
export const LAUNCHER_MANAGED_SOURCES = new Set<string>(["steam", "epic"]);

export function isLauncherManaged(code: string | null | undefined): boolean {
  return !!code && LAUNCHER_MANAGED_SOURCES.has(code);
}
