//! The user knowledge base layer.
//!
//! A user correction is the highest-authority statement about where a game keeps its
//! saves — they can see the folder, NOVARA is guessing. So the `user` layer sorts
//! above `community` and `builtin` in [`Db::match_kb_entries`].
//!
//! Three properties this module exists to hold:
//!
//! 1. **Validation is identical to the built-in layer.** Same
//!    [`validate_entry`](super::validate::validate_entry), no relaxations. A user is
//!    not more trusted than the shipped corpus, because "the user" is really "an
//!    entry that arrived through an IPC command" — and in Phase 8 the same shape
//!    arrives from a community payload.
//! 2. **Entries survive a built-in refresh** (invariant I7). Guaranteed by the
//!    layer-scoped delete in `replace_kb_layer`; asserted here from the user side.
//! 3. **No network.** A user entry is authored locally and stays local. Sharing is
//!    Phase 8 and requires explicit consent.

use crate::db::save_kb::NewKbEntry;
use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::validate::validate_entry;

pub const LAYER: &str = "user";

/// What a caller may supply. Narrower than [`NewKbEntry`]: the layer is not a
/// parameter, so no command can write into `builtin` by naming it.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UserEntryInput {
    pub match_kind: String,
    #[serde(default)]
    pub match_value: String,
    #[serde(default)]
    pub role: Option<String>,
    pub path_template: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// Deterministic id for a user entry.
///
/// Derived from the entry's content rather than random, so adding the same
/// correction twice is caught by the primary key instead of silently producing two
/// identical entries that both match.
fn user_entry_id(input: &UserEntryInput) -> String {
    let material = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        input.match_kind,
        input.match_value,
        input.role.as_deref().unwrap_or("saves"),
        input.path_template
    );
    let digest = ring::digest::digest(&ring::digest::SHA256, material.as_bytes());
    let hex: String = digest.as_ref()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("user:{hex}")
}

/// Validate and store a user entry.
///
/// Rejects before touching the database, so a refused entry leaves no trace and the
/// error names the specific problem rather than surfacing a constraint violation.
pub async fn add_user_entry(db: &Db, input: &UserEntryInput) -> AppResult<String> {
    let entry = NewKbEntry {
        id: user_entry_id(input),
        match_kind: input.match_kind.trim().to_string(),
        match_value: input.match_value.trim().to_string(),
        platform: "windows".into(),
        role: input.role.clone().unwrap_or_else(|| "saves".into()),
        // A location the user typed in is a user-defined layout by definition, and that
        // is what earns it curated authority in the decision table. The caller does not
        // get to choose -- a layout an untrusted payload could set would be a way to
        // grant itself binding power.
        layout: super::layout::USER_DEFINED.into(),
        path_template: input.path_template.trim().to_string(),
        glob: None,
        // Below the built-in curated band (10) so a user entry wins outright.
        priority: 1,
        note: input.note.clone(),
        source_ref: Some("user".into()),
    };

    // The same gate the shipped corpus passes through.
    validate_entry(LAYER, &entry).map_err(|e| AppError::Invalid(e.to_string()))?;

    if db.kb_entry(&entry.id).await?.is_some() {
        return Err(AppError::Invalid(
            "this knowledge-base entry already exists".into(),
        ));
    }

    db.add_kb_entry(LAYER, &entry).await?;
    Ok(entry.id)
}

/// Remove a user entry.
///
/// Scoped to the user layer so the command cannot be used to delete shipped entries
/// — that would let a caller quietly disable detection for a title.
pub async fn remove_user_entry(db: &Db, id: &str) -> AppResult<()> {
    match db.kb_entry(id).await? {
        Some(e) if e.layer == LAYER => db.delete_kb_entry(id).await,
        Some(_) => Err(AppError::Invalid(
            "only user knowledge-base entries can be removed".into(),
        )),
        None => Err(AppError::NotFound(format!("kb entry `{id}`"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saves::kb::builtin;
    use crate::test_support::test_db;

    fn input(template: &str) -> UserEntryInput {
        UserEntryInput {
            match_kind: "title_norm".into(),
            match_value: "mygame".into(),
            role: None,
            path_template: template.into(),
            note: Some("found it myself".into()),
        }
    }

    #[tokio::test]
    async fn a_valid_entry_is_stored_in_the_user_layer() {
        let db = test_db().await;
        let id = add_user_entry(&db, &input("{MYGAMES}/{TITLE}")).await.unwrap();

        let stored = db.kb_entry(&id).await.unwrap().expect("entry should exist");
        assert_eq!(stored.layer, "user");
        assert_eq!(stored.source_ref.as_deref(), Some("user"));
        assert_eq!(stored.note.as_deref(), Some("found it myself"));
    }

    /// The point of the module: no relaxation for locally authored entries.
    #[tokio::test]
    async fn user_entries_face_the_same_validation_as_the_builtin_corpus() {
        let db = test_db().await;
        for bad in [
            "C:/Windows/System32",
            "{APPDATA}/../../Windows",
            "{NONSENSE}/x",
            "{TITLE}/saves",
            "//server/share",
            "",
        ] {
            let err = add_user_entry(&db, &input(bad)).await;
            assert!(err.is_err(), "should refuse template: {bad}");
        }
        assert_eq!(
            db.count_kb_entries(LAYER).await.unwrap(),
            0,
            "a refused entry must leave no trace"
        );
    }

    #[tokio::test]
    async fn an_unnormalised_key_is_refused_with_a_useful_message() {
        let db = test_db().await;
        let mut i = input("{MYGAMES}/{TITLE}");
        i.match_value = "My Game".into();

        let err = add_user_entry(&db, &i).await.unwrap_err().to_string();
        assert!(
            err.contains("mygame"),
            "the error should name the expected form, got: {err}"
        );
    }

    /// A user entry must outrank everything shipped, or a correction would not
    /// correct anything.
    #[tokio::test]
    async fn a_user_entry_outranks_the_builtin_corpus() {
        let db = test_db().await;
        builtin::load(&db).await.unwrap().unwrap();
        add_user_entry(&db, &input("{MYGAMES}/Somewhere Else")).await.unwrap();

        let keys = vec![crate::db::save_kb::MatchKey::new("title_norm", "mygame")];
        let matched = db.match_kb_entries("windows", "saves", &keys).await.unwrap();

        assert_eq!(
            matched.first().map(|e| e.layer.as_str()),
            Some("user"),
            "the user layer must sort first"
        );
    }

    #[tokio::test]
    async fn adding_the_same_correction_twice_is_refused() {
        let db = test_db().await;
        add_user_entry(&db, &input("{MYGAMES}/{TITLE}")).await.unwrap();
        assert!(add_user_entry(&db, &input("{MYGAMES}/{TITLE}")).await.is_err());
        assert_eq!(db.count_kb_entries(LAYER).await.unwrap(), 1);
    }

    /// Invariant I7 from the user's side: a KB refresh must not lose corrections.
    #[tokio::test]
    async fn user_entries_survive_a_builtin_replacement() {
        let db = test_db().await;
        let id = add_user_entry(&db, &input("{MYGAMES}/{TITLE}")).await.unwrap();

        builtin::load(&db).await.unwrap().unwrap();
        db.replace_kb_layer("builtin", "next", "different-checksum", None, &[])
            .await
            .unwrap();

        assert!(
            db.kb_entry(&id).await.unwrap().is_some(),
            "a builtin replacement must not remove a user entry (I7)"
        );
    }

    #[tokio::test]
    async fn a_user_entry_can_be_removed() {
        let db = test_db().await;
        let id = add_user_entry(&db, &input("{MYGAMES}/{TITLE}")).await.unwrap();
        remove_user_entry(&db, &id).await.unwrap();
        assert!(db.kb_entry(&id).await.unwrap().is_none());
    }

    /// Removal is layer-scoped, so the command cannot be turned into a way to
    /// disable shipped detection.
    #[tokio::test]
    async fn a_builtin_entry_cannot_be_removed_through_the_user_command() {
        let db = test_db().await;
        builtin::load(&db).await.unwrap().unwrap();
        let before = db.count_kb_entries("builtin").await.unwrap();

        let shipped = db.kb_entry("builtin:celeste").await.unwrap().expect("shipped entry");
        assert!(remove_user_entry(&db, &shipped.id).await.is_err());
        assert_eq!(db.count_kb_entries("builtin").await.unwrap(), before);
    }

    #[tokio::test]
    async fn removing_an_unknown_entry_is_not_found() {
        let db = test_db().await;
        assert!(remove_user_entry(&db, "user:nope").await.is_err());
    }
}
