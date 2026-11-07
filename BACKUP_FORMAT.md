# Backup format

Everything a backup consists of on disk, and what it does — and does not —
cover.

## Directory layout

```
<backup root>/                               # user-chosen (Settings → backup root)
  manifest/<backup_id>.json                  # canonical manifest copies (all backups)
  repositories/<owner>/<repo>/
      full/mirror.git                        # live mirror, updated incrementally
      snapshots/<backup_id>.git              # point-in-time copies (hardlinked)
      selective/<backup_id>.<ext>            # file-level backups (.zip/.tar/.tar.zst or folder)
      metadata/<backup_id>.json              # manifest stored next to its payload
  logs/
  reports/
```

`<backup root>` defaults to the OS application-data directory
(`%APPDATA%\GitBackupRestore` on Windows, `~/Library/Application
Support/GitBackupRestore` on macOS, `~/.local/share/GitBackupRestore` on Linux)
and can be changed in Settings or with `gitbackup storage set-root`.

## Manifest

Every backup has a JSON manifest, written atomically (temp file → fsync →
rename) and stored in **two** places (root `manifest/` and next to the
payload) so a backup stays restorable even if the SQLite database is rebuilt.

Key fields:

| Field | Meaning |
|---|---|
| `manifest_version` | format version of this file (currently `1`) |
| `backup_id` | UUID; also the file name everywhere |
| `mode` | `full` (git mirror) or `selective` (files) |
| `format` | `mirror`, `folder`, `zip`, `tar`, `tar.zst` |
| `repository` | owner/name, source URL, default branch, visibility |
| `created_at` / `app_version` / `git_version` | provenance |
| `head_sha` | HEAD at backup time (full backups) |
| `refs[]` | every ref name → object id (full backups) |
| `payload_path` | path of the mirror/archive/folder, relative to the root |
| `payload_sha256` | SHA-256 of the archive file (`null` for mirror/folder — proven instead by `git fsck` + per-ref object checks) |
| `file_count` / `size_bytes` | measured, not estimated |
| `files[]` | per-file `{path, size, sha256}` (selective backups) |
| `lfs` | whether git-lfs was available and objects were captured, with a note when not |
| `verification` | status (`verified`/`failed`/`unverified`), method, timestamp, detail |
| `limitations` | the honest list below, embedded in every manifest |

## What a backup covers

- All git refs (branches, tags, remotes), commit history and objects —
  `git clone --mirror` plus `git remote update --prune` on later runs.
- LFS objects via `git lfs fetch --all` **when git-lfs is installed**;
  otherwise `lfs.objects_backed_up = false` with an explanatory note.
- A hardlinked snapshot per backup (byte-copy fallback across volumes), so
  retention or mirror corruption cannot destroy a point-in-time copy.
- For selective backups: exactly the selected working-tree files, each with a
  SHA-256.

## What a backup does **not** cover (also embedded in every manifest)

1. Git backups capture repository data (commits, branches, tags, and LFS
   objects when git-lfs is available).
2. GitHub platform data outside git is **NOT** captured: Issues, Pull
   Requests, Discussions, Projects, Wikis, Actions secrets and variables,
   Environments, Deploy keys, webhooks, branch protection rules, repository
   settings, and GitHub Release assets.
3. Hidden GitHub-only references such as `refs/pull/*` may be absent from a
   mirror clone.
4. A git backup is not a complete GitHub account backup.

## Verification methods

| Backup kind | Method |
|---|---|
| full (mirror/snapshot) | `git fsck --strict` + every recorded ref resolves to a present object |
| selective, folder | re-hash each file and compare to `files[].sha256` |
| selective, archive | SHA-256 of the archive vs `payload_sha256` |

`gitbackup verify <id>` / `gitbackup verify --all` re-runs these and updates
the manifest's `verification` block. Exit code 4 means at least one failed.

## Retention

Snapshots are trimmed oldest-first down to `snapshot_retention` (Settings,
default 10, minimum 1). The newest backup is never removed automatically —
retention can never destroy the only valid copy.

## Versioning

`manifest_version` is bumped only for breaking layout changes. A reader that
sees a higher version must refuse to restore rather than guess (this is
enforced in `manifest::read_manifest`).
