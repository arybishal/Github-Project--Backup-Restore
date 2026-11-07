use serde::Serialize;
use sqlx::Row;

use crate::auth::AuthInfo;
use crate::backup::{self, BackupRecordDto, StorageInfoDto};
use crate::db::Db;
use crate::error::Result;
use crate::manifest::StorageLayout;
use crate::repo_store;

#[derive(Debug, Clone, Serialize, Default)]
pub struct BackupHealth {
    pub ok: i64,
    pub unverified: i64,
    pub failed: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub account: Option<AuthInfo>,
    pub repos: repo_store::RepoCounts,
    pub total_backups: i64,
    pub verified_backups: i64,
    pub failed_backups: i64,
    pub last_success: Option<String>,
    pub storage_bytes: u64,
    pub pending_tasks: i64,
    pub recent_activity: Vec<crate::activity::ActivityEntry>,
    pub recent_backups: Vec<BackupRecordDto>,
    pub health: BackupHealth,
}

/// Everything the Dashboard page shows, in one round trip. Every number is
/// measured from the database or from disk — nothing is hardcoded.
pub async fn stats(db: &Db, layout: &StorageLayout) -> Result<DashboardStats> {
    let account = crate::auth::current_account(db).await?;
    let repos = repo_store::counts(db).await?;

    let totals = sqlx::query(
        "SELECT COUNT(*) as total,
                COALESCE(SUM(CASE WHEN verified = 1 THEN 1 ELSE 0 END), 0) as verified,
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as failed,
                MAX(CASE WHEN verified = 1 THEN created_at END) as last_success
         FROM backup_records",
    )
    .fetch_one(db.pool())
    .await?;

    let total_backups: i64 = totals.try_get("total").unwrap_or(0);
    let verified_backups: i64 = totals.try_get("verified").unwrap_or(0);
    let failed_backups: i64 = totals.try_get("failed").unwrap_or(0);
    let last_success: Option<String> = totals.try_get("last_success").unwrap_or(None);

    // Health splits verified / unverified-but-complete / failed.
    let health = BackupHealth {
        ok: verified_backups,
        unverified: (total_backups - verified_backups - failed_backups).max(0),
        failed: failed_backups,
    };

    let storage: StorageInfoDto = backup::storage_info(db, layout).await?;
    let pending_tasks = task_pending(db).await;
    let recent_activity = crate::activity::list(db, None, None, 8).await?;
    let recent_backups = backup::list_records(db, 6).await?;

    Ok(DashboardStats {
        account,
        repos,
        total_backups,
        verified_backups,
        failed_backups,
        last_success,
        storage_bytes: storage.total_bytes,
        pending_tasks,
        recent_activity,
        recent_backups,
        health,
    })
}

async fn task_pending(db: &Db) -> i64 {
    sqlx::query("SELECT COUNT(*) as c FROM task_history WHERE status IN ('running','queued')")
        .fetch_one(db.pool())
        .await
        .map(|r| r.try_get::<i64, _>("c").unwrap_or(0))
        .unwrap_or(0)
}
