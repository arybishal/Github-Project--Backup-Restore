use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{GbrError, Result};
use crate::paths::AppPaths;

pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupMode {
    Full,
    Selective,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIdentity {
    pub github_id: Option<i64>,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub source_url: String,
    pub default_branch: Option<String>,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefInfo {
    pub name: String,
    pub sha: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LfsInfo {
    pub git_lfs_available: bool,
    pub objects_backed_up: bool,
    /// Documented limitation: GitHub Releases / LFS objects hosted outside the
    /// repository's LFS store are not captured.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationInfo {
    pub status: String, // "verified" | "unverified" | "failed"
    pub method: Option<String>,
    pub verified_at: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub manifest_version: u32,
    pub backup_id: String,
    pub mode: BackupMode,
    /// "mirror" for full backups, "zip" | "tar" | "tar.zst" | "folder" for selective.
    pub format: String,
    pub repository: RepoIdentity,
    pub created_at: String,
    pub app_version: String,
    pub git_version: Option<String>,
    pub head_sha: Option<String>,
    pub refs: Vec<RefInfo>,
    /// Relative path of the backup payload inside the storage root.
    pub payload_path: String,
    /// SHA-256 of the payload file (archive/folder marker). For `mirror`
    /// format the payload is a directory and this is `null`; integrity of a
    /// mirror is proven by `git fsck` + per-ref object checks instead.
    pub payload_sha256: Option<String>,
    pub file_count: u64,
    pub size_bytes: u64,
    #[serde(default)]
    pub files: Vec<FileEntry>,
    pub lfs: LfsInfo,
    pub verification: VerificationInfo,
    /// Human-readable, honest limitations of what this backup does *not* cover.
    pub limitations: Vec<String>,
}

impl BackupManifest {
    pub fn limitations_for(mode: &BackupMode) -> Vec<String> {
        let mut v = vec![
            "GitHub platform data outside the Git repository is not captured: Issues, Pull Requests, Discussions, Projects, Wikis, Actions secrets/variables, Environments, Deploy keys, webhooks, branch protection rules, repository settings and GitHub Releases assets.".into(),
            "Hidden GitHub-only references (refs/pull/*, refs/hidden/*) may not be present in a mirror clone.".into(),
            "A successful git backup is not a complete GitHub account backup.".into(),
        ];
        if *mode == BackupMode::Selective {
            v.push(
                "Selective backups only contain the files explicitly selected; commit history and unselected paths are absent.".into(),
            );
        }
        v
    }

    pub fn new(
        backup_id: String,
        mode: BackupMode,
        format: String,
        repository: RepoIdentity,
        payload_path: String,
    ) -> Self {
        let limitations = Self::limitations_for(&mode);
        Self {
            manifest_version: MANIFEST_VERSION,
            backup_id,
            mode,
            format,
            repository,
            created_at: Utc::now().to_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            git_version: None,
            head_sha: None,
            refs: Vec::new(),
            payload_path,
            payload_sha256: None,
            file_count: 0,
            size_bytes: 0,
            files: Vec::new(),
            lfs: LfsInfo {
                git_lfs_available: false,
                objects_backed_up: false,
                note: None,
            },
            verification: VerificationInfo {
                status: "unverified".into(),
                method: None,
                verified_at: None,
                detail: None,
            },
            limitations,
        }
    }
}

// ---------------------------------------------------------------- paths

#[derive(Debug, Clone)]
pub struct StorageLayout {
    pub root: PathBuf,
}

impl StorageLayout {
    pub fn resolve(root: PathBuf) -> Self {
        Self { root }
    }

    /// Build the layout from the configured root, falling back to the
    /// application's default storage directory when unset.
    pub fn from_settings(app: &AppPaths, configured: &str) -> Self {
        let root = if configured.trim().is_empty() {
            app.default_backup_root()
        } else {
            PathBuf::from(configured)
        };
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn repo_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.root
            .join("repositories")
            .join(sanitize_component(owner))
            .join(sanitize_component(name))
    }

    pub fn full_mirror(&self, owner: &str, name: &str) -> PathBuf {
        self.repo_dir(owner, name).join("full").join("mirror.git")
    }

    pub fn snapshots_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.repo_dir(owner, name).join("snapshots")
    }

    pub fn selective_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.repo_dir(owner, name).join("selective")
    }

    pub fn metadata_dir(&self, owner: &str, name: &str) -> PathBuf {
        self.repo_dir(owner, name).join("metadata")
    }

    pub fn manifest_dir(&self) -> PathBuf {
        self.root.join("manifest")
    }

    pub fn manifest_path(&self, backup_id: &str) -> PathBuf {
        self.manifest_dir().join(format!("{backup_id}.json"))
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn reports_dir(&self) -> PathBuf {
        self.root.join("reports")
    }

    pub fn ensure(&self) -> Result<()> {
        for d in [
            self.root.clone(),
            self.manifest_dir(),
            self.logs_dir(),
            self.reports_dir(),
            self.root.join("repositories"),
        ] {
            std::fs::create_dir_all(&d).map_err(|e| GbrError::io_at(d, e))?;
        }
        Ok(())
    }
}

/// Owner/repo names come from the network; keep them filesystem-safe on every
/// platform (Windows forbids `:` `?` etc.).
pub fn sanitize_component(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    if cleaned.trim().is_empty() {
        "unknown".into()
    } else {
        cleaned
    }
}

// ---------------------------------------------------------------- io

/// Write JSON atomically: temp file in the same directory, fsync, rename.
/// A crash can never leave a half-written manifest next to real backup data.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| GbrError::InvalidInput("manifest path has no parent".into()))?;
    std::fs::create_dir_all(dir).map_err(|e| GbrError::io_at(dir, e))?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "manifest".into())
    ));
    let json = serde_json::to_vec_pretty(value)?;
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp).map_err(|e| GbrError::io_at(&tmp, e))?;
        f.write_all(&json).map_err(|e| GbrError::io_at(&tmp, e))?;
        f.sync_all().map_err(|e| GbrError::io_at(&tmp, e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| GbrError::io_at(path, e))?;
    Ok(())
}

pub fn read_manifest(path: &Path) -> Result<BackupManifest> {
    let data = std::fs::read(path).map_err(|e| GbrError::io_at(path, e))?;
    let m: BackupManifest = serde_json::from_slice(&data)?;
    if m.manifest_version > MANIFEST_VERSION {
        return Err(GbrError::InvalidInput(format!(
            "manifest version {} is newer than supported version {}",
            m.manifest_version, MANIFEST_VERSION
        )));
    }
    Ok(m)
}

/// SHA-256 of a file, streamed in 64 KiB blocks (never loads the file whole).
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut f = std::fs::File::open(path).map_err(|e| GbrError::io_at(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| GbrError::io_at(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Total size of a file tree in bytes.
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// Human readable byte size used across CLI and GUI.
pub fn bytes_human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Recursive copy that hardlinks files when possible (same volume) and falls
/// back to byte copy otherwise. Used to snapshot a live mirror without paying
/// for a second full copy of every object.
pub fn copy_tree(src: &Path, dst: &Path) -> Result<u64> {
    if dst.exists() {
        return Err(GbrError::Conflict(format!(
            "snapshot destination exists: {}",
            dst.display()
        )));
    }
    std::fs::create_dir_all(dst).map_err(|e| GbrError::io_at(dst, e))?;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| GbrError::Other(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| GbrError::Other(e.to_string()))?;
        let target = dst.join(rel);
        let meta = entry
            .metadata()
            .map_err(|e| GbrError::Other(e.to_string()))?;
        if meta.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| GbrError::io_at(&target, e))?;
        } else if meta.is_symlink() {
            let link =
                std::fs::read_link(entry.path()).map_err(|e| GbrError::io_at(entry.path(), e))?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, &target).map_err(|e| GbrError::io_at(&target, e))?;
            #[cfg(windows)]
            {
                if link.is_dir() {
                    std::os::windows::fs::symlink_dir(&link, &target)
                        .map_err(|e| GbrError::io_at(&target, e))?;
                } else {
                    std::os::windows::fs::symlink_file(&link, &target)
                        .map_err(|e| GbrError::io_at(&target, e))?;
                }
            }
        } else {
            if std::fs::hard_link(entry.path(), &target).is_ok() {
                bytes += meta.len();
                continue;
            }
            std::fs::copy(entry.path(), &target).map_err(|e| GbrError::io_at(&target, e))?;
            bytes += meta.len();
        }
    }
    Ok(bytes)
}

/// Available bytes on the volume containing `path`.
pub fn available_space(path: &Path) -> u64 {
    // fs2 works on any path; opening the dir itself is not portable, so probe
    // with the nearest existing ancestor.
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            return 0;
        }
    }
    fs2::available_space(&probe).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn manifest_roundtrip_is_atomic_and_versioned() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("manifest").join("abc.json");
        let m = BackupManifest::new(
            "abc".into(),
            BackupMode::Full,
            "mirror".into(),
            RepoIdentity {
                github_id: Some(1),
                owner: "o".into(),
                name: "n".into(),
                full_name: "o/n".into(),
                source_url: "https://github.com/o/n.git".into(),
                default_branch: Some("main".into()),
                is_private: false,
            },
            "repositories/o/n/full/mirror.git".into(),
        );
        write_json_atomic(&path, &m).unwrap();
        assert!(!path.with_file_name(".abc.json.tmp").exists());
        let back = read_manifest(&path).unwrap();
        assert_eq!(back.backup_id, "abc");
        assert_eq!(back.manifest_version, MANIFEST_VERSION);
        assert!(!back.limitations.is_empty());
    }

    #[test]
    fn sha256_streams() {
        let dir = tempdir().unwrap();
        let f = dir.path().join("x.bin");
        std::fs::write(&f, b"hello world").unwrap();
        // echo -n "hello world" | sha256sum
        assert_eq!(
            sha256_file(&f).unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn sanitize_is_filesystem_safe() {
        assert_eq!(sanitize_component("a:b/c"), "a_b_c");
        assert_eq!(sanitize_component(""), "unknown");
    }
}
