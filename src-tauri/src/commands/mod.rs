//! Tauri command handlers. Pattern: each module owns a coherent set
//! of commands (games, scan, achievements, saves, playtime, analytics).
//! Handlers are deliberately thin — they parse inputs, call a service,
//! return a serializable result. No business logic lives here.

pub mod achievements;
pub mod analytics;
pub mod games;
pub mod playtime;
pub mod saves;
pub mod scan;
pub mod system;
