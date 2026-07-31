//! Directory names detection must never offer as a save location.
//!
//! From `GAME_SAVE_DETECTION.md` §7.3, which says this is "maintained as data, not
//! code — it will grow, and it is the difference between the Write Witness being
//! useful and being noise". Kept as sorted `&str` tables for that reason: adding a
//! name is editing a list, not editing logic.
//!
//! Two separate lists, because they answer different questions and conflating them
//! would make both harder to reason about:
//!
//! * [`is_engine_noise`] — directories a *game* creates that are not saves. Caches,
//!   crash dumps, shader blobs. These generate false write and content evidence.
//! * [`is_user_content`] — directories a *person* creates that are not saves. Photo
//!   folders, downloads, cloud-sync roots.
//!
//! Both are matched on the folded name (case- and punctuation-insensitive), so
//! `Crash Dumps`, `CrashDumps` and `crashdumps` are one entry rather than three.

use super::kb::normalise_title;

/// Directories games create that hold generated data rather than saves.
///
/// §7.3's list, folded. Kept sorted for review, not for lookup — at this size a
/// linear scan is faster than anything cleverer.
const ENGINE_NOISE: &[&str] = &[
    "backup",
    "backups",
    "cache",
    "caches",
    "crashdumps",
    "crashes",
    "crashpad",
    "crashreportclient",
    "dxcache",
    "git",
    "gpucache",
    "log",
    "logs",
    "mediacache",
    "nodemodules",
    "shadercache",
    "shaders",
    "temp",
    "tmp",
    "webcache",
];

/// Directories a person creates, which are never a game's save folder.
///
/// **This is a precision trade, made deliberately.** A game genuinely called
/// `Photos` would no longer be detected by name — a false negative. That is the
/// cheaper error: `Documents/Photos` is overwhelmingly a photo folder, and offering
/// it as a save location invites a user to bind it and later restore over their
/// own files. `GAME_SAVE_DETECTION.md` §6 ranks name similarity as the weakest
/// signal there is, and this list is where that ranking is made concrete.
///
/// It is *not* a substitute for content verification. A folder called `Snapshots`
/// full of screenshots is still a false positive and still needs the verifier;
/// this list only removes the collisions that are predictable by name alone.
const USER_CONTENT: &[&str] = &[
    "contacts",
    "customofficetemplates",
    "desktop",
    "documents",
    "downloads",
    "dropbox",
    "favorites",
    "faxes",
    "googledrive",
    "links",
    "music",
    "mymusic",
    "mypictures",
    "myvideos",
    "onedrive",
    "outlookfiles",
    "photos",
    "pictures",
    "scanneddocuments",
    "searches",
    "videos",
    "zoom",
];

/// True when a directory holds generated game data rather than saves.
pub fn is_engine_noise(name: &str) -> bool {
    let folded = normalise_title(name);
    ENGINE_NOISE.contains(&folded.as_str())
}

/// True when a directory is user content rather than a game's saves.
pub fn is_user_content(name: &str) -> bool {
    let folded = normalise_title(name);
    USER_CONTENT.contains(&folded.as_str())
}

/// True when a directory name must never be offered as a save location.
pub fn is_ignored(name: &str) -> bool {
    is_engine_noise(name) || is_user_content(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_list_is_covered() {
        // Every name written out in GAME_SAVE_DETECTION.md 7.3, in its original
        // spelling. The folding is what makes these all resolve.
        for name in [
            "Crashes",
            "crashpad",
            "CrashDumps",
            "Logs",
            "Log",
            "Cache",
            "Caches",
            "GPUCache",
            "Shaders",
            "ShaderCache",
            "Temp",
            "tmp",
            "CrashReportClient",
            "Backup",
            ".git",
            "node_modules",
            "DXCache",
            "MediaCache",
            "webcache",
        ] {
            assert!(is_engine_noise(name), "7.3 lists `{name}` but it is not ignored");
        }
    }

    #[test]
    fn folding_makes_spelling_variants_one_entry() {
        for variant in ["CrashDumps", "crash dumps", "Crash_Dumps", "CRASHDUMPS"] {
            assert!(is_engine_noise(variant), "`{variant}` should fold to a listed name");
        }
    }

    #[test]
    fn user_content_folders_are_ignored() {
        for name in ["Photos", "Pictures", "My Music", "Downloads", "OneDrive"] {
            assert!(is_user_content(name), "`{name}` should be treated as user content");
        }
    }

    /// The list must not swallow names that really are save folders. This is the
    /// guard against the list growing until detection stops working.
    #[test]
    fn real_save_folder_names_are_not_ignored() {
        for name in [
            "Saves",
            "SaveGames",
            "Save",
            "SaveData",
            "Profiles",
            "Player",
            "My Games",
            "Skyrim Special Edition",
            "EldenRing",
            "Hollow Knight",
            "Terraria",
            "gamesaves",
            // Near-misses that must survive: a game called `Cached` or a studio
            // called `Logsdon` are not the listed names.
            "Cached",
            "Logsdon",
            "Templar",
        ] {
            assert!(!is_ignored(name), "`{name}` must not be ignored");
        }
    }

    #[test]
    fn matching_is_whole_name_not_substring() {
        // `Temp` is listed; `Tempest` is a game.
        assert!(is_ignored("Temp"));
        assert!(!is_ignored("Tempest"));
        // `Log` is listed; `Logic` is not.
        assert!(is_ignored("Log"));
        assert!(!is_ignored("Logic"));
    }
}
