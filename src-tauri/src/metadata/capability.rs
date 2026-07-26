//! Provider capability fingerprinting.
//!
//! # The problem this solves
//!
//! The artwork fill loop needs a terminal state, or it re-runs the whole provider
//! chain for every game on every scan — that was the defect Batch 5 fixed with
//! `skipped`. But "terminal" then hid two architecturally different conclusions:
//!
//! * **Unsupported** — no registered provider is *capable* of resolving this
//!   game's identity. Epic and manually-imported games are in exactly this
//!   position: `steam_local` and `steam_cdn` both require a Steam app-id, and
//!   `epic_catalog` is a deliberate stub, so every provider returns
//!   `Lookup::Unsupported`. Nothing was looked up. There is nobody to ask.
//! * **Not found** — a provider that could answer did answer, definitively.
//!
//! Neither should be retried on a timer. Retrying an unsupported game every few
//! hours spends work rediscovering a fact that cannot change until the *code*
//! changes. But the code does change — and when it does, those conclusions must be
//! revisited without asking the user to repair their database.
//!
//! # The mechanism
//!
//! A settled slot records a fingerprint of the provider set that settled it.
//! Eligibility is then a comparison, not a timer: a `skipped` slot is terminal
//! only while its fingerprint still matches the current provider set. Adding a
//! provider, removing one, or bumping [`CAPABILITY_EPOCH`] changes the
//! fingerprint, and every slot settled under the old set becomes eligible again on
//! the next sweep — automatically, once, with no manual repair.
//!
//! A `NULL` fingerprint (a row settled before this existed) counts as stale, for
//! the same reason: it was settled by an unknown set.
//!
//! # For whoever adds the next provider
//!
//! You do not need to do anything. Registering a provider changes the fingerprint,
//! which re-opens every previously unsupported slot on the next fill. If you
//! change an existing provider's *capability* without changing its code — teaching
//! `steam_cdn` to resolve by title, say — bump [`CAPABILITY_EPOCH`], because the
//! provider set looks identical from the outside and nothing else would notice.

/// Bumped by hand when a provider's capability changes without the set of
/// provider codes changing.
///
/// Increment this when an existing provider gains or loses the ability to resolve
/// something — a new identifier scheme, a new artwork kind, a title-search
/// fallback. Leave it alone when adding or removing a provider, since the codes
/// themselves already change the fingerprint.
pub const CAPABILITY_EPOCH: u32 = 1;

/// A stable identifier for a set of provider codes.
///
/// Sorted so registration order cannot change the fingerprint — order affects
/// which provider wins a kind, not whether the set is *capable*, and a
/// reordering must not needlessly re-open every settled slot.
pub fn fingerprint(codes: impl IntoIterator<Item = &'static str>) -> String {
    let mut codes: Vec<&str> = codes.into_iter().collect();
    codes.sort_unstable();
    codes.dedup();
    format!("e{CAPABILITY_EPOCH}:{}", codes.join(","))
}

/// Whether a slot settled under `settled_by` should be reconsidered.
///
/// `None` — settled before fingerprints existed, so by an unknown set — is stale.
pub fn is_stale(settled_by: Option<&str>, current: &str) -> bool {
    settled_by != Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_independent_of_registration_order() {
        assert_eq!(
            fingerprint(["steam_cdn", "steam_local", "epic_catalog"]),
            fingerprint(["epic_catalog", "steam_cdn", "steam_local"]),
            "reordering providers must not re-open every settled slot"
        );
    }

    #[test]
    fn adding_a_provider_changes_the_fingerprint() {
        let before = fingerprint(["steam_local", "steam_cdn"]);
        let after = fingerprint(["steam_local", "steam_cdn", "steamgriddb"]);
        assert_ne!(before, after);
        assert!(is_stale(Some(&before), &after), "old conclusions must reopen");
    }

    #[test]
    fn removing_a_provider_changes_the_fingerprint() {
        let before = fingerprint(["steam_local", "steam_cdn"]);
        let after = fingerprint(["steam_local"]);
        assert!(is_stale(Some(&before), &after));
    }

    #[test]
    fn duplicate_codes_do_not_affect_the_fingerprint() {
        assert_eq!(
            fingerprint(["steam_cdn", "steam_cdn"]),
            fingerprint(["steam_cdn"])
        );
    }

    #[test]
    fn an_unchanged_set_stays_terminal() {
        let current = fingerprint(["steam_local", "steam_cdn", "epic_catalog"]);
        assert!(!is_stale(Some(&current), &current));
    }

    /// Rows predating the column were settled by an unknown provider set, so they
    /// must be reconsidered — this is what re-opens the Epic games already marked
    /// `skipped` in an existing database, with no manual repair.
    #[test]
    fn slots_settled_before_fingerprints_existed_are_stale() {
        assert!(is_stale(None, &fingerprint(["steam_cdn"])));
    }

    #[test]
    fn the_epoch_is_part_of_the_fingerprint() {
        let current = fingerprint(["steam_cdn"]);
        assert!(
            current.starts_with(&format!("e{CAPABILITY_EPOCH}:")),
            "bumping the epoch must change the fingerprint: {current}"
        );
    }
}
