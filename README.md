# Git Backup Restore

Cross-platform GitHub repository **backup, restore and publishing** application — desktop GUI (Tauri 2 + React) and terminal CLI, both driven by one shared Rust core engine.

- **Developer:** Bishal Aryal — https://bishalaryal.com.np
- **Platforms:** Windows 10/11 x64, macOS (Apple Silicon + Intel), Linux (x86_64)
- **License:** MIT

## What it does

1. Connect a GitHub account (token stored in the OS credential manager, never in SQLite/logs).
2. Discover every repository you can access — public, private and organization repos — with search, filters and multi-select.
3. **Full backup:** `git clone --mirror` + incremental `git fetch --prune`, preserving commits, branches, tags and LFS objects where available.
4. **Selective backup:** pick folders/files, include/exclude patterns, ZIP / TAR / TAR.ZST or plain folder output, with sensitive-file warnings.
5. **Verify:** every backup is verified before it is reported successful — `git fsck` + per-ref object checks for full backups, per-file SHA-256 for selective backups.
6. **Restore:** to a new folder, merge, or replace (with explicit confirmation), with a pre-flight plan and post-restore re-verification.
7. **Publish:** turn any local folder into a new GitHub repository (init → secret scan → .gitignore → commit → push).
8. Storage manager, backup history, task centre, activity logs, settings — and the same capabilities from the `gitbackup` CLI.

## Requirements

- Git 2.x on `PATH` (or configured in Settings). Optional: git-lfs (without it
  LFS objects are not captured — the manifest says so).
- Rust (stable) and Node.js 20+ for building from source.

## Build

```bash
# Desktop app (Windows/macOS/Linux installers)
npm install
npm run tauri build        # builds ui/, then the Tauri bundle (MSI+NSIS on Windows)

# CLI only
cargo build --release -p gbr-cli
./target/release/gitbackup --help      # .exe on Windows
```

Prebuilt binaries for every platform are attached to the
[Releases](../../releases) page; macOS/Linux artifacts are produced by the CI
workflow (`.github/workflows/ci.yml`).

## Documentation

| Document | Purpose |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Module layout, data flow, shared core |
| [DEVELOPMENT.md](DEVELOPMENT.md) | Local dev workflow, tests, linting |
| [TASKS.md](TASKS.md) | Phase-by-phase progress ledger |
| [ROADMAP.md](ROADMAP.md) | Planned work and explicit non-goals |
| [CLI.md](CLI.md) | Command reference |
| [BACKUP_FORMAT.md](BACKUP_FORMAT.md) | On-disk backup layout + manifest schema |
| [RESTORE_GUIDE.md](RESTORE_GUIDE.md) | Restore scenarios and safety rules |
| [SECURITY.md](SECURITY.md) | Threat model and guarantees |
| [CHANGELOG.md](CHANGELOG.md) | Release history |
| [TEST_REPORT.md](TEST_REPORT.md) | Actual recorded test results |

## Design

The desktop UI is permanently **white / black / yellow**: `#FFFFFF` background, `#111111` text, `#F5C400` accents. No dark theme.
