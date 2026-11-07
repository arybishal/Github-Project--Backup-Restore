use gbr_core::db::Db;
use gbr_core::git::GitCli;
use gbr_core::tasks::TaskManager;

mod commands;

use commands::{spawn_progress_forwarder, AppState};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let paths = gbr_core::paths::AppPaths::default_root()?;
                paths.ensure()?;
                let db = Db::open(&paths.db_path()).await?;
                let git = GitCli::new(
                    gbr_core::settings::Settings::load(&db)
                        .await
                        .map(|s| s.git_path)
                        .unwrap_or_else(|_| "git".into()),
                );
                let (tasks, rx) = TaskManager::new(db.clone());
                spawn_progress_forwarder(handle.clone(), rx);
                handle.manage(AppState { db, git, tasks });
                Ok::<(), Box<dyn std::error::Error>>(())
            })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth_status,
            commands::auth_login_token,
            commands::auth_device_start,
            commands::auth_device_complete,
            commands::auth_logout,
            commands::repos_sync,
            commands::repos_list,
            commands::repos_counts,
            commands::backup_start,
            commands::backup_history,
            commands::backup_manifest,
            commands::backup_verify,
            commands::backup_delete,
            commands::restore_preview,
            commands::restore_start,
            commands::storage_info,
            commands::storage_set_root,
            commands::publish_preview,
            commands::publish_start,
            commands::settings_get,
            commands::settings_set,
            commands::logs_list,
            commands::logs_export,
            commands::dashboard_stats,
            commands::local_projects_list,
            commands::local_projects_add,
            commands::local_projects_remove,
            commands::app_version,
            commands::git_test,
            commands::pick_directory,
            commands::tasks_list,
            commands::task_cancel,
            commands::sensitive_scan,
            commands::check_for_updates,
            commands::install_update,
            commands::open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Git Backup Restore");
}
