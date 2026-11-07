use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::auth::{self, API_BASE};
use crate::error::{GbrError, Result};
use crate::git::GitCli;

/// Repository as returned by GitHub, mapped into our local model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub github_id: i64,
    pub node_id: Option<String>,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub is_private: bool,
    pub is_fork: bool,
    pub is_archived: bool,
    pub language: Option<String>,
    pub default_branch: Option<String>,
    pub size_kb: Option<i64>,
    pub html_url: Option<String>,
    pub clone_url: Option<String>,
    pub pushed_at: Option<String>,
    pub updated_at: Option<String>,
    /// Populated from our local database, never from the GitHub API.
    #[serde(default)]
    pub last_backup_status: Option<String>,
    #[serde(default)]
    pub last_backup_at: Option<String>,
}

impl Repo {
    pub fn clone_url_or(&self) -> String {
        self.clone_url
            .clone()
            .unwrap_or_else(|| format!("https://github.com/{}.git", self.full_name))
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct RateInfo {
    pub remaining: u32,
    pub limit: u32,
    pub reset_unix: i64,
}

/// GitHub REST API client. All requests are HTTPS, authenticated with the
/// token stored in the OS credential manager.
pub struct GitHubClient {
    http: reqwest::Client,
    token: String,
}

impl GitHubClient {
    pub async fn connect() -> Result<Self> {
        let token = auth::require_token().await?;
        Self::with_token(&token).await
    }

    pub async fn with_token(token: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(format!("git-backup-restore/{}", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| GbrError::Other(e.to_string()))?;
        Ok(Self {
            http,
            token: token.to_string(),
        })
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let resp = req.send().await?;
        let status = resp.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GbrError::AuthRequired(
                "GitHub rejected the stored credentials (expired or revoked)".into(),
            ));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            let reset = resp
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("0")
                .to_string();
            let remaining = resp
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("0")
                .to_string();
            if remaining == "0" {
                let reset_at = reset
                    .parse::<i64>()
                    .map(|t| {
                        chrono::DateTime::from_timestamp(t, 0)
                            .map(|d| d.to_rfc3339())
                            .unwrap_or(reset.clone())
                    })
                    .unwrap_or(reset);
                return Err(GbrError::RateLimited { reset_at });
            }
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or_else(|| {
                    if body.trim().is_empty() {
                        "request failed".into()
                    } else {
                        body.chars().take(300).collect()
                    }
                });
            return Err(GbrError::GitHub {
                status: status.as_u16(),
                message,
            });
        }
        Ok(resp)
    }

    pub async fn rate_limit(&self) -> Result<RateInfo> {
        let resp = self
            .send(self.request(reqwest::Method::GET, &format!("{API_BASE}/rate_limit")))
            .await?;
        let v: serde_json::Value = resp.json().await?;
        let core = v
            .get("resources")
            .and_then(|r| r.get("core"))
            .cloned()
            .unwrap_or_default();
        Ok(RateInfo {
            remaining: core.get("remaining").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            limit: core.get("limit").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            reset_unix: core.get("reset").and_then(|x| x.as_i64()).unwrap_or(0),
        })
    }

    /// First page of repositories (used by tests and simple listings).
    pub async fn list_repos_page(&self, page: u32, per_page: u32) -> Result<Vec<Repo>> {
        let url = format!(
            "{API_BASE}/user/repos?affiliation=owner%2Ccollaborator%2Corganization_member&sort=full_name&per_page={per_page}&page={page}"
        );
        let resp = self.send(self.request(reqwest::Method::GET, &url)).await?;
        let items: Vec<serde_json::Value> = resp.json().await?;
        Ok(items.iter().filter_map(parse_repo).collect())
    }

    pub async fn get_user(&self) -> Result<serde_json::Value> {
        let resp = self
            .send(self.request(reqwest::Method::GET, &format!("{API_BASE}/user")))
            .await?;
        Ok(resp.json().await?)
    }

    pub async fn list_orgs(&self) -> Result<Vec<String>> {
        let resp = self
            .send(self.request(
                reqwest::Method::GET,
                &format!("{API_BASE}/user/orgs?per_page=100"),
            ))
            .await?;
        let items: Vec<serde_json::Value> = resp.json().await?;
        Ok(items
            .iter()
            .filter_map(|o| o.get("login").and_then(|l| l.as_str()).map(String::from))
            .collect())
    }

    /// Create a repository under the authenticated user (or an org when
    /// `org` is given).
    pub async fn create_repo(
        &self,
        name: &str,
        description: Option<&str>,
        private: bool,
        auto_init: bool,
        org: Option<&str>,
    ) -> Result<serde_json::Value> {
        validate_repo_name(name)?;
        let url = match org {
            Some(o) => {
                if !o.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    return Err(GbrError::InvalidInput("invalid organization name".into()));
                }
                format!("{API_BASE}/orgs/{o}/repos")
            }
            None => format!("{API_BASE}/user/repos"),
        };
        let body = serde_json::json!({
            "name": name,
            "description": description,
            "private": private,
            "auto_init": auto_init,
            "has_issues": true,
            "has_projects": false,
            "has_wiki": false,
        });
        let resp = self
            .send(self.request(reqwest::Method::POST, &url).json(&body))
            .await?;
        Ok(resp.json().await?)
    }

    pub async fn get_repo(&self, full_name: &str) -> Result<serde_json::Value> {
        validate_repo_name(full_name)?;
        let resp = self
            .send(self.request(
                reqwest::Method::GET,
                &format!("{API_BASE}/repos/{full_name}"),
            ))
            .await?;
        Ok(resp.json().await?)
    }
}

fn validate_repo_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !name.starts_with('.')
        && !name.ends_with('.');
    if !ok {
        return Err(GbrError::InvalidInput(format!(
            "invalid repository name: {name:?}"
        )));
    }
    Ok(())
}

fn parse_repo(v: &serde_json::Value) -> Option<Repo> {
    Some(Repo {
        github_id: v.get("id")?.as_i64()?,
        node_id: v.get("node_id").and_then(|x| x.as_str()).map(String::from),
        owner: v.get("owner")?.get("login")?.as_str()?.to_string(),
        name: v.get("name")?.as_str()?.to_string(),
        full_name: v.get("full_name")?.as_str()?.to_string(),
        description: v
            .get("description")
            .and_then(|d| d.as_str())
            .map(String::from),
        is_private: v.get("private").and_then(|p| p.as_bool()).unwrap_or(false),
        is_fork: v.get("fork").and_then(|p| p.as_bool()).unwrap_or(false),
        is_archived: v.get("archived").and_then(|p| p.as_bool()).unwrap_or(false),
        language: v.get("language").and_then(|l| l.as_str()).map(String::from),
        default_branch: v
            .get("default_branch")
            .and_then(|l| l.as_str())
            .map(String::from),
        size_kb: v.get("size").and_then(|s| s.as_i64()),
        html_url: v.get("html_url").and_then(|s| s.as_str()).map(String::from),
        clone_url: v
            .get("clone_url")
            .and_then(|s| s.as_str())
            .map(String::from),
        pushed_at: v
            .get("pushed_at")
            .and_then(|s| s.as_str())
            .map(String::from),
        updated_at: v
            .get("updated_at")
            .and_then(|s| s.as_str())
            .map(String::from),
        last_backup_status: None,
        last_backup_at: None,
    })
}

/// Check that a local path exists and looks like a project directory.
pub fn check_local_project(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(GbrError::InvalidInput(format!(
            "path does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(GbrError::InvalidInput(format!(
            "not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Verify git is usable, returning its version string.
pub async fn check_git(git: &GitCli) -> Result<String> {
    git.version().await
}
