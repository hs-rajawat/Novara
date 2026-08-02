//! SQLite pool + migrations. All persistence flows through `Db`.
//!
//! Design notes:
//!   • We use sqlx with the runtime-tokio-rustls feature and a small
//!     pool (max 4) — SQLite is fundamentally single-writer, more
//!     connections just adds contention.
//!   • Migrations live in /migrations and are run on startup.
//!   • Repositories live in submodules so each table has a typed home
//!     instead of one ever-growing query file.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::AppResult;

pub mod achievements;
pub mod artwork;
pub mod games;
pub mod playtime;
pub mod save_candidates;
pub mod skipped_items;
pub mod save_kb;
pub mod saves;
pub mod settings;
pub mod title_match;

#[cfg(test)]
mod games_tests;
#[cfg(test)]
mod title_match_tests;

#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

impl Db {
    pub async fn open(db_path: &Path) -> AppResult<Self> {
        let url = format!("sqlite://{}", db_path.display());
        let opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}
