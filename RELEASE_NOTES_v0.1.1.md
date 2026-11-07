# Git Backup Restore v0.1.1

Maintenance and feature release introducing direct software update capabilities, embedded activity logs inside Settings, and resolution of the SQLite foreign key constraint issue.

## What's New in v0.1.1

### 1. In-App & CLI Direct Software Updates
- **Desktop GUI:** Added a new **Software Updates** card in **Settings** (`Settings` → `Software Updates`). Users can click **Check for Updates** to query GitHub Releases, inspect new changelogs, and click **Update Directly** to automatically download and launch the installer without leaving the app.
- **Terminal CLI:** Added `gitbackup update` command:
  - `gitbackup update --check`: Inspects if a new release is available with release notes.
  - `gitbackup update`: Automatically downloads and executes the update package for the current operating system.

### 2. Live Activity & Diagnostic Logs in Settings
- Embedded a real-time diagnostic log viewer directly inside the **Settings** view.
- Added level filtering (`All`, `Error`, `Warn`, `Info`, `Debug`), a 1-click **Copy to Clipboard** button, a **Download .txt Export** button, and an instant shortcut to the full Activity Logs page.

### 3. Bug Fixes & Engine Reliability
- **Resolved SQLite Error (code: 787)**: Fixed `FOREIGN KEY constraint failed` when triggering backups from the GUI by guaranteeing the `backup_jobs` record exists prior to writing `backup_records`, and reordering snapshot insertion to execute after the parent record is committed.
- **Cross-Platform Hard Link**: Fixed cross-platform compilation on Unix/Linux by adopting standard `std::fs::hard_link`.

## Release Assets

| Asset | Platform | Description |
|---|---|---|
| `Git.Backup.Restore_0.1.1_x64-setup.exe` | Windows x64 | Standard NSIS Windows Desktop Installer |
| `Git.Backup.Restore_0.1.1_x64_en-US.msi` | Windows x64 | Windows MSI Installer (Enterprise / GPO) |
| `git-backup-restore-v0.1.1-windows-x64-portable.zip` | Windows x64 | Standalone portable desktop GUI application |
| `gitbackup-v0.1.1-windows-x64.zip` | Windows x64 | Standalone terminal CLI binary + docs |
| `gitbackup-*-Linux.tar.gz` | Linux x86_64 | Native terminal CLI executable (via CI) |
| `gitbackup-*-macOS.tar.gz` | macOS | Native terminal CLI for Apple Silicon & Intel (via CI) |
