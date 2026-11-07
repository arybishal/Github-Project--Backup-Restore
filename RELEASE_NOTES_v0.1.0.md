# Git Backup Restore v0.1.0

First release of **Git Backup Restore** — a desktop app (Tauri 2 + React) and a CLI (`gitbackup`) sharing one Rust core (`gbr-core`) for backing up, verifying, and restoring local git repositories.

## What's in this release

| Asset | Platform | Built by |
|---|---|---|
| `Git Backup Restore_0.1.0_x64-setup.exe` (NSIS) | Windows x64 | This repo, built and run-tested locally on Windows 11 |
| `Git Backup Restore_0.1.0_x64_en-US.msi` | Windows x64 | This repo, built locally on Windows 11 |
| `gitbackup-windows-x86_64.zip` (CLI + docs) | Windows x64 | This repo, built and smoke-tested locally on Windows 11 |
| `gitbackup-linux-x86_64(.tar.gz)` | Linux x64 | GitHub Actions CI (on tag push) |
| `gitbackup-macos-aarch64(.tar.gz)` | macOS Apple Silicon | GitHub Actions CI (on tag push) |
| `gitbackup-macos-x86_64(.tar.gz)` | macOS Intel | GitHub Actions CI (on tag push) |
| Desktop bundles for Linux/macOS | Linux, macOS | GitHub Actions CI (on tag push) |

> **Verification status (honest):** Windows installer, Windows CLI, and the Windows desktop app were built and exercised on Windows 11 by the author. macOS and Linux binaries are produced by GitHub Actions on native runners for this tag; they are **not** hand-tested by the author.

## Features

- **Full backup** of local git repositories (objects, refs, config, stash, hooks, worktree metadata) into a single `.gbr` archive with a manifest and SHA-256 checksums.
- **Selective backup** with include/exclude patterns and a built-in secret scan (regex filename rules, e.g. `.env`, `*.pem`, `id_rsa`) that hard-blocks archiving detected secrets unless explicitly acknowledged.
- **Verification** of any archive (manifest, per-entry checksums, integrity report).
- **Restore** with conflict modes: `abort` (default, never silently overwrite), `overwrite`, `skip`, `rename`.
- **GitHub publisher**: push a backup's git data to a private GitHub repository over HTTPS using a token from the OS keychain (Windows Credential Manager / macOS Keychain / libsecret).
- **Tokens never touch SQLite or logs** — OS keychain only.
- **SQLite-backed history**: tasks, logs, and repo registry (SQLx + migrations).
- **CLI** with `--json` output and meaningful exit codes for scripting, plus shell completion (`config completion powershell|bash|zsh`).

## Install (Windows)

- **NSIS:** run `Git Backup Restore_0.1.0_x64-setup.exe` (per-user install, Start Menu shortcut).
- **MSI:** run `Git Backup Restore_0.1.0_x64_en-US.msi` (requires Windows Installer; use for enterprise/GPO deployment).
- **CLI only:** unzip `gitbackup-windows-x86_64.zip` and put `gitbackup.exe` on your `PATH`.

## Install (Linux / macOS)

Use the CLI tarballs, or build the desktop app from source (see `DEVELOPMENT.md`):

```sh
tar -xzf gitbackup-linux-x86_64.tar.gz && sudo install -m 755 gitbackup-linux-x86_64 /usr/local/bin/gitbackup
```

## Quick start

```sh
gitbackup doctor                     # check environment
gitbackup backup create <repo-dir>   # full backup
gitbackup verify <archive.gbr>       # verify integrity
gitbackup restore preview <archive>  # inspect before restoring
gitbackup restore create <archive> --target <dir> --conflict abort
```

Full reference: [`CLI.md`](https://github.com/arybishal/Github-Project--Backup-Restore/blob/main/CLI.md).

## Known limitations (v0.1.0)

- **GitHub platform data is not backed up**: Issues, Pull Requests, Actions secrets/settings, Releases, wikis, and branch protection live on GitHub's servers, not in git objects. This tool backs up what `git` itself stores.
- `refs/pull/*` refs are only present if they were fetched locally.
- **LFS**: repositories using Git LFS require `git-lfs` installed; otherwise LFS objects may be incomplete.
- **No scheduled/automatic backups** yet (see `ROADMAP.md`).
- **Installers are unsigned** — no code-signing certificate. Windows SmartScreen may warn on first run ("More info → Run anyway").
- Restore targets a **local directory** or a GitHub repository via publish; direct "restore into an existing GitHub repo's Issues/PRs" is not implemented.

## Development

- `cargo test --workspace` — 19/19 passing (includes a real-git integration round-trip: backup → verify → restore preserves branches, tags, and history).
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- CI runs fmt/clippy/tests + UI type-check on Ubuntu, macOS, and Windows for every push.

See `ARCHITECTURE.md`, `SECURITY.md`, `BACKUP_FORMAT.md`, `RESTORE_GUIDE.md`, and `TEST_REPORT.md` in the repository.
