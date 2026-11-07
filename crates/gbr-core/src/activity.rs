use chrono::Utc;
use serde::Serialize;
use sqlx::Row;

use crate::db::Db;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }

    pub fn parse(s: &str) -> LogLevel {
        match s.to_ascii_lowercase().as_str() {
            "debug" => LogLevel::Debug,
            "warn" | "warning" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityEntry {
    pub id: i64,
    pub ts: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub details: Option<String>,
}

/// Best-effort scrub of anything that looks like a credential before it can be
/// persisted or exported. GitHub tokens (`ghp_`, `gho_`, `ghs_`, `github_pat_`),
/// `Authorization:` headers and `token=` query params are removed.
pub fn redact(input: &str) -> String {
    let mut out = input.to_string();
    for pattern in [
        "ghp_",
        "gho_",
        "ghu_",
        "ghs_",
        "ghr_",
        "github_pat_",
        "Bearer ",
        "authorization:",
    ] {
        if let Some(idx) = out.to_ascii_lowercase().find(&pattern.to_ascii_lowercase()) {
            let tail = &out[idx + pattern.len()..];
            // redact until the next delimiter
            let cut = tail
                .find(['\'', '"', ' ', '\n', '\r'])
                .unwrap_or(tail.len());
            out.replace_range(idx + pattern.len()..idx + pattern.len() + cut, " ***");
        }
    }
    // token=... query parameter style
    if let Some(idx) = out.to_ascii_lowercase().find("token=") {
        let tail = &out[idx + 6..];
        let cut = tail.find(['&', '"', '\'', ' ']).unwrap_or(tail.len());
        out.replace_range(idx + 6..idx + 6 + cut, "***");
    }
    out
}

pub async fn log(db: &Db, level: LogLevel, source: &str, message: &str, details: Option<&str>) {
    let ts = Utc::now().to_rfc3339();
    let message = redact(message);
    let details = details.map(redact);
    if let Err(e) = sqlx::query(
        "INSERT INTO activity_logs (ts, level, source, message, details) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&ts)
    .bind(level.as_str())
    .bind(source)
    .bind(&message)
    .bind(&details)
    .execute(db.pool())
    .await
    {
        tracing::warn!("failed to persist activity log: {e}");
    }
    match level {
        LogLevel::Error => tracing::error!("[{source}] {message}"),
        LogLevel::Warn => tracing::warn!("[{source}] {message}"),
        LogLevel::Debug => tracing::debug!("[{source}] {message}"),
        LogLevel::Info => tracing::info!("[{source}] {message}"),
    }
}

pub async fn list(
    db: &Db,
    level: Option<&str>,
    search: Option<&str>,
    limit: i64,
) -> Result<Vec<ActivityEntry>> {
    let level = level.unwrap_or("").to_string();
    let search = search.unwrap_or("").to_string();
    let rows = sqlx::query(
        "SELECT id, ts, level, source, message, details FROM activity_logs
         WHERE (?1 = '' OR level = ?1)
           AND (?2 = '' OR message LIKE '%' || ?2 || '%' OR source LIKE '%' || ?2 || '%' OR details LIKE '%' || ?2 || '%')
         ORDER BY id DESC LIMIT ?3",
    )
    .bind(&level)
    .bind(&search)
    .bind(limit)
    .fetch_all(db.pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ActivityEntry {
            id: r.try_get("id").unwrap_or(0),
            ts: r.try_get("ts").unwrap_or_default(),
            level: r.try_get("level").unwrap_or_default(),
            source: r.try_get("source").unwrap_or_default(),
            message: r.try_get("message").unwrap_or_default(),
            details: r.try_get("details").unwrap_or(None),
        })
        .collect())
}

pub async fn export_text(db: &Db, limit: i64) -> Result<String> {
    let entries = list(db, None, None, limit).await?;
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "[{}] {} {}: {}",
            e.ts, e.level, e.source, e.message
        ));
        if let Some(d) = e.details {
            out.push_str(&format!(" | {d}"));
        }
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens() {
        let s = redact("failed with token ghp_abc123DEF456ghijk and token=xyz789 end");
        assert!(!s.contains("ghp_abc123DEF456ghijk"), "leaked: {s}");
        assert!(!s.contains("xyz789"), "leaked: {s}");
        let s2 = redact("Authorization: Bearer github_pat_11AAAA222333");
        assert!(!s2.contains("github_pat_11AAAA222333"), "leaked: {s2}");
    }
}
