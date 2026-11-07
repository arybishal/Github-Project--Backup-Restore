use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::Db;
use crate::error::{GbrError, Result};
use crate::git::GitCli;
use crate::github::{GitHubClient, Repo};
use crate::manifest;
use crate::secretscan;
use crate::selective::{self, ArchiveFormat, SelectiveOptions};
use crate::tasks::TaskHandle;

/// GitHub rejects individual files over 100 MB; we warn well before that.
const LARGE_FILE_WARN: u64 = 50 * 1024 * 1024;
const LARGE_FILE_HARD: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishPreview {
    pub path: String,
    pub is_git_repo: bool,
    pub has_commits: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub large_files: Vec<LargeFile>,
    pub secret_hits: Vec<secretscan::SecretHit>,
    pub suggested_gitignore: String,
    pub current_branch: Option<String>,
    pub has_gitignore: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    pub path: String,
    pub repo_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub auto_init_readme: bool,
    pub commit_message: String,
    /// Push even when the remote already has commits we do not. Off by default.
    #[serde(default)]
    pub allow_force: bool,
    /// Connect to an already-existing remote instead of creating a repository.
    #[serde(default)]
    pub existing_remote_url: Option<String>,
    #[serde(default)]
    pub org: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishOutcome {
    pub repo_full_name: String,
    pub html_url: String,
    pub pushed_refs: usize,
    pub committed_files: usize,
}

/// Scan a local project and describe exactly what publishing would do.
pub async fn preview(path: &Path, git: &GitCli) -> Result<PublishPreview> {
    crate::github::check_local_project(path)?;

    let opts = SelectiveOptions {
        format: ArchiveFormat::Folder,
        include_hidden: true,
        include_ignored: false, // only what would actually be committed
        include_git_metadata: false,
        ..Default::default()
    };

    let files = selective::collect_selection(path, &opts)?;
    let paths: Vec<String> = files.iter().map(|(p, _, _)| p.clone()).collect();
    let secret_hits = secretscan::scan(&paths);

    let mut large: Vec<LargeFile> = files
        .iter()
        .filter(|(_, _, size)| *size >= LARGE_FILE_WARN)
        .map(|(p, _, size)| LargeFile {
            path: p.clone(),
            size: *size,
        })
        .collect();
    large.sort_by_key(|f| std::cmp::Reverse(f.size));

    let total_bytes: u64 = files.iter().map(|(_, _, s)| *s).sum();

    let is_git_repo = git.is_git_repo(path).await;
    let has_commits = if is_git_repo {
        git.has_commits(path).await.unwrap_or(false)
    } else {
        false
    };
    let current_branch = if is_git_repo {
        git.current_branch(path).await.unwrap_or(None)
    } else {
        None
    };
    let has_gitignore = path.join(".gitignore").exists();
    let suggested_gitignore = gitignore_template(path);

    let mut warnings = Vec::new();
    if !secret_hits.is_empty() {
        warnings.push(format!(
            "{} file(s) look like they contain secrets and would be published",
            secret_hits.len()
        ));
    }
    for f in &large {
        if f.size >= LARGE_FILE_HARD {
            warnings.push(format!(
                "{} is {} — GitHub will reject it (100 MB hard limit); use git-lfs",
                f.path,
                manifest::bytes_human(f.size)
            ));
        } else if f.size >= LARGE_FILE_WARN {
            warnings.push(format!(
                "{} is {} — close to GitHub's 100 MB limit",
                f.path,
                manifest::bytes_human(f.size)
            ));
        }
    }
    if is_git_repo && !has_commits {
        warnings
            .push("repository has no commits yet; the first push will create the branch".into());
    }

    Ok(PublishPreview {
        path: path.to_string_lossy().to_string(),
        is_git_repo,
        has_commits,
        file_count: files.len() as u64,
        total_bytes,
        large_files: large,
        secret_hits,
        suggested_gitignore,
        current_branch,
        has_gitignore,
        warnings,
    })
}

/// Publish a local project to GitHub: init if needed, commit if needed,
/// create the repository (unless connecting to an existing remote), push.
pub async fn run(
    db: &Db,
    git: &GitCli,
    task: &TaskHandle,
    gh: &GitHubClient,
    req: &PublishRequest,
) -> Result<PublishOutcome> {
    let path = PathBuf::from(&req.path);
    crate::github::check_local_project(&path)?;

    let repo_name = if req.repo_name.trim().is_empty() {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        req.repo_name.trim().to_string()
    };
    if repo_name.is_empty() {
        return Err(GbrError::InvalidInput("repository name is required".into()));
    }

    task.report("preparing", Some(5.0), None);
    if !git.is_git_repo(&path).await {
        task.report("initialising", Some(10.0), Some("git init".into()));
        git.init(&path, false).await?;
        // Ensure a sensible initial branch on fresh repos.
        let _ = git
            .checked(
                &[
                    "symbolic-ref".into(),
                    "HEAD".into(),
                    "refs/heads/main".into(),
                ],
                Some(&path),
                None,
            )
            .await;
    }
    task.ensure_active()?;

    // Secrets: refuse silently publishing credentials unless confirmed.
    let opts = SelectiveOptions {
        include_hidden: true,
        ..Default::default()
    };
    let files = selective::collect_selection(&path, &opts)?;
    let paths: Vec<String> = files.iter().map(|(p, _, _)| p.clone()).collect();
    let hits = secretscan::scan(&paths);
    if !hits.is_empty() {
        task.report(
            "secret-scan",
            Some(15.0),
            Some(format!("{} potential secret file(s) found", hits.len())),
        );
    }

    // .gitignore: create one when the project has none.
    if !path.join(".gitignore").exists() {
        let tpl = gitignore_template(&path);
        if !tpl.trim().is_empty() {
            std::fs::write(path.join(".gitignore"), &tpl)
                .map_err(|e| GbrError::io_at(path.join(".gitignore"), e))?;
        }
    }

    // README creation when requested and none exists yet.
    if req.auto_init_readme && !path.join("README.md").exists() {
        std::fs::write(path.join("README.md"), format!("# {repo_name}\n"))
            .map_err(|e| GbrError::io_at(path.join("README.md"), e))?;
    }

    task.ensure_active()?;
    task.report("committing", Some(30.0), None);

    let has_commits = git.has_commits(&path).await?;
    let status = git.status_porcelain(&path).await?;
    let mut committed_files = 0usize;

    if (!status.trim().is_empty() || !has_commits)
        && (!req.commit_message.trim().is_empty() || !has_commits)
    {
        let msg = if req.commit_message.trim().is_empty() {
            "Initial commit".to_string()
        } else {
            req.commit_message.clone()
        };
        git.checked(&["add".into(), "-A".into()], Some(&path), None)
            .await?;
        // `--allow-empty` is required for a brand-new repo whose files are
        // all ignored, otherwise the first push has nothing to send.
        let mut commit_args = vec!["commit".into(), "-m".into(), msg];
        if !has_commits {
            commit_args.push("--allow-empty".into());
        }
        git.checked(&commit_args, Some(&path), None).await?;
        committed_files = status.lines().count();
    }

    task.ensure_active()?;
    task.report("creating-repository", Some(50.0), None);

    let (html_url, full_name) = if let Some(remote) = &req.existing_remote_url {
        if git
            .run(
                &["remote".into(), "get-url".into(), "origin".into()],
                Some(&path),
            )
            .await
            .map(|o| o.status == 0)
            .unwrap_or(false)
        {
            git.set_remote_url(&path, "origin", remote).await?;
        } else {
            git.add_remote(&path, "origin", remote).await?;
        }
        let name = remote
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or(repo_name.as_str())
            .to_string();
        let owner = remote
            .trim_end_matches(".git")
            .split('/')
            .nth(3)
            .unwrap_or("")
            .to_string();
        (
            remote.clone(),
            if owner.is_empty() {
                name
            } else {
                format!("{owner}/{name}")
            },
        )
    } else {
        let created = gh
            .create_repo(
                &repo_name,
                req.description.as_deref(),
                req.private,
                false, // we push real history ourselves
                req.org.as_deref(),
            )
            .await?;
        let html = created
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let full = created
            .get("full_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        (html, full)
    };

    if full_name.is_empty() {
        return Err(GbrError::GitHub {
            status: 0,
            message: "repository creation returned no name".into(),
        });
    }

    task.report("pushing", Some(70.0), Some(full_name.clone()));
    let token = crate::auth::load_token()?.unwrap_or_default();

    // Refuse a non-fast-forward push unless the user opted in.
    if !req.allow_force && has_commits {
        if let Some(remote_sha) = remote_head_sha(git, &path, &full_name, Some(&token)).await {
            let local_sha = git.head_sha(&path).await?.unwrap_or_default();
            if !local_sha.is_empty() && remote_sha != local_sha {
                let base = common_ancestor(git, &path, &remote_sha).await;
                if base.as_deref() != Some(remote_sha.as_str()) {
                    return Err(GbrError::Conflict(
                        "remote branch has commits that are not in your local history; \
                         enable force push only if you intend to overwrite them"
                            .into(),
                    ));
                }
            }
        }
    }

    let force = req.allow_force;
    let token_ref = Some(token.as_str());
    let branch = git
        .current_branch(&path)
        .await?
        .unwrap_or_else(|| "main".to_string());
    let refspec = format!("HEAD:refs/heads/{branch}");
    let out = git
        .push(&path, "origin", &refspec, force, token_ref)
        .await?;

    task.report("verifying", Some(95.0), None);

    let pushed_refs = out.lines().filter(|l| l.contains("->")).count().max(1);

    record_publish_job(
        db,
        &req.path,
        &repo_name,
        if req.private { "private" } else { "public" },
        &html_url,
        "succeeded",
        Some(
            &hits
                .iter()
                .map(|h| format!("{} ({})", h.path, h.kind))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        None,
    )
    .await;

    crate::activity::log(
        db,
        crate::activity::LogLevel::Info,
        "publish",
        &format!("published {full_name} ({pushed_refs} ref(s) pushed)"),
        None,
    )
    .await;

    Ok(PublishOutcome {
        repo_full_name: full_name,
        html_url,
        pushed_refs,
        committed_files,
    })
}

async fn remote_head_sha(
    git: &GitCli,
    path: &Path,
    full_name: &str,
    token: Option<&str>,
) -> Option<String> {
    let url = format!("https://github.com/{full_name}.git");
    let out = git
        .run_with_token(
            &[
                "ls-remote".into(),
                "--heads".into(),
                url,
                "refs/heads/main".into(),
            ],
            Some(path),
            token,
        )
        .await
        .ok()?;
    out.stdout
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().next())
        .map(|s| s.to_string())
}

async fn common_ancestor(git: &GitCli, path: &Path, remote_sha: &str) -> Option<String> {
    let out = git
        .run(
            &["merge-base".into(), "HEAD".into(), remote_sha.to_string()],
            Some(path),
        )
        .await
        .ok()?;
    if out.status == 0 {
        Some(out.stdout.trim().to_string())
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)] // one row per publish job; a struct would just be noise
pub async fn record_publish_job(
    db: &Db,
    local_path: &str,
    repo_name: &str,
    visibility: &str,
    remote_url: &str,
    status: &str,
    warnings: Option<&str>,
    error: Option<&str>,
) {
    let now = Utc::now().to_rfc3339();
    if let Err(e) = sqlx::query(
        "INSERT INTO publish_jobs (id, local_path, repo_name, visibility, remote_url, status, warnings, started_at, finished_at, error)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(local_path)
    .bind(repo_name)
    .bind(visibility)
    .bind(remote_url)
    .bind(status)
    .bind(warnings)
    .bind(&now)
    .bind(error)
    .execute(db.pool())
    .await
    {
        tracing::warn!("could not record publish job: {e}");
    }
}

/// Best-effort .gitignore template based on what the project contains.
pub fn gitignore_template(path: &Path) -> String {
    let has = |f: &str| path.join(f).exists();
    let mut lines: Vec<&str> = Vec::new();

    if has("package.json") || has("node_modules") {
        lines.extend_from_slice(&["node_modules/", "dist/", "build/", ".npm", "*.log"]);
    }
    if has("Cargo.toml") {
        lines.extend_from_slice(&["/target/", "*.rs.bk"]);
    }
    if has("requirements.txt") || has("pyproject.toml") || has("setup.py") {
        lines.extend_from_slice(&["__pycache__/", "*.pyc", ".venv/", "venv/", "*.egg-info/"]);
    }
    if has(".env") || has(".env.local") {
        // Never let environment files travel to a public remote.
        lines.extend_from_slice(&[".env", ".env.*", "!.env.example"]);
    }
    if has("vendor") && has("go.mod") {
        lines.push("/vendor/");
    }
    lines.extend_from_slice(&[
        "",
        "# OS / editor",
        ".DS_Store",
        "Thumbs.db",
        ".idea/",
        ".vscode/",
    ]);

    let mut out = String::new();
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Local projects tracked in the Local Projects page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProject {
    pub path: String,
    pub name: String,
    pub is_git_repo: bool,
    pub added_at: String,
}

pub async fn list_local_projects(db: &Db) -> Result<Vec<LocalProject>> {
    let rows = sqlx::query("SELECT value FROM application_settings WHERE key = 'local_projects'")
        .fetch_all(db.pool())
        .await?;
    let raw: Option<String> = rows
        .first()
        .and_then(|r| r.try_get::<String, _>("value").ok());
    match raw {
        Some(json) => Ok(serde_json::from_str(&json).unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

pub async fn add_local_project(db: &Db, git: &GitCli, path: &Path) -> Result<LocalProject> {
    crate::github::check_local_project(path)?;
    let mut list = list_local_projects(db).await?;
    let path_str = path.to_string_lossy().to_string();
    if list.iter().any(|p| p.path == path_str) {
        return Err(GbrError::Conflict("project already added".into()));
    }
    let proj = LocalProject {
        path: path_str.clone(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path_str.clone()),
        is_git_repo: git.is_git_repo(path).await,
        added_at: Utc::now().to_rfc3339(),
    };
    list.push(proj.clone());
    crate::settings::set_raw(db, "local_projects", &list).await?;
    Ok(proj)
}

pub async fn remove_local_project(db: &Db, path: &str) -> Result<bool> {
    let mut list = list_local_projects(db).await?;
    let before = list.len();
    list.retain(|p| p.path != path);
    let removed = list.len() != before;
    crate::settings::set_raw(db, "local_projects", &list).await?;
    Ok(removed)
}

/// Helper used by the GUI: build a `Repo` from a GitHub API payload.
pub fn repo_from_json(v: &serde_json::Value) -> Option<Repo> {
    serde_json::from_value(v.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_covers_env_and_languages() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        let tpl = gitignore_template(dir.path());
        assert!(tpl.contains("node_modules/"));
        assert!(tpl.contains(".env"));
        assert!(!tpl.contains("SECRET"));
    }
}
