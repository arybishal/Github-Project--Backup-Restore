// API layer — one async function per Tauri command. Command names and argument
// names are the frozen contract shared with src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  AccountInfo,
  BackupManifest,
  BackupRecord,
  BackupRequest,
  DashboardStats,
  DeviceCode,
  LocalProject,
  LogEntry,
  PublishPreview,
  PublishRequest,
  Repo,
  RepoCounts,
  RepoFilter,
  RepoSyncStats,
  RestoreRequest,
  Settings,
  StorageInfo,
  TaskDto,
  TaskProgress,
  VerifyResult,
} from './types';

/** True when running inside the Tauri desktop shell (v2 injects __TAURI_INTERNALS__). */
export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const BACKEND_UNAVAILABLE =
  'Desktop backend not available. This screen only works inside the Git Backup Restore desktop app — open the app window instead of a plain browser tab.';

function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error && e.message) return e.message;
  return String(e);
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) throw new Error(BACKEND_UNAVAILABLE);
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new Error(errMsg(e));
  }
}

// --- auth ------------------------------------------------------------------
export async function auth_status(): Promise<AccountInfo | null> {
  return call<AccountInfo | null>('auth_status');
}
export async function auth_login_token(args: { token: string }): Promise<AccountInfo> {
  return call<AccountInfo>('auth_login_token', args);
}
export async function auth_device_start(): Promise<DeviceCode> {
  return call<DeviceCode>('auth_device_start');
}
export async function auth_device_complete(args: {
  clientId: string;
  deviceCode: string;
}): Promise<AccountInfo> {
  // Tauri camelCases args; Rust takes `client_id` / `device_code`.
  return call<AccountInfo>('auth_device_complete', args);
}
export async function auth_logout(): Promise<boolean> {
  return call<boolean>('auth_logout');
}

// --- repositories ----------------------------------------------------------
export async function repos_sync(): Promise<RepoSyncStats> {
  return call<RepoSyncStats>('repos_sync');
}
export async function repos_list(args: { filter: RepoFilter }): Promise<Repo[]> {
  return call<Repo[]>('repos_list', args);
}
export async function repos_counts(): Promise<RepoCounts> {
  return call<RepoCounts>('repos_counts');
}

// --- backup ----------------------------------------------------------------
export async function backup_start(args: { req: BackupRequest }): Promise<string> {
  return call<string>('backup_start', args);
}
export async function tasks_list(): Promise<TaskDto[]> {
  return call<TaskDto[]>('tasks_list');
}
export async function task_cancel(args: { id: string }): Promise<void> {
  return call<void>('task_cancel', args);
}
export async function backup_history(args: { limit: number }): Promise<BackupRecord[]> {
  return call<BackupRecord[]>('backup_history', args);
}
export async function backup_manifest(args: { backupId: string }): Promise<BackupManifest> {
  return call<BackupManifest>('backup_manifest', args);
}
export async function backup_verify(args: { backupId: string }): Promise<VerifyResult> {
  return call<VerifyResult>('backup_verify', args);
}
export async function backup_delete(args: { backupId: string }): Promise<void> {
  return call<void>('backup_delete', args);
}

// --- restore ---------------------------------------------------------------
export async function restore_start(args: { req: RestoreRequest }): Promise<string> {
  return call<string>('restore_start', args);
}

// --- storage ---------------------------------------------------------------
export async function storage_info(): Promise<StorageInfo> {
  return call<StorageInfo>('storage_info');
}
export async function storage_set_root(args: { path: string }): Promise<void> {
  return call<void>('storage_set_root', args);
}

// --- publish ---------------------------------------------------------------
export async function publish_preview(args: { path: string }): Promise<PublishPreview> {
  return call<PublishPreview>('publish_preview', args);
}
export async function publish_start(args: { req: PublishRequest }): Promise<string> {
  return call<string>('publish_start', args);
}

// --- settings / logs / dashboard ------------------------------------------
export async function settings_get(): Promise<Settings> {
  return call<Settings>('settings_get');
}
export async function settings_set(args: { settings: Settings }): Promise<void> {
  return call<void>('settings_set', args);
}
export async function logs_list(args: {
  level: string;
  search: string;
  limit: number;
}): Promise<LogEntry[]> {
  return call<LogEntry[]>('logs_list', args);
}
export async function logs_export(): Promise<string> {
  return call<string>('logs_export');
}
export async function dashboard_stats(): Promise<DashboardStats> {
  return call<DashboardStats>('dashboard_stats');
}

// --- local projects --------------------------------------------------------
export async function local_projects_list(): Promise<LocalProject[]> {
  return call<LocalProject[]>('local_projects_list');
}
export async function local_projects_add(args: { path: string }): Promise<LocalProject> {
  return call<LocalProject>('local_projects_add', args);
}
export async function local_projects_remove(args: { path: string }): Promise<void> {
  return call<void>('local_projects_remove', args);
}

// --- misc ------------------------------------------------------------------
export async function app_version(): Promise<string> {
  return call<string>('app_version');
}
export async function pick_directory(): Promise<string | null> {
  return call<string | null>('pick_directory');
}

/**
 * NOT part of the frozen command contract: the Settings "Test" button needs a
 * git version probe. If the backend does not expose `git_test`, the returned
 * error is shown verbatim in the button's error state.
 */
export async function git_test(): Promise<string> {
  return call<string>('git_test');
}

// --- events ----------------------------------------------------------------
/**
 * Subscribe to live task progress. Best-effort: in a plain browser this returns
 * a no-op unlisten, and polling (tasks_list) still keeps the Task Center fresh.
 */
export async function onTaskProgress(
  cb: (p: TaskProgress) => void,
): Promise<UnlistenFn> {
  if (!isTauri) return () => {};
  try {
    return await listen<TaskProgress>('task_progress', (e) => cb(e.payload));
  } catch {
    return () => {};
  }
}
