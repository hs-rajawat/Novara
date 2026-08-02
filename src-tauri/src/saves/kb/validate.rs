//! Entry validation, shared by every layer.
//!
//! **The same function validates the built-in KB, a user entry, and (in Phase 8) a
//! community payload.** That is deliberate: a validation path that only runs for
//! untrusted input eventually diverges from the one that runs for trusted input,
//! and then the trusted path ships something the untrusted path would have caught.
//! There is one gate.
//!
//! Validation is the load-bearing control that ADR-0008 relies on when it defers
//! payload signing: a KB that cannot express a traversal, cannot name an absolute
//! path, and cannot use an unknown variable is bounded to "suggests a useless
//! path".

use crate::db::save_kb::NewKbEntry;

use super::template::{self, TemplateError};

/// Layers a KB entry may belong to.
pub const LAYERS: [&str; 3] = ["builtin", "community", "user"];
/// Identity kinds an entry may match on. Mirrors the migration's CHECK.
pub const MATCH_KINDS: [&str; 6] = [
    "steam_appid",
    "gog_id",
    "epic_id",
    "exe_name",
    "title_norm",
    "any",
];
pub const PLATFORMS: [&str; 3] = ["windows", "linux", "macos"];
pub const ROLES: [&str; 3] = ["saves", "config", "screenshots"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryError {
    EmptyId,
    UnknownLayer(String),
    UnknownMatchKind(String),
    UnknownPlatform(String),
    UnknownRole(String),
    /// A keyed entry with nothing to key on.
    MissingMatchValue(String),
    /// `any` carries no value, so a value would be silently ignored.
    UnexpectedMatchValue,
    /// `title_norm` / `exe_name` values must be stored pre-normalised or a lookup
    /// can never hit. See `KNOWLEDGE_BASE.md` §4.1.
    NotNormalised { kind: String, given: String, expected: String },
    BadTemplate(TemplateError),
}

impl std::fmt::Display for EntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryError::EmptyId => write!(f, "entry id is empty"),
            EntryError::UnknownLayer(v) => write!(f, "unknown layer `{v}`"),
            EntryError::UnknownMatchKind(v) => write!(f, "unknown match_kind `{v}`"),
            EntryError::UnknownPlatform(v) => write!(f, "unknown platform `{v}`"),
            EntryError::UnknownRole(v) => write!(f, "unknown role `{v}`"),
            EntryError::MissingMatchValue(k) => {
                write!(f, "match_kind `{k}` needs a match_value")
            }
            EntryError::UnexpectedMatchValue => write!(
                f,
                "match_kind `any` must have an empty match_value — a value here would \
                 be silently ignored"
            ),
            EntryError::NotNormalised { kind, given, expected } => write!(
                f,
                "match_value for `{kind}` must be stored normalised: got `{given}`, \
                 expected `{expected}` (see KNOWLEDGE_BASE.md §4.1)"
            ),
            EntryError::BadTemplate(e) => write!(f, "path_template: {e}"),
        }
    }
}

/// Validate one entry for a given layer.
///
/// Rejects rather than repairs. A near-miss — an unnormalised `title_norm`, say — is
/// an authoring mistake worth surfacing, and quietly fixing it would hide the same
/// mistake in a contributed payload.
pub fn validate_entry(layer: &str, entry: &NewKbEntry) -> Result<(), EntryError> {
    if entry.id.trim().is_empty() {
        return Err(EntryError::EmptyId);
    }
    if !LAYERS.contains(&layer) {
        return Err(EntryError::UnknownLayer(layer.to_string()));
    }
    if !MATCH_KINDS.contains(&entry.match_kind.as_str()) {
        return Err(EntryError::UnknownMatchKind(entry.match_kind.clone()));
    }
    if !PLATFORMS.contains(&entry.platform.as_str()) {
        return Err(EntryError::UnknownPlatform(entry.platform.clone()));
    }
    if !ROLES.contains(&entry.role.as_str()) {
        return Err(EntryError::UnknownRole(entry.role.clone()));
    }

    if entry.match_kind == "any" {
        if !entry.match_value.is_empty() {
            return Err(EntryError::UnexpectedMatchValue);
        }
    } else if entry.match_value.trim().is_empty() {
        return Err(EntryError::MissingMatchValue(entry.match_kind.clone()));
    }

    // Derived keys must already be in the form a lookup will use.
    let expected = match entry.match_kind.as_str() {
        "title_norm" => Some(super::normalise_title(&entry.match_value)),
        "exe_name" => Some(super::normalise_exe(&entry.match_value)),
        _ => None,
    };
    if let Some(expected) = expected {
        if expected != entry.match_value {
            return Err(EntryError::NotNormalised {
                kind: entry.match_kind.clone(),
                given: entry.match_value.clone(),
                expected,
            });
        }
    }

    template::validate(&entry.path_template).map_err(EntryError::BadTemplate)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_entry() -> NewKbEntry {
        NewKbEntry {
            id: "builtin:test".into(),
            match_kind: "steam_appid".into(),
            match_value: "367520".into(),
            platform: "windows".into(),
            role: "saves".into(),
            layout: crate::saves::kb::layout::OFFICIAL.into(),
            path_template: "{MYGAMES}/{TITLE}".into(),
            glob: None,
            priority: 100,
            note: None,
            source_ref: Some("test".into()),
        }
    }

    #[test]
    fn a_well_formed_entry_is_accepted() {
        assert!(validate_entry("builtin", &ok_entry()).is_ok());
    }

    #[test]
    fn an_empty_id_is_refused() {
        let mut e = ok_entry();
        e.id = "  ".into();
        assert_eq!(validate_entry("builtin", &e), Err(EntryError::EmptyId));
    }

    #[test]
    fn unknown_enumerations_are_refused() {
        let mut layer = ok_entry();
        assert_eq!(
            validate_entry("vibes", &layer),
            Err(EntryError::UnknownLayer("vibes".into()))
        );

        layer.match_kind = "horoscope".into();
        assert_eq!(
            validate_entry("builtin", &layer),
            Err(EntryError::UnknownMatchKind("horoscope".into()))
        );

        let mut platform = ok_entry();
        platform.platform = "beos".into();
        assert_eq!(
            validate_entry("builtin", &platform),
            Err(EntryError::UnknownPlatform("beos".into()))
        );

        let mut role = ok_entry();
        role.role = "mods".into();
        assert_eq!(
            validate_entry("builtin", &role),
            Err(EntryError::UnknownRole("mods".into()))
        );
    }

    #[test]
    fn a_keyed_entry_needs_a_value() {
        let mut e = ok_entry();
        e.match_value = "".into();
        assert_eq!(
            validate_entry("builtin", &e),
            Err(EntryError::MissingMatchValue("steam_appid".into()))
        );
    }

    #[test]
    fn an_any_entry_must_not_carry_a_value() {
        let mut e = ok_entry();
        e.match_kind = "any".into();
        e.match_value = "something".into();
        assert_eq!(
            validate_entry("builtin", &e),
            Err(EntryError::UnexpectedMatchValue)
        );

        e.match_value = "".into();
        assert!(validate_entry("builtin", &e).is_ok());
    }

    /// The mistake this catches is silent in production: an unnormalised value
    /// simply never matches, and the entry looks present but does nothing.
    #[test]
    fn a_title_norm_value_must_already_be_normalised() {
        let mut e = ok_entry();
        e.match_kind = "title_norm".into();
        e.match_value = "Hollow Knight".into();

        match validate_entry("builtin", &e) {
            Err(EntryError::NotNormalised { expected, .. }) => {
                assert_eq!(expected, "hollowknight")
            }
            other => panic!("expected a normalisation error, got {other:?}"),
        }

        e.match_value = "hollowknight".into();
        assert!(validate_entry("builtin", &e).is_ok());
    }

    #[test]
    fn an_exe_name_value_must_already_be_normalised() {
        let mut e = ok_entry();
        e.match_kind = "exe_name".into();
        e.match_value = "HollowKnight.exe".into();
        assert!(matches!(
            validate_entry("builtin", &e),
            Err(EntryError::NotNormalised { .. })
        ));

        e.match_value = "hollowknight".into();
        assert!(validate_entry("builtin", &e).is_ok());
    }

    /// Template rejection is inherited wholesale, so a KB entry can never express
    /// something the template module refuses.
    #[test]
    fn template_rejection_applies_to_entries() {
        for bad in [
            "C:/Windows/System32",
            "{APPDATA}/../../Windows",
            "{NONSENSE}/x",
            "{TITLE}/saves",
            "",
        ] {
            let mut e = ok_entry();
            e.path_template = bad.into();
            assert!(
                matches!(validate_entry("builtin", &e), Err(EntryError::BadTemplate(_))),
                "should refuse template: {bad}"
            );
        }
    }

    /// Identical validation for every layer is the property this module exists for.
    #[test]
    fn every_layer_is_held_to_the_same_rules() {
        let mut e = ok_entry();
        e.path_template = "{APPDATA}/../escape".into();
        for layer in LAYERS {
            assert!(
                matches!(validate_entry(layer, &e), Err(EntryError::BadTemplate(_))),
                "layer `{layer}` must reject a traversal template just like the others"
            );
        }
    }
}
