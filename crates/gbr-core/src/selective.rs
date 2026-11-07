use std::path::{Path, PathBuf};

use globset::{Glob, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::error::{GbrError, Result};
use crate::manifest::{self, FileEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Folder,
    Zip,
    Tar,
    TarZst,
}

impl ArchiveFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "folder" | "dir" => Ok(ArchiveFormat::Folder),
            "zip" => Ok(ArchiveFormat::Zip),
            "tar" => Ok(ArchiveFormat::Tar),
            "tar.zst" | "taz" | "tzst" => Ok(ArchiveFormat::TarZst),
            other => Err(GbrError::InvalidInput(format!("unknown format: {other}"))),
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveFormat::Folder => "",
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarZst => "tar.zst",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ArchiveFormat::Folder => "folder",
            ArchiveFormat::Zip => "zip",
            ArchiveFormat::Tar => "tar",
            ArchiveFormat::TarZst => "tar.zst",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectiveOptions {
    pub format: ArchiveFormat,
    /// Glob patterns; empty means "everything not excluded".
    #[serde(default)]
    pub include_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub include_hidden: bool,
    /// Include files that .gitignore excludes (and .gitignore itself).
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default)]
    pub include_git_metadata: bool,
    /// User explicitly confirmed that secret-looking files are intended.
    #[serde(default)]
    pub sensitive_acknowledged: bool,
    #[serde(default = "default_compression")]
    pub compression_level: u32,
}

fn default_compression() -> u32 {
    6
}

impl Default for SelectiveOptions {
    fn default() -> Self {
        Self {
            format: ArchiveFormat::Folder,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            include_hidden: false,
            include_ignored: false,
            include_git_metadata: false,
            sensitive_acknowledged: false,
            compression_level: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveHit {
    pub path: String,
    pub kind: String,
}

const SENSITIVE_RULES: &[(&str, &str)] = &[
    ("^\\.env($|\\.)", "environment file"),
    ("\\.(pem|key|pfx|p12|keystore|jks)$", "private key material"),
    ("(^|/)id_(rsa|dsa|ecdsa|ed25519)(\\.pub)?$", "SSH key"),
    (
        "(^|/)(credentials|secrets?)(\\.[^/]*)?$",
        "credentials file",
    ),
    ("(^|/)\\.netrc$", "netrc credentials"),
    ("(^|/)\\.npmrc$", "npm config (may contain auth token)"),
    ("(^|/)\\.aws/credentials$", "AWS credentials"),
    ("(^|/)\\.git-credentials$", "stored git credentials"),
    ("(^|/)[^/]*\\.pem$", "certificate/key"),
    ("(^|/)htpasswd$", "password file"),
];

pub fn scan_sensitive(rel_paths: &[String]) -> Vec<SensitiveHit> {
    // SENSITIVE_RULES are regex patterns; globset would treat `^`/`$` as
    // literal characters and silently match nothing.
    let compiled: Vec<(regex::Regex, &str)> = SENSITIVE_RULES
        .iter()
        .filter_map(|(pat, kind)| regex::Regex::new(pat).ok().map(|re| (re, *kind)))
        .collect();

    rel_paths
        .iter()
        .filter_map(|p| {
            let posix = p.replace('\\', "/");
            let (_, kind) = compiled.iter().find(|(m, _)| m.is_match(&posix))?;
            Some(SensitiveHit {
                path: p.clone(),
                kind: (*kind).to_string(),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- selection

fn build_sets(
    opts: &SelectiveOptions,
) -> Result<(Option<globset::GlobSet>, Option<globset::GlobSet>)> {
    let build = |pats: &[String]| -> Result<Option<globset::GlobSet>> {
        if pats.is_empty() {
            return Ok(None);
        }
        let mut b = GlobSetBuilder::new();
        for p in pats {
            // A bare directory name ("build", "dist") should match its whole
            // subtree, which is what users expect from exclude patterns.
            let pattern = if p.contains('*') || p.contains('?') || p.contains('[') {
                p.clone()
            } else {
                format!("**/{p}/**")
            };
            b.add(
                Glob::new(&pattern)
                    .map_err(|e| GbrError::InvalidInput(format!("bad glob {p:?}: {e}")))?,
            );
            b.add(
                Glob::new(p).map_err(|e| GbrError::InvalidInput(format!("bad glob {p:?}: {e}")))?,
            );
        }
        Ok(Some(
            b.build()
                .map_err(|e| GbrError::InvalidInput(e.to_string()))?,
        ))
    };
    Ok((
        build(&opts.include_patterns)?,
        build(&opts.exclude_patterns)?,
    ))
}

/// Walk `root` and return the files selected by `opts`, sorted for stable
/// manifests. Never returns `.git` internals unless explicitly requested.
pub fn collect_selection(
    root: &Path,
    opts: &SelectiveOptions,
) -> Result<Vec<(String, PathBuf, u64)>> {
    if !root.is_dir() {
        return Err(GbrError::InvalidInput(format!(
            "not a directory: {}",
            root.display()
        )));
    }
    let (includes, excludes) = build_sets(opts)?;
    let mut out = Vec::new();

    // Two walk strategies behind one candidate list so the selection logic
    // below stays single-sourced. Entry filtering is done per entry rather
    // than with `filter_entry` so no closure outlives the borrow of `root`.
    let mut candidates: Vec<PathBuf> = Vec::new();
    if opts.include_ignored {
        for entry in walkdir::WalkDir::new(root) {
            let Ok(e) = entry else { continue };
            if !opts.include_git_metadata && is_git_internal(e.path(), root) {
                continue;
            }
            candidates.push(e.into_path());
        }
    } else {
        // `ignore` crate applies .gitignore / .ignore rules.
        let mut b = ignore::WalkBuilder::new(root);
        b.hidden(false); // hidden files are filtered explicitly so patterns can opt in
        b.git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .parents(true);
        for entry in b.build() {
            let Ok(e) = entry else { continue };
            if !opts.include_git_metadata && is_git_internal(e.path(), root) {
                continue;
            }
            candidates.push(e.into_path());
        }
    }

    for path in candidates {
        if path == root {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| GbrError::Other(e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() {
            continue;
        }

        let is_hidden = rel
            .split('/')
            .any(|c| c.starts_with('.') && c != "." && c != "..");
        if is_hidden && !opts.include_hidden {
            // an explicit include pattern may opt a hidden path back in
            let explicitly_included = includes.as_ref().map(|s| s.is_match(&rel)).unwrap_or(false);
            if !explicitly_included {
                continue;
            }
        }

        if let Some(ex) = &excludes {
            if ex.is_match(&rel) {
                continue;
            }
        }
        if let Some(inc) = &includes {
            if !inc.is_match(&rel) {
                continue;
            }
        }

        let meta = match path.symlink_metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_dir() {
            continue;
        }
        if meta.is_symlink() {
            // Record symlinks only when they stay inside root; anything else
            // could point at sensitive host paths outside the project.
            if let Ok(target) = std::fs::read_link(&path) {
                let resolved = if target.is_absolute() {
                    target.clone()
                } else {
                    path.parent().unwrap_or(root).join(target)
                };
                if !resolved.starts_with(root) {
                    continue;
                }
            }
        }
        out.push((rel, path.clone(), meta.len()));
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn is_git_internal(path: &Path, root: &Path) -> bool {
    match path.strip_prefix(root) {
        Ok(rel) => {
            rel.components().next().map(|c| c.as_os_str()) == Some(std::ffi::OsStr::new(".git"))
        }
        Err(_) => false,
    }
}

// ---------------------------------------------------------------- archiving

/// Hash every selected file (streamed, 64 KiB blocks).
pub fn hash_files(files: &[(String, PathBuf, u64)]) -> Result<Vec<FileEntry>> {
    let mut out = Vec::with_capacity(files.len());
    for (rel, abs, size) in files {
        out.push(FileEntry {
            path: rel.clone(),
            size: *size,
            sha256: manifest::sha256_file(abs)?,
        });
    }
    Ok(out)
}

/// Materialise the selection at `dest`:
/// - `Folder` copies files preserving structure
/// - `Zip` / `Tar` / `TarZst` write a single archive
///
/// Returns (payload_path, total_bytes, file_count).
pub fn write_payload(
    files: &[(String, PathBuf, u64)],
    dest_dir: &Path,
    file_stem: &str,
    opts: &SelectiveOptions,
) -> Result<(PathBuf, u64, u64)> {
    std::fs::create_dir_all(dest_dir).map_err(|e| GbrError::io_at(dest_dir, e))?;

    match opts.format {
        ArchiveFormat::Folder => {
            let dest = dest_dir.join(file_stem);
            if dest.exists() {
                return Err(GbrError::Conflict(format!(
                    "destination exists: {}",
                    dest.display()
                )));
            }
            std::fs::create_dir_all(&dest).map_err(|e| GbrError::io_at(&dest, e))?;
            let mut total = 0u64;
            for (rel, abs, size) in files {
                let target = dest.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
                if let Some(p) = target.parent() {
                    std::fs::create_dir_all(p).map_err(|e| GbrError::io_at(p, e))?;
                }
                std::fs::copy(abs, &target).map_err(|e| GbrError::io_at(&target, e))?;
                total += size;
            }
            Ok((dest, total, files.len() as u64))
        }
        ArchiveFormat::Zip => {
            let dest = dest_dir.join(format!("{file_stem}.zip"));
            write_zip(files, &dest, opts.compression_level)?;
            let size = manifest::dir_size(&dest);
            Ok((dest, size, files.len() as u64))
        }
        ArchiveFormat::Tar => {
            let dest = dest_dir.join(format!("{file_stem}.tar"));
            write_tar(files, &dest, None, 0)?;
            let size = manifest::dir_size(&dest);
            Ok((dest, size, files.len() as u64))
        }
        ArchiveFormat::TarZst => {
            let dest = dest_dir.join(format!("{file_stem}.tar.zst"));
            write_tar(files, &dest, Some(opts.compression_level), 0)?;
            let size = manifest::dir_size(&dest);
            Ok((dest, size, files.len() as u64))
        }
    }
}

fn write_zip(files: &[(String, PathBuf, u64)], dest: &Path, level: u32) -> Result<()> {
    use std::io::{Read, Write};

    let file = std::fs::File::create(dest).map_err(|e| GbrError::io_at(dest, e))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(level.clamp(1, 10) as i64));

    for (rel, abs, _) in files {
        let mut f = std::fs::File::open(abs).map_err(|e| GbrError::io_at(abs, e))?;
        zip.start_file(rel.as_str(), opts)
            .map_err(|e| GbrError::Other(format!("zip: {e}")))?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| GbrError::io_at(abs, e))?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n])
                .map_err(|e| GbrError::io_at(dest, e))?;
        }
    }
    zip.finish()
        .map_err(|e| GbrError::Other(format!("zip finish: {e}")))?;
    Ok(())
}

fn write_tar(
    files: &[(String, PathBuf, u64)],
    dest: &Path,
    zstd_level: Option<u32>,
    _pad: u64,
) -> Result<()> {
    use std::io::Write;

    let file = std::fs::File::create(dest).map_err(|e| GbrError::io_at(dest, e))?;
    match zstd_level {
        None => {
            let mut builder = tar::Builder::new(file);
            append_tar(&mut builder, files)?;
            builder
                .finish()
                .map_err(|e| GbrError::Other(format!("tar finish: {e}")))?;
        }
        Some(level) => {
            let encoder = zstd::Encoder::new(file, level.clamp(1, 22) as i32)
                .map_err(|e| GbrError::Other(format!("zstd: {e}")))?;
            let mut builder = tar::Builder::new(encoder);
            append_tar(&mut builder, files)?;
            let encoder = builder
                .into_inner()
                .map_err(|e| GbrError::Other(format!("tar: {e}")))?;
            let mut encoder = encoder
                .finish()
                .map_err(|e| GbrError::Other(format!("zstd finish: {e}")))?;
            encoder.flush().map_err(|e| GbrError::io_at(dest, e))?;
        }
    }
    Ok(())
}

fn append_tar<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    files: &[(String, PathBuf, u64)],
) -> Result<()> {
    for (rel, abs, _) in files {
        builder
            .append_path_with_name(abs, rel)
            .map_err(|e| GbrError::Other(format!("tar append {rel}: {e}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn opts() -> SelectiveOptions {
        SelectiveOptions::default()
    }

    #[test]
    fn sensitive_scan_finds_secrets() {
        let hits = scan_sensitive(&[
            "src/main.rs".into(),
            ".env".into(),
            ".env.production".into(),
            "certs/server.pem".into(),
            "config/credentials.json".into(),
            "id_rsa".into(),
        ]);
        assert_eq!(hits.len(), 5, "{hits:?}");
        assert!(!scan_sensitive(&["README.md".into(), "src/lib.rs".into()])
            .into_iter()
            .any(|h| h.kind == "environment file"));
    }

    #[test]
    fn selection_respects_hidden_and_excludes() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("build")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("src/a.rs"), "a").unwrap();
        std::fs::write(root.join("build/out.o"), "o").unwrap();
        std::fs::write(root.join(".secret"), "s").unwrap();
        std::fs::write(root.join(".hidden/x"), "x").unwrap();

        let mut o = opts();
        o.exclude_patterns = vec!["build".into()];
        let sel = collect_selection(root, &o).unwrap();
        let paths: Vec<&str> = sel.iter().map(|(p, _, _)| p.as_str()).collect();
        assert!(paths.contains(&"src/a.rs"), "{paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with("build")), "{paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with('.')), "{paths:?}");

        o.include_hidden = true;
        let sel2 = collect_selection(root, &o).unwrap();
        assert!(sel2.iter().any(|(p, _, _)| p == ".secret"), "{sel2:?}");
    }

    #[test]
    fn include_patterns_narrow_selection() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/a.md"), "a").unwrap();
        std::fs::write(root.join("readme.md"), "r").unwrap();
        let mut o = opts();
        o.include_patterns = vec!["docs/**".into()];
        let sel = collect_selection(root, &o).unwrap();
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].0, "docs/a.md");
    }

    #[test]
    fn archives_roundtrip_bytes() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("src");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/f.txt"), "hello selective").unwrap();
        let files = collect_selection(&root, &opts()).unwrap();
        assert_eq!(files.len(), 1);

        for fmt in [
            ArchiveFormat::Zip,
            ArchiveFormat::Tar,
            ArchiveFormat::TarZst,
        ] {
            let mut o = opts();
            o.format = fmt;
            let dest = dir.path().join(fmt.as_str());
            let (payload, _, count) = write_payload(&files, &dest, "backup", &o).unwrap();
            assert_eq!(count, 1);
            assert!(payload.exists(), "{fmt:?} missing");
            assert!(payload.metadata().unwrap().len() > 0);
        }
    }

    #[test]
    fn folder_payload_copies() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("src");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/f.txt"), "x").unwrap();
        let files = collect_selection(&root, &opts()).unwrap();
        let (payload, _, count) =
            write_payload(&files, &dir.path().join("out"), "snap", &opts()).unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            std::fs::read_to_string(payload.join("sub/f.txt")).unwrap(),
            "x"
        );
    }
}
