use std::path::PathBuf;

use crate::error::{GbrError, Result};

/// Application-level directories. Everything is derived from the platform's
/// standard data dir so nothing is hardcoded to a fixed storage location.
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    /// `root` is normally [`AppPaths::default_root`], but tests and portable
    /// installs can point it anywhere.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn default_root() -> Result<Self> {
        let base = dirs::data_dir().ok_or_else(|| {
            GbrError::Other("could not resolve the platform data directory".into())
        })?;
        Ok(Self::new(base.join("GitBackupRestore")))
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("gbr.sqlite3")
    }

    /// Root of the user-selected backup storage tree.
    pub fn default_backup_root(&self) -> PathBuf {
        self.root.join("GitBackupStorage")
    }

    /// Scratch space for in-progress (not yet atomic) operations.
    pub fn staging(&self) -> PathBuf {
        self.root.join(".staging")
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.default_backup_root(),
            self.staging(),
        ] {
            std::fs::create_dir_all(&dir).map_err(|e| GbrError::io_at(dir, e))?;
        }
        Ok(())
    }
}

/// Reject anything that could escape a destination root (path traversal,
/// absolute paths, `..` components, NUL bytes).
pub fn validate_relative_path(p: &str) -> Result<std::path::PathBuf> {
    if p.contains('\0') {
        return Err(GbrError::UnsafePath("NUL byte in path".into()));
    }
    let path = PathBuf::from(p);
    if path.is_absolute() {
        return Err(GbrError::UnsafePath(format!("absolute path: {p}")));
    }
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => {
                return Err(GbrError::UnsafePath(format!("illegal component in: {p}")));
            }
        }
    }
    Ok(path)
}

/// Ensure `child` resolves inside `root`, after canonicalising both.
pub fn ensure_inside(root: &std::path::Path, child: &std::path::Path) -> Result<()> {
    let root_c = root.canonicalize().map_err(|e| GbrError::io_at(root, e))?;
    let child_c = child
        .canonicalize()
        .map_err(|e| GbrError::io_at(child, e))?;
    if !child_c.starts_with(&root_c) {
        return Err(GbrError::UnsafePath(format!(
            "{} escapes {}",
            child.display(),
            root.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(validate_relative_path("../etc/passwd").is_err());
        assert!(validate_relative_path("/abs/path").is_err());
        assert!(validate_relative_path("a/../../b").is_err());
        assert!(validate_relative_path("ok/nested/file.txt").is_ok());
    }
}
