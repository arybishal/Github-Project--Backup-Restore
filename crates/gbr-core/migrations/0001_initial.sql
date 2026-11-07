-- Git Backup Restore — initial schema (v1)
-- Credentials are intentionally absent: tokens live in the OS credential manager.

CREATE TABLE IF NOT EXISTS accounts (
    id            TEXT PRIMARY KEY,
    provider      TEXT NOT NULL DEFAULT 'github',
    username      TEXT NOT NULL,
    avatar_url    TEXT,
    scopes        TEXT,
    auth_method   TEXT NOT NULL DEFAULT 'token',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repositories (
    github_id           INTEGER PRIMARY KEY,
    node_id             TEXT,
    owner               TEXT NOT NULL,
    name                TEXT NOT NULL,
    full_name           TEXT NOT NULL UNIQUE,
    description         TEXT,
    is_private          INTEGER NOT NULL DEFAULT 0,
    is_fork             INTEGER NOT NULL DEFAULT 0,
    is_archived         INTEGER NOT NULL DEFAULT 0,
    language            TEXT,
    default_branch      TEXT,
    size_kb             INTEGER,
    html_url            TEXT,
    clone_url           TEXT,
    pushed_at           TEXT,
    updated_at          TEXT,
    last_backup_status  TEXT,
    last_backup_at      TEXT,
    raw_json            TEXT,
    synced_at           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repositories_owner ON repositories(owner);
CREATE INDEX IF NOT EXISTS idx_repositories_name ON repositories(name);

CREATE TABLE IF NOT EXISTS backup_jobs (
    id           TEXT PRIMARY KEY,
    mode         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'queued',
    total        INTEGER NOT NULL DEFAULT 0,
    completed    INTEGER NOT NULL DEFAULT 0,
    failed       INTEGER NOT NULL DEFAULT 0,
    destination  TEXT,
    started_at   TEXT,
    finished_at  TEXT,
    error        TEXT
);

CREATE TABLE IF NOT EXISTS backup_records (
    id               TEXT PRIMARY KEY,
    job_id           TEXT,
    repo_full_name   TEXT NOT NULL,
    repo_github_id   INTEGER,
    mode             TEXT NOT NULL,
    format           TEXT,
    path             TEXT NOT NULL,
    size_bytes       INTEGER NOT NULL DEFAULT 0,
    file_count       INTEGER NOT NULL DEFAULT 0,
    manifest_path    TEXT,
    status           TEXT NOT NULL DEFAULT 'pending',
    verified         INTEGER NOT NULL DEFAULT 0,
    git_lfs          INTEGER NOT NULL DEFAULT 0,
    head_sha         TEXT,
    default_branch   TEXT,
    error            TEXT,
    created_at       TEXT NOT NULL,
    completed_at     TEXT,
    FOREIGN KEY (job_id) REFERENCES backup_jobs(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_backup_records_repo ON backup_records(repo_full_name);
CREATE INDEX IF NOT EXISTS idx_backup_records_created ON backup_records(created_at DESC);

CREATE TABLE IF NOT EXISTS backup_snapshots (
    id          TEXT PRIMARY KEY,
    backup_id   TEXT NOT NULL,
    label       TEXT,
    path        TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    sha256      TEXT,
    created_at  TEXT NOT NULL,
    FOREIGN KEY (backup_id) REFERENCES backup_records(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS backup_files (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    backup_id   TEXT NOT NULL,
    path        TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    sha256      TEXT,
    FOREIGN KEY (backup_id) REFERENCES backup_records(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_backup_files_backup ON backup_files(backup_id);

CREATE TABLE IF NOT EXISTS backup_manifests (
    backup_id    TEXT PRIMARY KEY,
    manifest_json TEXT NOT NULL,
    sha256       TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    FOREIGN KEY (backup_id) REFERENCES backup_records(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS restore_jobs (
    id             TEXT PRIMARY KEY,
    backup_id      TEXT,
    destination    TEXT NOT NULL,
    conflict_mode  TEXT NOT NULL DEFAULT 'new_folder',
    status         TEXT NOT NULL DEFAULT 'pending',
    files_restored INTEGER NOT NULL DEFAULT 0,
    started_at     TEXT,
    finished_at    TEXT,
    error          TEXT
);

CREATE TABLE IF NOT EXISTS publish_jobs (
    id             TEXT PRIMARY KEY,
    local_path     TEXT NOT NULL,
    repo_name      TEXT NOT NULL,
    visibility     TEXT NOT NULL DEFAULT 'private',
    remote_url     TEXT,
    status         TEXT NOT NULL DEFAULT 'pending',
    warnings       TEXT,
    started_at     TEXT,
    finished_at    TEXT,
    error          TEXT
);

CREATE TABLE IF NOT EXISTS task_history (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL,
    title        TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    progress     INTEGER NOT NULL DEFAULT 0,
    detail       TEXT,
    created_at   TEXT NOT NULL,
    finished_at  TEXT,
    error        TEXT
);

CREATE TABLE IF NOT EXISTS application_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS activity_logs (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts      TEXT NOT NULL,
    level   TEXT NOT NULL,
    source  TEXT NOT NULL,
    message TEXT NOT NULL,
    details TEXT
);

CREATE INDEX IF NOT EXISTS idx_activity_ts ON activity_logs(ts DESC);
CREATE INDEX IF NOT EXISTS idx_activity_level ON activity_logs(level);
