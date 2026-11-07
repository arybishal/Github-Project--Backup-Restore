# TASKS — phase ledger

Status legend: `todo` · `in progress` · `done` · `blocked (reason)`

Updated as work completes. Nothing is marked `done` until it has been run and
observed to work.

## Phase 0 — Architecture and project foundation

| # | Task | Status | Evidence |
|---|---|---|---|
| 0.1 | Inspect development environment | done | Windows 10/11 x64, Node 24.18.1, npm 11.16.0, Git 2.55.0, Rust 1.99.1 installed during setup, VS Build Tools 2022 (14.44) installed for MSVC linking |
| 0.2 | Establish repository structure | done | Cargo workspace: `crates/gbr-core`, `crates/gbr-cli`, `src-tauri`, `ui` |
| 0.3 | Shared Rust core library | done | `crates/gbr-core` with 14 modules |
| 0.4 | CLI binary scaffold | done | `gitbackup` bin, clap 4 derive |
| 0.5 | SQLite migrations | done | `migrations/0001_initial.sql`, 12 tables |
| 0.6 | Design tokens | done | `ui/src/index.css` + `tailwind.config.js` (white/black/yellow) |
| 0.7 | Linting / formatting config | in progress | `cargo fmt` + `cargo clippy`; prettier/eslint in `ui/` |
| 0.8 | README.md | done | root |
| 0.9 | ARCHITECTURE.md | done | root |
| 0.10 | TASKS.md | done | this file |
| 0.11 | DEVELOPMENT.md | done | root |

## Phase 1 — Desktop UI and navigation

| # | Task | Status |
|---|---|---|
| 1.1 | White/black/yellow design system | done (tokens + Tailwind mapping) |
| 1.2 | Sidebar with 12 destinations | done |
| 1.3 | Dashboard page | done |
| 1.4 | Repositories page (filters, multi-select, pagination) | done |
| 1.5 | Backup page (mode, destination, format, sensitive warning) | done |
| 1.6 | Restore page (manifest, conflict modes, confirmation) | done |
| 1.7 | Backup History page | done |
| 1.8 | Local Projects page | done |
| 1.9 | GitHub Publisher page | done |
| 1.10 | Storage Manager page | done |
| 1.11 | Task Center page (live `task_progress` events) | done |
| 1.12 | Activity Logs page (search, filter, export) | done |
| 1.13 | Settings + About pages | done |
| 1.14 | Responsive resize 1200 → 900 px | done |
| 1.15 | Reusable components (Button, Card, Table, Dialog, …) | done |

## Phase 2 — GitHub authentication

| # | Task | Status |
|---|---|---|
| 2.1 | Personal-access-token login (validated against `/user`) | done |
| 2.2 | Device authorization flow (user-supplied client id, no secret) | done |
| 2.3 | Token stored in OS credential manager (`keyring`) | done |
| 2.4 | Logout / reconnect | done |
| 2.5 | Expired-credential detection → `AuthRequired` | done |
| 2.6 | Rate-limit handling with reset time | done |
| 2.7 | Unit test: redaction keeps tokens out of logs | done |

## Phase 3 — Repository discovery

| # | Task | Status |
|---|---|---|
| 3.1 | Paginated `/user/repos` sync (100/page, transactional upsert) | done |
| 3.2 | Search / owner / language / visibility / archived filters | done |
| 3.3 | Sort by name, updated, size | done |
| 3.4 | Multi-select + select all/none/private/public | done |
| 3.5 | Facets for filter dropdowns | done |
| 3.6 | Organization repo counts (cached org membership) | done |

## Phase 4 — Full git backup engine

| # | Task | Status |
|---|---|---|
| 4.1 | `git clone --mirror` initial backup | done |
| 4.2 | Incremental `git remote update --prune` | done |
| 4.3 | Point-in-time snapshots (hardlink, copy fallback) | done |
| 4.4 | git-lfs `fetch --all` with honest fallback | done |
| 4.5 | Manifest (atomic write, refs, sizes, limitations) | done |
| 4.6 | SHA-256 + `git fsck` verification | done |
| 4.7 | Retention policy (never below 1) | done |
| 4.8 | Integration test: branches/tags/history survive backup + restore | done |
| 4.9 | Multi-repo / all-repo batch with per-repo failure isolation | done |

## Phase 5 — Selective backup engine

| # | Task | Status |
|---|---|---|
| 5.1 | File selection with gitignore awareness | done |
| 5.2 | Include / exclude globs | done |
| 5.3 | Hidden + ignored file options | done |
| 5.4 | ZIP / TAR / TAR.ZST / folder output | done |
| 5.5 | Per-file SHA-256 | done |
| 5.6 | Sensitive-file detection + hard stop without confirmation | done |
| 5.7 | Unit tests (selection, archives round-trip, secret scan) | done |

## Phase 6 — Restore engine

| # | Task | Status |
|---|---|---|
| 6.1 | Restore full mirror to a new folder (+ working tree) | done |
| 6.2 | Restore selective files (folder + archives) | done |
| 6.3 | Conflict modes: new folder / merge / replace-with-confirmation | done |
| 6.4 | Pre-flight plan (space, permissions, conflicts, blockers) | done |
| 6.5 | Zip-slip / tar-slip / zip-bomb protection | done |
| 6.6 | Post-restore integrity re-verification | done |

## Phase 7 — GitHub publisher

| # | Task | Status |
|---|---|---|
| 7.1 | Detect / init git repository | done |
| 7.2 | Secret scan before publish | done |
| 7.3 | `.gitignore` generation (node/rust/python/env-aware) | done |
| 7.4 | Repository creation via REST API | done |
| 7.5 | Remote setup + commit when needed | done |
| 7.6 | Push with non-fast-forward guard (`--force-with-lease` only on confirm) | done |
| 7.7 | README creation on request | done |
| 7.8 | Large-file warnings (50 MB warn / 100 MB reject) | done |

## Phase 8 — CLI application

| # | Task | Status |
|---|---|---|
| 8.1 | `login` / `logout` / `status` | done |
| 8.2 | `repos list` / `search` / `sync` / `facets` | done |
| 8.3 | `backup` (single, `--all`, mode, format, includes/excludes) | done |
| 8.4 | `restore` (id, `--latest`, conflict modes, confirmation) | done |
| 8.5 | `verify` (single / `--all`, exit code 4 on failure) | done |
| 8.6 | `history` / `storage info` / `storage delete` / `storage export` | done |
| 8.7 | `publish` (preview, secret gate, force gate) | done |
| 8.8 | `github create-repo` / `rate-limit` | done |
| 8.9 | `config show` / `set` / `completion` | done |
| 8.10 | `doctor` | done |
| 8.11 | JSON output mode + non-interactive `--yes` | done |
| 8.12 | Documented exit codes | done |
| 8.13 | Ctrl+C cancels batch backups cleanly | done |

## Phase 9 — Storage and history

| # | Task | Status |
|---|---|---|
| 9.1 | Storage Manager statistics (disk-measured, per repository) | done |
| 9.2 | Backup history listing | done |
| 9.3 | Manifest export | done |
| 9.4 | Safe deletion (single-backup protection + path containment) | done |
| 9.5 | Retention policy | done |

## Phase 10 — Hardening and release

| # | Task | Status |
|---|---|---|
| 10.1 | Security pass (redaction, path containment, no shell interpolation) | done (unit tests) |
| 10.2 | Error-handling audit | in progress |
| 10.3 | Performance check (large repo, streaming hashes) | todo |
| 10.4 | Full regression run + TEST_REPORT.md | in progress |
| 10.5 | Cross-platform build configuration (CI matrix) | todo |
| 10.6 | Installers (MSI/NSIS, DMG, AppImage/deb/rpm) | todo |
| 10.7 | GitHub push + release | todo |

## Open items / blockers

- macOS and Linux builds cannot be verified from this Windows machine; they
  must be produced by native CI runners and are reported as *configured, not
  locally verified* until then.
- Selective backup of a repository requires either an existing local mirror
  (offline) or network access for a shallow clone.
