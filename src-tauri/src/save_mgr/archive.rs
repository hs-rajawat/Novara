//! The `.gvbk` save archive format.
//!
//! ```text
//! HEADER   b"GVBK" + version:u8
//! ENTRIES  repeated [u32 path_len][path utf8][u64 size][bytes]
//! END      u32 == 0                       (zero-length path)
//! FOOTER   [u64 file_count][u32 crc32]    (version 2 only)
//! ```
//!
//! Version 1 archives have no footer. They are still readable — a user's
//! existing backups must not become unrestorable — but cannot be integrity
//! checked, so they are validated structurally only.
//!
//! # Everything here treats the archive as untrusted input
//!
//! An archive is a file on disk. It can be truncated by a full volume, corrupted
//! by failing storage, or hand-crafted. Three properties are therefore enforced
//! before a single byte is written to the filesystem:
//!
//! 1. **No allocation from a declared length.** The previous implementation did
//!    `vec![0u8; size as usize]` straight from the archive's own size field, so
//!    a corrupt 8-byte value asked for an arbitrary allocation. Sizes are now
//!    checked against the bytes actually remaining in the file, and payloads are
//!    streamed rather than buffered whole.
//! 2. **No escaping the destination.** Entry paths came from the archive and
//!    were joined directly onto the target directory, so `../../..` in an entry
//!    path wrote wherever it liked. Paths are now validated component by
//!    component and the result is confirmed to stay inside the destination.
//! 3. **Validate before extracting.** See the invariant below.
//!
//! # ARCHITECTURAL INVARIANT: validation is a complete, read-only phase
//!
//! **No filesystem mutation may occur until every entry in the archive has been
//! validated.** This is a security property of the restore pipeline, not an
//! implementation detail, and it is what the whole module is arranged around.
//!
//! Concretely, [`read_archive`] is two phases and they may not be interleaved:
//!
//! * [`validate`] walks the entire archive — header, every entry header, every
//!   entry path, every payload byte for the checksum, and the footer — and
//!   **creates, opens for writing, renames, or deletes nothing**. Its only
//!   effects are reads and the in-memory entry list it returns.
//! * [`extract`] runs only after validation has returned `Ok`, and writes only
//!   the entries validation approved, re-checking each path through
//!   [`safe_join`] as a defence in depth.
//!
//! Why this must not be relaxed for performance or convenience:
//!
//! * A single-pass "validate as you go" extractor writes the first *n* valid
//!   entries before discovering that entry *n+1* is corrupt, truncated, or
//!   escapes the destination. The caller is then left with a partially applied
//!   restore — the worst possible outcome for save data, because it is neither
//!   the old state nor the new one and the user cannot tell which files came
//!   from where.
//! * It also makes traversal exploitable again: an archive can place a benign
//!   entry first and a malicious path second, and a streaming extractor will
//!   have already written outside the destination by the time it objects.
//! * The checksum is only meaningful before extraction. Verifying it afterwards
//!   can report that what was just written is wrong, which is a diagnosis, not a
//!   protection.
//!
//! The cost is reading the archive twice. Save data is small relative to disk
//! bandwidth, and [`SaveManager::restore`](super::SaveManager::restore) layers a
//! second guarantee on top — extraction targets a staging directory and the live
//! folder is only swapped once extraction has fully succeeded — so a failure at
//! any point leaves the user's saves exactly as they were.
//!
//! Any future change here (compression, a new format version, parallel
//! extraction, an in-place fast path) must preserve this ordering.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use globset::GlobSet;

use crate::error::{AppError, AppResult};

const MAGIC: &[u8; 4] = b"GVBK";
/// Current format version. Version 1 is read-only legacy.
pub const VERSION: u8 = 2;

/// Longest entry path accepted, in bytes.
///
/// Comfortably above any real save layout while bounding the allocation a
/// corrupt length field can request.
const MAX_PATH_LEN: u32 = 4096;

/// Most entries accepted in one archive, as a guard against a corrupt stream
/// that never reaches its end marker.
const MAX_ENTRIES: u64 = 1_000_000;

/// Streaming copy buffer. Payloads are never held in memory in full.
const COPY_BUF: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveStats {
    pub total_bytes: i64,
    pub file_count: i64,
}

/// One entry, as recorded during validation.
#[derive(Debug, Clone)]
struct Entry {
    rel: String,
    size: u64,
    /// Byte offset of the payload within the archive.
    offset: u64,
}

// ───────────────────────────── crc32 ─────────────────────────────

/// CRC-32 (IEEE), enough to detect corruption and truncation.
///
/// Deliberately not a cryptographic digest: this detects damaged storage and
/// truncated writes, and claiming tamper resistance would need a keyed MAC and a
/// place to keep the key. Implemented here rather than pulled in as a dependency
/// because it is fifteen lines.
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u32::from(b);
            for _ in 0..8 {
                let mask = (self.0 & 1).wrapping_neg();
                self.0 = (self.0 >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
    }

    fn finish(self) -> u32 {
        !self.0
    }
}

/// Writer that checksums everything passing through it.
struct CrcWriter<W: Write> {
    inner: W,
    crc: Crc32,
}

impl<W: Write> Write for CrcWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.crc.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

// ──────────────────────── path validation ────────────────────────

/// Resolve an archive entry path against `dest`, refusing anything that could
/// escape it.
///
/// This is the fix for the traversal hole: entry paths are attacker- or
/// corruption-controlled, and `dest.join(rel)` happily accepts `..`, an absolute
/// path, or a Windows drive prefix, all of which write outside the save folder.
///
/// Rejected: absolute paths, any `.` or `..` component, empty components,
/// embedded NULs, backslashes (which are a separator on Windows and could
/// smuggle a component past a `/`-only split), and drive or UNC prefixes.
fn safe_join(dest: &Path, rel: &str) -> AppResult<PathBuf> {
    let reject = |why: &str| -> AppError {
        AppError::SaveMgr(format!("unsafe path in archive ({why}): {rel:?}"))
    };

    if rel.is_empty() {
        return Err(reject("empty"));
    }
    if rel.contains('\0') {
        return Err(reject("contains NUL"));
    }
    // Entries are written with `/` separators, so a backslash never belongs in
    // one — and on Windows it *is* a separator, which a `/`-only check misses.
    if rel.contains('\\') {
        return Err(reject("contains a backslash"));
    }
    if rel.starts_with('/') {
        return Err(reject("absolute"));
    }

    let candidate = Path::new(rel);
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                if part.is_empty() {
                    return Err(reject("empty component"));
                }
            }
            Component::ParentDir => return Err(reject("parent directory traversal")),
            Component::CurDir => return Err(reject("current directory component")),
            // Covers `C:`, `\\?\`, `\\server\share` and a leading root.
            Component::Prefix(_) | Component::RootDir => {
                return Err(reject("absolute or prefixed"))
            }
        }
    }

    let joined = dest.join(candidate);
    // Belt and braces: the component walk above should make this unreachable,
    // but the containment check is cheap and this is the last line of defence
    // before a write.
    if !joined.starts_with(dest) {
        return Err(reject("escapes the destination"));
    }
    Ok(joined)
}

// ───────────────────────────── writing ─────────────────────────────

/// Archive every file under `source` into `dest`.
///
/// `filter`, when present, is `save_profiles.glob` compiled to a matcher; only
/// paths matching it are included. Patterns are matched against the entry's
/// relative path with `/` separators, so they behave the same on every platform.
///
/// Writes to a temporary file and renames into place, so a failure part-way
/// through never leaves a half-written archive at the destination that a later
/// restore could try to use.
pub fn write_archive(
    source: &Path,
    dest: &Path,
    filter: Option<&GlobSet>,
) -> AppResult<ArchiveStats> {
    let tmp = temp_sibling(dest, "gvbk.part")?;
    let stats = write_archive_inner(source, &tmp, filter);
    match stats {
        Ok(stats) => {
            fs::rename(&tmp, dest).map_err(|e| {
                let _ = fs::remove_file(&tmp);
                AppError::SaveMgr(format!("finalize archive {}: {e}", dest.display()))
            })?;
            Ok(stats)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write_archive_inner(
    source: &Path,
    tmp: &Path,
    filter: Option<&GlobSet>,
) -> AppResult<ArchiveStats> {
    let file = File::create(tmp)
        .map_err(|e| AppError::SaveMgr(format!("create archive {}: {e}", tmp.display())))?;
    let mut out = CrcWriter {
        inner: BufWriter::new(file),
        crc: Crc32::new(),
    };

    // The header is outside the checksummed region: it identifies the format,
    // and a wrong magic is caught before the checksum is even relevant.
    out.inner
        .write_all(MAGIC)
        .and_then(|_| out.inner.write_all(&[VERSION]))
        .map_err(|e| AppError::SaveMgr(format!("write archive header: {e}")))?;

    let mut total: i64 = 0;
    let mut files: i64 = 0;

    // Sorted for a deterministic archive: the same folder must produce the same
    // bytes, which is also what makes the checksum meaningful across runs.
    let mut entries: Vec<PathBuf> = walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();
    entries.sort();

    for path in entries {
        let rel = path
            .strip_prefix(source)
            .map_err(|e| AppError::SaveMgr(e.to_string()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.is_empty() {
            continue;
        }
        if let Some(set) = filter {
            if !set.is_match(&rel_str) {
                continue;
            }
        }
        let rel_bytes = rel_str.as_bytes();
        if rel_bytes.len() as u32 > MAX_PATH_LEN {
            // Refuse to write what we would refuse to read.
            return Err(AppError::SaveMgr(format!(
                "path too long to archive: {rel_str}"
            )));
        }

        let mut input = File::open(&path)
            .map_err(|e| AppError::SaveMgr(format!("read {}: {e}", path.display())))?;
        let size = input
            .metadata()
            .map_err(|e| AppError::SaveMgr(format!("stat {}: {e}", path.display())))?
            .len();

        out.write_all(&(rel_bytes.len() as u32).to_le_bytes())
            .and_then(|_| out.write_all(rel_bytes))
            .and_then(|_| out.write_all(&size.to_le_bytes()))
            .map_err(|e| AppError::SaveMgr(format!("write entry header: {e}")))?;

        // Streamed rather than `fs::read` into a Vec: a single large save file
        // should not have to fit in memory.
        let copied = io::copy(&mut (&mut input).take(size), &mut out)
            .map_err(|e| AppError::SaveMgr(format!("archive {}: {e}", path.display())))?;
        if copied != size {
            return Err(AppError::SaveMgr(format!(
                "{} changed size while archiving (expected {size}, wrote {copied})",
                path.display()
            )));
        }

        total = total.saturating_add(size as i64);
        files += 1;
    }

    out.write_all(&0u32.to_le_bytes())
        .map_err(|e| AppError::SaveMgr(format!("write archive end marker: {e}")))?;

    let crc = out.crc.finish();
    let mut inner = out.inner;
    inner
        .write_all(&(files as u64).to_le_bytes())
        .and_then(|_| inner.write_all(&crc.to_le_bytes()))
        .and_then(|_| inner.flush())
        .map_err(|e| AppError::SaveMgr(format!("write archive footer: {e}")))?;
    // Surface a failed flush-on-drop rather than losing it silently.
    inner
        .into_inner()
        .map_err(|e| AppError::SaveMgr(format!("flush archive: {e}")))?
        .sync_all()
        .map_err(|e| AppError::SaveMgr(format!("sync archive: {e}")))?;

    Ok(ArchiveStats {
        total_bytes: total,
        file_count: files,
    })
}

// ───────────────────────────── reading ─────────────────────────────

/// Validate `archive` in full, then extract it into `dest`.
///
/// Upholds the module's architectural invariant: validation is a complete,
/// read-only phase and nothing is written until it has returned `Ok`. A
/// truncated, corrupt, or hostile archive therefore fails with `dest` untouched
/// rather than leaving a partial restore behind. Do not merge these two phases.
pub fn read_archive(archive: &Path, dest: &Path) -> AppResult<ArchiveStats> {
    let (entries, stats) = validate(archive, dest)?;
    extract(archive, dest, &entries)?;
    Ok(stats)
}

/// Walk the archive without writing anything, returning its entries.
///
/// **This function must never mutate the filesystem** — no create, no open for
/// writing, no rename, no delete, not even a directory. It is the read-only half
/// of the invariant documented at the top of this module, and the reason a
/// failure here is always safe.
fn validate(archive: &Path, dest: &Path) -> AppResult<(Vec<Entry>, ArchiveStats)> {
    let file = File::open(archive)
        .map_err(|e| AppError::SaveMgr(format!("open {}: {e}", archive.display())))?;
    let file_len = file
        .metadata()
        .map_err(|e| AppError::SaveMgr(format!("stat {}: {e}", archive.display())))?
        .len();
    let mut reader = BufReader::new(file);

    let mut header = [0u8; 5];
    reader
        .read_exact(&mut header)
        .map_err(|_| AppError::SaveMgr("archive is truncated (no header)".into()))?;
    if &header[..4] != MAGIC {
        return Err(AppError::SaveMgr("not a gvbk archive".into()));
    }
    let version = header[4];
    if version != 1 && version != VERSION {
        return Err(AppError::SaveMgr(format!(
            "unsupported gvbk version {version}; this build understands 1 and {VERSION}"
        )));
    }

    let mut crc = Crc32::new();
    let mut position: u64 = 5;
    let mut entries: Vec<Entry> = Vec::new();
    let mut total: i64 = 0;

    loop {
        if entries.len() as u64 > MAX_ENTRIES {
            return Err(AppError::SaveMgr(format!(
                "archive declares more than {MAX_ENTRIES} entries; refusing to continue"
            )));
        }

        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|_| AppError::SaveMgr("archive is truncated (entry header)".into()))?;
        crc.update(&len_buf);
        position += 4;
        let path_len = u32::from_le_bytes(len_buf);
        if path_len == 0 {
            break;
        }
        if path_len > MAX_PATH_LEN {
            return Err(AppError::SaveMgr(format!(
                "archive entry path length {path_len} exceeds the {MAX_PATH_LEN} byte limit"
            )));
        }

        let mut path_buf = vec![0u8; path_len as usize];
        reader
            .read_exact(&mut path_buf)
            .map_err(|_| AppError::SaveMgr("archive is truncated (entry path)".into()))?;
        crc.update(&path_buf);
        position += u64::from(path_len);
        let rel = String::from_utf8(path_buf)
            .map_err(|_| AppError::SaveMgr("archive entry path is not valid UTF-8".into()))?;
        // Validated now, during the read-only pass, so an unsafe path aborts
        // before anything has been written.
        safe_join(dest, &rel)?;

        let mut size_buf = [0u8; 8];
        reader
            .read_exact(&mut size_buf)
            .map_err(|_| AppError::SaveMgr("archive is truncated (entry size)".into()))?;
        crc.update(&size_buf);
        position += 8;
        let size = u64::from_le_bytes(size_buf);

        // The precise bound: a declared size can never exceed the bytes left in
        // the file. This is what makes a corrupt length field harmless instead
        // of an arbitrary allocation.
        let remaining = file_len.saturating_sub(position);
        if size > remaining {
            return Err(AppError::SaveMgr(format!(
                "archive entry {rel:?} declares {size} bytes but only {remaining} remain in the file"
            )));
        }

        // Read the payload only to checksum it; nothing is retained.
        let mut limited = (&mut reader).take(size);
        let mut buf = [0u8; COPY_BUF];
        let mut read_total: u64 = 0;
        loop {
            let n = limited
                .read(&mut buf)
                .map_err(|e| AppError::SaveMgr(format!("read archive payload: {e}")))?;
            if n == 0 {
                break;
            }
            crc.update(&buf[..n]);
            read_total += n as u64;
        }
        if read_total != size {
            return Err(AppError::SaveMgr(format!(
                "archive is truncated: entry {rel:?} has {read_total} of {size} bytes"
            )));
        }

        entries.push(Entry {
            rel,
            size,
            offset: position,
        });
        position += size;
        total = total.saturating_add(size as i64);
    }

    let stats = ArchiveStats {
        total_bytes: total,
        file_count: entries.len() as i64,
    };

    if version == 1 {
        // No footer to check. Legacy archives stay restorable, but the absence
        // of a checksum is why version 2 exists.
        return Ok((entries, stats));
    }

    let expected_crc = crc.finish();
    let mut footer = [0u8; 12];
    reader
        .read_exact(&mut footer)
        .map_err(|_| AppError::SaveMgr("archive is truncated (missing footer)".into()))?;
    let declared_count = u64::from_le_bytes(footer[..8].try_into().unwrap());
    let declared_crc = u32::from_le_bytes(footer[8..].try_into().unwrap());

    if declared_count != entries.len() as u64 {
        return Err(AppError::SaveMgr(format!(
            "archive declares {declared_count} files but contains {}",
            entries.len()
        )));
    }
    if declared_crc != expected_crc {
        return Err(AppError::SaveMgr(format!(
            "archive checksum mismatch (declared {declared_crc:08x}, computed {expected_crc:08x}); \
             the file is corrupt and will not be restored"
        )));
    }

    Ok((entries, stats))
}

/// Write validated entries out. Every path here has already passed
/// [`safe_join`] during validation.
fn extract(archive: &Path, dest: &Path, entries: &[Entry]) -> AppResult<()> {
    let file = File::open(archive)
        .map_err(|e| AppError::SaveMgr(format!("open {}: {e}", archive.display())))?;
    let mut reader = BufReader::new(file);

    for entry in entries {
        let out_path = safe_join(dest, &entry.rel)?;
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::SaveMgr(format!("create {}: {e}", parent.display()))
            })?;
        }
        reader
            .seek(SeekFrom::Start(entry.offset))
            .map_err(|e| AppError::SaveMgr(format!("seek archive: {e}")))?;
        let mut out = BufWriter::new(File::create(&out_path).map_err(|e| {
            AppError::SaveMgr(format!("create {}: {e}", out_path.display()))
        })?);
        let copied = io::copy(&mut (&mut reader).take(entry.size), &mut out)
            .map_err(|e| AppError::SaveMgr(format!("extract {}: {e}", out_path.display())))?;
        out.flush()
            .map_err(|e| AppError::SaveMgr(format!("flush {}: {e}", out_path.display())))?;
        if copied != entry.size {
            return Err(AppError::SaveMgr(format!(
                "short read extracting {}: {copied} of {} bytes",
                out_path.display(),
                entry.size
            )));
        }
    }
    Ok(())
}

// ───────────────────────────── helpers ─────────────────────────────

/// A sibling path of `path` with `suffix` appended to its **full file name**.
///
/// `Path::with_extension` cannot be used for this: it replaces everything after
/// the last dot, so for a save folder like `S.T.A.L.K.E.R.` or
/// `Company.of.Heroes` the result is not a sibling of the original at all — it
/// silently targets a different path. Appending to the whole file name is
/// correct for any name.
pub fn sibling_with_suffix(path: &Path, suffix: &str) -> AppResult<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        AppError::SaveMgr(format!("path has no parent directory: {}", path.display()))
    })?;
    let name = path.file_name().ok_or_else(|| {
        AppError::SaveMgr(format!("path has no file name: {}", path.display()))
    })?;
    let mut joined = name.to_os_string();
    joined.push(".");
    joined.push(suffix);
    Ok(parent.join(joined))
}

fn temp_sibling(path: &Path, suffix: &str) -> AppResult<PathBuf> {
    let unique = format!("{}.{}", std::process::id(), uuid::Uuid::new_v4());
    sibling_with_suffix(path, &format!("{suffix}.{unique}"))
}

/// Compile `save_profiles.glob` into a matcher.
///
/// `None` or blank means "everything". Multiple patterns may be separated by
/// `;` or newlines, and an entry matches if **any** pattern matches — the
/// natural reading of "which files belong to this save".
pub fn compile_glob(pattern: Option<&str>) -> AppResult<Option<GlobSet>> {
    let Some(raw) = pattern else { return Ok(None) };
    let patterns: Vec<&str> = raw
        .split([';', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let glob = globset::GlobBuilder::new(pattern)
            // Save folders are user data on a case-insensitive filesystem on
            // Windows; `*.SAV` and `*.sav` must mean the same thing.
            .case_insensitive(true)
            // `*` should not cross directory boundaries; `**` is how a user asks
            // for that, matching every other glob tool they have used.
            .literal_separator(true)
            .build()
            .map_err(|e| AppError::Invalid(format!("invalid save filter {pattern:?}: {e}")))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| AppError::Invalid(format!("invalid save filter: {e}")))
}
