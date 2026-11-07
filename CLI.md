# `gitbackup` — CLI reference

The terminal interface shares **100 %** of its logic with the desktop app
(both drive `gbr-core`). The CLI never requires the GUI.

## Global flags

| Flag | Effect |
|---|---|
| `--json` | machine-readable output where supported |
| `-y`, `--yes` | never prompt; assume yes for confirmations |
| `-v` / `-vv` | log info / debug to stderr |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success |
| 1 | error |
| 2 | usage / invalid input |
| 3 | authentication required |
| 4 | verification failed |
| 5 | cancelled (by the user or by a confirmation that was declined) |

```powershell
gitbackup verify --all; $LASTEXITCODE   # 4 means at least one backup failed verification
```

## Commands

### `login`

```powershell
gitbackup login --token <PAT>          # personal access token (validated against /user)
gitbackup login --device               # browser device flow; needs `github_client_id` in config
gitbackup login                        # interactive prompt (token is not echoed)
gitbackup logout
```

Tokens are stored in the **OS credential manager** (Windows Credential
Manager / macOS Keychain / Secret Service), never in SQLite, a manifest or a
log line.

### `status`

Authentication, git/git-lfs availability, backup root, storage, cached repo
counts, backup counts. `--json` for scripts.

### `repos`

```powershell
gitbackup repos sync                              # pull /user/repos into the local cache
gitbackup repos list --search api --visibility private --sort updated --limit 50
gitbackup repos search <keyword>
gitbackup repos facets                            # owners + languages for filters
```

### `backup`

```powershell
gitbackup backup owner/repo                        # full git mirror backup
gitbackup backup --all                             # every cached repository
gitbackup backup owner/repo --mode selective --format zip --include "docs/**" --exclude build
gitbackup backup owner/repo --dest D:\Backups --retention 20 --lfs true
```

- **full** (default): `git clone --mirror`, incremental on later runs,
  optional LFS capture, hardlinked snapshot, `git fsck` verification.
- **selective**: materialised working tree → include/exclude globs →
  folder/zip/tar/tar.zst with per-file SHA-256.
- Secret-looking files (`.env`, `*.pem`, `id_rsa`, credentials, …) cause a
  **hard stop** unless `--yes` was given — the error lists every hit.
- Ctrl+C cancels the batch cleanly: completed backups stay on disk and
  verified, the rest simply stop.

### `restore`

```powershell
gitbackup restore <backup-id> --dest .\restored
gitbackup restore --latest owner/repo --dest .\restored --conflict merge
gitbackup restore <id> --dest .\out --conflict replace      # asks for confirmation
```

Conflict modes: `new_folder` (default; destination must be empty/absent),
`merge` (never overwrites an existing differing file), `replace` (requires
confirmation; only files the backup owns are removed). The pre-flight plan
runs first — missing payload, insufficient disk space or a non-empty
destination in `new_folder` mode block the restore with exit code 2.

### `verify`

```powershell
gitbackup verify <backup-id>
gitbackup verify --all          # exit code 4 if anything fails
```

Full backups: `git fsck --strict` + every recorded ref resolves to a present
object. Selective folder backups: per-file SHA-256. Archives: archive SHA-256.

### `history` / `storage`

```powershell
gitbackup history --limit 50
gitbackup storage info
gitbackup storage set-root D:\Backups
gitbackup storage delete <id> --force      # --force needed for a repository's only backup
gitbackup storage export .\manifests
```

### `publish`

```powershell
gitbackup publish .\my-project --name my-project --description "…" --public --readme
gitbackup publish .\my-project --remote https://github.com/me/existing.git
gitbackup publish .\my-project --allow-secrets      # only if you really mean it
```

Previews the project first (git detection, file count, large files, secret
hits, suggested `.gitignore`), creates the repository through the GitHub API,
commits if needed, then pushes. Non-fast-forward pushes are refused unless
`--force` was given **and** confirmed.

### `github`

```powershell
gitbackup github create-repo my-repo --public --org my-org
gitbackup github rate-limit
```

### `tasks` / `logs` / `config` / `doctor` / `update`

```powershell
gitbackup tasks list --limit 20
gitbackup logs --level warn --search backup --limit 100
gitbackup logs --export .\gbr-log.txt            # redacted
gitbackup config show
gitbackup config set backup_root D:\Backups
gitbackup config set github_client_id <oauth-app-client-id>
gitbackup config completion powershell > gitbackup.ps1
gitbackup doctor                                  # git, git-lfs, DB, storage, credentials, API
gitbackup update --check                          # check if a new release is available
gitbackup update                                  # download & install latest release directly
```

## Notes & limitations

- Scheduled backups are not implemented yet (see `ROADMAP.md`).
- A git backup does **not** capture GitHub platform data (Issues, PRs,
  Actions secrets, Releases, branch protection, webhooks, settings). Every
  manifest carries this list; see `BACKUP_FORMAT.md`.
- `git lfs fetch --all` needs git-lfs installed; without it the manifest
  records `objects_backed_up: false` with an explanatory note.
