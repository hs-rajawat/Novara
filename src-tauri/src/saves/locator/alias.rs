//! Alias generation and name similarity.
//!
//! Aliases only *propose*; they never decide — `GAME_SAVE_DETECTION.md` §8, and §6
//! rule 9 makes name similarity the weakest row in the decision table for a reason.
//! Everything here exists to widen what detection can *find*, and every widening is
//! paired with a rule that keeps it from finding the wrong thing.
//!
//! ## Why confidence is attached to the transform, not the match
//!
//! An exact title is worth more than an initialism, regardless of how cleanly the
//! initialism matched. Confidence therefore belongs to the *alias*, and a fuzzy
//! match multiplies it down rather than replacing it. A weak alias matched loosely
//! must not be able to outrank a strong alias matched exactly.

use super::super::bounds;
use crate::saves::kb::normalise_title;

/// How an alias was derived. Carried so a candidate's `hint` can say what matched,
/// which is the difference between a user trusting a suggestion and guessing at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasKind {
    /// The title as given, or a pure case/separator restatement of it.
    Exact,
    /// A recognised reduction: subtitle removed, edition suffix removed, and so on.
    Reduced,
    /// A guess that is cheap to try and easy to get wrong.
    Weak,
}

#[derive(Debug, Clone)]
pub struct Alias {
    /// The directory name, or `Vendor/Title` pair, to look for.
    pub name: String,
    pub confidence: f32,
    pub kind: AliasKind,
}

impl Alias {
    fn new(confidence: f32, kind: AliasKind, name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            confidence,
            kind,
        }
    }

    /// Whether this alias may be matched *approximately* against a real directory
    /// name, as opposed to only exactly.
    ///
    /// Weak aliases are exact-only. An initialism like `TW3` or a bare first word is
    /// already a guess; allowing an edit distance on top of a guess compounds two
    /// sources of error, and the combination is where name-based detection earns its
    /// bad reputation.
    pub fn allows_fuzzy(&self) -> bool {
        self.kind != AliasKind::Weak
            && !self.name.contains('/')
            && normalise_title(&self.name).len() >= bounds::MIN_LEN_FOR_FUZZY
    }
}

/// Words that identify a vendor or a container rather than a game.
///
/// A first-word or reduced alias that lands on one of these would match a folder
/// shared by an entire publisher's catalogue — `Documents/My Games` itself, say —
/// and every game in the library would claim it.
///
/// Entries are stored **folded** ([`normalise_title`]), which removes spaces. Compact
/// forms of multi-word containers therefore have to be listed as one word:
/// `mygames`, not `my games`. Listing only the individual words `my` and `games` was
/// not enough — a title of "My Games" folds to `mygames` and matched neither.
const VENDOR_STOPWORDS: &[&str] = &[
    "appdata",
    "applicationdata",
    "common",
    "commonfiles",
    "data",
    "documents",
    "entertainment",
    "games",
    "interactive",
    "launcher",
    "local",
    "localsettings",
    "my",
    "mydocuments",
    "mygames",
    "profile",
    "profiles",
    "program",
    "programdata",
    "programfiles",
    "roaming",
    "save",
    "saved",
    "savedata",
    "savedgame",
    "savedgames",
    "savegame",
    "savegames",
    "saves",
    "settings",
    "shared",
    "studio",
    "studios",
    "the",
    "user",
    "userdata",
    "userprofile",
    "users",
];

/// Edition and release suffixes that rarely appear in a folder name.
const EDITION_SUFFIXES: &[&str] = &[
    "anniversary edition",
    "complete edition",
    "definitive edition",
    "deluxe edition",
    "directors cut",
    "enhanced edition",
    "game of the year edition",
    "goty edition",
    "gold edition",
    "legendary edition",
    "remastered",
    "ultimate edition",
    "goty",
];

/// Leading articles, dropped so `The Witcher` can match `Witcher`.
const LEADING_ARTICLES: &[&str] = &["the ", "a ", "an "];

/// Generate the directory names worth looking for, strongest first.
///
/// `developer` and `publisher` enable two-level aliases — `CDPR/Witcher3` — which a
/// title-only matcher cannot produce. §8 calls this out as free accuracy from data
/// NOVARA already resolves for metadata.
pub fn aliases(title: &str, developer: Option<&str>, publisher: Option<&str>) -> Vec<Alias> {
    let mut out: Vec<Alias> = Vec::new();
    let title = title.trim();
    if title.is_empty() {
        return out;
    }

    // ── Retained from the original detector ───────────────────────────────
    out.push(Alias::new(1.00, AliasKind::Exact, title));

    let lower = title.to_lowercase();
    if lower != title {
        out.push(Alias::new(0.92, AliasKind::Exact, &lower));
    }

    let stripped = strip_trailing_number(title);
    if stripped != title {
        out.push(Alias::new(0.75, AliasKind::Reduced, &stripped));
        out.push(Alias::new(0.68, AliasKind::Reduced, stripped.to_lowercase()));
    }

    let underscored = title.replace(' ', "_");
    if underscored != title {
        out.push(Alias::new(0.72, AliasKind::Exact, underscored));
    }

    let compact = title.replace(' ', "");
    if compact != title && compact != title.replace(' ', "_") {
        out.push(Alias::new(0.60, AliasKind::Exact, &compact));
        out.push(Alias::new(0.55, AliasKind::Exact, compact.to_lowercase()));
    }

    // ── Added in task 1.13 ───────────────────────────────────────────────

    // Subtitle removed. Very common: `NieR: Automata` lives in `NieR`.
    if let Some(main) = strip_subtitle(title) {
        out.push(Alias::new(0.80, AliasKind::Reduced, &main));
        out.push(Alias::new(0.70, AliasKind::Reduced, main.replace(' ', "")));
    }

    // Edition suffix removed. `Skyrim Special Edition` is shelved as `Skyrim` about
    // as often as it is spelled out.
    if let Some(base) = strip_edition(title) {
        out.push(Alias::new(0.82, AliasKind::Reduced, &base));
        out.push(Alias::new(0.72, AliasKind::Reduced, base.replace(' ', "")));
    }

    // Punctuation dropped. `S.T.A.L.K.E.R.` becomes `STALKER`, which is what the
    // folder is actually called.
    let depunctuated = drop_punctuation(title);
    if !depunctuated.is_empty() && depunctuated != title && depunctuated != compact {
        out.push(Alias::new(0.78, AliasKind::Reduced, &depunctuated));
    }

    // Leading article dropped.
    if let Some(no_article) = strip_leading_article(title) {
        out.push(Alias::new(0.79, AliasKind::Reduced, &no_article));
        out.push(Alias::new(0.69, AliasKind::Reduced, no_article.replace(' ', "")));
    }

    // Initialism, keeping any trailing number: `The Witcher 3` → `TW3`.
    if let Some(initials) = initialism(title) {
        out.push(Alias::new(0.30, AliasKind::Weak, initials));
    }

    // First word, if distinctive enough to be worth a probe.
    if let Some(first) = title.split_whitespace().next() {
        if first.len() >= 5 {
            out.push(Alias::new(0.40, AliasKind::Weak, first));
            out.push(Alias::new(0.35, AliasKind::Weak, first.to_lowercase()));
        }
    }

    // Two-level vendor pairs. Both orders of name are tried because studios appear
    // under their full and short names inconsistently.
    for vendor in [developer, publisher].into_iter().flatten() {
        let vendor = vendor.trim();
        if vendor.is_empty() || is_vendor_stopword(vendor) {
            continue;
        }
        out.push(Alias::new(0.88, AliasKind::Reduced, format!("{vendor}/{title}")));
        let compact_title = title.replace(' ', "");
        out.push(Alias::new(
            0.76,
            AliasKind::Reduced,
            format!("{vendor}/{compact_title}"),
        ));
    }

    // ── Filtering ────────────────────────────────────────────────────────
    //
    // A single-segment alias that is a bare vendor or container word would match a
    // folder belonging to a whole catalogue — `Documents/My Games` itself, say — and
    // every game in the library would claim it. Applied once after generation rather
    // than at each site, so a transform added later cannot forget the rule.
    //
    // Two-segment aliases are exempt: the vendor word is the point of them, and the
    // second segment still has to match.
    out.retain(|a| !a.name.trim().is_empty());
    out.retain(|a| a.name.contains('/') || !is_vendor_stopword(&a.name));

    dedupe_keeping_strongest(out)
}

/// True when a name is a container or vendor word rather than a game.
pub fn is_vendor_stopword(name: &str) -> bool {
    let folded = normalise_title(name);
    VENDOR_STOPWORDS.contains(&folded.as_str())
}

/// Keep one entry per distinct name, at its highest confidence.
fn dedupe_keeping_strongest(mut aliases: Vec<Alias>) -> Vec<Alias> {
    aliases.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut seen = std::collections::HashSet::new();
    aliases.retain(|a| seen.insert(a.name.to_lowercase()));
    aliases
}

/// Everything before the first `:`, `–` or `—`, when that leaves something usable.
fn strip_subtitle(title: &str) -> Option<String> {
    let index = title.find([':', '–', '—'])?;
    let main = title[..index].trim();
    // ` - ` is deliberately not a separator: hyphens appear inside names
    // (`Spider-Man`, `Half-Life`) far more often than they separate a subtitle.
    (main.len() >= 3 && main != title).then(|| main.to_string())
}

/// The title with a recognised edition suffix removed.
fn strip_edition(title: &str) -> Option<String> {
    let lower = title.to_lowercase();
    for suffix in EDITION_SUFFIXES {
        if let Some(base) = lower.strip_suffix(suffix) {
            let base = title[..base.len()].trim().trim_end_matches(['-', ':', ',']).trim();
            if base.len() >= 3 && base != title {
                return Some(base.to_string());
            }
        }
    }
    None
}

/// Alphanumerics and single spaces only: `S.T.A.L.K.E.R.` → `STALKER`.
fn drop_punctuation(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut pending_space = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        } else if ch == ' ' {
            pending_space = true;
        }
        // Any other punctuation vanishes without leaving a gap, which is what
        // turns dotted initialisms into a single word.
    }
    out.trim().to_string()
}

fn strip_leading_article(title: &str) -> Option<String> {
    let lower = title.to_lowercase();
    for article in LEADING_ARTICLES {
        if lower.starts_with(article) {
            let rest = title[article.len()..].trim();
            if rest.len() >= 3 {
                return Some(rest.to_string());
            }
        }
    }
    None
}

/// First letters of the significant words, plus a trailing number if present.
fn initialism(title: &str) -> Option<String> {
    let words: Vec<&str> = title.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }

    let mut initials = String::new();
    let mut trailing = String::new();
    for word in &words {
        let folded = normalise_title(word);
        if folded.is_empty() {
            continue;
        }
        if is_number_token(word) {
            trailing = folded.to_uppercase();
            continue;
        }
        if let Some(c) = folded.chars().next() {
            initials.push(c.to_ascii_uppercase());
        }
    }
    initials.push_str(&trailing);

    // Two-character initialisms are noise; three is the shortest that carries
    // enough information to be worth a probe.
    (initials.len() >= 3).then_some(initials)
}

pub fn strip_trailing_number(s: &str) -> String {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if let Some(last) = tokens.last() {
        if is_number_token(last) && tokens.len() > 1 {
            return tokens[..tokens.len() - 1].join(" ");
        }
    }
    s.to_string()
}

pub fn is_number_token(s: &str) -> bool {
    let folded = normalise_title(s);
    if folded.is_empty() {
        return false;
    }
    if folded.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    matches!(
        folded.as_str(),
        "i" | "ii"
            | "iii"
            | "iv"
            | "v"
            | "vi"
            | "vii"
            | "viii"
            | "ix"
            | "x"
            | "xi"
            | "xii"
            | "xiii"
            | "xiv"
            | "xv"
            | "xvi"
            | "xvii"
            | "xviii"
            | "xix"
            | "xx"
    )
}

// ─────────────────────────────────────────────────────────────────────────
// Similarity
// ─────────────────────────────────────────────────────────────────────────

/// The trailing sequel marker of a folded name: `fallout4` → `Some("4")`.
///
/// Extracted so it can be compared *separately* from the rest of the name. See
/// [`similarity`] for why that is load-bearing.
fn trailing_version(folded: &str) -> Option<String> {
    let digits: String = folded
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !digits.is_empty() {
        return Some(digits.chars().rev().collect());
    }
    // Roman numerals, but only as a whole trailing run that is not the entire
    // name — `civ` must not read as the numerals c, i, v.
    let romans: String = folded
        .chars()
        .rev()
        .take_while(|c| matches!(c, 'i' | 'v' | 'x'))
        .collect();
    if romans.is_empty() || romans.len() == folded.len() {
        return None;
    }
    let romans: String = romans.chars().rev().collect();
    is_number_token(&romans).then_some(romans)
}

/// Normalised similarity between an alias and a real directory name, 0.0 – 1.0.
///
/// Both sides are folded by [`normalise_title`] first, so `Witcher3` and
/// `witcher 3` are simply equal and cost nothing — case and punctuation differences
/// are not "fuzzy" at all, they are noise, and treating them as noise is what keeps
/// the edit distance for cases that genuinely need it.
///
/// ## Sequels are refused outright
///
/// This is the rule that makes fuzzy matching safe enough to ship. `fallout4` and
/// `fallout3` differ by one character in eight: a normalised edit distance scores
/// them 0.875 and would happily bind *Fallout 4* to *Fallout 3*'s saves. Same for
/// `darksoulsii` against `darksoulsiii` at 0.917.
///
/// So a non-exact match requires the trailing sequel marker to be **identical**. A
/// title with no marker cannot approximately match a numbered folder and vice
/// versa. This costs nothing real — a game and its own folder agree about which
/// instalment they are — and removes the single most likely wrong binding in the
/// whole design.
pub fn similarity(alias: &str, dir_name: &str) -> f32 {
    let a = normalise_title(alias);
    let b = normalise_title(dir_name);

    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    if a.len() > bounds::MAX_NAME_LEN_FOR_SIMILARITY
        || b.len() > bounds::MAX_NAME_LEN_FOR_SIMILARITY
    {
        return 0.0;
    }
    // Different instalments of the same series are different games.
    if trailing_version(&a) != trailing_version(&b) {
        return 0.0;
    }

    let distance = levenshtein(&a, &b);
    let longest = a.chars().count().max(b.chars().count());
    1.0 - (distance as f32 / longest as f32)
}

/// Standard two-row Levenshtein. Bounded by [`bounds::MAX_NAME_LEN_FOR_SIMILARITY`]
/// at the call site, so the quadratic term is capped at 64×64.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}
