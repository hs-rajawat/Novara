//! Key/value settings stored as JSON values. The frontend reads them
//! as `serde_json::Value`, so it can ship whatever shape it needs
//! without us having to migrate the schema.

use serde_json::Value;

use crate::error::AppResult;

use super::Db;

impl Db {
    pub async fn get_setting(&self, key: &str) -> AppResult<Option<Value>> {
        let raw: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(raw.and_then(|(s,)| serde_json::from_str(&s).ok()))
    }

    pub async fn all_settings(&self) -> AppResult<serde_json::Map<String, Value>> {
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT key, value FROM settings").fetch_all(&self.pool).await?;
        let mut map = serde_json::Map::new();
        for (k, v) in rows {
            if let Ok(parsed) = serde_json::from_str::<Value>(&v) {
                map.insert(k, parsed);
            }
        }
        Ok(map)
    }

    pub async fn set_setting(&self, key: &str, value: &Value) -> AppResult<()> {
        let s = serde_json::to_string(value)?;
        sqlx::query(
            r#"
            INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
        )
        .bind(key)
        .bind(s)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
