use std::path::PathBuf;
use std::sync::Arc;

use gbr_core::activity;
use gbr_core::auth::{self, DeviceCode};
use gbr_core::backup::{self, BackupEngine, FullBackupOptions};
use gbr_core::backup::{BackupRecordDto, StorageInfoDto, VerifyResultDto};
use gbr_core::dashboard;
use gbr_core::db::Db;
use gbr_core::error::GbrError;
use gbr_core::git::GitCli;
use gbr_core::github::GitHubClient;
use gbr_core::manifest::BackupManifest;
use gbr_core::manifest::StorageLayout;
use gbr_core::publish::LocalProject;
use gbr_core::publish::{self, PublishPreview, PublishRequest};
use gbr_core::repo_store::{self, RepoCounts, RepoFilter, RepoSyncStats};
use gbr_core::restore::RestorePlan;
use gbr_core::restore::{self, RestoreRequest};
use gbr_core::secretscan;
use gbr_core::selective::{ArchiveFormat, SelectiveOptions};
use gbr_core::settings::Settings;
use gbr_core::tasks::{TaskDto, TaskManager, TaskProgress};
use gbr_core::{auth::AuthInfo, github::Repo};
use serde::Deserialize;

/// Shared, immutable-ish application state managed by Tauri.
pub struct AppState {
    pub db: Db,
    pub git: GitCli,
    pub tasks: Arc<TaskManager>,
}

impl AppState {
    pub async fn layout(&self) -> Result<StorageLayout, String> {
        let settings = Settings::load(&self.db).await.map_err(err)?;
        let paths = gbr_core::paths::AppPaths::default_root().map_err(err)?;
        let layout = StorageLayout::from_settings(&paths, &settings.backup_root);
        layout.ensure().map_err(err)?;
        Ok(layout)
    }

    pub async fn token(&self) -> Result<String, String> {
        auth::load_token()
            .map_err(err)?
            .ok_or_else(|| GbrError::AuthRequired("no GitHub account connected".into()).to_string())
    }
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- auth

#[tauri::command]
pub async fn auth_status(state: tauri::State<'_, AppState>) -> Result<Option<AuthInfo>, String> {
    auth::current_account(&state.db).await.map_err(err)
}

#[tauri::command]
pub async fn auth_login_token(
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<AuthInfo, String> {
    auth::login_with_token(&state.db, &token).await.map_err(err)
}

#[tauri::command]
pub async fn auth_device_start(state: tauri::State<'_, AppState>) -> Result<DeviceCode, String> {
    // The client id comes from Settings — this app never ships a secret.
    let settings = Settings::load(&state.db).await.map_err(err)?;
    auth::start_device_flow(&settings.github_client_id)
        .await
        .map_err(err)
}

/// Polls until the user approves the device code in their browser (or the
/// code expires). Runs off the UI thread, so the window stays responsive.
#[tauri::command]
pub async fn auth_device_complete(
    state: tauri::State<'_, AppState>,
    client_id: String,
    device_code: String,
) -> Result<AuthInfo, String> {
    let code = DeviceCode {
        device_code,
        user_code: String::new(),
        verification_uri: String::new(),
        expires_in: 900,
        interval: 5,
    };
    auth::complete_device_flow(&state.db, &client_id, &code, || false)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn auth_logout(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    auth::logout(&state.db).await.map_err(err)
}

// ---------------------------------------------------------------- repositories

#[tauri::command]
pub async fn repos_sync(state: tauri::State<'_, AppState>) -> Result<RepoSyncStats, String> {
    let gh = GitHubClient::connect().await.map_err(err)?;
    let stats = repo_store::sync_from_github(&state.db, &gh)
        .await
        .map_err(err)?;
    let _ = repo_store::cache_orgs(&state.db, &gh).await;
    Ok(stats)
}

#[tauri::command]
pub async fn repos_list(
    state: tauri::State<'_, AppState>,
    filter: RepoFilter,
) -> Result<Vec<Repo>, String> {
    repo_store::list(&state.db, &filter).await.map_err(err)
}

#[tauri::command]
pub async fn repos_counts(state: tauri::State<'_, AppState>) -> Result<RepoCounts, String> {
    repo_store::counts(&state.db).await.map_err(err)
}

// ---------------------------------------------------------------- tasks

#[tauri::command]
pub async fn tasks_list(state: tauri::State<'_, AppState>) -> Result<Vec<TaskDto>, String> {
    state.tasks.history(100).await.map_err(err)
}

#[tauri::command]
pub async fn task_cancel(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    state.tasks.cancel(&id).await.map_err(err)?;
    Ok(())
}

// ---------------------------------------------------------------- backup

#[derive(Debug, Clone, Deserialize)]
pub struct BackupRequest {
    pub mode: String,
    pub format: Option<String>,
    pub destination: Option<String>,
    pub repos: Vec<String>,
    pub all: Option<bool>,
    pub include_lfs: Option<bool>,
    pub retention: Option<u32>,
    /// Selective scope: a local checkout to read files from (optional).
    pub checkout_root: Option<String>,
    pub include_patterns: Option<Vec<String>>,
    pub exclude_patterns: Option<Vec<String>>,
    pub hidden: Option<bool>,
    pub ignored: Option<bool>,
    /// Explicit user acknowledgement that secret-looking files may be
    /// included. Defaults to `false`: without it the engine refuses.
    #[serde(alias = "sensitive_acknowledged")]
    pub acknowledge_secrets: Option<bool>,
}

#[tauri::command]
pub async fn backup_start(
    state: tauri::State<'_, AppState>,
    req: BackupRequest,
) -> Result<String, String> {
    let layout = match &req.destination {
        Some(d) if !d.trim().is_empty() => {
            let paths = gbr_core::paths::AppPaths::default_root().map_err(err)?;
            let _ = paths;
            let l = StorageLayout::resolve(PathBuf::from(d));
            l.ensure().map_err(err)?;
            l
        }
        _ => state.layout().await?,
    };

    let repos = if req.all.unwrap_or(false) {
        repo_store::list(
            &state.db,
            &RepoFilter {
                limit: Some(2000),
                ..Default::default()
            },
        )
        .await
        .map_err(err)?
    } else {
        let mut out = Vec::new();
        for name in &req.repos {
            let r = repo_store::get_by_full_name(&state.db, name)
                .await
                .map_err(err)?
                .ok_or_else(|| format!("unknown repository '{name}' — run a sync first"))?;
            out.push(r);
        }
        out
    };

    if repos.is_empty() {
        return Err("no repositories selected".into());
    }

    let settings = Settings::load(&state.db).await.map_err(err)?;
    let engine = BackupEngine::new(state.db.clone(), state.git.clone(), layout);
    let token = state.token().await?;
    let task = state.tasks.begin("backup", "Repository backup").await;
    let id = task.id.clone();
    let mode = req.mode.clone();
    let format = req
        .format
        .clone()
        .unwrap_or_else(|| settings.default_format.clone());
    let include_lfs = req.include_lfs.unwrap_or(true);
    let retention = req.retention.unwrap_or(settings.snapshot_retention);
    let checkout_root = req.checkout_root.clone();
    let include_patterns = req.include_patterns.clone().unwrap_or_default();
    let exclude_patterns = req.exclude_patterns.clone().unwrap_or_default();
    let hidden = req.hidden.unwrap_or(false);
    let ignored = req.ignored.unwrap_or(false);
    let acknowledge_secrets = req.acknowledge_secrets.unwrap_or(false);
    let compression = settings.compression_level;
    let local_tree = checkout_root.as_deref().map(PathBuf::from);
    let db = state.db.clone();
    let tasks = state.tasks.clone();

    backup::record_job_start(&state.db, &id, &mode, repos.len()).await;

    tauri::async_runtime::spawn(async move {
        let total = repos.len();
        let mut error: Option<String> = None;
        for (i, repo) in repos.iter().enumerate() {
            if task.is_cancelled() {
                error = Some("operation cancelled".into());
                break;
            }
            let pct = (i as f32 / total as f32) * 100.0;
            task.report_repo(
                if mode == "selective" {
                    "selective backup"
                } else {
                    "full backup"
                },
                Some(repo.full_name.clone()),
                Some(pct),
                None,
            );

            let res = if mode == "selective" {
                let opts = SelectiveOptions {
                    format: ArchiveFormat::parse(&format).unwrap_or(ArchiveFormat::Zip),
                    include_patterns: include_patterns.clone(),
                    exclude_patterns: exclude_patterns.clone(),
                    include_hidden: hidden,
                    include_ignored: ignored,
                    include_git_metadata: false,
                    // Hard stop unless the UI collected an explicit
                    // acknowledgement from the user first.
                    sensitive_acknowledged: acknowledge_secrets,
                    compression_level: compression,
                };
                engine
                    .run_selective(
                        &task,
                        repo,
                        &token,
                        &opts,
                        Some(&task.id),
                        local_tree.as_deref(),
                    )
                    .await
            } else {
                let opts = FullBackupOptions {
                    include_lfs,
                    retention,
                    snapshot: true,
                };
                engine
                    .run_full(&task, repo, &token, &opts, Some(&task.id))
                    .await
            };

            if let Err(e) = res {
                backup::mark_failed(&db, &repo.full_name, &e.to_string()).await;
                error = Some(format!("{}: {e}", repo.full_name));
                // One failing repository must not abandon the rest of the batch.
                continue;
            }
        }
        task.report("complete", Some(100.0), None);
        backup::record_job_finish(&db, &task.id, error.as_deref()).await;
        tasks.finish(&task, error).await;
    });

    Ok(id)
}

#[tauri::command]
pub async fn backup_history(
    state: tauri::State<'_, AppState>,
    limit: i64,
) -> Result<Vec<BackupRecordDto>, String> {
    backup::list_records(&state.db, limit).await.map_err(err)
}

#[tauri::command]
pub async fn backup_manifest(
    state: tauri::State<'_, AppState>,
    backup_id: String,
) -> Result<BackupManifest, String> {
    backup::load_manifest(&state.db, &backup_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn backup_verify(
    state: tauri::State<'_, AppState>,
    backup_id: String,
) -> Result<VerifyResultDto, String> {
    let layout = state.layout().await?;
    backup::verify_backup(&state.git, &layout, &state.db, &backup_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn backup_delete(
    state: tauri::State<'_, AppState>,
    backup_id: String,
    force: Option<bool>,
) -> Result<(), String> {
    let layout = state.layout().await?;
    backup::delete_backup(&state.db, &layout, &backup_id, force.unwrap_or(false))
        .await
        .map_err(err)
}

// ---------------------------------------------------------------- restore

#[tauri::command]
pub async fn restore_preview(
    state: tauri::State<'_, AppState>,
    backup_id: String,
    destination: String,
    conflict_mode: String,
) -> Result<RestorePlan, String> {
    let layout = state.layout().await?;
    let request = RestoreRequest {
        backup_id,
        destination,
        conflict_mode: restore::ConflictMode::parse(&conflict_mode).map_err(err)?,
        as_worktree: true,
    };
    restore::plan(&state.db, &layout, &request)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn restore_start(
    state: tauri::State<'_, AppState>,
    req: RestoreRequest,
) -> Result<String, String> {
    let layout = state.layout().await?;
    let engine = BackupEngine::new(state.db.clone(), state.git.clone(), layout);
    let task = state.tasks.begin("restore", "Restore backup").await;
    let id = task.id.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    let conflict_label = serde_json::to_string(&req.conflict_mode)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();

    tauri::async_runtime::spawn(async move {
        // `replace` is never applied silently: the UI must confirm first,
        // which it does by sending conflict_mode "replace" only after the
        // user ticks the warning checkbox (allow_replace).
        let allow_replace = req.conflict_mode == restore::ConflictMode::Replace;
        let res = restore::run(&engine, &task, &req, allow_replace).await;
        match &res {
            Ok(out) => {
                let _ = restore::record_job(
                    &db,
                    &out.backup_id,
                    &out.destination,
                    &conflict_label,
                    "succeeded",
                    out.files_restored,
                    None,
                )
                .await;
                task.report("complete", Some(100.0), Some(out.detail.clone()));
                tasks.finish(&task, None).await;
            }
            Err(e) => {
                let _ = restore::record_job(
                    &db,
                    &req.backup_id,
                    &req.destination,
                    &conflict_label,
                    "failed",
                    0,
                    Some(&e.to_string()),
                )
                .await;
                tasks.finish(&task, Some(e.to_string())).await;
            }
        }
    });

    Ok(id)
}

#[tauri::command]
pub async fn restore_to_github_start(
    state: tauri::State<'_, AppState>,
    req: restore::RestoreToGitHubRequest,
) -> Result<String, String> {
    let layout = state.layout().await?;
    let engine = BackupEngine::new(state.db.clone(), state.git.clone(), layout);
    let task = state
        .tasks
        .begin("restore", "Restore backup to GitHub")
        .await;
    let id = task.id.clone();
    let tasks = state.tasks.clone();

    tauri::async_runtime::spawn(async move {
        let res = restore::restore_to_github(&task, &engine, &req).await;
        match &res {
            Ok(out) => {
                task.report("complete", Some(100.0), Some(out.detail.clone()));
                tasks.finish(&task, None).await;
            }
            Err(e) => {
                tasks.finish(&task, Some(e.to_string())).await;
            }
        }
    });

    Ok(id)
}

// ---------------------------------------------------------------- storage

#[tauri::command]
pub async fn storage_info(state: tauri::State<'_, AppState>) -> Result<StorageInfoDto, String> {
    let layout = state.layout().await?;
    backup::storage_info(&state.db, &layout).await.map_err(err)
}

#[tauri::command]
pub async fn storage_set_root(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let mut s = Settings::load(&state.db).await.map_err(err)?;
    s.backup_root = path;
    s.save(&state.db).await.map_err(err)
}

// ---------------------------------------------------------------- publish

#[tauri::command]
pub async fn publish_preview(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<PublishPreview, String> {
    publish::preview(&PathBuf::from(path), &state.git)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn publish_start(
    state: tauri::State<'_, AppState>,
    req: PublishRequest,
) -> Result<String, String> {
    let gh = GitHubClient::connect().await.map_err(err)?;
    let task = state.tasks.begin("publish", "Publish to GitHub").await;
    let id = task.id.clone();
    let tasks = state.tasks.clone();
    let db = state.db.clone();
    let git = state.git.clone();

    tauri::async_runtime::spawn(async move {
        let res = publish::run(&db, &git, &task, &gh, &req).await;
        match &res {
            Ok(o) => {
                task.report(
                    "complete",
                    Some(100.0),
                    Some(format!("{} — {}", o.repo_full_name, o.html_url)),
                );
                tasks.finish(&task, None).await;
            }
            Err(e) => {
                publish::record_publish_job(
                    &db,
                    &req.path,
                    &req.repo_name,
                    if req.private { "private" } else { "public" },
                    "",
                    "failed",
                    None,
                    Some(&e.to_string()),
                )
                .await;
                tasks.finish(&task, Some(e.to_string())).await;
            }
        }
    });

    Ok(id)
}

// ---------------------------------------------------------------- settings / logs

#[tauri::command]
pub async fn settings_get(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    Settings::load(&state.db).await.map_err(err)
}

#[tauri::command]
pub async fn settings_set(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    settings.save(&state.db).await.map_err(err)
}

#[tauri::command]
pub async fn logs_list(
    state: tauri::State<'_, AppState>,
    level: String,
    search: String,
    limit: i64,
) -> Result<Vec<activity::ActivityEntry>, String> {
    let level = if level.is_empty() {
        None
    } else {
        Some(level.as_str())
    };
    let search = if search.is_empty() {
        None
    } else {
        Some(search.as_str())
    };
    activity::list(&state.db, level, search, limit)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn logs_export(state: tauri::State<'_, AppState>) -> Result<String, String> {
    // Redaction happens at write time and again on export.
    activity::export_text(&state.db, 5000).await.map_err(err)
}

// ---------------------------------------------------------------- dashboard / projects

#[tauri::command]
pub async fn dashboard_stats(
    state: tauri::State<'_, AppState>,
) -> Result<dashboard::DashboardStats, String> {
    let layout = state.layout().await?;
    dashboard::stats(&state.db, &layout).await.map_err(err)
}

#[tauri::command]
pub async fn local_projects_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<LocalProject>, String> {
    publish::list_local_projects(&state.db).await.map_err(err)
}

#[tauri::command]
pub async fn local_projects_add(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<LocalProject, String> {
    publish::add_local_project(&state.db, &state.git, &PathBuf::from(path))
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn local_projects_remove(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    publish::remove_local_project(&state.db, &path)
        .await
        .map_err(err)?;
    Ok(())
}

// ---------------------------------------------------------------- misc

#[tauri::command]
pub fn app_version() -> String {
    gbr_core::VERSION.to_string()
}

/// Returns the detected git version, or an actionable error message.
#[tauri::command]
pub async fn git_test(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let version = state.git.version().await.map_err(err)?;
    let lfs = state.git.lfs_available().await;
    Ok(format!(
        "{version} (git-lfs: {})",
        if lfs {
            "available"
        } else {
            "not installed — LFS objects will not be captured"
        }
    ))
}

/// Native folder picker. Blocks this command's thread only; the window stays
/// responsive because the command runs off the UI thread.
#[tauri::command]
pub fn pick_directory(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn sensitive_scan(paths: Vec<String>) -> Result<Vec<secretscan::SecretHit>, String> {
    Ok(secretscan::scan(&paths))
}

#[tauri::command]
pub async fn check_for_updates() -> Result<gbr_core::update::UpdateInfoDto, String> {
    gbr_core::update::check_for_updates().await.map_err(err)
}

#[tauri::command]
pub async fn install_update(download_url: String, asset_name: String) -> Result<String, String> {
    let dest_dir = gbr_core::update::download_dir();
    let target_file = dest_dir.join(&asset_name);
    gbr_core::update::download_asset(&download_url, &target_file)
        .await
        .map_err(err)?;
    gbr_core::update::launch_installer(&target_file).map_err(err)?;
    Ok(format!("Installer launched: {}", target_file.display()))
}

#[tauri::command]
pub fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(&url, None::<&str>).map_err(err)
}

/// Forward engine progress events to every window as `task_progress`.
pub fn spawn_progress_forwarder(
    app: tauri::AppHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TaskProgress>,
) {
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        while let Some(p) = rx.recv().await {
            if let Err(e) = app.emit("task_progress", &p) {
                eprintln!("warn: could not emit task progress: {e}");
            }
        }
    });
}
