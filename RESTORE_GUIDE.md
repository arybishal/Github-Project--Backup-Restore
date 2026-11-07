# Restore guide

How to get your data back — and the safety rails you will hit along the way.

## Choosing what to restore

List what exists:

```powershell
gitbackup history --limit 50          # CLI
# or: Backup History page in the desktop app
```

Each row shows repository, mode, size, verification status and date. The
manifest (GUI: "View manifest", CLI: `manifest/<id>.json` on disk) contains
the full reference list, per-file hashes and the embedded limitations list —
read it before restoring anything you care about.

## Pre-flight plan

Nothing is written until the plan passes. The plan reports:

- **payload present?** — a missing mirror/archive blocks the restore;
- **enough free space?** — required vs available bytes;
- **destination state** — exists? empty?;
- **conflicts** — what would be overwritten;
- **blockers** — anything that makes `can_proceed` false (exit code 2 in the
  CLI, disabled Start button in the GUI).

## Conflict modes

| Mode | Behaviour |
|---|---|
| `new_folder` (default) | Destination must be absent or empty. Refuses a non-empty directory. |
| `merge` | Copies in, **never overwrites** an existing differing file. For git mirrors: refused if the bare repo already exists. |
| `replace` | Overwrites — requires explicit confirmation (CLI prompts / `--yes`; GUI red checkbox). Only files the backup owns are removed, never a blanket wipe of a folder we did not create. |

## Restoring a full (git mirror) backup

```powershell
gitbackup restore <backup-id> --dest D:\restored\myrepo
```

1. Verifies the mirror first (`git fsck` + ref/object checks) — an unverified
   mirror is never propagated.
2. Copies the bare mirror to `<dest>.git`.
3. Unless `--worktree false`, creates a checked-out working copy at `<dest>`
   by cloning from the restored mirror.
4. Runs `git fsck` again on the result.

Result: full branches, tags and history. Push it back to GitHub with
`git push --mirror <url>` if you are recovering an account.

## Restoring a selective backup

- **folder format** — files are copied back with their structure; each file
  is re-hashed against the manifest afterwards.
- **zip / tar / tar.zst** — extracted with zip-slip protection (absolute
  paths, `..` and symlinks are rejected) and a zip-bomb guard; the archive
  hash is re-checked after extraction.

## Failure modes you should know about

| Symptom | Meaning | What to do |
|---|---|---|
| `payload missing on disk` | the mirror/archive was deleted outside the app | restore from another backup id (`history` shows siblings) |
| `hash mismatch` | the file changed after backup | treat that file as corrupt; the manifest's hash is authoritative |
| `git fsck` failure | mirror corruption | use the snapshot copy (`snapshots/<id>.git`) — it is independent of the live mirror |
| `not enough free space` | plan blocker | free space or pick another destination |
| `destination exists and is not empty` | `new_folder` mode | choose merge or replace, or pick a new folder |

## Recovering without the app

The format is deliberately open: a `mirror.git` folder **is** a git
repository — `git clone mirror.git out` works with nothing but git. Archives
are standard zip/tar/tar.zst. Manifests are plain JSON. No proprietary
container, no lock-in.
