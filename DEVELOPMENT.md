# DEVELOPMENT

How to build, run and test Git Backup Restore from source.

## Prerequisites

| Tool | Version used | Notes |
|---|---|---|
| Rust (stable) | 1.99.1 | `rustup` recommended |
| Node.js | 24.x | UI build only |
| git | 2.5x | required at runtime, not to build |
| Windows: MSVC Build Tools | 2022 (14.44) | needed to link (`link.exe`) |
| git-lfs | optional | without it LFS objects are not captured (recorded in the manifest) |

On Windows, `cargo` is usually at `%USERPROFILE%\.cargo\bin` — add it to `PATH`
for your shell session if it is not already there.

## Repository layout

```
Cargo.toml               workspace root
crates/gbr-core/         shared engine (all domain logic + migrations)
crates/gbr-cli/          `gitbackup` terminal binary
src-tauri/               Tauri 2 desktop shell (commands delegate to gbr-core)
ui/                      React + TypeScript + Tailwind front end
docs/                    user + contributor documentation
```

## Common commands

```powershell
# type-check everything (fastest full signal)
cargo check --workspace --all-targets

# unit + integration tests (uses the real `git` binary)
cargo test --workspace

# lint / format
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# UI
cd ui; npm install; npm run build        # tsc && vite build
cd ui; npm run dev                        # dev server on :5173

# desktop app, development (opens the Tauri window) — always run from the repo root
npm run tauri:dev
npm run tauri:build                       # release bundle (MSI + NSIS on Windows)

# CLI release binary
cargo build --release -p gbr-cli          # → target/release/gitbackup(.exe)
```

### Icons

`src-tauri/icons/` is generated. To regenerate after changing the artwork:

```powershell
npx -y @tauri-apps/cli@2 icon path\to\1024x1024.png
```

## Development model

1. **All domain logic lives in `gbr-core`.** The Tauri commands and the CLI
   only parse input, call the core, and format output. If you find yourself
   writing backup/restore/auth logic in `src-tauri/` or `gbr-cli/`, move it
   into the core instead.
2. **The API contract is frozen** in `ui/src/api/types.ts` and mirrored by the
   serde structs in `src-tauri/src/commands.rs`. Change both together, never
   one side only. Tauri camelCases arguments (`backupId`), Rust fields stay
   snake_case.
3. **No fake data, ever.** Every number the UI shows comes from SQLite or from
   disk. Empty states explain what to do next instead of showing zeros that
   pretend to be measurements.
4. **Credentials never touch the database, a manifest or a log line.** They
   live in the OS credential manager and are injected into `git` per-invocation
   via `GIT_CONFIG_*` environment variables.

## Testing strategy

- **Unit tests** live next to the code (`#[cfg(test)] mod tests`) — selection
  rules, archive round-trips, secret scanning, path-traversal rejection.
- **Integration tests** in `crates/gbr-core/src/lib.rs` build a *real* git
  repository in a temp directory, mirror-backup it, mutate the source,
  restore, and assert that branches, tags and history survived.
- **UI** is covered by `tsc` strict mode + a production build; runtime
  behaviour is verified manually inside the Tauri window (a browser cannot
  reach the backend by design).

Run everything:

```powershell
cargo test --workspace; if ($LASTEXITCODE -eq 0) { cd ui; npm run build }
```

## Release checklist

1. `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
2. `cargo test --workspace` green.
3. `npm run build` in `ui/` green (zero TS errors).
4. `npm run tauri:build` produces installers for the current OS.
5. macOS/Linux artifacts come from native CI runners — never claim a platform
   build you did not produce and run.
6. Update `CHANGELOG.md`, tag, push, publish the release with the artifacts
   and an honest statement of which platforms were verified.
