# SECURITY

Security-relevant behaviour of Git Backup Restore, and what it deliberately
does not do.

## Credential handling

- **Where tokens live:** the OS credential manager only (Windows Credential
  Manager, macOS Keychain, Linux Secret Service) via the `keyring` crate.
  Tokens are **never** written to SQLite, settings, manifests, activity logs
  or error messages.
- **How git receives them:** per-invocation environment variables —
  `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_0=http.extraheader`,
  `GIT_CONFIG_VALUE_0=AUTHORIZATION: basic base64(x-access-token:<token>)`.
  Nothing is persisted in `.git/config`, so a backed-up mirror contains no
  credentials.
- **Redaction:** activity logging redacts token-shaped strings at write time
  and again on export (`gitbackup logs --export`, GUI export).
- **Device flow** needs only an OAuth client id (public by design); this
  application never ships a client secret.
- **Logout** deletes the credential-manager entry, not just a DB row.

## Backup integrity

- Manifests are written **atomically** (temp file → fsync → rename) — a crash
  can never leave a half-written manifest that looks valid.
- Every backup is verified before it is reported successful: `git fsck
  --strict` + per-ref object presence (full), per-file SHA-256 (selective
  folder), archive SHA-256 (zip/tar/tar.zst).
- Verification status is stored in the manifest and in `backup_records`; the
  UI/CLI shows `unverified` rather than implying success.

## Restore safety

- The payload must resolve **inside** the configured storage root; the
  destination may not be the storage root.
- `new_folder` refuses a non-empty destination; `replace` requires an
  explicit confirmation flag from the UI/CLI (never defaulted on).
- Archive extraction rejects absolute paths, `..` components and symlinks
  (zip-slip / tar-slip), and bounds total expansion (zip-bomb guard).
- Post-restore, files are re-hashed against the manifest; full backups are
  re-checked with `git fsck`.

## Filesystem operations

- No shell interpolation anywhere: `git` is spawned with an argument vector
  (`tokio::process`), so repository names, paths and patterns cannot inject
  commands.
- Deletion targets are canonicalised and must be inside the storage root;
  the last remaining backup of a repository requires `--force` /
  explicit confirmation.
- Selective backup refuses to include symlinks that resolve outside the
  project tree.

## Secret scanning

- Before a selective backup ships files and before a publish pushes a
  repository, filenames and (for small text files) contents are scanned for
  `.env`, private keys, `credentials`, AWS/GitHub/Slack/Google token shapes,
  `.netrc`, `.git-credentials`, `.npmrc`, etc.
- Selective backup **hard-fails** without `acknowledge_secrets`/`--yes`;
  publishing requires an explicit confirmation.
- The scanner is best-effort (pattern based), not a guarantee — it is a
  guard rail, not a substitute for reviewing what you publish.

## Reporting a vulnerability

Open a private security advisory on the repository, or email the maintainer
via https://bishalaryal.com.np. Please do not file a public issue for
unfixed vulnerabilities.
