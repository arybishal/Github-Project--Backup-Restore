// Shared API contract between the Tauri backend and the React UI.
// Keep in sync 1:1 with the serde structs in src-tauri/src/commands.rs.

export type BackupMode = 'full' | 'selective';
export type BackupFormat = 'mirror' | 'zip' | 'tar' | 'tar.zst' | 'folder';
export type TaskStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';
export type ConflictMode = 'new_folder' | 'merge' | 'replace';

export interface AccountInfo {
  username: string;
  avatar_url: string | null;
  scopes: string | null;
  auth_method: string;
}

export interface DeviceCode {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface Repo {
  github_id: number;
  node_id: string | null;
  owner: string;
  name: string;
  full_name: string;
  description: string | null;
  is_private: boolean;
  is_fork: boolean;
  is_archived: boolean;
  language: string | null;
  default_branch: string | null;
  size_kb: number | null;
  html_url: string | null;
  clone_url: string | null;
  pushed_at: string | null;
  updated_at: string | null;
  last_backup_status: string | null;
  last_backup_at: string | null;
}

export interface RepoSyncStats {
  fetched: number;
  pages: number;
  synced_at: string;
}

export interface RepoCounts {
  total: number;
  public: number;
  private: number;
  organization: number;
  archived: number;
}

export interface RepoFilter {
  search?: string;
  owner?: string | null;
  language?: string | null;
  visibility?: 'all' | 'public' | 'private';
  archived?: 'all' | 'archived' | 'active';
  sort?: 'name' | 'updated' | 'size';
  limit?: number;
  offset?: number;
}

export interface BackupRequest {
  mode: BackupMode;
  format?: BackupFormat;
  destination?: string | null;
  repos: string[]; // full_name list; empty + all = every repo
  all?: boolean;
  include_lfs?: boolean;
  retention?: number;
  // Selective backup scope (optional; backend reads them)
  checkout_root?: string | null;
  include_patterns?: string[];
  exclude_patterns?: string[];
  hidden?: boolean;
  ignored?: boolean;
  // Hard gate: engine refuses secret-looking files unless this is true
  acknowledge_secrets?: boolean;
}

export interface TaskProgress {
  task_id: string;
  title: string;
  stage: string;
  current_repo: string | null;
  percent: number | null;
  detail: string | null;
  transferred_bytes: number | null;
  elapsed_ms: number;
}

export interface TaskDto {
  id: string;
  kind: string;
  title: string;
  status: TaskStatus;
  progress: number;
  detail: string | null;
  created_at: string;
  finished_at: string | null;
  error: string | null;
}

export interface FileEntry {
  path: string;
  size: number;
  sha256: string;
}

export interface BackupManifest {
  manifest_version: number;
  backup_id: string;
  mode: BackupMode;
  format: BackupFormat;
  repository: {
    github_id: number | null;
    owner: string;
    name: string;
    full_name: string;
    source_url: string;
    default_branch: string | null;
    is_private: boolean;
  };
  created_at: string;
  app_version: string;
  git_version: string | null;
  head_sha: string | null;
  refs: { name: string; sha: string }[];
  payload_path: string;
  file_count: number;
  size_bytes: number;
  files: FileEntry[];
  lfs: {
    git_lfs_available: boolean;
    objects_backed_up: boolean;
    note: string | null;
  };
  verification: {
    status: string;
    method: string | null;
    verified_at: string | null;
    detail: string | null;
  };
  limitations: string[];
}

export interface BackupRecord {
  id: string;
  job_id: string | null;
  repo_full_name: string;
  mode: BackupMode;
  format: string | null;
  path: string;
  size_bytes: number;
  file_count: number;
  manifest_path: string | null;
  status: string;
  verified: boolean;
  git_lfs: boolean;
  head_sha: string | null;
  default_branch: string | null;
  error: string | null;
  created_at: string;
  completed_at: string | null;
}

export interface RestoreRequest {
  backup_id: string;
  destination: string;
  conflict_mode: ConflictMode;
  as_worktree?: boolean;
}

export interface RestoreToGitHubRequest {
  backup_id: string;
  target_repo?: string;
  private?: boolean;
  org?: string;
  allow_force: boolean;
}

export interface PublishPreview {
  path: string;
  is_git_repo: boolean;
  has_commits: boolean;
  file_count: number;
  total_bytes: number;
  large_files: { path: string; size: number }[];
  secret_hits: { path: string; kind: string }[];
  suggested_gitignore: string;
  current_branch: string | null;
}

export interface PublishRequest {
  path: string;
  repo_name: string;
  description: string | null;
  private: boolean;
  auto_init_readme: boolean;
  commit_message: string;
  allow_force: boolean;
}

export interface StorageInfo {
  root: string;
  total_bytes: number;
  backup_count: number;
  repo_count: number;
  snapshot_count: number;
  available_bytes: number;
  per_repo: { full_name: string; bytes: number; backups: number }[];
}

export interface DashboardStats {
  account: AccountInfo | null;
  repos: RepoCounts;
  total_backups: number;
  verified_backups: number;
  failed_backups: number;
  last_success: string | null;
  storage_bytes: number;
  pending_tasks: number;
  recent_activity: LogEntry[];
  recent_backups: BackupRecord[];
  health: { ok: number; unverified: number; failed: number };
}

export interface LogEntry {
  id: number;
  ts: string;
  level: string;
  source: string;
  message: string;
  details: string | null;
}

export interface Settings {
  backup_root: string;
  default_mode: BackupMode;
  default_format: BackupFormat;
  compression_level: number;
  snapshot_retention: number;
  git_path: string;
  github_client_id: string;
  log_level: string;
  notifications: boolean;
  encrypt_backups: boolean;
}

export interface LocalProject {
  path: string;
  name: string;
  is_git_repo: boolean;
  added_at: string;
}

export interface VerifyResult {
  backup_id: string;
  ok: boolean;
  checked_files: number;
  detail: string;
}

export interface UpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  release_name: string;
  release_notes: string;
  release_url: string;
  published_at: string | null;
  download_url: string | null;
  asset_name: string | null;
  asset_size: number | null;
}
