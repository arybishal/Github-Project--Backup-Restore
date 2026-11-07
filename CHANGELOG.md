# Changelog

All notable changes to Git Backup Restore. Format follows
[Keep a Changelog](https://keepachangelog.com/); versioning follows SemVer.

## [0.1.0] — 2026-11-07

First release: a complete, working backup/restore/publish tool with a desktop
GUI and a matching terminal CLI sharing one engine.

### Added

- **Shared engine (`gbr-core`)** used by both the Tauri desktop app and the
  `gitbackup` CLI — no duplicated backup/restore/auth logic.
- **Full git backups**: `git clone --mirror`, incremental
  `git remote update --prune`, hardlinked point-in-time snapshots,
  `git lfs fetch --all` when git-lfs is installed, `git fsck` + per-ref object
  verification, retention (never below 1).
- **Selective backups**: gitignore-aware selection, include/exclude globs,
  hidden/ignored file options, folder/zip/tar/tar.zst output, per-file SHA-256,
  secret scanning with a hard stop unless explicitly acknowledged.
- **Restore**: pre-flight plan (space, destination, conflicts, blockers),
  conflict modes `new_folder` / `merge` / `replace` (replace requires explicit
  confirmation), zip-slip and zip-bomb protection, post-restore re-verification.
- **GitHub publisher**: project preview (git detection, file count, large
  files, secret hits, suggested `.gitignore`), repository creation via REST,
  commit when needed, non-fast-forward guard (force push only on confirm).
- **Authentication**: personal access token login and GitHub device flow
  (user-supplied OAuth client id, no shipped secret); tokens stored in the OS
  credential manager only, injected into git via `GIT_CONFIG_*` env vars.
- **Repository discovery**: paginated `/user/repos` sync, search/owner/
  language/visibility/archived filters, sorting, facets, org counts.
- **Desktop UI** (Tauri 2 + React + TypeScript + Tailwind): white/black/yellow
  design system, 12 pages (Dashboard, Repositories, Backup, Restore, Backup
  History, Local Projects, GitHub Publisher, Storage Manager, Task Center,
  Activity Logs, Settings, About), live `task_progress` events, responsive to
  900 px, accessibility-labelled controls.
- **CLI** (`gitbackup`): documented exit codes (0/1/2/3/4/5), `--json`
  machine output, `--yes` non-interactive mode, `-v/-vv` verbosity,
  shell completions (bash/zsh/fish/powershell), `doctor` diagnostics,
  Ctrl+C batch cancellation.
- **Storage manager**: disk-measured usage, per-repository breakdown, manifest
  export, safe deletion (path containment + last-backup protection).
- **Docs**: README, ARCHITECTURE, DEVELOPMENT, SECURITY, CLI, BACKUP_FORMAT,
  RESTORE_GUIDE, TASKS, ROADMAP, TEST_REPORT.
- **Tests**: unit tests (selection, archives, secret scan, path traversal,
  conflict modes) plus integration tests that build a real git repository,
  back it up, restore it and assert branches/tags/history survived.

### Known limitations (deliberate, documented in every manifest)

- GitHub platform data outside git (Issues, PRs, Actions secrets, Releases,
  branch protection, webhooks, settings) is not captured.
- Hidden GitHub-only refs (`refs/pull/*`) may be absent from mirror clones.
- LFS capture requires git-lfs installed; otherwise recorded as not captured.
- Scheduled backups are not implemented (see ROADMAP).
