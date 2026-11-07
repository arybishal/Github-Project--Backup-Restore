use std::path::{Path, PathBuf};
use std::process::Stdio;

use base64::Engine;
use tokio::process::Command;

use crate::error::{GbrError, Result};

/// Thin, safe wrapper around the native `git` executable.
///
/// Every invocation passes arguments as a vector — untrusted input is never
/// interpolated into a shell string. Credentials are supplied through
/// `GIT_CONFIG_*` environment variables so they never appear in the remote URL
/// written to `.git/config`, nor in manifests or logs.
#[derive(Debug, Clone)]
pub struct GitCli {
    pub git_path: String,
}

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl GitCli {
    pub fn new(git_path: impl Into<String>) -> Self {
        let p = git_path.into();
        Self {
            git_path: if p.trim().is_empty() { "git".into() } else { p },
        }
    }

    fn base_command(&self) -> Command {
        let mut cmd = Command::new(&self.git_path);
        // Never prompt (and hang) for credentials; never show interactive GUI.
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    /// Run git with an optional working directory and optional bearer token.
    pub async fn run(&self, args: &[String], cwd: Option<&Path>) -> Result<GitOutput> {
        self.run_with_token(args, cwd, None).await
    }

    pub async fn run_with_token(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        token: Option<&str>,
    ) -> Result<GitOutput> {
        let mut cmd = self.base_command();
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        if let Some(tok) = token.filter(|t| !t.is_empty()) {
            // basic <base64(x-access-token:TOKEN)> is what GitHub's own
            // credential helper uses; kept in env so it is not persisted.
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{tok}"));
            cmd.env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", "http.extraheader")
                .env(
                    "GIT_CONFIG_VALUE_0",
                    format!("AUTHORIZATION: basic {encoded}"),
                );
        }
        cmd.args(args);

        let output = cmd
            .output()
            .await
            .map_err(|e| GbrError::Git(format!("failed to spawn '{}': {e}", self.git_path)))?;

        Ok(GitOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run and fail loudly when git exits non-zero.
    pub async fn checked(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        token: Option<&str>,
    ) -> Result<String> {
        let out = self.run_with_token(args, cwd, token).await?;
        if out.status != 0 {
            return Err(GbrError::Git(format!(
                "`git {}` failed (exit {}): {}",
                args.join(" "),
                out.status,
                out.stderr
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or("unknown error")
                    .trim()
            )));
        }
        Ok(out.stdout)
    }

    pub async fn version(&self) -> Result<String> {
        let out = self.run(&["version".to_string()], None).await?;
        if out.status != 0 {
            return Err(GbrError::Git(format!(
                "'{}' is not a usable git executable",
                self.git_path
            )));
        }
        Ok(out.stdout.trim().to_string())
    }

    pub async fn is_git_repo(&self, path: &Path) -> bool {
        if !path.join(".git").exists() && !path.join("HEAD").exists() {
            return false;
        }
        self.run(
            &["rev-parse".to_string(), "--git-dir".to_string()],
            Some(path),
        )
        .await
        .map(|o| o.status == 0)
        .unwrap_or(false)
    }

    pub async fn lfs_available(&self) -> bool {
        self.run(&["lfs".to_string(), "version".to_string()], None)
            .await
            .map(|o| o.status == 0)
            .unwrap_or(false)
    }

    /// Create a mirror clone: `git clone --mirror <url> <dest>`.
    pub async fn clone_mirror(&self, url: &str, dest: &Path, token: Option<&str>) -> Result<()> {
        if dest.exists() {
            return Err(GbrError::Conflict(format!(
                "destination already exists: {}",
                dest.display()
            )));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GbrError::io_at(parent.to_path_buf(), e))?;
        }
        self.checked(
            &[
                "clone".into(),
                "--mirror".into(),
                url.into(),
                dest.to_string_lossy().into_owned(),
            ],
            None,
            token,
        )
        .await?;
        Ok(())
    }

    /// Incrementally update an existing mirror (`git remote update --prune`).
    pub async fn update_mirror(&self, dest: &Path, token: Option<&str>) -> Result<()> {
        self.checked(
            &["remote".into(), "update".into(), "--prune".into()],
            Some(dest),
            token,
        )
        .await?;
        Ok(())
    }

    /// Clone a working tree from a local path or URL.
    pub async fn clone_worktree(
        &self,
        source: &str,
        dest: &Path,
        token: Option<&str>,
        depth: Option<u32>,
    ) -> Result<()> {
        if dest.exists() {
            return Err(GbrError::Conflict(format!(
                "destination already exists: {}",
                dest.display()
            )));
        }
        let mut args = vec!["clone".to_string()];
        if let Some(d) = depth {
            args.push("--depth".into());
            args.push(d.to_string());
        }
        args.push(source.into());
        args.push(dest.to_string_lossy().into_owned());
        self.checked(&args, None, token).await?;
        Ok(())
    }

    /// All refs (heads, tags, remotes) with their object ids.
    pub async fn all_refs(&self, repo: &Path) -> Result<Vec<(String, String)>> {
        let out = self
            .checked(
                &[
                    "for-each-ref".into(),
                    "--format=%(refname) %(objectname)".into(),
                ],
                Some(repo),
                None,
            )
            .await?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut it = l.splitn(2, ' ');
                Some((it.next()?.to_string(), it.next()?.to_string()))
            })
            .collect())
    }

    pub async fn head_sha(&self, repo: &Path) -> Result<Option<String>> {
        let out = self
            .run(
                &["rev-parse".into(), "--verify".into(), "HEAD".into()],
                Some(repo),
            )
            .await?;
        Ok(if out.status == 0 {
            Some(out.stdout.trim().to_string())
        } else {
            None
        })
    }

    pub async fn object_exists(&self, repo: &Path, sha: &str) -> Result<bool> {
        let out = self
            .run(
                &["cat-file".into(), "-e".into(), format!("{sha}^{{object}}")],
                Some(repo),
            )
            .await?;
        Ok(out.status == 0)
    }

    /// Connectivity check used by verification.
    pub async fn fsck(&self, repo: &Path) -> Result<String> {
        let out = self
            .run(&["fsck".into(), "--strict".into()], Some(repo))
            .await?;
        if out.status != 0 {
            return Err(GbrError::Verification(format!(
                "git fsck failed: {}",
                out.stderr.trim()
            )));
        }
        Ok(out.stdout)
    }

    /// Fetch every LFS object for every ref into the repository's LFS store.
    /// Returns `false` when git-lfs is not installed (documented limitation).
    pub async fn lfs_fetch_all(&self, repo: &Path, token: Option<&str>) -> Result<bool> {
        if !self.lfs_available().await {
            return Ok(false);
        }
        self.checked(
            &["lfs".into(), "fetch".into(), "--all".into()],
            Some(repo),
            token,
        )
        .await?;
        Ok(true)
    }

    pub async fn init(&self, path: &Path, bare: bool) -> Result<()> {
        std::fs::create_dir_all(path).map_err(|e| GbrError::io_at(path, e))?;
        let mut args = vec!["init".to_string()];
        if bare {
            args.push("--bare".into());
        }
        self.checked(&args, Some(path), None).await?;
        Ok(())
    }

    pub async fn add_remote(&self, repo: &Path, name: &str, url: &str) -> Result<()> {
        self.checked(
            &["remote".into(), "add".into(), name.into(), url.into()],
            Some(repo),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn set_remote_url(&self, repo: &Path, name: &str, url: &str) -> Result<()> {
        self.checked(
            &["remote".into(), "set-url".into(), name.into(), url.into()],
            Some(repo),
            None,
        )
        .await
        .map(|_| ())
    }

    pub async fn push(
        &self,
        repo: &Path,
        remote: &str,
        refspec: &str,
        force: bool,
        token: Option<&str>,
    ) -> Result<String> {
        let mut args = vec!["push".to_string(), remote.to_string(), refspec.to_string()];
        if force {
            args.push("--force-with-lease".into());
        }
        self.checked(&args, Some(repo), token).await
    }

    pub async fn push_mirror(
        &self,
        repo: &Path,
        remote_url: &str,
        force: bool,
        token: Option<&str>,
    ) -> Result<String> {
        let mut args = vec![
            "push".to_string(),
            "--mirror".to_string(),
            remote_url.to_string(),
        ];
        if force {
            args.push("--force".into());
        }
        self.checked(&args, Some(repo), token).await
    }

    pub async fn status_porcelain(&self, repo: &Path) -> Result<String> {
        self.checked(&["status".into(), "--porcelain".into()], Some(repo), None)
            .await
    }

    pub async fn has_commits(&self, repo: &Path) -> Result<bool> {
        let out = self
            .run(
                &[
                    "rev-parse".into(),
                    "--verify".into(),
                    "--quiet".into(),
                    "HEAD".into(),
                ],
                Some(repo),
            )
            .await?;
        Ok(out.status == 0)
    }

    pub async fn current_branch(&self, repo: &Path) -> Result<Option<String>> {
        let out = self
            .run(
                &["symbolic-ref".into(), "--short".into(), "HEAD".into()],
                Some(repo),
            )
            .await?;
        Ok(if out.status == 0 {
            Some(out.stdout.trim().to_string())
        } else {
            None
        })
    }

    pub async fn default_remote_branch(&self, repo: &Path) -> Result<Option<String>> {
        let out = self
            .run(
                &[
                    "symbolic-ref".into(),
                    "--short".into(),
                    "refs/remotes/origin/HEAD".into(),
                ],
                Some(repo),
            )
            .await?;
        if out.status == 0 {
            let s = out.stdout.trim().to_string();
            return Ok(s.strip_prefix("origin/").map(|b| b.to_string()));
        }
        // fall back to the first remote head
        let out = self
            .run(
                &[
                    "for-each-ref".into(),
                    "--format=%(refname:short)".into(),
                    "refs/remotes/origin".into(),
                ],
                Some(repo),
            )
            .await?;
        Ok(out
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .find(|l| l != "origin/HEAD")
            .and_then(|l| l.strip_prefix("origin/").map(|b| b.to_string())))
    }

    /// Directory size in bytes (objects, packs, LFS store, worktree).
    pub fn dir_size(path: &Path) -> u64 {
        walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum()
    }
}

/// Public clone URL for a repository, with credentials never embedded.
pub fn https_clone_url(owner: &str, name: &str) -> String {
    format!("https://github.com/{owner}/{name}.git")
}

pub fn local_path_str(p: &Path) -> String {
    PathBuf::from(p).to_string_lossy().into_owned()
}
