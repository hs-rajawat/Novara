//! Save layouts: what *kind* of location a knowledge-base entry describes.
//!
//! A game does not have one save location, it has several possible ones, and they are
//! not equivalent claims:
//!
//! | Layout | The claim it makes |
//! |---|---|
//! | [`OFFICIAL`] | This is where the game as shipped writes saves |
//! | [`ENGINE`] | This is where games built on this engine write saves |
//! | [`OS`] | This is a conventional location on this operating system |
//! | [`LAUNCHER`] | This is where this storefront keeps save data |
//! | [`COMMUNITY`] | Installs using a particular save-redirection layer write here |
//! | [`PORTABLE`] | Self-contained installs write beside the executable |
//! | [`USER_DEFINED`] | The user told us |
//!
//! ## Two orthogonal axes
//!
//! Layout is **not** the same thing as `layer`, and conflating them was the defect this
//! module exists to fix:
//!
//! * `layer` — *who authored the entry*: `builtin`, `community`, `user`. Provenance.
//! * `layout` — *what sort of location it names*. The nature of the claim.
//!
//! A shipped built-in entry can perfectly well describe a community layout. Before this
//! distinction existed, the only proxy for layout was `match_kind != 'any'` ("keyed"),
//! which is a matching mechanism. A community layout entered as a keyed built-in entry
//! therefore satisfied decision table row 5 and bound with exactly the authority of the
//! official path.
//!
//! ## Adding a layout is data; granting it authority is code
//!
//! `layout` is a free-form string with no CHECK constraint, so a corpus update or a
//! community contribution can introduce a new layout kind without a migration and
//! without touching Rust. [`authority`] maps known layouts to a tier and returns
//! [`Authority::Advisory`] — the lower tier — for anything it does not recognise.
//!
//! That asymmetry is deliberate and is the security boundary. If authority were
//! declared *by* the data, a contributed entry could mark itself `official` and acquire
//! the right to bind automatically. Instead:
//!
//! * `layer` is set by the loader, never by the payload (`replace_kb_layer` takes it as
//!   a parameter; `add_user_entry` hardcodes it), so an entry cannot choose its own
//!   provenance.
//! * `layout` is chosen freely by the data but only *classified* here.
//! * An unrecognised layout is under-trusted, never over-trusted.
//!
//! Promoting a layout to [`Authority::Curated`] is one line in [`CURATED_LAYOUTS`] and a
//! deliberate review, which is the right weight for a privilege decision.
//!
//! ## The resolver never reads a layout name
//!
//! [`crate::saves::resolver`] is expressed over [`Authority`], not over the strings
//! below. A new layout flows through the existing decision rows with no new row and no
//! code change — which is what keeps "support a new save layout" a data task.

/// Where the game as shipped writes its saves.
pub const OFFICIAL: &str = "official";
/// A convention of the engine the game was built on — Unreal, Unity.
pub const ENGINE: &str = "engine";
/// A convention of the operating system — the Windows known folders.
pub const OS: &str = "os";
/// A convention of the storefront rather than the game.
pub const LAUNCHER: &str = "launcher";
/// A location used by a save-redirection layer that wraps many games.
///
/// Described by directory, never by whoever produced the layer. A path is a filesystem
/// fact; attributing it is neither NOVARA's business nor necessary to back up a save.
pub const COMMUNITY: &str = "community";
/// Beside the executable, for self-contained installs.
pub const PORTABLE: &str = "portable";
/// The user said so.
pub const USER_DEFINED: &str = "user_defined";
/// No classification recorded. The default for rows written before layouts existed.
pub const UNSPECIFIED: &str = "unspecified";

/// How much a layout is trusted to settle the question on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    /// A statement about *this game's* real save location. Enough to bind, given a
    /// keyed match from a trusted layer and a path that exists.
    Curated,
    /// A statement about a *class* of installs. Whether this install is a member of
    /// that class is unknown until something corroborates it, so an advisory layout
    /// suggests and is promoted by content, mtime correlation or a write witness.
    Advisory,
}

/// Layouts that may settle the question alone.
///
/// Short on purpose, and every addition is a privilege grant. `OFFICIAL` qualifies
/// because it describes the game itself; `USER_DEFINED` qualifies because §5.3 ranks the
/// user terminal — they can see the folder, NOVARA is inferring.
///
/// Everything else describes a *class* of installs. An engine convention says "Unreal
/// games write here", which is true and still does not establish that this particular
/// directory belongs to this particular game.
const CURATED_LAYOUTS: &[&str] = &[OFFICIAL, USER_DEFINED];

/// Classify a layout.
///
/// Unrecognised layouts are [`Authority::Advisory`]. This is the whole basis of
/// "new layouts are data": an unknown value is usable immediately and safe by default.
pub fn authority(layout: &str) -> Authority {
    if CURATED_LAYOUTS.contains(&layout) {
        Authority::Curated
    } else {
        Authority::Advisory
    }
}

/// Every layout this build knows how to describe, for display and for tests.
///
/// Not a validation list — a layout absent from here is still accepted as data.
pub const KNOWN: &[&str] = &[
    OFFICIAL,
    ENGINE,
    OS,
    LAUNCHER,
    COMMUNITY,
    PORTABLE,
    USER_DEFINED,
    UNSPECIFIED,
];

/// A short phrase for the explanation shown to a user.
///
/// Falls back to the raw layout string, so a layout added as data still produces a
/// readable sentence rather than an empty one (invariant I9).
pub fn describe(layout: &str) -> &str {
    match layout {
        OFFICIAL => "the game's own save location",
        ENGINE => "a location conventional for this game engine",
        OS => "a conventional Windows save location",
        LAUNCHER => "a location this storefront uses",
        COMMUNITY => "an alternative layout some installs use",
        PORTABLE => "a folder beside the game's executable",
        USER_DEFINED => "a location you chose",
        UNSPECIFIED => "a known save location",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_and_user_layouts_may_settle_the_question() {
        assert_eq!(authority(OFFICIAL), Authority::Curated);
        assert_eq!(authority(USER_DEFINED), Authority::Curated);
    }

    /// Every layout that describes a *class* of installs is advisory, because membership
    /// of that class is exactly what is unknown.
    #[test]
    fn class_describing_layouts_are_advisory() {
        for layout in [ENGINE, OS, LAUNCHER, COMMUNITY, PORTABLE, UNSPECIFIED] {
            assert_eq!(
                authority(layout),
                Authority::Advisory,
                "`{layout}` describes a class of installs and must not bind alone"
            );
        }
    }

    /// **The security property.** A layout nobody has reviewed cannot acquire binding
    /// authority by appearing in data. This is what makes a free-form column safe.
    #[test]
    fn an_unknown_layout_is_advisory_never_curated() {
        for invented in [
            "official ",           // trailing space
            "Official",            // different case
            "official_but_better", // wishful
            "user_defined2",
            "",
            "😀",
            "'; DROP TABLE save_kb_entries; --",
        ] {
            assert_eq!(
                authority(invented),
                Authority::Advisory,
                "`{invented}` must not be treated as curated"
            );
        }
    }

    /// Adding a layout must not require touching this module. A value it has never seen
    /// is usable immediately — it simply lands in the conservative tier.
    #[test]
    fn a_layout_this_build_has_never_seen_is_still_usable() {
        let future = "emulator_state_slot";
        assert!(!KNOWN.contains(&future));
        assert_eq!(authority(future), Authority::Advisory);
        assert_eq!(
            describe(future), future,
            "an unknown layout must still produce a non-empty description"
        );
    }

    #[test]
    fn every_known_layout_has_a_description() {
        for layout in KNOWN {
            assert!(
                !describe(layout).trim().is_empty(),
                "`{layout}` has no description"
            );
            assert_ne!(
                describe(layout), *layout,
                "`{layout}` should have a phrase, not just its own name"
            );
        }
    }

    /// The curated list is a privilege list. If it grows, that should be a visible
    /// change rather than a quiet one.
    #[test]
    fn the_curated_list_stays_small() {
        assert_eq!(
            CURATED_LAYOUTS.len(),
            2,
            "adding a curated layout grants binding authority — is that intended?"
        );
    }
}
