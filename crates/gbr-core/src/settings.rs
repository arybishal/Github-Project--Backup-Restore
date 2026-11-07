use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::Row;

use crate::db::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub backup_root: String,
    pub default_mode: String,
    pub default_format: String,
    pub compression_level: u32,
    pub snapshot_retention: u32,
    pub git_path: String,
    pub github_client_id: String,
    pub log_level: String,
    pub notifications: bool,
    pub encrypt_backups: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backup_root: String::new(), // empty = use AppPaths::default_backup_root
            default_mode: "full".into(),
            default_format: "folder".into(),
            compression_level: 6,
            snapshot_retention: 10,
            git_path: "git".into(),
            github_client_id: String::new(), // user-supplied OAuth app id, never a secret
            log_level: "info".into(),
            notifications: true,
            encrypt_backups: false,
        }
    }
}

impl Settings {
    pub async fn load(db: &Db) -> Result<Self> {
        let rows = sqlx::query("SELECT key, value FROM application_settings")
            .fetch_all(db.pool())
            .await?;
        let mut s = Settings::default();
        for row in rows {
            let key: String = row.try_get("key")?;
            let value: String = row.try_get("value")?;
            match key.as_str() {
                "backup_root" => s.backup_root = value,
                "default_mode" => s.default_mode = value,
                "default_format" => s.default_format = value,
                "compression_level" => s.compression_level = value.parse().unwrap_or(6),
                "snapshot_retention" => s.snapshot_retention = value.parse().unwrap_or(10),
                "git_path" => s.git_path = value,
                "github_client_id" => s.github_client_id = value,
                "log_level" => s.log_level = value,
                "notifications" => s.notifications = value == "true",
                "encrypt_backups" => s.encrypt_backups = value == "true",
                _ => {}
            }
        }
        Ok(s)
    }

    pub async fn save(&self, db: &Db) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let pairs: Vec<(&str, String)> = vec![
            ("backup_root", self.backup_root.clone()),
            ("default_mode", self.default_mode.clone()),
            ("default_format", self.default_format.clone()),
            ("compression_level", self.compression_level.to_string()),
            ("snapshot_retention", self.snapshot_retention.to_string()),
            ("git_path", self.git_path.clone()),
            ("github_client_id", self.github_client_id.clone()),
            ("log_level", self.log_level.clone()),
            ("notifications", self.notifications.to_string()),
            ("encrypt_backups", self.encrypt_backups.to_string()),
        ];
        for (key, value) in pairs {
            sqlx::query(
                "INSERT INTO application_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(value)
            .bind(&now)
            .execute(db.pool())
            .await?;
        }
        Ok(())
    }
}

/// Store any serialisable value as a single settings row (used by the CLI for
/// small ad-hoc state such as last used destination).
pub async fn set_raw<T: Serialize>(db: &Db, key: &str, value: &T) -> Result<()> {
    let json = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO application_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(json)
    .bind(&now)
    .execute(db.pool())
    .await?;
    Ok(())
}

pub async fn get_raw<T: DeserializeOwned>(db: &Db, key: &str) -> Result<Option<T>> {
    let row: Option<String> = sqlx::query("SELECT value FROM application_settings WHERE key = ?1")
        .bind(key)
        .fetch_optional(db.pool())
        .await?
        .map(|r| r.try_get::<String, _>("value").unwrap_or_default());
    match row {
        Some(v) => Ok(Some(serde_json::from_str(&v)?)),
        None => Ok(None),
    }
}
