# Roadmap

Planned work after 0.1.0. Nothing here is implemented yet — items move to
`CHANGELOG.md` only when they actually exist and are tested.

## Next

- **Scheduled backups** — per-repository schedules (daily/weekly) running in
  the background with notification on failure. Requires a persistent scheduler
  in the core plus a Task Center entry for each run.
- **GitHub platform metadata backup** — Issues, Pull Requests, Releases,
  webhooks and branch protection via the REST/GraphQL APIs, stored alongside
  the git mirror so a manifest can state exactly what was captured. This is
  the single largest gap between "git backup" and "account backup".
- **Backup encryption at rest** — optional age/AES-GCM encryption of selective
  archives and manifests (setting already exists as `encrypt_backups`,
  deliberately inert until implemented — it must never pretend to be on).
- **Restore to a fresh GitHub account** — guided flow that pushes restored
  mirrors and (where captured) metadata back in a safe order.

## Later

- **Verification scheduler** — periodic re-verification of stored backups
  with a health badge per repository (the query surface exists; the scheduler
  does not).
- **Bandwidth / rate-limit controls** — configurable concurrency and
  `git`/API throttle presets for very large accounts.
- **Delta reporting** — show what changed between two snapshots (new commits,
  changed refs) using the recorded ref lists.
- **Plugin-free themes** — the white/black/yellow system is permanent for 0.x;
  additional accent palettes may follow without ever adding a dark mode by
  default.
- **Packaging** — signed macOS DMG (notarised), Linux AppImage/deb/rpm, and
  auto-update channel once the release pipeline is stable.

## Explicit non-goals

- Replacing `git` itself — we shell out to the real binary, never reimplement
  the object model.
- Storing credentials anywhere but the OS credential manager.
- A hosted/synced service: backups stay on the user's own disk.
