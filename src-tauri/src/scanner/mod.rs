//! Pluggable game-source scanner.
//!
//!   trait Scanner { code, scan(roots) -> Vec<DetectedGame> }
//!
//! The orchestrator runs scanners in parallel, dedups against the DB,
//! and emits events for the UI to react to.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::db::{games::UpsertGame, Db};
use crate::error::AppResult;
use crate::events::{AppEvent, EventBus};

pub mod manual;
pub mod steam;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedGame {
    pub source_code: String,
    pub title: String,
    pub install_dir: PathBuf,
    pub executable: Option<PathBuf>,
    pub source_app_id: Option<String>,
    pub install_size_bytes: Option<i64>,
}

#[async_trait]
pub trait Scanner: Send + Sync {
    fn code(&self) -> &'static str;

    /// Scan the provided roots (and any source-specific known locations)
    /// for games. May return an empty vec if the source isn't installed.
    async fn scan(&self, roots: &[PathBuf]) -> AppResult<Vec<DetectedGame>>;
}

#[derive(Clone)]
pub struct ScannerOrchestrator {
    db: Db,
    bus: EventBus,
    scanners: Arc<Vec<Box<dyn Scanner>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub source: String,
    pub found: u32,
    pub added: u32,
}

impl ScannerOrchestrator {
    pub fn new(db: Db, bus: EventBus) -> Self {
        let scanners: Vec<Box<dyn Scanner>> = vec![
            Box::new(steam::SteamScanner),
            Box::new(manual::ManualScanner),
            // Stubs for additional sources — add when implemented.
            // Box::new(epic::EpicScanner::default()),
            // Box::new(gog::GogScanner::default()),
            // Box::new(xbox::XboxScanner::default()),
        ];
        Self {
            db,
            bus,
            scanners: Arc::new(scanners),
        }
    }

    pub async fn run(&self, roots: Vec<PathBuf>) -> AppResult<Vec<ScanReport>> {
        let mut reports = Vec::with_capacity(self.scanners.len());
        for scanner in self.scanners.iter() {
            let code = scanner.code();
            self.bus.emit(AppEvent::ScanStarted {
                source: code.to_string(),
            });

            let started = crate::models::now_rfc3339();
            let scan_id: i64 = sqlx::query_scalar(
                "INSERT INTO scan_runs (source_code, started_at) VALUES (?1, ?2) RETURNING id",
            )
            .bind(code)
            .bind(&started)
            .fetch_one(&self.db.pool)
            .await?;

            let result = scanner.scan(&roots).await;

            let mut added = 0u32;
            let mut found = 0u32;
            let mut err: Option<String> = None;

            match result {
                Ok(games) => {
                    found = games.len() as u32;
                    for g in games {
                        // Bind to locals first so the temporary `Cow<str>`
                        // and `String` outlive the borrow we hand to
                        // UpsertGame.
                        let install_dir = g.install_dir.to_string_lossy().into_owned();
                        let exe_str = g
                            .executable
                            .as_ref()
                            .map(|p| p.to_string_lossy().into_owned());
                        match self
                            .db
                            .upsert_game(UpsertGame {
                                title: &g.title,
                                source_code: &g.source_code,
                                source_app_id: g.source_app_id.as_deref(),
                                install_dir: install_dir.as_str(),
                                executable: exe_str.as_deref(),
                                install_size_bytes: g.install_size_bytes,
                            })
                            .await
                        {
                            Ok(r) if r.created => {
                                added += 1;
                                self.bus.emit(AppEvent::GameAdded {
                                    game_id: r.game_id_owned.to_string(),
                                    title: g.title.clone(),
                                });
                            }
                            Ok(_) => {}
                            Err(e) => warn!(error = %e, title = %g.title, "upsert failed"),
                        }
                    }
                }
                Err(e) => {
                    warn!(source = code, error = %e, "scanner failed");
                    err = Some(e.to_string());
                }
            }

            sqlx::query(
                r#"
                UPDATE scan_runs
                SET finished_at = ?1, found_count = ?2, added_count = ?3, error = ?4
                WHERE id = ?5
                "#,
            )
            .bind(crate::models::now_rfc3339())
            .bind(found as i64)
            .bind(added as i64)
            .bind(err)
            .bind(scan_id)
            .execute(&self.db.pool)
            .await?;

            self.bus.emit(AppEvent::ScanFinished {
                source: code.to_string(),
                added,
                found,
            });

            info!(source = code, found, added, "scan complete");
            reports.push(ScanReport {
                source: code.to_string(),
                found,
                added,
            });
        }
        Ok(reports)
    }
}
