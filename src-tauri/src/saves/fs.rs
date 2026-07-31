//! The filesystem abstraction detection reads through.
//!
//! Two reasons this exists, one practical and one structural — see
//! `docs/architecture/adr/0012-filesystem-behind-a-trait.md`.
//!
//! Practically, detection previously called `dirs::` and `std::fs` directly, so a
//! test would read the developer's own `%APPDATA%`: results differed per machine
//! and CI was meaningless. Nothing in
//! `docs/testing/SAVE_DETECTION_TEST_PLAN.md` can exist without an injectable
//! filesystem.
//!
//! Structurally, this trait is **metadata-only**. There is deliberately no method
//! that reads a file's contents, which makes "detection never reads save files"
//! (ADR-0003) a property of the type system rather than a rule a contributor has to
//! remember. A verifier physically cannot open a save.
//!
//! Scope is detection only. The vault reads and writes archive bytes and continues
//! to use `std::fs` directly, tested against real temporary directories — see
//! ADR-0015 for why virtualising it would prove less, not more.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A named directory a KB path template can anchor on.
///
/// Overlaps [`RootKind`] but is not the same concept, and the difference is worth
/// keeping: a *root* is somewhere detection searches, an *anchor* is somewhere a
/// template can start from. `{USERPROFILE}` and `{PUBLIC}` are legitimate anchors
/// that would be terrible search roots — walking a whole user profile is the
/// disk-crawl this design exists to avoid.
///
/// [`RootKind::anchor`] maps one way only, so the six shared values cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anchor {
    AppDataRoaming,
    AppDataLocalLow,
    AppDataLocal,
    Documents,
    MyGames,
    SavedGames,
    UserProfile,
    Public,
}

impl Anchor {
    /// The template variable that names this anchor, without braces.
    pub fn variable(&self) -> &'static str {
        match self {
            Anchor::AppDataRoaming => "APPDATA",
            Anchor::AppDataLocalLow => "LOCALLOW",
            Anchor::AppDataLocal => "LOCALAPPDATA",
            Anchor::Documents => "DOCUMENTS",
            Anchor::MyGames => "MYGAMES",
            Anchor::SavedGames => "SAVEDGAMES",
            Anchor::UserProfile => "USERPROFILE",
            Anchor::Public => "PUBLIC",
        }
    }

    /// Every anchor, longest variable name first.
    ///
    /// The ordering matters for substitution: `LOCALAPPDATA` must be matched before
    /// `APPDATA`, or the shorter name would rewrite the middle of the longer one.
    pub const ALL_LONGEST_FIRST: [Anchor; 8] = [
        Anchor::AppDataLocalLow,  // LOCALLOW
        Anchor::AppDataLocal,     // LOCALAPPDATA
        Anchor::SavedGames,       // SAVEDGAMES
        Anchor::UserProfile,      // USERPROFILE
        Anchor::AppDataRoaming,   // APPDATA
        Anchor::Documents,        // DOCUMENTS
        Anchor::MyGames,          // MYGAMES
        Anchor::Public,           // PUBLIC
    ];
}

/// A directory NOVARA searches for save folders.
///
/// Named rather than a bare path so the label that reaches the UI is derived from
/// the kind, not re-spelled at each call site. `label()` values cross IPC inside
/// [`crate::saves::locator::DetectedPath::hint`] and are therefore a wire contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    AppDataRoaming,
    AppDataLocalLow,
    AppDataLocal,
    DocumentsMyGames,
    Documents,
    SavedGames,
    /// The game's own installation directory (ADR-0004).
    ///
    /// Unlike every other root this is **per-game**, so it never comes from
    /// [`FileSystem::roots`] — the locator synthesises it from the game context.
    /// It is also the first root that is *not* a template anchor, which is why
    /// [`RootKind::anchor`] is partial.
    InstallDir,
}

impl RootKind {
    /// Human-readable label, as shown in the detection panel.
    ///
    /// These exact strings are part of the IPC payload the frontend renders. Do not
    /// reword them without treating it as a frontend-visible change.
    pub fn label(&self) -> &'static str {
        match self {
            RootKind::AppDataRoaming => "AppData/Roaming",
            RootKind::AppDataLocalLow => "AppData/LocalLow",
            RootKind::AppDataLocal => "AppData/Local",
            RootKind::DocumentsMyGames => "Documents/My Games",
            RootKind::Documents => "Documents",
            RootKind::SavedGames => "Saved Games",
            RootKind::InstallDir => "Install directory",
        }
    }

    /// The template anchor this root corresponds to, if it is one.
    ///
    /// **Partial on purpose.** An anchor is a machine location a KB template can
    /// start from; the install directory is a property of one game, so there is no
    /// `{INSTALL}` anchor for a filesystem to resolve — `template::expand`
    /// substitutes it from the game context instead. Returning `Option` keeps that
    /// distinction in the type rather than forcing an invented variant.
    pub fn anchor(&self) -> Option<Anchor> {
        match self {
            RootKind::AppDataRoaming => Some(Anchor::AppDataRoaming),
            RootKind::AppDataLocalLow => Some(Anchor::AppDataLocalLow),
            RootKind::AppDataLocal => Some(Anchor::AppDataLocal),
            RootKind::DocumentsMyGames => Some(Anchor::MyGames),
            RootKind::Documents => Some(Anchor::Documents),
            RootKind::SavedGames => Some(Anchor::SavedGames),
            RootKind::InstallDir => None,
        }
    }
}

/// Join `relative` onto `base`, refusing anything that could leave `base`.
///
/// The locator builds candidate paths from **game metadata** — titles, developer
/// and publisher names — which is attacker-influenced data in exactly the way a KB
/// template is. `Path::join` is unsafe for this: joining a string that happens to be
/// absolute silently *replaces* the base, so a game titled `C:/Windows` would
/// otherwise turn a search of `Documents` into a probe of the system directory.
///
/// Refuses rather than truncates, because a partially-applied path is a plausible
/// wrong answer and those are worse than no answer.
///
/// This is the same guarantee [`crate::saves::kb::template`] enforces for KB
/// templates, expressed for a different call shape. The two are kept honest by
/// `locator::tests::both_path_guards_refuse_the_same_hostile_input`, which feeds
/// identical hostile input to both and requires neither to escape.
pub fn join_under(base: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = base.to_path_buf();
    let mut pushed = 0usize;

    for segment in relative.split(['/', '\\']).filter(|s| !s.is_empty()) {
        // `..` climbs out; `.` is harmless but signals a caller that did not mean
        // to build a plain relative name. A colon is a drive prefix (`C:`) or an
        // NTFS alternate data stream, neither of which belongs in a folder name.
        if segment == ".." || segment == "." || segment.contains(':') {
            return None;
        }
        out.push(segment);
        pushed += 1;
    }

    (pushed > 0).then_some(out)
}

/// A search root: where it is on this machine, and which well-known location it is.
#[derive(Debug, Clone)]
pub struct Root {
    pub path: PathBuf,
    pub kind: RootKind,
}

/// One entry in a directory listing. Name only — no path, so a listing cannot be
/// used to construct a path outside the directory that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryMeta {
    pub name: String,
    pub is_dir: bool,
}

/// What detection is allowed to know about a filesystem entry.
///
/// Size, kind and modification time — enough for the plausibility signals in
/// `docs/architecture/GAME_SAVE_DETECTION.md` §9, and nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub is_dir: bool,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

/// Read-only, metadata-only filesystem access for save detection.
///
/// There is no `read`, no `write` and no `create`. That absence is the point: see
/// the module docs.
pub trait FileSystem: Send + Sync {
    /// The well-known locations to search, in priority order.
    fn roots(&self) -> Vec<Root>;

    /// Resolve a template anchor, or `None` if this machine has no such directory.
    ///
    /// Separate from [`Self::roots`] because not every anchor is somewhere we
    /// search — see [`Anchor`].
    fn anchor(&self, anchor: Anchor) -> Option<PathBuf>;

    /// Whether anything exists at `path`.
    fn exists(&self, path: &Path) -> bool;

    /// Names of the entries directly inside `path`.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryMeta>>;

    /// Metadata for a single entry.
    fn metadata(&self, path: &Path) -> io::Result<FileMeta>;

    /// Whether `path` is a directory. Provided so implementors only supply
    /// [`Self::metadata`]; a missing or unreadable path is not a directory.
    fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path).map(|m| m.is_dir).unwrap_or(false)
    }
}

/// The production implementation, backed by `dirs` and `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealFs;

impl FileSystem for RealFs {
    /// Resolves the six well-known roots.
    ///
    /// Order is preserved from the original `save_detect::candidate_roots`, because
    /// it is the tie-break when the same directory is reachable through two roots.
    ///
    /// The game's own install directory is **not** here. Adding it is ADR-0004, a
    /// deliberate behaviour change belonging to Phase 1 — Phase 0 is behaviour-
    /// neutral, so this returns exactly what the previous implementation did.
    fn roots(&self) -> Vec<Root> {
        let mut roots: Vec<Root> = Vec::new();

        // AppData/Roaming (%APPDATA% / XDG_CONFIG_HOME / ~/Library/Application Support)
        if let Some(p) = dirs::config_dir() {
            roots.push(Root {
                path: p,
                kind: RootKind::AppDataRoaming,
            });
        }

        // AppData/Local (%LOCALAPPDATA% on Windows). LocalLow is the parent's
        // sibling — there is no stdlib constant for it.
        if let Some(p) = dirs::data_local_dir() {
            if let Some(parent) = p.parent() {
                roots.push(Root {
                    path: parent.join("LocalLow"),
                    kind: RootKind::AppDataLocalLow,
                });
            }
            roots.push(Root {
                path: p,
                kind: RootKind::AppDataLocal,
            });
        }

        // Documents, and the My Games convention inside it.
        if let Some(p) = dirs::document_dir() {
            roots.push(Root {
                path: p.join("My Games"),
                kind: RootKind::DocumentsMyGames,
            });
            roots.push(Root {
                path: p,
                kind: RootKind::Documents,
            });
        }

        // %USERPROFILE%\Saved Games on Windows.
        if let Some(home) = dirs::home_dir() {
            roots.push(Root {
                path: home.join("Saved Games"),
                kind: RootKind::SavedGames,
            });
        }

        roots
    }

    fn anchor(&self, anchor: Anchor) -> Option<PathBuf> {
        match anchor {
            Anchor::AppDataRoaming => dirs::config_dir(),
            Anchor::AppDataLocal => dirs::data_local_dir(),
            // LocalLow is the sibling of Local; there is no stdlib constant.
            Anchor::AppDataLocalLow => dirs::data_local_dir()
                .and_then(|p| p.parent().map(|parent| parent.join("LocalLow"))),
            Anchor::Documents => dirs::document_dir(),
            Anchor::MyGames => dirs::document_dir().map(|p| p.join("My Games")),
            Anchor::SavedGames => dirs::home_dir().map(|p| p.join("Saved Games")),
            Anchor::UserProfile => dirs::home_dir(),
            // %PUBLIC% has no `dirs` accessor. Derived from the home directory's
            // parent, which is where Windows places it.
            Anchor::Public => dirs::home_dir()
                .and_then(|p| p.parent().map(|parent| parent.join("Public"))),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntryMeta>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            // A name that is not valid UTF-8 cannot be matched against a game title
            // anyway, so it is skipped rather than lossily converted.
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            // `file_type` does not follow symlinks, which is what we want: a link
            // out of a search root must not be walked (ADR-0003, bounds).
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            out.push(DirEntryMeta { name, is_dir });
        }
        Ok(out)
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
        let m = std::fs::symlink_metadata(path)?;
        Ok(FileMeta {
            is_dir: m.is_dir(),
            len: m.len(),
            modified: m.modified().ok(),
        })
    }

    fn is_dir(&self, path: &Path) -> bool {
        // Cheaper than going through `metadata`, and matches the previous
        // implementation's `Path::is_dir` semantics exactly.
        path.is_dir()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn real_fs_reports_the_six_well_known_roots_in_order() {
        let kinds: Vec<RootKind> = RealFs.roots().into_iter().map(|r| r.kind).collect();

        // Every kind present must appear in this relative order. Which are present
        // depends on the host, so the assertion is on order, not membership.
        let expected_order = [
            RootKind::AppDataRoaming,
            RootKind::AppDataLocalLow,
            RootKind::AppDataLocal,
            RootKind::DocumentsMyGames,
            RootKind::Documents,
            RootKind::SavedGames,
        ];
        let positions: Vec<usize> = kinds
            .iter()
            .map(|k| expected_order.iter().position(|e| e == k).unwrap())
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "roots out of order: {kinds:?}"
        );
    }

    #[test]
    fn labels_are_the_strings_the_frontend_renders() {
        // These cross IPC inside DetectedPath::hint. Changing one is a
        // frontend-visible change, so they are pinned here.
        assert_eq!(RootKind::AppDataRoaming.label(), "AppData/Roaming");
        assert_eq!(RootKind::AppDataLocalLow.label(), "AppData/LocalLow");
        assert_eq!(RootKind::AppDataLocal.label(), "AppData/Local");
        assert_eq!(RootKind::DocumentsMyGames.label(), "Documents/My Games");
        assert_eq!(RootKind::Documents.label(), "Documents");
        assert_eq!(RootKind::SavedGames.label(), "Saved Games");
    }

    #[test]
    fn read_dir_lists_names_and_kinds() {
        let tmp = TempDir::new("realfs-readdir");
        std::fs::create_dir_all(tmp.path().join("Saves")).unwrap();
        std::fs::write(tmp.path().join("config.ini"), b"x").unwrap();

        let mut entries = RealFs.read_dir(tmp.path()).unwrap();
        entries.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(
            entries,
            vec![
                DirEntryMeta {
                    name: "Saves".into(),
                    is_dir: true
                },
                DirEntryMeta {
                    name: "config.ini".into(),
                    is_dir: false
                },
            ]
        );
    }

    #[test]
    fn metadata_reports_size_and_kind() {
        let tmp = TempDir::new("realfs-meta");
        std::fs::write(tmp.path().join("save.dat"), b"1234567890").unwrap();

        let file = RealFs.metadata(&tmp.path().join("save.dat")).unwrap();
        assert!(!file.is_dir);
        assert_eq!(file.len, 10);
        assert!(file.modified.is_some());

        let dir = RealFs.metadata(tmp.path()).unwrap();
        assert!(dir.is_dir);
    }

    #[test]
    fn missing_paths_are_absent_and_not_directories() {
        let tmp = TempDir::new("realfs-missing");
        let nope = tmp.path().join("does-not-exist");

        assert!(!RealFs.exists(&nope));
        assert!(!RealFs.is_dir(&nope));
        assert!(RealFs.read_dir(&nope).is_err());
        assert!(RealFs.metadata(&nope).is_err());
    }

    #[test]
    fn read_dir_on_a_file_is_an_error_not_a_panic() {
        let tmp = TempDir::new("realfs-notdir");
        let file = tmp.path().join("plain.txt");
        std::fs::write(&file, b"x").unwrap();

        assert!(RealFs.read_dir(&file).is_err());
        assert!(!RealFs.is_dir(&file));
    }
}
