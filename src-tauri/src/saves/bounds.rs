//! Enumeration bounds for detection.
//!
//! From `GAME_SAVE_DETECTION.md` §7.2. These exist because the failure mode they
//! prevent is not a wrong answer but an **unresponsive application**: `Documents`
//! on a real machine can hold tens of thousands of entries, and a detector that
//! walks it without a ceiling turns a library refresh into a disk thrash.
//!
//! Every bound is a *ceiling, not a target*. Hitting one means the input is
//! unusual, and the honest response is to stop and record that the scan was
//! truncated rather than to keep going or to pretend the result is complete.
//!
//! Only the bounds with a consumer today are defined here. The verifier's read
//! ceilings (§7.2 rows 3 and 4) and the per-game time budget arrive with the
//! verifier and the backoff gate respectively — a constant no test can exercise is
//! a claim, not a control.

/// Deepest directory level below a root that detection may look at.
///
/// **Currently a documented ceiling rather than an enforced one**, because nothing
/// recurses: the locator reads each root exactly one level deep. It is recorded
/// here so that the first code tempted to descend has a number to respect, and so
/// the §7.2 contract is visible in the code rather than only in prose.
pub const MAX_DEPTH_BELOW_ROOT: usize = 4;

/// Ceiling on candidates returned for one game.
///
/// §7.2: "Beyond this the alias generator is malfunctioning." Treated exactly that
/// way — the cap is a symptom detector, not a paging mechanism.
pub const MAX_CANDIDATES_PER_GAME: usize = 200;

/// Ceiling on directory entries examined in a single root.
///
/// Not in the §7.2 table, because fuzzy matching did not exist when it was written.
/// Enumeration is the one operation whose cost scales with the *user's* filesystem
/// rather than with anything NOVARA controls, so it needs its own ceiling: a
/// `Documents` folder with 50,000 entries must not cost 50,000 similarity
/// computations per game.
pub const MAX_ENTRIES_PER_ROOT: usize = 2_048;

/// Longest directory name compared with an edit distance.
///
/// Similarity is O(n·m); without a cap, two pathological 4 KB names would cost
/// millions of cell updates. No real folder name approaches this.
pub const MAX_NAME_LEN_FOR_SIMILARITY: usize = 64;

/// Shortest normalised name eligible for *fuzzy* matching.
///
/// Short names collide far too easily under an edit distance: `Data` and `Date`
/// score 0.75 and would pass the threshold. Names below this length must match
/// exactly after normalisation, which costs recall on titles like `Ori` and buys
/// a great deal of precision.
pub const MIN_LEN_FOR_FUZZY: usize = 6;

/// Minimum normalised similarity for a name match to be recorded at all.
///
/// `GAME_SAVE_DETECTION.md` §8: "Threshold for evidence: 0.75. Below that, no
/// `NameMatch` is recorded at all."
pub const SIMILARITY_THRESHOLD: f32 = 0.75;

// ─────────────────────────────────────────────────────────────────────────
// Verifier (§7.2 rows 3 and 4, §9)
// ─────────────────────────────────────────────────────────────────────────

/// Ceiling on `metadata()` calls for one candidate directory.
///
/// §7.2: "Enough to characterise a directory." This is the *only* per-file cost the
/// verifier pays. Extension classification needs the file's name, which `read_dir`
/// already returned, so the histogram is free — see `verifier` module docs.
pub const VERIFIER_MAX_METADATA_READS: usize = 64;

/// Deepest level below a candidate directory the verifier looks at.
///
/// §9: "Saves are usually ≤ 2 levels below the folder." Needed for correctness, not
/// just recall: `Terraria/` holds only `Players/` and `Worlds/`, so a depth-0 scan
/// would find no files and wrongly conclude the directory is empty.
///
/// This is bounded *descent into a directory already identified as a candidate*, not
/// the recursive search of a root that §7 forbids. The distinction is that the
/// verifier never discovers a new path to offer — it only characterises one it was
/// handed.
pub const VERIFIER_MAX_DEPTH: usize = 2;

/// Ceiling on directory entries the verifier walks for one candidate, across all
/// depths. Guards against a candidate that happens to be a junction to a huge tree.
///
/// Comfortably above [`VERIFIER_MANY_FILES`] on purpose. When these two sat close
/// together, the cache heuristic could only fire in the narrow window between them —
/// see `verifier::signals_for` for why that was backwards. Walking an entry costs only
/// a name from a listing already in hand, so a wide ceiling here is cheap; the
/// expensive per-file work is rationed separately by
/// [`VERIFIER_MAX_METADATA_READS`].
pub const VERIFIER_MAX_ENTRIES: usize = 2_048;

/// At or below this size a file carries no save data worth the name.
///
/// A directory of 0–64 byte files is a marker or lock directory — `.lock`,
/// `desktop.ini`, sentinel files — not saves. §9: "a folder of 4-byte files is a
/// marker directory".
pub const TINY_FILE_BYTES: u64 = 64;

/// Above this many files a directory is behaving like a cache, not a save folder.
///
/// §9: "Hundreds of files → probably a cache." Counted from the listing, so this can
/// exceed [`VERIFIER_MAX_METADATA_READS`] without extra cost.
pub const VERIFIER_MANY_FILES: usize = 400;

/// Ceiling on candidates verified for one game.
///
/// Each verification costs up to [`VERIFIER_MAX_METADATA_READS`] stats, and the locator
/// may legitimately return up to [`MAX_CANDIDATES_PER_GAME`]. Without this, one game
/// could cost 200 × 64 syscalls. Candidates are verified in confidence order, so this
/// covers everything a user would look at, and an unverified candidate is *kept* —
/// not having looked is never a reason to reject.
pub const VERIFIER_MAX_CANDIDATES_PER_GAME: usize = 32;

/// Files modified within this span of each other were written by one save event.
pub const WRITE_BURST_WINDOW_SECS: u64 = 15 * 60;

/// Fraction of files that must be media before a directory is called a media folder.
///
/// Deliberately short of 1.0: a screenshots folder often carries a stray `.txt` or
/// `Thumbs.db`, and requiring purity would let one junk file defeat the check.
pub const MEDIA_DOMINANCE_RATIO: f32 = 0.7;
