use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{now_rfc3339, SaveBackup, SaveProfile};

use super::Db;

impl Db {
    pub async fn list_save_profiles(&self, game_id: &str) -> AppResult<Vec<SaveProfile>> {
        Ok(sqlx::query_as::<_, SaveProfile>(
            "SELECT * FROM save_profiles WHERE game_id = ?1 ORDER BY created_at",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn create_save_profile(
        &self,
        game_id: &str,
        label: &str,
        source_dir: &str,
        glob: Option<&str>,
        auto_backup: bool,
    ) -> AppResult<SaveProfile> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO save_profiles (id, game_id, label, source_dir, glob, auto_backup, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&id)
        .bind(game_id)
        .bind(label)
        .bind(source_dir)
        .bind(glob)
        .bind(auto_backup as i64)
        .bind(now_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(sqlx::query_as::<_, SaveProfile>("SELECT * FROM save_profiles WHERE id = ?1")
            .bind(&id)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn record_backup(
        &self,
        profile_id: &str,
        archive_path: &str,
        size_bytes: i64,
        file_count: i64,
        note: Option<&str>,
    ) -> AppResult<i64> {
        let row: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO save_backups (profile_id, archive_path, size_bytes, file_count, note, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            RETURNING id
            "#,
        )
        .bind(profile_id)
        .bind(archive_path)
        .bind(size_bytes)
        .bind(file_count)
        .bind(note)
        .bind(now_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn list_backups(&self, profile_id: &str) -> AppResult<Vec<SaveBackup>> {
        Ok(sqlx::query_as::<_, SaveBackup>(
            "SELECT * FROM save_backups WHERE profile_id = ?1 ORDER BY created_at DESC",
        )
        .bind(profile_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_save_profile(&self, id: &str) -> AppResult<Option<SaveProfile>> {
        Ok(sqlx::query_as::<_, SaveProfile>("SELECT * FROM save_profiles WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn get_backup(&self, id: i64) -> AppResult<Option<SaveBackup>> {
        Ok(sqlx::query_as::<_, SaveBackup>("SELECT * FROM save_backups WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?)
    }
}
