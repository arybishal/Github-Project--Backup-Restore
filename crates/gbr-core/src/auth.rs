use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::activity::{self, LogLevel};
use crate::db::Db;
use crate::error::{GbrError, Result};

const KEYRING_SERVICE: &str = "git-backup-restore";
const KEYRING_ACCOUNT: &str = "github-token";

/// Minimum scopes we ask for: private repo access + org membership listing.
pub const REQUESTED_SCOPES: &str = "repo read:org workflow";

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const API_BASE: &str = "https://api.github.com";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    pub username: String,
    pub avatar_url: Option<String>,
    pub scopes: Option<String>,
    pub auth_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

// ---------------------------------------------------------------- credential store

fn keyring_entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| GbrError::Credential(e.to_string()))
}

pub fn store_token(token: &str) -> Result<()> {
    keyring_entry()?
        .set_password(token)
        .map_err(|e| GbrError::Credential(e.to_string()))
}

pub fn load_token() -> Result<Option<String>> {
    match keyring_entry()?.get_password() {
        Ok(t) if t.trim().is_empty() => Ok(None),
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(GbrError::Credential(e.to_string())),
    }
}

pub fn clear_token() -> Result<bool> {
    match keyring_entry()?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(GbrError::Credential(e.to_string())),
    }
}

pub async fn require_token() -> Result<String> {
    load_token()?
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| GbrError::AuthRequired("no GitHub account connected".into()))
}

// ---------------------------------------------------------------- account records

pub async fn save_account(db: &Db, info: &AuthInfo, method: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO accounts (id, provider, username, avatar_url, scopes, auth_method, created_at, updated_at)
         VALUES ('github', 'github', ?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(id) DO UPDATE SET username = excluded.username, avatar_url = excluded.avatar_url,
             scopes = excluded.scopes, auth_method = excluded.auth_method, updated_at = excluded.updated_at",
    )
    .bind(&info.username)
    .bind(&info.avatar_url)
    .bind(&info.scopes)
    .bind(method)
    .bind(&now)
    .execute(db.pool())
    .await?;
    activity::log(
        db,
        LogLevel::Info,
        "auth",
        &format!("connected GitHub account @{}", info.username),
        None,
    )
    .await;
    Ok(())
}

pub async fn current_account(db: &Db) -> Result<Option<AuthInfo>> {
    let row = sqlx::query(
        "SELECT username, avatar_url, scopes, auth_method FROM accounts WHERE id = 'github'",
    )
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(|r| AuthInfo {
        username: r.try_get("username").unwrap_or_default(),
        avatar_url: r.try_get("avatar_url").ok().flatten(),
        scopes: r.try_get("scopes").ok().flatten(),
        auth_method: r.try_get("auth_method").unwrap_or_else(|_| "token".into()),
    }))
}

/// Disconnect: drops the stored token and the account record. Never touches
/// repository or backup data.
pub async fn logout(db: &Db) -> Result<bool> {
    let had_token = clear_token()?;
    sqlx::query("DELETE FROM accounts WHERE id = 'github'")
        .execute(db.pool())
        .await?;
    if had_token {
        activity::log(
            db,
            LogLevel::Info,
            "auth",
            "disconnected GitHub account",
            None,
        )
        .await;
    }
    Ok(had_token)
}

// ---------------------------------------------------------------- login flows

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("git-backup-restore/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| GbrError::Other(e.to_string()))
}

/// Validate a personal access token / fine-grained token against the API and
/// persist it in the OS credential manager.
pub async fn login_with_token(db: &Db, token: &str) -> Result<AuthInfo> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(GbrError::InvalidInput("token is empty".into()));
    }
    let client = http()?;
    let resp = client
        .get(format!("{API_BASE}/user"))
        .bearer_auth(&token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GbrError::AuthRequired(
            "token was rejected by GitHub".into(),
        ));
    }
    if !resp.status().is_success() {
        return Err(GbrError::GitHub {
            status: resp.status().as_u16(),
            message: "could not validate token".into(),
        });
    }

    let user: serde_json::Value = resp.json().await?;
    let scopes = user
        .get("token")
        .and_then(|t| t.get("scopes"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let info = AuthInfo {
        username: user
            .get("login")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        avatar_url: user
            .get("avatar_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        scopes: scopes
            .clone()
            .or_else(|| Some(REQUESTED_SCOPES.to_string())),
        auth_method: "token".into(),
    };

    store_token(&token)?;
    save_account(db, &info, "token").await?;
    Ok(info)
}

/// Step 1 of the Device Authorization Flow. Requires the user to register an
/// OAuth application (only the public client id is needed — device flow never
/// uses a client secret).
pub async fn start_device_flow(client_id: &str) -> Result<DeviceCode> {
    if client_id.trim().is_empty() {
        return Err(GbrError::InvalidInput(
            "no GitHub OAuth client id configured (Settings → GitHub)".into(),
        ));
    }
    let client = http()?;
    let resp = client
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", REQUESTED_SCOPES)])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(GbrError::GitHub {
            status: resp.status().as_u16(),
            message: "device code request failed".into(),
        });
    }
    let body: DeviceCodeResponse = resp.json().await?;
    Ok(DeviceCode {
        device_code: body.device_code,
        user_code: body.user_code,
        verification_uri: body.verification_uri,
        expires_in: body.expires_in,
        interval: body.interval.max(5),
    })
}

/// Step 2: poll until the user approves in the browser, the flow expires, or
/// `should_cancel` returns true.
pub async fn complete_device_flow<F>(
    db: &Db,
    client_id: &str,
    code: &DeviceCode,
    mut should_cancel: F,
) -> Result<AuthInfo>
where
    F: FnMut() -> bool + Send,
{
    let client = http()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(code.expires_in);
    let mut interval = std::time::Duration::from_secs(code.interval);

    loop {
        if should_cancel() {
            return Err(GbrError::AuthRequired("login cancelled".into()));
        }
        if std::time::Instant::now() >= deadline {
            return Err(GbrError::AuthRequired("device code expired".into()));
        }

        let resp = client
            .post(ACCESS_TOKEN_URL)
            .form(&[
                ("client_id", client_id),
                ("device_code", &code.device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .header("Accept", "application/json")
            .send()
            .await?;

        let body: TokenResponse = resp.json().await?;
        match body.error.as_deref() {
            None => {
                let token = body
                    .access_token
                    .ok_or_else(|| GbrError::AuthRequired("no token returned".into()))?;
                let mut info = login_with_token(db, &token).await?;
                if let Some(sc) = body.scope {
                    info.scopes = Some(sc);
                    save_account(db, &info, "device_flow").await?;
                }
                return Ok(info);
            }
            Some("authorization_pending") => {}
            Some("slow_down") => interval += std::time::Duration::from_secs(5),
            Some("expired_token") => {
                return Err(GbrError::AuthRequired("device code expired".into()))
            }
            Some("access_denied") => return Err(GbrError::AuthRequired("login was denied".into())),
            Some(other) => {
                return Err(GbrError::AuthRequired(format!(
                    "{other}: {}",
                    body.error_description.unwrap_or_default()
                )))
            }
        }

        tokio::time::sleep(interval).await;
    }
}
