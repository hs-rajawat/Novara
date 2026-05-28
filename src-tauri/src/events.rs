//! Tokio broadcast bus. Services emit events; the Tauri layer forwards
//! anything relevant to the frontend over the Tauri event system, and
//! services can subscribe to react to each other (e.g. save_mgr watches
//! play_session.ended to take an auto-backup).

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    ScanStarted { source: String },
    ScanFinished { source: String, added: u32, found: u32 },
    GameAdded { game_id: String, title: String },
    GameUpdated { game_id: String },
    AchievementUnlocked { game_id: String, achievement_id: String, points: i64 },
    SessionStarted { game_id: String, session_id: i64 },
    SessionEnded { game_id: String, session_id: i64, duration_seconds: i64 },
    SaveBackupCreated { profile_id: String, backup_id: i64 },
    Notice { level: NoticeLevel, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AppEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn emit(&self, ev: AppEvent) {
        // Lagging subscribers shouldn't crash producers. Drop is fine —
        // the frontend will resync on focus.
        let _ = self.tx.send(ev);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.tx.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}
