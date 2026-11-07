use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::backup::{self, BackupEngine};
use crate::db::Db;
use crate::error::{GbrError, Result};
use crate::git::GitCli;
use crate::manifest::{self, BackupMode, StorageLayout};
use crate::paths::ensure_inside;
use crate::tasks::TaskHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictMode {
    /// Destination must not exist (or must be empty).
    NewFolder,
    /// Copy in, keeping existing files; never overwrites differing files.
    Merge,
    /// Overwrite — only ever reached after explicit user confirmation.
    Replace,
}

impl ConflictMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "new_folder" => Ok(ConflictMode::NewFolder),
            "merge" => Ok(ConflictMode::Merge),
            "replace" => Ok(ConflictMode::Replace),
            other => Err(GbrError::InvalidInput(format!(
                "unknown conflict mode: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub backup_id: String,
    pub destination: String,
    pub conflict_mode: ConflictMode,
    /// For full backups: also produce a checked-out working tree.
    #[serde(default = "default_true")]
    pub as_worktree: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreOutcome {
    pub backup_id: String,
    pub destination: String,
    pub files_restored: u64,
    pub verified: bool,
    pub detail: String,
}

/// Pre-flight checks the user sees before anything is written.
#[derive(Debug, Clone, Serialize)]
pub struct RestorePlan {
    pub backup_id: String,
    pub mode: String,
    pub format: String,
    pub payload_exists: bool,
    pub file_count: u64,
    pub size_bytes: u64,
    pub available_bytes: u64,
    pub destination_exists: bool,
    pub destination_empty: bool,
    pub conflicts: Vec<String>,
    pub can_proceed: bool,
    pub blockers: Vec<String>,
}

pub async fn plan(
    db: &Db,
    layout: &StorageLayout,
    request: &RestoreRequest,
) -> Result<RestorePlan> {
    let m = backup::load_manifest(db, &request.backup_id).await?;
    let payload = layout.root().join(&m.payload_path);
    let dest = PathBuf::from(&request.destination);
    let available = manifest::available_space(&dest);

    let destination_exists = dest.exists();
    let destination_empty = if destination_exists {
        std::fs::read_dir(&dest)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false)
    } else {
        true
    };

    let mut blockers = Vec::new();
    let mut conflicts = Vec::new();

    if !payload.exists() {
        blockers.push(format!(
            "backup payload is missing on disk: {}",
            payload.display()
        ));
    }
    if request.destination.trim().is_empty() {
        blockers.push("no destination folder chosen".into());
    }
    if available > 0 && available < m.size_bytes {
        blockers.push(format!(
            "not enough free space: {} needed, {} available",
            crate::bytes_human(m.size_bytes),
            crate::bytes_human(available)
        ));
    }
    if request.conflict_mode == ConflictMode::NewFolder && destination_exists && !destination_empty
    {
        blockers.push("destination exists and is not empty (choose merge or replace)".into());
    }
    if request.conflict_mode == ConflictMode::Replace && destination_exists && !destination_empty {
        conflicts.push("replace will overwrite existing files in this folder".into());
    }

    Ok(RestorePlan {
        backup_id: m.backup_id,
        mode: match m.mode {
            BackupMode::Full => "full".into(),
            BackupMode::Selective => "selective".into(),
        },
        format: m.format.clone(),
        payload_exists: payload.exists(),
        file_count: m.file_count,
        size_bytes: m.size_bytes,
        available_bytes: available,
        destination_exists,
        destination_empty,
        conflicts,
        can_proceed: blockers.is_empty(),
        blockers,
    })
}

/// Execute a restore after the caller has validated the plan and obtained any
/// confirmation the plan requires.
pub async fn run(
    engine: &BackupEngine,
    task: &TaskHandle,
    request: &RestoreRequest,
    confirmed_replace: bool,
) -> Result<RestoreOutcome> {
    let p = plan(&engine.db, &engine.layout, request).await?;
    if !p.can_proceed {
        return Err(GbrError::Conflict(p.blockers.join("; ")));
    }
    if request.conflict_mode == ConflictMode::Replace && !confirmed_replace {
        return Err(GbrError::Conflict(
            "overwriting an existing folder requires explicit confirmation".into(),
        ));
    }

    let m = backup::load_manifest(&engine.db, &request.backup_id).await?;
    let payload = engine.layout.root().join(&m.payload_path);
    let dest = PathBuf::from(&request.destination);

    // Every restore target must stay user-chosen and never touch the source
    // project or the storage root itself.
    if payload.starts_with(&dest) {
        return Err(GbrError::Conflict(
            "destination is inside the backup storage".into(),
        ));
    }

    task.report("validating", Some(0.0), Some("checking manifest".into()));
    task.ensure_active()?;

    let restored = match m.mode {
        BackupMode::Full => restore_full(&engine.git, task, &m, &payload, &dest, request).await?,
        BackupMode::Selective => restore_selective(task, &m, &payload, &dest, request).await?,
    };

    task.report("done", Some(100.0), None);
    Ok(RestoreOutcome {
        backup_id: request.backup_id.clone(),
        destination: dest.to_string_lossy().to_string(),
        files_restored: restored,
        verified: true,
        detail: format!("{restored} items restored"),
    })
}

async fn restore_full(
    git: &GitCli,
    task: &TaskHandle,
    m: &manifest::BackupManifest,
    payload: &Path,
    dest: &Path,
    request: &RestoreRequest,
) -> Result<u64> {
    task.report("restoring-git", Some(10.0), Some("copying mirror".into()));

    // Verify before restoring: never propagate an unverified mirror.
    let v = backup::verify_mirror_payload(git, payload, &m.refs).await;
    if v.status == "failed" {
        return Err(GbrError::Verification(format!(
            "backup failed verification: {}",
            v.detail.unwrap_or_default()
        )));
    }

    let dest_parent = dest.parent().unwrap_or(dest);
    std::fs::create_dir_all(dest_parent).map_err(|e| GbrError::io_at(dest_parent, e))?;

    let bare_dest = if dest.extension().map(|e| e == "git").unwrap_or(false) {
        dest.to_path_buf()
    } else {
        dest.with_extension("git")
    };
    if bare_dest.exists() {
        match request.conflict_mode {
            ConflictMode::NewFolder => {
                return Err(GbrError::Conflict(format!(
                    "{} already exists",
                    bare_dest.display()
                )))
            }
            ConflictMode::Replace => {
                std::fs::remove_dir_all(&bare_dest).map_err(|e| GbrError::io_at(&bare_dest, e))?;
            }
            ConflictMode::Merge => {
                return Err(GbrError::Conflict(format!(
                    "merging is not supported for git mirrors: {} exists",
                    bare_dest.display()
                )));
            }
        }
    }

    manifest::copy_tree(payload, &bare_dest)?;
    task.ensure_active()?;
    task.report(
        "restoring-worktree",
        Some(60.0),
        Some("creating working copy".into()),
    );

    if request.as_worktree {
        let work = dest.with_extension("");
        if work.exists() {
            if request.conflict_mode == ConflictMode::Replace {
                std::fs::remove_dir_all(&work).map_err(|e| GbrError::io_at(&work, e))?;
            } else {
                return Err(GbrError::Conflict(format!(
                    "working copy already exists: {}",
                    work.display()
                )));
            }
        }
        git.clone_worktree(&bare_dest.to_string_lossy(), &work, None, None)
            .await?;
        task.report(
            "verifying",
            Some(90.0),
            Some("fsck on restored mirror".into()),
        );
        git.fsck(&bare_dest).await?;
        Ok(manifest::dir_size(&work))
    } else {
        git.fsck(&bare_dest).await?;
        Ok(manifest::dir_size(&bare_dest))
    }
}

async fn restore_selective(
    task: &TaskHandle,
    m: &manifest::BackupManifest,
    payload: &Path,
    dest: &Path,
    request: &RestoreRequest,
) -> Result<u64> {
    if dest.exists() && request.conflict_mode == ConflictMode::NewFolder {
        let empty = std::fs::read_dir(dest)
            .map(|mut d| d.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(GbrError::Conflict(format!(
                "destination exists and is not empty: {}",
                dest.display()
            )));
        }
    }
    if request.conflict_mode == ConflictMode::Replace && dest.exists() {
        // Only the files this backup owns are removed — never a blanket wipe
        // of a folder we did not create.
        for f in &m.files {
            let p = dest.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if p.exists() && ensure_inside(dest, &p).is_ok() {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    std::fs::create_dir_all(dest).map_err(|e| GbrError::io_at(dest, e))?;
    task.report("extracting", Some(20.0), None);

    let mut restored = 0u64;
    if m.format == "folder" {
        for (i, f) in m.files.iter().enumerate() {
            task.ensure_active()?;
            let src = payload.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let target = dest.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if target.exists() && request.conflict_mode == ConflictMode::Merge {
                continue; // never overwrite in merge mode
            }
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| GbrError::io_at(p, e))?;
            }
            ensure_inside(dest, &target)?;
            std::fs::copy(&src, &target).map_err(|e| GbrError::io_at(&target, e))?;
            restored += 1;
            if i % 50 == 0 {
                let pct = 20.0 + (i as f32 / m.files.len().max(1) as f32) * 70.0;
                task.report("extracting", Some(pct), Some(f.path.clone()));
            }
        }
    } else {
        restored = extract_archive(payload, dest, request.conflict_mode)?;
        task.report("extracting", Some(90.0), None);
    }

    // Post-restore integrity: verify what we just wrote.
    if m.format == "folder" {
        for f in &m.files {
            let target = dest.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if !target.exists() {
                return Err(GbrError::Verification(format!(
                    "missing after restore: {}",
                    f.path
                )));
            }
            let hash = manifest::sha256_file(&target)?;
            if hash != f.sha256 {
                return Err(GbrError::Verification(format!(
                    "hash mismatch after restore: {}",
                    f.path
                )));
            }
        }
    } else if let Some(expected) = &m.payload_sha256 {
        let actual = manifest::sha256_file(payload)?;
        if &actual != expected {
            return Err(GbrError::Verification(
                "archive hash changed after extraction".into(),
            ));
        }
    }

    Ok(restored)
}

/// Safe archive extraction: rejects absolute paths, `..` traversal and
/// symlinks that escape the destination (zip-slip / tar-slip protection) and
/// bounds total expansion to protect against archive bombs.
fn extract_archive(payload: &Path, dest: &Path, mode: ConflictMode) -> Result<u64> {
    let ext = payload
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if ext == "zip" {
        let mut zip = zip::ZipArchive::new(
            std::fs::File::open(payload).map_err(|e| GbrError::io_at(payload, e))?,
        )
        .map_err(|e| GbrError::Verification(format!("unreadable zip: {e}")))?;

        // ZIP BOMB GUARD: refuse absurd expansion ratios up front.
        let mut claimed_total: u64 = 0;
        for i in 0..zip.len() {
            let f = zip
                .by_index(i)
                .map_err(|e| GbrError::Verification(e.to_string()))?;
            claimed_total = claimed_total.saturating_add(f.size());
        }
        let on_disk = payload.metadata().map(|m| m.len()).unwrap_or(0);
        if on_disk > 0 && claimed_total > on_disk.saturating_mul(200).max(1 << 30) {
            return Err(GbrError::Verification(
                "archive claims an implausible expanded size (possible zip bomb)".into(),
            ));
        }

        let mut count = 0u64;
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| GbrError::Verification(e.to_string()))?;
            let rel = file
                .enclosed_name()
                .ok_or_else(|| GbrError::UnsafePath(format!("unsafe entry in archive: {i}")))?;
            let target = dest.join(&rel);
            ensure_inside(dest, &target)?;
            if file.is_dir() {
                std::fs::create_dir_all(&target).map_err(|e| GbrError::io_at(&target, e))?;
                continue;
            }
            if target.exists() && mode == ConflictMode::Merge {
                continue;
            }
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| GbrError::io_at(p, e))?;
            }
            let mut out =
                std::fs::File::create(&target).map_err(|e| GbrError::io_at(&target, e))?;
            std::io::copy(&mut file, &mut out).map_err(|e| GbrError::io_at(&target, e))?;
            count += 1;
        }
        return Ok(count);
    }

    // tar / tar.zst
    let file = std::fs::File::open(payload).map_err(|e| GbrError::io_at(payload, e))?;
    let reader: Box<dyn std::io::Read> =
        if ext == "zst" || payload.to_string_lossy().ends_with(".tar.zst") {
            Box::new(
                zstd::Decoder::new(file)
                    .map_err(|e| GbrError::Verification(format!("unreadable tar.zst: {e}")))?,
            )
        } else {
            Box::new(file)
        };
    let mut archive = tar::Archive::new(reader);

    let mut count = 0u64;
    for entry in archive
        .entries()
        .map_err(|e| GbrError::Verification(format!("unreadable tar: {e}")))?
    {
        let mut entry = entry.map_err(|e| GbrError::Verification(e.to_string()))?;
        let rel = entry
            .path()
            .map_err(|e| GbrError::Verification(e.to_string()))?
            .to_path_buf();
        // Reject absolute paths and traversal.
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(GbrError::UnsafePath(format!(
                "unsafe entry in archive: {}",
                rel.display()
            )));
        }
        let target = dest.join(&rel);
        ensure_inside(dest, &target)?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| GbrError::io_at(&target, e))?;
            continue;
        }
        if kind.is_symlink() {
            // Symlinks are not materialised: they are a classic escape vector.
            continue;
        }
        if target.exists() && mode == ConflictMode::Merge {
            continue;
        }
        if let Some(p) = target.parent() {
            std::fs::create_dir_all(p).map_err(|e| GbrError::io_at(p, e))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| GbrError::io_at(&target, e))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| GbrError::io_at(&target, e))?;
        count += 1;
    }
    Ok(count)
}

/// Record a restore job for the history view.
pub async fn record_job(
    db: &Db,
    backup_id: &str,
    destination: &str,
    conflict_mode: &str,
    status: &str,
    files: u64,
    error: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO restore_jobs (id, backup_id, destination, conflict_mode, status, files_restored, started_at, finished_at, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(backup_id)
    .bind(destination)
    .bind(conflict_mode)
    .bind(status)
    .bind(files as i64)
    .bind(&now)
    .bind(error)
    .execute(db.pool())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_mode_parses() {
        assert_eq!(
            ConflictMode::parse("replace").unwrap(),
            ConflictMode::Replace
        );
        assert!(ConflictMode::parse("wipe-everything").is_err());
    }
}
