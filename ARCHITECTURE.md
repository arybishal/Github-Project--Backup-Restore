# Architecture — Git Backup Restore

Single source of truth for how the system is put together. Anything that
contradicts this document is a bug.

## 1. One core, two front ends

```
┌────────────────────┐      ┌────────────────────┐
│  Desktop GUI       │      │  Terminal CLI       │
│  Tauri 2 + React   │      │  `gitbackup`        │
│  src-tauri/ + ui/  │      │  crates/gbr-cli     │
└─────────┬──────────┘      └─────────┬───────────┘
          │  invoke() / events        │  clap + direct calls
          ▼                           ▼
┌─────────────────────────────────────────────────┐
│  gbr-core  (crates/gbr-core)                   │
│  auth · github · repo_store · git · backup ·   │
│  selective · restore · publish · manifest ·    │
│  tasks · db · settings · activity · dashboard  │
└─────────────────────────────────────────────────┘
          │                    │
          ▼                    ▼
   native `git` CLI      SQLite (SQLx)  +  OS credential manager
```

**Rule:** no backup, restore, authentication or repository-management logic
exists in `src-tauri/` or `crates/gbr-cli/`. Those crates only parse input,
call `gbr-core`, and format output. This is what keeps GUI and CLI behaviour
identical.

## 2. Crates

| Crate | Output | Role |
|---|---|---|
| `gbr-core` | rlib | All domain logic, no UI, no process-global state |
| `gbr-cli` | `gitbackup.exe` / `gitbackup` | clap 4 CLI over `gbr-core` |
| `src-tauri` | desktop app + bundled CLI | Tauri 2 commands that delegate to `gbr-core` |
| `ui` | static assets | React + TypeScript + Tailwind front end |

## 3. Data flow — backup (mode A, full)

1. `repos_sync()` pulls `/user/repos` page by page (100/page) into SQLite in a
   transaction. No page is held longer than its own transaction.
2. `backup_start()` allocates a `TaskHandle` (cancellation + progress) and
   spawns work on the Tokio runtime; the UI subscribes to `task_progress`.
3. Per repository:
   - `git clone --mirror <url> <root>/repositories/<owner>/<repo>/full/mirror.git`
     on first run, `git remote update --prune` afterwards (incremental).
   - Credentials are injected as `GIT_CONFIG_*` env vars
     (`http.extraheader = AUTHORIZATION: basic …`) so the token never lands in
     `.git/config`, a manifest, or a log line.
   - `git lfs fetch --all` when git-lfs is installed (otherwise recorded as a
     documented limitation, not silently skipped).
   - Snapshot: hardlink-copy of the mirror into `snapshots/<backup_id>.git`
     (byte-copy fallback across volumes).
   - Verify: `git fsck --strict` + every recorded ref resolves to a present
     object.
4. Manifests are written **atomically** (temp file → `fsync` → rename) to both
   `manifest/<id>.json` and `repositories/<owner>/<repo>/metadata/<id>.json`.
5. Retention trims old snapshots (never below 1).

## 4. Data flow — selective backup (mode B)

1. Materialise a working tree: clone from the local mirror when present
   (offline, no credentials), otherwise a shallow clone from GitHub.
2. Select files with `ignore` (gitignore-aware) or `walkdir` when the user
   asked for ignored files; apply include/exclude globs; filter hidden files
   unless opted in; drop symlinks that escape the tree.
3. Secret scan (filenames + content patterns). **Hard stop** unless the user
   explicitly acknowledged.
4. Stream to ZIP / TAR / TAR.ZST / plain folder; hash every file (64 KiB
   blocks, never whole-file in RAM).
5. Write manifest with per-file SHA-256 (+ archive hash), then verify what was
   just written before declaring success.

## 5. Data flow — restore

`plan()` runs first and is what the UI shows: manifest present? payload
present? free space? destination state? conflicts? It returns `can_proceed` +
`blockers`, and nothing is written until the caller confirms.

Safety rails, all enforced in `restore::run`:
- payload must live **inside** the storage root (and the destination must not
  be the storage root),
- `new_folder` requires an empty/absent destination,
- `replace` requires an explicit confirmation flag,
- archive extraction rejects absolute paths, `..`, and symlinks (zip-slip),
- zip expansion is bounded (bomb guard),
- full restores re-run `git fsck` before finishing; selective restores re-hash
  every extracted file and compare against the manifest.

## 6. Storage layout

```
<backup root>/                     # user-chosen; default: app data dir
  manifest/<backup_id>.json        # canonical manifest copies
  repositories/<owner>/<repo>/
      full/mirror.git              # live mirror, updated incrementally
      snapshots/<backup_id>.git    # point-in-time copies (hardlinked)
      selective/<backup_id>.<ext>  # file-level backups
      metadata/<backup_id>.json    # manifest next to its payload
  logs/
  reports/
```

Deleting anything is gated: paths are canonicalised and must start with the
storage root, and the last remaining backup for a repository cannot be deleted
without `force`.

## 7. Database

SQLite via SQLx, WAL mode, versioned migrations in
`crates/gbr-core/migrations/`. Tables: `accounts`, `repositories`,
`backup_jobs`, `backup_records`, `backup_snapshots`, `backup_files`,
`backup_manifests`, `restore_jobs`, `publish_jobs`, `task_history`,
`application_settings`, `activity_logs`.

**No credential is ever written here.** Tokens live only in the OS credential
manager (Windows Credential Manager / macOS Keychain / Secret Service).

## 8. Concurrency & cancellation

- All I/O is async (`tokio`); subprocess calls use `tokio::process`.
- Each user-visible operation is a task with a `watch`-channel cancel flag;
  work checks `ensure_active()` between steps and returns a clean
  `cancelled` error rather than leaving half-written state (staging
  directories are always cleaned up).
- Progress reports `None` for percentages that cannot be measured — nothing is
  ever invented.

## 9. Error model

`GbrError` is the single error type; the CLI maps it to documented exit
codes (see `CLI.md`). Every error message is written for a human: it says what
failed, where, and what to do next.

## 10. Known architectural limits (honest list)

- Hardlinked snapshots share immutable object files with the live mirror.
  Git never rewrites objects in place, so this is safe for git data; a
  non-git-aware writer pointed at the mirror could invalidate both.
- `git lfs fetch --all` requires git-lfs on the machine; without it LFS
  objects are not captured and the manifest says so.
- GitHub platform metadata (Issues, PRs, Actions secrets, Releases, branch
  protection, webhooks, settings) is **not** part of a git backup. Every
  manifest carries this list.
- Scheduled backups are a planned module, not implemented.
