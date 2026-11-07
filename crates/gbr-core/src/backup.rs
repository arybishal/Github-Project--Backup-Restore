use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::Db;
use crate::error::{GbrError, Result};
use crate::git::GitCli;
use crate::github::Repo;
use crate::manifest::{
    self, BackupManifest, BackupMode, LfsInfo, RefInfo, RepoIdentity, StorageLayout,
    VerificationInfo,
};
use crate::paths::ensure_inside;
use crate::secretscan;
use crate::selective::{self, ArchiveFormat, SelectiveOptions};
use crate::tasks::TaskHandle;

/// Options for a full (git mirror) backup run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullBackupOptions {
    pub include_lfs: bool,
    pub retention: u32,
    /// Create a point-in-time snapshot copy of the mirror (hardlinked).
    pub snapshot: bool,
}

impl Default for FullBackupOptions {
    fn default() -> Self {
        Self {
            include_lfs: true,
            retention: 10,
            snapshot: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupOutcome {
    pub backup_id: String,
    pub repo_full_name: String,
    pub path: String,
    pub size_bytes: u64,
    pub refs: usize,
    pub verified: bool,
    pub lfs_backed_up: bool,
    pub updated_existing: bool,
}

pub struct BackupEngine {
    pub db: Db,
    pub git: GitCli,
    pub layout: StorageLayout,
}

// ---------------------------------------------------------------- records

pub async fn insert_record(
    db: &Db,
    job_id: Option<&str>,
    manifest: &BackupManifest,
    path: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();

    if let Some(jid) = job_id {
        let mode_str = match manifest.mode {
            BackupMode::Full => "full",
            BackupMode::Selective => "selective",
        };
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO backup_jobs (id, mode, status, started_at) VALUES (?1, ?2, 'running', ?3)",
        )
        .bind(jid)
        .bind(mode_str)
        .bind(&now)
        .execute(db.pool())
        .await;
    }

    sqlx::query(
        "INSERT INTO backup_records
         (id, job_id, repo_full_name, repo_github_id, mode, format, path, size_bytes, file_count,
          manifest_path, status, verified, git_lfs, head_sha, default_branch, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'completed',?11,?12,?13,?14,?15)",
    )
    .bind(&manifest.backup_id)
    .bind(job_id)
    .bind(&manifest.repository.full_name)
    .bind(manifest.repository.github_id)
    .bind(match manifest.mode {
        BackupMode::Full => "full",
        BackupMode::Selective => "selective",
    })
    .bind(&manifest.format)
    .bind(path)
    .bind(manifest.size_bytes as i64)
    .bind(manifest.file_count as i64)
    .bind(path)
    .bind(manifest.verification.status == "verified")
    .bind(manifest.lfs.objects_backed_up)
    .bind(&manifest.head_sha)
    .bind(&manifest.repository.default_branch)
    .bind(&now)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO backup_manifests (backup_id, manifest_json, sha256, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(backup_id) DO UPDATE SET manifest_json = excluded.manifest_json,
             sha256 = excluded.sha256",
    )
    .bind(&manifest.backup_id)
    .bind(serde_json::to_string(manifest)?)
    .bind(manifest::sha256_file(std::path::Path::new(path)).unwrap_or_default())
    .bind(&now)
    .execute(db.pool())
    .await?;

    for f in &manifest.files {
        sqlx::query(
            "INSERT INTO backup_files (backup_id, path, size_bytes, sha256) VALUES (?1,?2,?3,?4)",
        )
        .bind(&manifest.backup_id)
        .bind(&f.path)
        .bind(f.size as i64)
        .bind(&f.sha256)
        .execute(db.pool())
        .await?;
    }

    sqlx::query(
        "UPDATE repositories SET last_backup_status = 'ok', last_backup_at = ?2 WHERE full_name = ?1",
    )
    .bind(&manifest.repository.full_name)
    .bind(&now)
    .execute(db.pool())
    .await?;

    Ok(())
}

pub async fn mark_failed(db: &Db, repo_full_name: &str, error: &str) {
    let now = Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query(
        "UPDATE repositories SET last_backup_status = 'failed', last_backup_at = ?2 WHERE full_name = ?1",
    )
    .bind(repo_full_name)
    .bind(&now)
    .execute(db.pool())
    .await
    {
        tracing::warn!("could not mark failed backup: {e}");
    }
    crate::activity::log(
        db,
        crate::activity::LogLevel::Error,
        "backup",
        &format!("backup failed for {repo_full_name}: {error}"),
        None,
    )
    .await;
}

pub async fn record_job_start(db: &Db, job_id: &str, mode: &str, total: usize) {
    let now = Utc::now().to_rfc3339();
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO backup_jobs (id, mode, status, total, started_at) VALUES (?1, ?2, 'running', ?3, ?4)",
    )
    .bind(job_id)
    .bind(mode)
    .bind(total as i64)
    .bind(&now)
    .execute(db.pool())
    .await;
}

pub async fn record_job_finish(db: &Db, job_id: &str, error: Option<&str>) {
    let now = Utc::now().to_rfc3339();
    let status = if error.is_some() {
        "failed"
    } else {
        "completed"
    };
    let _ = sqlx::query(
        "UPDATE backup_jobs SET status = ?2, finished_at = ?3, error = ?4 WHERE id = ?1",
    )
    .bind(job_id)
    .bind(status)
    .bind(&now)
    .bind(error)
    .execute(db.pool())
    .await;
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupRecordDto {
    pub id: String,
    pub job_id: Option<String>,
    pub repo_full_name: String,
    pub mode: String,
    pub format: Option<String>,
    pub path: String,
    pub size_bytes: i64,
    pub file_count: i64,
    pub status: String,
    pub verified: bool,
    pub git_lfs: bool,
    pub head_sha: Option<String>,
    pub default_branch: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub async fn list_records(db: &Db, limit: i64) -> Result<Vec<BackupRecordDto>> {
    let rows = sqlx::query(
        "SELECT id, job_id, repo_full_name, mode, format, path, size_bytes, file_count, status,
                verified, git_lfs, head_sha, default_branch, error, created_at, completed_at
         FROM backup_records ORDER BY created_at DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(db.pool())
    .await?;
    Ok(rows.into_iter().map(row_to_record).collect())
}

pub async fn get_record(db: &Db, id: &str) -> Result<Option<BackupRecordDto>> {
    let row = sqlx::query(
        "SELECT id, job_id, repo_full_name, mode, format, path, size_bytes, file_count, status,
                verified, git_lfs, head_sha, default_branch, error, created_at, completed_at
         FROM backup_records WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(row_to_record))
}

fn row_to_record(r: sqlx::sqlite::SqliteRow) -> BackupRecordDto {
    BackupRecordDto {
        id: r.try_get("id").unwrap_or_default(),
        job_id: r.try_get("job_id").unwrap_or(None),
        repo_full_name: r.try_get("repo_full_name").unwrap_or_default(),
        mode: r.try_get("mode").unwrap_or_default(),
        format: r.try_get("format").unwrap_or(None),
        path: r.try_get("path").unwrap_or_default(),
        size_bytes: r.try_get("size_bytes").unwrap_or(0),
        file_count: r.try_get("file_count").unwrap_or(0),
        status: r.try_get("status").unwrap_or_default(),
        verified: r.try_get::<bool, _>("verified").unwrap_or(false),
        git_lfs: r.try_get::<bool, _>("git_lfs").unwrap_or(false),
        head_sha: r.try_get("head_sha").unwrap_or(None),
        default_branch: r.try_get("default_branch").unwrap_or(None),
        error: r.try_get("error").unwrap_or(None),
        created_at: r.try_get("created_at").unwrap_or_default(),
        completed_at: r.try_get("completed_at").unwrap_or(None),
    }
}

pub async fn load_manifest(db: &Db, backup_id: &str) -> Result<BackupManifest> {
    if let Some(path) = manifest_lookup_path(db, backup_id).await? {
        if path.exists() {
            return manifest::read_manifest(&path);
        }
    }
    let row: Option<String> =
        sqlx::query("SELECT manifest_json FROM backup_manifests WHERE backup_id = ?1")
            .bind(backup_id)
            .fetch_optional(db.pool())
            .await?
            .map(|r| r.try_get::<String, _>("manifest_json").unwrap_or_default());
    match row {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Err(GbrError::BackupNotFound(backup_id.to_string())),
    }
}

/// Manifests are stored both in the DB and inside the storage root; prefer the
/// on-disk copy so a backup stays restorable even if the DB is rebuilt.
async fn manifest_lookup_path(db: &Db, backup_id: &str) -> Result<Option<std::path::PathBuf>> {
    let row: Option<String> = sqlx::query("SELECT path FROM backup_records WHERE id = ?1")
        .bind(backup_id)
        .fetch_optional(db.pool())
        .await?
        .map(|r| r.try_get::<String, _>("path").unwrap_or_default());
    Ok(row.map(|p| std::path::PathBuf::from(p).join("manifest.json")))
}

// ---------------------------------------------------------------- full backup

impl BackupEngine {
    pub fn new(db: Db, git: GitCli, layout: StorageLayout) -> Self {
        Self { db, git, layout }
    }

    /// Complete git backup: mirror clone (or incremental update), optional LFS
    /// capture, point-in-time snapshot, manifest, verification.
    pub async fn run_full(
        &self,
        task: &TaskHandle,
        repo: &Repo,
        token: &str,
        opts: &FullBackupOptions,
        job_id: Option<&str>,
    ) -> Result<BackupOutcome> {
        task.report_repo("preparing", Some(repo.full_name.clone()), None, None);
        let mirror = self.layout.full_mirror(&repo.owner, &repo.name);
        let mut updated_existing = mirror.exists();

        if mirror.exists() {
            task.report_repo(
                "fetching",
                Some(repo.full_name.clone()),
                None,
                Some("incremental update".into()),
            );
            self.git
                .update_mirror(&mirror, Some(token))
                .await
                .map_err(|e| {
                    // A corrupt local mirror must not be trusted for the next
                    // backup; tell the caller clearly instead of silently
                    // producing a partial result.
                    GbrError::Git(format!(
                        "incremental update failed for {}: {e}",
                        repo.full_name
                    ))
                })?;
        } else {
            task.report_repo(
                "cloning",
                Some(repo.full_name.clone()),
                None,
                Some("mirror clone".into()),
            );
            self.git
                .clone_mirror(&repo.clone_url_or(), &mirror, Some(token))
                .await?;
            updated_existing = false;
        }
        task.ensure_active()?;

        let mut lfs = LfsInfo {
            git_lfs_available: self.git.lfs_available().await,
            objects_backed_up: false,
            note: None,
        };
        if opts.include_lfs && lfs.git_lfs_available {
            task.report_repo(
                "lfs",
                Some(repo.full_name.clone()),
                None,
                Some("fetching all LFS objects".into()),
            );
            match self.git.lfs_fetch_all(&mirror, Some(token)).await {
                Ok(true) => lfs.objects_backed_up = true,
                Ok(false) => {}
                Err(e) => {
                    lfs.note = Some(format!("git lfs fetch failed: {e}"));
                }
            }
        } else if !lfs.git_lfs_available {
            lfs.note = Some("git-lfs is not installed; LFS objects were not captured".into());
        }
        task.ensure_active()?;

        let refs = self.git.all_refs(&mirror).await?;
        let head = self.git.head_sha(&mirror).await?;
        let git_version = self.git.version().await.ok();
        let default_branch = match repo.default_branch.clone() {
            Some(b) => Some(b),
            None => self
                .git
                .default_remote_branch(&mirror)
                .await
                .unwrap_or(None),
        };
        let size = manifest::dir_size(&mirror);

        let backup_id = uuid::Uuid::new_v4().to_string();
        let payload_rel = rel_path(self.layout.root(), &mirror)?;

        let mut m = BackupManifest::new(
            backup_id.clone(),
            BackupMode::Full,
            "mirror".into(),
            identity_of(repo, default_branch),
            payload_rel,
        );
        m.git_version = git_version;
        m.head_sha = head;
        m.refs = refs
            .iter()
            .map(|(n, s)| RefInfo {
                name: n.clone(),
                sha: s.clone(),
            })
            .collect();
        m.file_count = m.refs.len() as u64;
        m.size_bytes = size;
        m.lfs = lfs;

        // Snapshot before verification so we verify exactly what was captured.
        let mut snapshot_path = mirror.clone();
        if opts.snapshot {
            task.report_repo("snapshotting", Some(repo.full_name.clone()), None, None);
            let snap_dir = self.layout.snapshots_dir(&repo.owner, &repo.name);
            std::fs::create_dir_all(&snap_dir).map_err(|e| GbrError::io_at(&snap_dir, e))?;
            let snap = snap_dir.join(format!("{backup_id}.git"));
            manifest::copy_tree(&mirror, &snap)?;
            snapshot_path = snap.clone();
        }

        task.report_repo("verifying", Some(repo.full_name.clone()), None, None);
        m.verification = verify_mirror_payload(&self.git, &snapshot_path, &m.refs).await;

        // Persist: manifest inside the repo metadata dir, then DB rows.
        let meta_dir = self.layout.metadata_dir(&repo.owner, &repo.name);
        std::fs::create_dir_all(&meta_dir).map_err(|e| GbrError::io_at(&meta_dir, e))?;
        let meta_path = meta_dir.join(format!("{backup_id}.json"));
        manifest::write_json_atomic(&meta_path, &m)?;
        let root_manifest = self.layout.manifest_path(&backup_id);
        manifest::write_json_atomic(&root_manifest, &m)?;

        let db_path = meta_path.to_string_lossy().to_string();
        insert_record(&self.db, job_id, &m, &db_path).await?;

        if opts.snapshot {
            let _ = insert_snapshot(&self.db, &backup_id, &snapshot_path, m.size_bytes).await;
        }

        apply_retention(
            &self.db,
            &self.layout,
            &repo.owner,
            &repo.name,
            opts.retention.max(1),
        )
        .await;

        Ok(BackupOutcome {
            backup_id,
            repo_full_name: repo.full_name.clone(),
            path: db_path,
            size_bytes: m.size_bytes,
            refs: m.refs.len(),
            verified: m.verification.status == "verified",
            lfs_backed_up: m.lfs.objects_backed_up,
            updated_existing,
        })
    }
}

pub fn identity_of(repo: &Repo, default_branch: Option<String>) -> RepoIdentity {
    RepoIdentity {
        github_id: Some(repo.github_id),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        full_name: repo.full_name.clone(),
        source_url: repo.clone_url_or(),
        default_branch,
        is_private: repo.is_private,
    }
}

fn rel_path(root: &std::path::Path, target: &std::path::Path) -> Result<String> {
    Ok(target
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| target.to_string_lossy().to_string()))
}

async fn insert_snapshot(
    db: &Db,
    backup_id: &str,
    path: &std::path::Path,
    size: u64,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO backup_snapshots (id, backup_id, label, path, size_bytes, created_at)
         VALUES (?1, ?2, 'mirror-snapshot', ?3, ?4, ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(backup_id)
    .bind(path.to_string_lossy().to_string())
    .bind(size as i64)
    .bind(&now)
    .execute(db.pool())
    .await?;
    Ok(())
}

/// Verify a mirror snapshot: `git fsck --strict` plus every recorded ref
/// resolving to a present object. Returns the manifest verification block.
pub async fn verify_mirror_payload(
    git: &GitCli,
    path: &std::path::Path,
    expected: &[RefInfo],
) -> VerificationInfo {
    if !path.exists() {
        return VerificationInfo {
            status: "failed".into(),
            method: Some("presence".into()),
            verified_at: Some(Utc::now().to_rfc3339()),
            detail: Some(format!("payload missing: {}", path.display())),
        };
    }
    if let Err(e) = git.fsck(path).await {
        return VerificationInfo {
            status: "failed".into(),
            method: Some("git-fsck".into()),
            verified_at: Some(Utc::now().to_rfc3339()),
            detail: Some(e.to_string()),
        };
    }
    for r in expected {
        match git.object_exists(path, &r.sha).await {
            Ok(true) => {}
            Ok(false) => {
                return VerificationInfo {
                    status: "failed".into(),
                    method: Some("ref-objects".into()),
                    verified_at: Some(Utc::now().to_rfc3339()),
                    detail: Some(format!("missing object for {} ({})", r.name, r.sha)),
                };
            }
            Err(e) => {
                return VerificationInfo {
                    status: "failed".into(),
                    method: Some("ref-objects".into()),
                    verified_at: Some(Utc::now().to_rfc3339()),
                    detail: Some(e.to_string()),
                };
            }
        }
    }
    VerificationInfo {
        status: "verified".into(),
        method: Some("git-fsck + ref-objects".into()),
        verified_at: Some(Utc::now().to_rfc3339()),
        detail: Some(format!("{} refs verified", expected.len())),
    }
}

/// Delete snapshots beyond `keep`, newest first. The newest backup is never
/// removed — retention can never destroy the only valid copy.
pub async fn apply_retention(db: &Db, layout: &StorageLayout, owner: &str, name: &str, keep: u32) {
    let snapshots_dir = layout.snapshots_dir(owner, name);
    let Ok(entries) = std::fs::read_dir(&snapshots_dir) else {
        return;
    };
    let mut dirs: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    if dirs.len() <= keep as usize {
        return;
    }
    dirs.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    }); // oldest first
    let excess = dirs.len() - keep as usize;
    for victim in dirs.iter().take(excess) {
        // Safety: only ever delete inside the storage root's snapshot folder.
        if ensure_inside(layout.root(), victim).is_err() || !victim.starts_with(&snapshots_dir) {
            tracing::warn!(
                "refusing to delete {} (outside storage root)",
                victim.display()
            );
            continue;
        }
        if let Err(e) = std::fs::remove_dir_all(victim) {
            tracing::warn!("retention cleanup failed for {}: {e}", victim.display());
        }
    }
    let _ = db;
}

// ---------------------------------------------------------------- verification

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResultDto {
    pub backup_id: String,
    pub ok: bool,
    pub checked_files: u64,
    pub detail: String,
}

/// Verify any backup (mirror or selective) against its manifest.
pub async fn verify_backup(
    git: &GitCli,
    layout: &StorageLayout,
    db: &Db,
    backup_id: &str,
) -> Result<VerifyResultDto> {
    let m = load_manifest(db, backup_id).await?;
    let payload = layout.root().join(&m.payload_path);
    if !payload.exists() {
        return Err(GbrError::Verification(format!(
            "payload missing on disk: {}",
            payload.display()
        )));
    }

    let (ok, checked, detail) = if m.mode == BackupMode::Full {
        let v = verify_mirror_payload(git, &payload, &m.refs).await;
        (
            v.status == "verified",
            m.refs.len() as u64,
            v.detail.unwrap_or_default(),
        )
    } else if m.format == "folder" {
        let mut checked = 0u64;
        for f in &m.files {
            let p = payload.join(&f.path);
            if !p.exists() {
                return Ok(VerifyResultDto {
                    backup_id: backup_id.into(),
                    ok: false,
                    checked_files: checked,
                    detail: format!("missing file: {}", f.path),
                });
            }
            let hash = manifest::sha256_file(&p)?;
            if hash != f.sha256 {
                return Ok(VerifyResultDto {
                    backup_id: backup_id.into(),
                    ok: false,
                    checked_files: checked,
                    detail: format!("hash mismatch: {}", f.path),
                });
            }
            checked += 1;
        }
        (true, checked, format!("{checked} file hashes matched"))
    } else {
        let actual = manifest::sha256_file(&payload)?;
        match &m.payload_sha256 {
            Some(expected) if expected == &actual => {
                (true, 1, "archive sha256 matched".to_string())
            }
            Some(expected) => {
                return Ok(VerifyResultDto {
                    backup_id: backup_id.into(),
                    ok: false,
                    checked_files: 0,
                    detail: format!("archive hash mismatch (expected {expected}, got {actual})"),
                })
            }
            None => (true, 1, "archive present (no recorded hash)".to_string()),
        }
    };

    let now = Utc::now().to_rfc3339();
    let status = if ok { "verified" } else { "failed" };
    sqlx::query("UPDATE backup_records SET verified = ?2 WHERE id = ?1")
        .bind(backup_id)
        .bind(ok)
        .execute(db.pool())
        .await?;
    if let Ok(mut m2) = load_manifest(db, backup_id).await {
        m2.verification = VerificationInfo {
            status: status.into(),
            method: Some("sha256 + git-fsck".into()),
            verified_at: Some(now.clone()),
            detail: Some(detail.clone()),
        };
        if let Some(rec) = get_record(db, backup_id).await? {
            let p = std::path::PathBuf::from(&rec.path);
            let _ = manifest::write_json_atomic(&p, &m2);
            let _ = manifest::write_json_atomic(&layout.manifest_path(backup_id), &m2);
        }
    }

    Ok(VerifyResultDto {
        backup_id: backup_id.into(),
        ok,
        checked_files: checked,
        detail,
    })
}

/// Permanently delete one backup (manifest + payload + snapshots). Refuses to
/// delete when it is the only remaining backup for that repository unless
/// `force` is set — the caller must have obtained explicit confirmation.
pub async fn delete_backup(
    db: &Db,
    layout: &StorageLayout,
    backup_id: &str,
    force: bool,
) -> Result<()> {
    let rec = get_record(db, backup_id)
        .await?
        .ok_or_else(|| GbrError::BackupNotFound(backup_id.to_string()))?;

    let siblings: i64 =
        sqlx::query("SELECT COUNT(*) as c FROM backup_records WHERE repo_full_name = ?1")
            .bind(&rec.repo_full_name)
            .fetch_one(db.pool())
            .await
            .map(|r| r.try_get::<i64, _>("c").unwrap_or(1))
            .unwrap_or(1);

    if siblings <= 1 && !force {
        return Err(GbrError::Conflict(
            "this is the only backup for that repository; explicit confirmation is required".into(),
        ));
    }

    let path = std::path::PathBuf::from(&rec.path);
    if ensure_inside(layout.root(), &path).is_err() {
        return Err(GbrError::UnsafePath(format!(
            "{} is outside the storage root",
            path.display()
        )));
    }
    if path.exists() {
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(|e| GbrError::io_at(&path, e))?;
        } else {
            std::fs::remove_file(&path).map_err(|e| GbrError::io_at(&path, e))?;
        }
    }

    // Snapshot rows/files for this backup
    let snaps: Vec<String> = sqlx::query("SELECT path FROM backup_snapshots WHERE backup_id = ?1")
        .bind(backup_id)
        .fetch_all(db.pool())
        .await?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("path").ok())
        .collect();
    for s in snaps {
        let p = std::path::PathBuf::from(s);
        if ensure_inside(layout.root(), &p).is_ok() && p.exists() {
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    for table in ["backup_manifests", "backup_files", "backup_snapshots"] {
        sqlx::query(&format!(
            "DELETE FROM {table} WHERE backup_id = ?1 OR id = ?1"
        ))
        .bind(backup_id)
        .execute(db.pool())
        .await?;
    }
    sqlx::query("DELETE FROM backup_records WHERE id = ?1")
        .bind(backup_id)
        .execute(db.pool())
        .await?;
    let _ = std::fs::remove_file(layout.manifest_path(backup_id));

    Ok(())
}

/// Aggregate storage statistics for the Storage Manager page.
#[derive(Debug, Clone, Serialize)]
pub struct StorageInfoDto {
    pub root: String,
    pub total_bytes: u64,
    pub backup_count: i64,
    pub repo_count: i64,
    pub snapshot_count: i64,
    pub available_bytes: u64,
    pub per_repo: Vec<RepoUsage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoUsage {
    pub full_name: String,
    pub bytes: u64,
    pub backups: i64,
}

pub async fn storage_info(db: &Db, layout: &StorageLayout) -> Result<StorageInfoDto> {
    let root = layout.root().to_path_buf();
    let total_bytes = manifest::dir_size(&root);
    let available_bytes = manifest::available_space(&root);

    let backup_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM backup_records")
        .fetch_one(db.pool())
        .await?
        .try_get("c")
        .unwrap_or(0);
    let repo_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM repositories")
        .fetch_one(db.pool())
        .await?
        .try_get("c")
        .unwrap_or(0);
    let snapshot_count: i64 = sqlx::query("SELECT COUNT(*) as c FROM backup_snapshots")
        .fetch_one(db.pool())
        .await?
        .try_get("c")
        .unwrap_or(0);

    // Repository-wise usage is measured on disk (authoritative), grouped by
    // the storage layout rather than by DB rows, so missing DB entries still
    // show up.
    let mut per_repo = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root.join("repositories")) {
        for owner in rd.filter_map(|e| e.ok()) {
            if !owner.path().is_dir() {
                continue;
            }
            let Ok(repo_dirs) = std::fs::read_dir(owner.path()) else {
                continue;
            };
            for repo in repo_dirs.filter_map(|e| e.ok()) {
                if !repo.path().is_dir() {
                    continue;
                }
                let bytes = manifest::dir_size(&repo.path());
                let full_name = format!(
                    "{}/{}",
                    owner.file_name().to_string_lossy(),
                    repo.file_name().to_string_lossy()
                );
                let backups: i64 = sqlx::query(
                    "SELECT COUNT(*) as c FROM backup_records WHERE repo_full_name = ?1",
                )
                .bind(&full_name)
                .fetch_one(db.pool())
                .await
                .map(|r| r.try_get::<i64, _>("c").unwrap_or(0))
                .unwrap_or(0);
                if bytes > 0 || backups > 0 {
                    per_repo.push(RepoUsage {
                        full_name,
                        bytes,
                        backups,
                    });
                }
            }
        }
    }
    per_repo.sort_by_key(|r| std::cmp::Reverse(r.bytes));

    Ok(StorageInfoDto {
        root: root.to_string_lossy().to_string(),
        total_bytes,
        backup_count,
        repo_count,
        snapshot_count,
        available_bytes,
        per_repo,
    })
}

// ---------------------------------------------------------------- selective backup

impl BackupEngine {
    /// Selective (files only) backup: materialise a working tree, honour
    /// include/exclude rules, refuse silently shipping secrets, archive to the
    /// requested format, hash everything, then record a verifiable manifest.
    ///
    /// `local_tree` lets a caller (GUI/CLI) back up an existing local project
    /// directly instead of cloning from GitHub.
    pub async fn run_selective(
        &self,
        task: &TaskHandle,
        repo: &Repo,
        token: &str,
        opts: &SelectiveOptions,
        job_id: Option<&str>,
        local_tree: Option<&std::path::Path>,
    ) -> Result<BackupOutcome> {
        if let Some(tree) = local_tree {
            if !tree.is_dir() {
                return Err(GbrError::InvalidInput(format!(
                    "not a directory: {}",
                    tree.display()
                )));
            }
            return self
                .selective_from_tree(task, repo, tree, opts, job_id)
                .await;
        }

        task.report_repo("preparing", Some(repo.full_name.clone()), None, None);

        let staging = std::env::temp_dir().join(format!("gbr-selective-{}", uuid::Uuid::new_v4()));
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(|e| GbrError::io_at(&staging, e))?;
        }

        // Prefer the local mirror: no network, no credentials, no download.
        let mirror = self.layout.full_mirror(&repo.owner, &repo.name);
        let source = if mirror.exists() {
            task.report_repo(
                "materialising",
                Some(repo.full_name.clone()),
                None,
                Some("from local mirror".into()),
            );
            mirror.to_string_lossy().to_string()
        } else {
            task.report_repo(
                "cloning",
                Some(repo.full_name.clone()),
                None,
                Some("shallow clone".into()),
            );
            repo.clone_url_or()
        };

        let clone_res = self
            .git
            .clone_worktree(
                &source,
                &staging,
                if mirror.exists() { None } else { Some(token) },
                Some(1),
            )
            .await;
        if let Err(e) = clone_res {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
        task.ensure_active()?;

        // Selection happens inside a closure so the staging tree can be
        // dropped on every early exit without leaking temp directories.
        let result = self
            .selective_from_tree(task, repo, &staging, opts, job_id)
            .await;

        if let Err(e) = &result {
            // Staging is scratch space: always clean it up.
            if let Err(cleanup) = std::fs::remove_dir_all(&staging) {
                tracing::warn!(
                    "selective backup failed ({e}) and staging cleanup failed: {cleanup}"
                );
            }
        } else if std::fs::remove_dir_all(&staging).is_err() {
            tracing::warn!("could not clean staging dir {}", staging.display());
        }
        result
    }

    async fn selective_from_tree(
        &self,
        task: &TaskHandle,
        repo: &Repo,
        tree: &std::path::Path,
        opts: &SelectiveOptions,
        job_id: Option<&str>,
    ) -> Result<BackupOutcome> {
        task.report_repo("selecting", Some(repo.full_name.clone()), None, None);
        let files = selective::collect_selection(tree, opts)?;
        if files.is_empty() {
            return Err(GbrError::InvalidInput(
                "no files matched the selection rules".into(),
            ));
        }
        task.ensure_active()?;

        // Sensitive files: hard stop unless the user explicitly acknowledged.
        let paths: Vec<String> = files.iter().map(|(p, _, _)| p.clone()).collect();
        let hits = secretscan::scan(&paths);
        if !hits.is_empty() && !opts.sensitive_acknowledged {
            let list: Vec<String> = hits
                .iter()
                .map(|h| format!("{} ({})", h.path, h.kind))
                .collect();
            return Err(GbrError::Conflict(format!(
                "refusing to back up {} secret-looking file(s) without confirmation: {}",
                hits.len(),
                list.join(", ")
            )));
        }

        task.report_repo("hashing", Some(repo.full_name.clone()), None, None);
        let entries = selective::hash_files(&files)?;

        task.report_repo("archiving", Some(repo.full_name.clone()), None, None);
        let backup_id = uuid::Uuid::new_v4().to_string();
        let dest_dir = self.layout.selective_dir(&repo.owner, &repo.name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| GbrError::io_at(&dest_dir, e))?;

        let (payload, size_bytes, file_count) =
            selective::write_payload(&files, &dest_dir, &backup_id, opts)?;
        task.ensure_active()?;

        let payload_sha = if opts.format == ArchiveFormat::Folder {
            None
        } else {
            Some(manifest::sha256_file(&payload)?)
        };

        let default_branch = self.git.default_remote_branch(tree).await.unwrap_or(None);
        let mut m = BackupManifest::new(
            backup_id.clone(),
            BackupMode::Selective,
            opts.format.as_str().to_string(),
            identity_of(repo, default_branch),
            rel_path(self.layout.root(), &payload)?,
        );
        m.payload_sha256 = payload_sha;
        m.git_version = self.git.version().await.ok();
        m.head_sha = self.git.head_sha(tree).await.ok().flatten();
        m.refs = Vec::new(); // selective backups carry files, not refs
        m.file_count = file_count;
        m.size_bytes = size_bytes;
        m.files = entries;
        m.lfs = LfsInfo {
            git_lfs_available: self.git.lfs_available().await,
            objects_backed_up: false,
            note: Some("selective backup contains only the selected working-tree files".into()),
        };

        // Integrity proof for what we just wrote.
        let verified = match (&m.payload_sha256, opts.format) {
            (Some(expected), _) => manifest::sha256_file(&payload)
                .map(|actual| actual == *expected)
                .unwrap_or(false),
            (None, _) => m.files.iter().all(|f| {
                payload
                    .join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR))
                    .exists()
            }),
        };
        m.verification = VerificationInfo {
            status: if verified { "verified" } else { "failed" }.into(),
            method: Some("sha256".into()),
            verified_at: Some(chrono::Utc::now().to_rfc3339()),
            detail: Some(format!(
                "{} file(s), {}",
                m.file_count,
                manifest::bytes_human(m.size_bytes)
            )),
        };
        if !verified {
            return Err(GbrError::Verification(
                "selective payload failed post-write verification".into(),
            ));
        }

        let meta_dir = self.layout.metadata_dir(&repo.owner, &repo.name);
        std::fs::create_dir_all(&meta_dir).map_err(|e| GbrError::io_at(&meta_dir, e))?;
        let meta_path = meta_dir.join(format!("{backup_id}.json"));
        manifest::write_json_atomic(&meta_path, &m)?;
        manifest::write_json_atomic(&self.layout.manifest_path(&backup_id), &m)?;

        let db_path = meta_path.to_string_lossy().to_string();
        insert_record(&self.db, job_id, &m, &db_path).await?;

        Ok(BackupOutcome {
            backup_id,
            repo_full_name: repo.full_name.clone(),
            path: db_path,
            size_bytes: m.size_bytes,
            refs: 0,
            verified: true,
            lfs_backed_up: false,
            updated_existing: false,
        })
    }
}
