//! Domain models. These cross the IPC boundary so field names are
//! deliberately snake_case for sqlx and serde_camelCase is *not* applied —
//! the TS layer adapts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub release_year: Option<i64>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub cover_path: Option<String>,
    pub hero_path: Option<String>,
    pub icon_path: Option<String>,
    pub metadata_json: Option<String>,
    pub metadata_source: Option<String>,
    pub is_favorite: i64,
    pub is_hidden: i64,
    pub completion_pct: f64,
    pub completion_state: String,
    pub user_rating: Option<i64>,
    pub user_notes: Option<String>,
    pub total_playtime_seconds: i64,
    pub last_played_at: Option<String>,
    pub added_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Installation {
    pub id: String,
    pub game_id: String,
    pub source_id: i64,
    pub install_dir: String,
    pub executable: Option<String>,
    pub launch_args: Option<String>,
    pub source_app_id: Option<String>,
    pub install_size_bytes: Option<i64>,
    pub is_primary: i64,
    pub detected_at: String,
    /// 1 if the executable was set manually by the user; scanner will not
    /// overwrite it on subsequent rescans.
    pub executable_override: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Achievement {
    pub id: String,
    pub game_id: String,
    pub template_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon_path: Option<String>,
    pub points: i64,
    pub is_secret: i64,
    pub is_unlocked: i64,
    pub unlocked_at: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SaveProfile {
    pub id: String,
    pub game_id: String,
    pub label: String,
    pub source_dir: String,
    pub glob: Option<String>,
    pub auto_backup: i64,
    pub created_at: String,
    /// 1 if the user manually chose this save directory; 0 if auto-detected.
    /// Re-detection will not overwrite a manual override.
    pub is_manual_override: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SaveBackup {
    pub id: i64,
    pub profile_id: String,
    pub archive_path: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PlaySession {
    pub id: i64,
    pub game_id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_seconds: i64,
    pub idle_seconds: i64,
    pub process_name: Option<String>,
}

/// Helper for fresh RFC3339 timestamps. Kept in one place so we never
/// stringify times two different ways.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

#[allow(dead_code)]
pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}
