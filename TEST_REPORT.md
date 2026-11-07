# Test report

Date: 2026-11-07 · Version: 0.1.0 · Machine: Windows 11 x64

## Summary

| Suite | Result |
|---|---|
| `cargo test --workspace` | **PASS** — 19/19 tests, 0 failures |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** — 0 warnings |
| `cargo fmt --all -- --check` | **PASS** |
| `npm run build` (tsc strict + vite) | **PASS** — 0 TypeScript errors |
| `gitbackup --version` / `--help` | PASS |
| `gitbackup doctor` on this machine | see below |
| Full backup → verify → restore round-trip (integration test) | PASS |
| Desktop bundle (`cargo tauri build` → MSI + NSIS) | see below |

## Rust tests (19)

### Unit tests

| Test | What it proves |
|---|---|
| `manifest::tests::sanitize_is_filesystem_safe` | path components cannot escape the storage root |
| `manifest::tests::sha256_streams` | streamed hashing matches whole-file hashing |
| `manifest::tests::manifest_roundtrip_is_atomic_and_versioned` | manifest write→read round-trip + version gate |
| `paths::tests::rejects_traversal` | `..` and absolute paths rejected at the boundary |
| `restore::tests::conflict_mode_parses` | unknown conflict modes are refused, not defaulted |
| `activity::tests::redacts_tokens` | token-shaped strings never reach the log store |
| `selective::tests::selection_respects_hidden_and_excludes` | hidden files excluded unless opted in; excludes prune subtrees |
| `selective::tests::include_patterns_narrow_selection` | include globs restrict the selection |
| `selective::tests::archives_roundtrip_bytes` | zip/tar/tar.zst all produce a non-empty payload |
| `selective::tests::folder_payload_copies` | folder payload preserves structure and content |
| `selective::tests::sensitive_scan_finds_secrets` | `.env`, `.pem`, `credentials`, `id_rsa` detected (5/5), clean files ignored |
| `secretscan::tests::name_rules_match_relative_paths` | filename rules match relative paths |
| `secretscan::tests::detects_private_key_content` | PEM private-key blocks detected by content |
| `secretscan::tests::detects_github_token_content` | GitHub token shapes detected by content |
| `secretscan::tests::clean_file_is_not_flagged` | no false positive on ordinary source code |
| `publish::tests::gitignore_covers_env_and_languages` | generated `.gitignore` covers `node_modules` + `.env`, never echoes secrets |

### Integration tests (real `git` binary, temp directories)

| Test | What it proves |
|---|---|
| `full_backup_preserves_branches_tags_and_history` | builds a real repo (2 commits, `feature` branch, `v1.0` tag) → mirror backup (verified) → second run is an incremental update → restore to a new folder → `git log --all` still has both commits, tag present, branch present, `git fsck` passes, `verify_backup` agrees |
| `database_migrations_apply_to_fresh_and_existing_db` | migrations run on a fresh DB and are a no-op on re-open; ≥12 tables exist |

## UI

- `tsc` strict + `noUnusedLocals/Parameters`: 0 errors.
- Production build (`vite build`): 1765 modules, ~270 kB JS / ~19 kB CSS.
- All 12 pages reachable from the sidebar; every page has loading, error and
  empty states; running outside Tauri shows "Desktop backend not available"
  instead of fake data.

## Fixed during this cycle (found by the suites above)

1. **Secret filename rules matched nothing** — rules were regex patterns
   (`^\.env($|\.)`) compiled with `globset`, which treats `^`/`$` as literal
   characters. Switched name matching to the `regex` crate in both
   `secretscan` and `selective`; the tests that had failed (0 hits) now
   detect 5/5 and 2/2.
2. **Integration test depended on machine git config** — `git commit` failed
   with "unable to auto-detect email address" on hosts without a global
   identity. The test now sets `user.email`/`user.name` locally in the
   fixture repository.

## Not verified on this machine (honest list)

- **macOS and Linux builds** — this is a Windows machine. The CI workflow
  (`.github/workflows/ci.yml`) builds and tests on `ubuntu-latest`,
  `macos-latest` and `windows-latest`, and publishes artifacts on a `v*`
  tag; those runs are the evidence for those platforms, not this report.
- **GUI behaviour inside the real window** — pages were built and type-checked;
  end-to-end click-through happens after the first installable bundle exists.
- **Signing/notarization** — no code-signing certificate is configured, so
  installers are unsigned (Windows SmartScreen / macOS Gatekeeper will warn).
- **Scheduled backups / GitHub metadata backup** — not implemented; see
  `ROADMAP.md`.
