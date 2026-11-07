use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{GbrError, Result};
use crate::VERSION;

pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/arybishal/Github-Project--Backup-Restore/releases/latest";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfoDto {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_notes: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    html_url: String,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

pub fn parse_version(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches('v');
    let mut parts = clean.split('.').filter_map(|p| p.parse::<u32>().ok());
    let major = parts.next().unwrap_or(0);
    let minor = parts.next().unwrap_or(0);
    let patch = parts.next().unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn pick_asset_for_platform(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    #[cfg(windows)]
    {
        if let Some(a) = assets
            .iter()
            .find(|a| a.name.ends_with("-setup.exe") || a.name.ends_with("-Setup.exe"))
        {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".exe")) {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".msi")) {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower.contains("windows") && lower.ends_with(".zip")
        }) {
            return Some(a);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".dmg")) {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower.contains("macos") && lower.ends_with(".tar.gz")
        }) {
            return Some(a);
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".AppImage")) {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| a.name.ends_with(".deb")) {
            return Some(a);
        }
        if let Some(a) = assets.iter().find(|a| {
            let lower = a.name.to_lowercase();
            lower.contains("linux") && lower.ends_with(".tar.gz")
        }) {
            return Some(a);
        }
    }

    assets.first()
}

pub async fn check_for_updates() -> Result<UpdateInfoDto> {
    let client = reqwest::Client::builder()
        .user_agent(format!("GitBackupRestore/{}", VERSION))
        .build()
        .map_err(|e| GbrError::Other(e.to_string()))?;

    let resp = client
        .get(LATEST_RELEASE_URL)
        .send()
        .await
        .map_err(|e| GbrError::Other(format!("failed to query GitHub releases: {e}")))?;

    if !resp.status().is_success() {
        return Err(GbrError::Other(format!(
            "GitHub release check failed: HTTP {}",
            resp.status()
        )));
    }

    let release: GithubRelease = resp
        .json()
        .await
        .map_err(|e| GbrError::Other(format!("failed to parse GitHub release JSON: {e}")))?;

    let clean_tag = release.tag_name.trim_start_matches('v').to_string();
    let is_newer = is_newer_version(&clean_tag, VERSION);
    let matched_asset = pick_asset_for_platform(&release.assets);

    Ok(UpdateInfoDto {
        current_version: VERSION.to_string(),
        latest_version: clean_tag,
        update_available: is_newer,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_notes: release.body.unwrap_or_default(),
        release_url: release.html_url,
        published_at: release.published_at,
        download_url: matched_asset.map(|a| a.browser_download_url.clone()),
        asset_name: matched_asset.map(|a| a.name.clone()),
        asset_size: matched_asset.map(|a| a.size),
    })
}

pub async fn download_asset(url: &str, target_path: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent(format!("GitBackupRestore/{}", VERSION))
        .build()
        .map_err(|e| GbrError::Other(e.to_string()))?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| GbrError::Other(format!("failed to download asset: {e}")))?;

    if !resp.status().is_success() {
        return Err(GbrError::Other(format!(
            "download failed: HTTP {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| GbrError::Other(format!("failed to read asset body: {e}")))?;

    if let Some(p) = target_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }

    std::fs::write(target_path, bytes).map_err(|e| GbrError::io_at(target_path, e))?;
    Ok(())
}

pub fn download_dir() -> PathBuf {
    std::env::temp_dir().join("GitBackupRestore-Updates")
}

pub fn launch_installer(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| GbrError::Other(format!("failed to launch installer: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| GbrError::Other(format!("failed to open update package: {e}")))?;
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| GbrError::Other(format!("failed to open update package: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing_and_comparison() {
        assert_eq!(parse_version("0.1.0"), (0, 1, 0));
        assert_eq!(parse_version("v0.1.0"), (0, 1, 0));
        assert_eq!(parse_version("v1.2.3"), (1, 2, 3));
        assert!(is_newer_version("0.2.0", "0.1.0"));
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.0.9", "0.1.0"));
    }
}
