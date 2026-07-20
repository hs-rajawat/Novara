//! Source-behavior semantics — the single place that encodes what a source
//! *code* means for launch and library behavior, so per-launcher knowledge
//! isn't scattered as hard-coded `"steam"` / `"epic"` checks across the
//! backend and frontend. The frontend mirror lives in `src/lib/sources.ts`;
//! keep the two in sync.

/// The deep-link a launcher-managed source launches through. One arm per
/// launcher — this is the single place to extend for GOG / Xbox / etc.
/// Returns `None` for sources NOVARA launches by spawning an executable
/// directly (e.g. `manual`).
pub fn launch_uri(source_code: &str, app_id: &str) -> Option<String> {
    match source_code {
        "steam" => Some(format!("steam://run/{app_id}")),
        "epic" => Some(format!(
            "com.epicgames.launcher://apps/{app_id}?action=launch&silent=true"
        )),
        _ => None,
    }
}

/// Whether a source's installs are owned by an external launcher: launched
/// by URI, and (when missing) treated as launcher-uninstalled rather than
/// user-lost. Definitionally a source that has a launch URI, so there is no
/// second list to keep in sync with `launch_uri`.
pub fn is_launcher_managed(source_code: &str) -> bool {
    launch_uri(source_code, "").is_some()
}
