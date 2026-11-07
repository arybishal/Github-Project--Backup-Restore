use std::path::Path;

use serde::{Deserialize, Serialize};

/// Best-effort detection of credential material before anything is pushed to
/// a remote. Filename rules run always; content rules run for small text
/// files so large binaries are never read into memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretHit {
    pub path: String,
    pub kind: String,
}

const NAME_RULES: &[(&str, &str)] = &[
    ("^\\.env($|\\.)", "environment file"),
    ("\\.(pem|pfx|p12|keystore|jks)$", "private key / keystore"),
    ("(^|/)id_(rsa|dsa|ecdsa|ed25519)$", "SSH private key"),
    ("(^|/)credentials(\\.[^/]*)?$", "credentials file"),
    ("(^|/)secrets?(\\.[^/]*)?$", "secrets file"),
    ("(^|/)\\.netrc$", "netrc credentials"),
    ("(^|/)\\.git-credentials$", "stored git credentials"),
    ("(^|/)\\.npmrc$", "npm config (may hold an auth token)"),
    ("(^|/)\\.aws/credentials$", "AWS credentials"),
    ("(^|/)htpasswd$", "password file"),
];

const CONTENT_RULES: &[(&str, &str)] = &[
    (
        "-----BEGIN (RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY",
        "private key block",
    ),
    ("AKIA[0-9A-Z]{16}", "AWS access key id"),
    ("gh[pousr]_[A-Za-z0-9]{30,}", "GitHub token"),
    ("github_pat_[A-Za-z0-9_]{20,}", "GitHub fine-grained token"),
    ("xox[baprs]-[A-Za-z0-9-]{10,}", "Slack token"),
    ("AIza[0-9A-Za-z_-]{35}", "Google API key"),
    ("-----BEGIN CERTIFICATE-----", "certificate"),
];

const MAX_CONTENT_SCAN: u64 = 512 * 1024;
const MAX_HITS: usize = 50;

pub fn scan(rel_paths: &[String]) -> Vec<SecretHit> {
    let mut hits = Vec::new();
    // NAME_RULES are regex patterns (anchors, alternation), so they are
    // compiled with `regex` — globset treats `^`/`$` as literal characters.
    let name_matchers: Vec<(regex::Regex, &str)> = NAME_RULES
        .iter()
        .filter_map(|(p, k)| regex::Regex::new(p).ok().map(|re| (re, *k)))
        .collect();

    for rel in rel_paths {
        if hits.len() >= MAX_HITS {
            break;
        }
        let posix = rel.replace('\\', "/");
        if let Some((_, kind)) = name_matchers.iter().find(|(m, _)| m.is_match(&posix)) {
            hits.push(SecretHit {
                path: rel.clone(),
                kind: (*kind).to_string(),
            });
            continue;
        }
        if let Some(kind) = content_hit(Path::new(rel)) {
            hits.push(SecretHit {
                path: rel.clone(),
                kind: kind.to_string(),
            });
        }
    }
    hits
}

/// Content scan for a single file. `rel` is resolved against the process CWD
/// by callers that pass absolute paths; the GUI passes absolute paths.
fn content_hit(path: &Path) -> Option<&'static str> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_CONTENT_SCAN {
        return None;
    }
    // Only sniff plausible text files.
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    const BINARY_EXT: &[&str] = &[
        "png", "jpg", "jpeg", "gif", "webp", "ico", "pdf", "zip", "tar", "gz", "zst", "7z", "rar",
        "exe", "dll", "so", "dylib", "bin", "db", "sqlite", "sqlite3", "woff", "woff2", "ttf",
        "otf", "mp3", "mp4", "mov", "avi", "jar", "class", "wasm", "o", "a", "lib",
    ];
    if BINARY_EXT.contains(&ext.as_str()) {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None; // binary content
    }
    let text = String::from_utf8_lossy(&bytes);
    for (pat, kind) in CONTENT_RULES {
        if regex_like_contains(&text, pat) {
            return Some(kind);
        }
    }
    None
}

/// Tiny literal/interval matcher for the handful of content rules above —
/// avoids pulling a regex engine into the hot path of every publish scan.
/// Rules use a restricted syntax: literal text, `[A-Z]`/`[0-9]` classes,
/// `{n,}` repetition and one optional space group.
fn regex_like_contains(haystack: &str, rule: &str) -> bool {
    // For our fixed rule set a plain `regex` crate check is simpler and
    // safer than hand-rolling; `regex` is already in the dependency tree
    // transitively, but we keep an explicit lightweight fallback instead.
    match regex::Regex::new(rule) {
        Ok(re) => re.is_match(haystack),
        Err(_) => haystack.contains(rule),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_private_key_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("server_key");
        std::fs::write(&f, "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQ...\n").unwrap();
        let hits = scan(&[f.to_string_lossy().to_string()]);
        assert!(
            hits.iter().any(|h| h.kind.contains("private key")),
            "{hits:?}"
        );
    }

    #[test]
    fn detects_github_token_content() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("notes.txt");
        std::fs::write(&f, "token is ghp_1234567890abcdefghijklmnopqrstuvwxyzABCD").unwrap();
        let hits = scan(&[f.to_string_lossy().to_string()]);
        assert!(
            hits.iter().any(|h| h.kind.contains("GitHub token")),
            "{hits:?}"
        );
    }

    #[test]
    fn clean_file_is_not_flagged() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("main.rs");
        std::fs::write(&f, "fn main() { println!(\"hello\"); }").unwrap();
        let hits = scan(&[f.to_string_lossy().to_string()]);
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn name_rules_match_relative_paths() {
        let hits = scan(&[
            ".env".into(),
            "config/credentials.json".into(),
            "src/main.rs".into(),
        ]);
        assert_eq!(hits.len(), 2, "{hits:?}");
    }
}
