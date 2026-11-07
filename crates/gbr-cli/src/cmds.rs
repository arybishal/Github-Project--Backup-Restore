use std::path::PathBuf;

use gbr_core::activity;
use gbr_core::auth;
use gbr_core::backup::{self, BackupEngine, FullBackupOptions};
use gbr_core::bytes_human;
use gbr_core::dashboard;
use gbr_core::error::GbrError;
use gbr_core::github::GitHubClient;
use gbr_core::manifest::{self, StorageLayout};
use gbr_core::publish::{self, PublishRequest};
use gbr_core::repo_store::{self, RepoFilter};
use gbr_core::restore::{self, ConflictMode, RestoreRequest};
use gbr_core::selective::{ArchiveFormat, SelectiveOptions};
use gbr_core::settings::Settings;
use gbr_core::tasks::TaskHandle;

use crate::ctx::{Ctx, ExitCode};
use crate::{Cli, Command, ConfigCmd, GitHubCmd, ReposCmd, StorageCmd, TasksCmd};

pub async fn dispatch(cli: Cli) -> ExitCode {
    let ctx = match Ctx::open(cli.json, cli.yes).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not initialise application state: {e:#}");
            return ExitCode::Error;
        }
    };

    let result = match cli.command {
        Command::Login { token, device } => cmd_login(&ctx, token, device).await,
        Command::Logout => cmd_logout(&ctx).await,
        Command::Status => cmd_status(&ctx).await,
        Command::Repos { cmd } => cmd_repos(&ctx, cmd).await,
        Command::Backup {
            repository,
            all,
            mode,
            format,
            dest,
            include,
            exclude,
            hidden,
            ignored,
            lfs,
            retention,
        } => {
            let o = BackupArgs {
                repository,
                all,
                mode,
                format,
                dest,
                include,
                exclude,
                hidden,
                ignored,
                lfs,
                retention,
            };
            cmd_backup(&ctx, o).await
        }
        Command::Restore {
            backup_id,
            latest,
            dest,
            conflict,
            worktree,
        } => {
            let o = RestoreArgs {
                backup_id,
                latest,
                dest,
                conflict,
                worktree,
            };
            cmd_restore(&ctx, o).await
        }
        Command::Verify { backup_id, all } => cmd_verify(&ctx, backup_id, all).await,
        Command::History { limit } => cmd_history(&ctx, limit).await,
        Command::Storage { cmd } => cmd_storage(&ctx, cmd).await,
        Command::Publish {
            path,
            name,
            description,
            public,
            message,
            readme,
            force,
            remote,
            allow_secrets,
        } => {
            let o = PublishArgs {
                path,
                name,
                description,
                public,
                message,
                readme,
                force,
                remote,
                allow_secrets,
            };
            cmd_publish(&ctx, o).await
        }
        Command::GitHub { cmd } => cmd_github(&ctx, cmd).await,
        Command::Tasks { cmd } => cmd_tasks(&ctx, cmd).await,
        Command::Logs {
            level,
            search,
            limit,
            export,
        } => cmd_logs(&ctx, level, search, limit, export).await,
        Command::Config { cmd } => cmd_config(&ctx, cmd).await,
        Command::Doctor => cmd_doctor(&ctx).await,
        Command::Update { check } => cmd_update(&ctx, check).await,
    };

    match result {
        Ok(code) => code,
        Err(e) => report_error(&e),
    }
}

fn report_error(e: &GbrError) -> ExitCode {
    eprintln!("error: {e}");
    match e {
        GbrError::AuthRequired(_) => ExitCode::AuthRequired,
        GbrError::Verification(_) => ExitCode::VerificationFailed,
        GbrError::InvalidInput(_) => ExitCode::Usage,
        GbrError::Other(m) if m.contains("cancelled") => ExitCode::Cancelled,
        _ => ExitCode::Error,
    }
}

type CmdResult = std::result::Result<ExitCode, GbrError>;
// ---------------------------------------------------------------- auth

async fn cmd_login(ctx: &Ctx, token: Option<String>, device: bool) -> CmdResult {
    let info = if let Some(t) = token {
        auth::login_with_token(&ctx.db, &t).await?
    } else if device {
        let client_id = ctx.settings.github_client_id.clone();
        if client_id.trim().is_empty() {
            return Err(GbrError::InvalidInput(
                "device flow needs a client id: run `gitbackup config set github_client_id <id>` \
                 (register a GitHub OAuth app; device flow never uses a secret)"
                    .into(),
            ));
        }
        let code = auth::start_device_flow(&client_id).await?;
        if !ctx.json {
            println!("Open:  {}", code.verification_uri);
            println!("Code:  {}", code.user_code);
            println!("Waiting for approval in your browser...\n");
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let c2 = cancel.clone();
        let ctrl = tokio::spawn(async move {
            use tokio::signal::ctrl_c;
            let _ = ctrl_c().await;
            c2.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let res = auth::complete_device_flow(&ctx.db, &client_id, &code, || {
            cancel.load(std::sync::atomic::Ordering::SeqCst)
        })
        .await;
        ctrl.abort();
        res?
    } else {
        // Interactive token entry when a TTY is available.
        let t = ctx
            .prompt_secret("GitHub personal access token:")
            .ok_or_else(|| {
                GbrError::InvalidInput(
                    "provide --token <TOKEN> or --device in non-interactive mode".into(),
                )
            })?;
        auth::login_with_token(&ctx.db, &t).await?
    };

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Connected as @{} ({})", info.username, info.auth_method);
        if let Some(s) = &info.scopes {
            println!("Scopes: {s}");
        }
    }
    Ok(ExitCode::Ok)
}

async fn cmd_logout(ctx: &Ctx) -> CmdResult {
    let removed = auth::logout(&ctx.db).await?;
    if ctx.json {
        println!("{}", serde_json::json!({ "removed": removed }));
    } else if removed {
        println!("Disconnected. Tokens were removed from the OS credential manager.");
    } else {
        println!("No stored credentials found.");
    }
    Ok(ExitCode::Ok)
}

async fn cmd_status(ctx: &Ctx) -> CmdResult {
    let account = auth::current_account(&ctx.db).await?;
    let has_token = auth::load_token()?.is_some();
    let git_version = ctx
        .git
        .version()
        .await
        .unwrap_or_else(|e| format!("ERROR: {e}"));
    let lfs = ctx.git.lfs_available().await;
    let storage = backup::storage_info(&ctx.db, &ctx.layout).await?;
    let counts = repo_store::counts(&ctx.db).await?;
    let stats = dashboard::stats(&ctx.db, &ctx.layout).await?;

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "authenticated": has_token,
                "account": account,
                "git_version": git_version,
                "git_lfs": lfs,
                "backup_root": storage.root,
                "storage_bytes": storage.total_bytes,
                "available_bytes": storage.available_bytes,
                "repositories": counts,
                "total_backups": stats.total_backups,
                "pending_tasks": stats.pending_tasks,
            }))?
        );
        return Ok(ExitCode::Ok);
    }

    println!("Git Backup Restore {}", gbr_core::VERSION);
    println!(
        "GitHub:       {}",
        match &account {
            Some(a) => format!("@{} ({})", a.username, a.auth_method),
            None => "not connected".to_string(),
        }
    );
    println!(
        "Token:        {}",
        if has_token {
            "stored in OS keychain"
        } else {
            "missing"
        }
    );
    println!("Git:          {git_version}");
    println!(
        "Git LFS:      {}",
        if lfs { "available" } else { "not installed" }
    );
    println!("Backup root:  {}", storage.root);
    println!(
        "Storage:      {} used, {} available",
        bytes_human(storage.total_bytes),
        bytes_human(storage.available_bytes)
    );
    println!(
        "Repositories: {} cached ({} public, {} private, {} org)",
        counts.total, counts.public, counts.private, counts.organization
    );
    println!(
        "Backups:      {} total, {} pending task(s)",
        stats.total_backups, stats.pending_tasks
    );
    if !has_token {
        println!("\nRun `gitbackup login` to connect a GitHub account.");
    }
    Ok(ExitCode::Ok)
}

// ---------------------------------------------------------------- repos

async fn cmd_repos(ctx: &Ctx, cmd: ReposCmd) -> CmdResult {
    match cmd {
        ReposCmd::Sync => {
            let gh = GitHubClient::connect().await?;
            let stats = repo_store::sync_from_github(&ctx.db, &gh).await?;
            let _ = repo_store::cache_orgs(&ctx.db, &gh).await;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
            } else {
                println!(
                    "Synced {} repositories across {} page(s).",
                    stats.fetched, stats.pages
                );
            }
            Ok(ExitCode::Ok)
        }
        ReposCmd::List {
            search,
            owner,
            language,
            visibility,
            archived,
            sort,
            limit,
            offset,
        } => {
            let filter = RepoFilter {
                search,
                owner,
                language,
                visibility: Some(visibility),
                archived: Some(archived),
                sort: Some(sort),
                limit: Some(limit),
                offset: Some(offset),
            };
            let repos = repo_store::list(&ctx.db, &filter).await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&repos)?);
            } else if repos.is_empty() {
                println!("No repositories match. Run `gitbackup repos sync` first.");
            } else {
                println!(
                    "{:<44} {:<10} {:<12} {:<10} {:<12}",
                    "REPOSITORY", "VISIBILITY", "LANGUAGE", "SIZE", "LAST BACKUP"
                );
                for r in &repos {
                    println!(
                        "{:<44} {:<10} {:<12} {:<10} {:<12}",
                        truncate(&r.full_name, 44),
                        if r.is_private { "private" } else { "public" },
                        r.language.as_deref().unwrap_or("-"),
                        r.size_kb
                            .map(|s| bytes_human(s as u64 * 1024))
                            .unwrap_or_else(|| "-".into()),
                        r.last_backup_status.as_deref().unwrap_or("never"),
                    );
                }
                println!("\n{} repository(ies).", repos.len());
            }
            Ok(ExitCode::Ok)
        }
        ReposCmd::Search { keyword } => {
            let filter = RepoFilter {
                search: Some(keyword),
                limit: Some(200),
                ..Default::default()
            };
            let repos = repo_store::list(&ctx.db, &filter).await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&repos)?);
            } else if repos.is_empty() {
                println!("No matches.");
            } else {
                for r in &repos {
                    println!(
                        "{}  [{}]  {}",
                        r.full_name,
                        if r.is_private { "private" } else { "public" },
                        r.description.as_deref().unwrap_or("")
                    );
                }
            }
            Ok(ExitCode::Ok)
        }
        ReposCmd::Facets => {
            let (owners, langs) = repo_store::facets(&ctx.db).await?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "owners": owners, "languages": langs
                    }))?
                );
            } else {
                println!("Owners:    {}", owners.join(", "));
                println!("Languages: {}", langs.join(", "));
            }
            Ok(ExitCode::Ok)
        }
    }
}

// ---------------------------------------------------------------- backup

struct BackupArgs {
    repository: Option<String>,
    all: bool,
    mode: String,
    format: String,
    dest: Option<String>,
    include: Vec<String>,
    exclude: Vec<String>,
    hidden: bool,
    ignored: bool,
    lfs: bool,
    retention: Option<u32>,
}

async fn cmd_backup(ctx: &Ctx, a: BackupArgs) -> CmdResult {
    if a.repository.is_none() && !a.all {
        return Err(GbrError::InvalidInput(
            "provide a repository (owner/name) or --all".into(),
        ));
    }

    // --dest overrides the configured storage root for this run only.
    let layout = match &a.dest {
        Some(d) => {
            let l = StorageLayout::resolve(PathBuf::from(d));
            l.ensure()?;
            l
        }
        None => ctx.layout.clone(),
    };

    let repos = if a.all {
        repo_store::list(
            &ctx.db,
            &RepoFilter {
                limit: Some(2000),
                ..Default::default()
            },
        )
        .await?
    } else {
        let name = a.repository.clone().unwrap_or_default();
        match repo_store::get_by_full_name(&ctx.db, &name).await? {
            Some(r) => vec![r],
            None => {
                return Err(GbrError::InvalidInput(format!(
                    "unknown repository '{name}' — run `gitbackup repos sync` first"
                )));
            }
        }
    };

    if repos.is_empty() {
        return Err(GbrError::InvalidInput(
            "no repositories to back up (run `gitbackup repos sync`)".into(),
        ));
    }

    let engine = BackupEngine::new(ctx.db.clone(), ctx.git.clone(), layout.clone());
    let token = auth::load_token()?.unwrap_or_default();
    let handle = TaskHandle::detached(uuid::new_v4_string(), "backup");
    let total = repos.len();

    // Ctrl+C cancels the batch cooperatively: already-completed backups stay
    // on disk and are verified, the rest simply stop.
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cflag = cancelled.clone();
    let ctrl = tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cflag.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let mut succeeded = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();

    if !ctx.json {
        println!(
            "Backing up {} repository(ies) → {}\n",
            total,
            layout.root().display()
        );
    }

    for (i, repo) in repos.iter().enumerate() {
        if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            ctx.progress_done();
            println!("\nCancelled after {i} of {total} repository(ies).");
            ctrl.abort();
            return Ok(ExitCode::Cancelled);
        }
        ctx.progress(i + 1, total, &repo.full_name);
        let res = if a.mode == "selective" {
            let opts = SelectiveOptions {
                format: ArchiveFormat::parse(&a.format)?,
                include_patterns: a.include.clone(),
                exclude_patterns: a.exclude.clone(),
                include_hidden: a.hidden,
                include_ignored: a.ignored,
                include_git_metadata: false,
                sensitive_acknowledged: ctx.yes,
                compression_level: ctx.settings.compression_level,
            };
            engine
                .run_selective(&handle, repo, &token, &opts, None, None)
                .await
        } else {
            let opts = FullBackupOptions {
                include_lfs: a.lfs,
                retention: a.retention.unwrap_or(ctx.settings.snapshot_retention),
                snapshot: true,
            };
            engine.run_full(&handle, repo, &token, &opts, None).await
        };

        match res {
            Ok(out) => {
                if !out.verified {
                    failed.push((
                        repo.full_name.clone(),
                        "backup written but verification failed".into(),
                    ));
                } else {
                    succeeded.push(out);
                }
            }
            Err(e) => {
                backup::mark_failed(&ctx.db, &repo.full_name, &e.to_string()).await;
                failed.push((repo.full_name.clone(), e.to_string()));
            }
        }
    }
    ctx.progress_done();
    ctrl.abort();

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "succeeded": succeeded,
                "failed": failed.iter().map(|(r, e)| serde_json::json!({"repo": r, "error": e})).collect::<Vec<_>>(),
            }))?
        );
    } else {
        for out in &succeeded {
            println!(
                "  ok  {}  {}  {} refs  {}  {}",
                out.repo_full_name,
                if out.updated_existing {
                    "(updated)"
                } else {
                    "(initial)"
                },
                out.refs,
                bytes_human(out.size_bytes),
                if out.verified {
                    "verified"
                } else {
                    "UNVERIFIED"
                },
            );
        }
        for (r, e) in &failed {
            println!("  FAIL {r}: {e}");
        }
        println!("\n{} succeeded, {} failed.", succeeded.len(), failed.len());
    }

    if !failed.is_empty() {
        return Ok(ExitCode::Error);
    }
    Ok(ExitCode::Ok)
}

// ---------------------------------------------------------------- restore

struct RestoreArgs {
    backup_id: Option<String>,
    latest: Option<String>,
    dest: Option<String>,
    conflict: String,
    worktree: bool,
}

async fn cmd_restore(ctx: &Ctx, a: RestoreArgs) -> CmdResult {
    let backup_id = if let Some(id) = a.backup_id {
        id
    } else if let Some(repo) = &a.latest {
        let rows = backup::list_records(&ctx.db, 500).await?;
        rows.into_iter()
            .find(|r| &r.repo_full_name == repo && r.status == "completed")
            .map(|r| r.id)
            .ok_or_else(|| GbrError::BackupNotFound(format!("no completed backup for {repo}")))?
    } else {
        return Err(GbrError::InvalidInput(
            "provide a backup id or --latest owner/name".into(),
        ));
    };

    let dest = a.dest.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .join(format!("restored-{backup_id}"))
            .to_string_lossy()
            .to_string()
    });

    let conflict = ConflictMode::parse(&a.conflict)?;
    let request = RestoreRequest {
        backup_id: backup_id.clone(),
        destination: dest.clone(),
        conflict_mode: conflict,
        as_worktree: a.worktree,
    };

    let plan = restore::plan(&ctx.db, &ctx.layout, &request).await?;
    if !plan.can_proceed {
        for b in &plan.blockers {
            eprintln!("blocker: {b}");
        }
        return Ok(ExitCode::Usage);
    }
    for c in &plan.conflicts {
        eprintln!("warning: {c}");
    }
    if conflict == ConflictMode::Replace
        && !ctx.confirm(&format!(
            "Overwrite files in {dest}? This cannot be undone."
        ))
    {
        println!("Cancelled.");
        return Ok(ExitCode::Cancelled);
    }

    let engine = BackupEngine::new(ctx.db.clone(), ctx.git.clone(), ctx.layout.clone());
    let handle = TaskHandle::detached(uuid_str(), "restore");
    let outcome = restore::run(
        &engine,
        &handle,
        &request,
        conflict == ConflictMode::Replace,
    )
    .await?;
    restore::record_job(
        &ctx.db,
        &backup_id,
        &dest,
        &a.conflict,
        "succeeded",
        outcome.files_restored,
        None,
    )
    .await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!(
            "Restored {} → {} ({} items, {})",
            backup_id,
            dest,
            outcome.files_restored,
            if outcome.verified {
                "verified"
            } else {
                "unverified"
            }
        );
    }
    Ok(ExitCode::Ok)
}

async fn cmd_verify(ctx: &Ctx, backup_id: Option<String>, all: bool) -> CmdResult {
    let ids: Vec<String> = if all {
        backup::list_records(&ctx.db, 1000)
            .await?
            .into_iter()
            .map(|r| r.id)
            .collect()
    } else {
        vec![backup_id
            .ok_or_else(|| GbrError::InvalidInput("provide a backup id or --all".into()))?]
    };

    if ids.is_empty() {
        println!("No backups to verify.");
        return Ok(ExitCode::Ok);
    }

    let mut results = Vec::new();
    let mut any_failed = false;
    for (i, id) in ids.iter().enumerate() {
        ctx.progress(i + 1, ids.len(), id);
        match backup::verify_backup(&ctx.git, &ctx.layout, &ctx.db, id).await {
            Ok(r) => {
                if !r.ok {
                    any_failed = true;
                }
                if !ctx.json {
                    println!(
                        "\n  {} {} — {}",
                        if r.ok { "PASS" } else { "FAIL" },
                        r.backup_id,
                        r.detail
                    );
                }
                results.push(r);
            }
            Err(e) => {
                any_failed = true;
                results.push(backup::VerifyResultDto {
                    backup_id: id.clone(),
                    ok: false,
                    checked_files: 0,
                    detail: e.to_string(),
                });
            }
        }
    }
    ctx.progress_done();

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        println!(
            "\n{} verified, {} failed.",
            results.iter().filter(|r| r.ok).count(),
            results.iter().filter(|r| !r.ok).count()
        );
    }
    Ok(if any_failed {
        ExitCode::VerificationFailed
    } else {
        ExitCode::Ok
    })
}

async fn cmd_history(ctx: &Ctx, limit: i64) -> CmdResult {
    let records = backup::list_records(&ctx.db, limit).await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
        return Ok(ExitCode::Ok);
    }
    if records.is_empty() {
        println!("No backups yet.");
        return Ok(ExitCode::Ok);
    }
    println!(
        "{:<38} {:<34} {:<10} {:<10} {:<10} {:<21}",
        "BACKUP ID", "REPOSITORY", "MODE", "SIZE", "VERIFIED", "CREATED"
    );
    for r in &records {
        println!(
            "{:<38} {:<34} {:<10} {:<10} {:<10} {:<21}",
            truncate(&r.id, 38),
            truncate(&r.repo_full_name, 34),
            r.mode,
            bytes_human(r.size_bytes.max(0) as u64),
            if r.verified { "yes" } else { "no" },
            truncate(&r.created_at, 21),
        );
    }
    Ok(ExitCode::Ok)
}

// ---------------------------------------------------------------- storage

async fn cmd_storage(ctx: &Ctx, cmd: StorageCmd) -> CmdResult {
    match cmd {
        StorageCmd::Info => {
            let info = backup::storage_info(&ctx.db, &ctx.layout).await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&info)?);
                return Ok(ExitCode::Ok);
            }
            println!("Root:      {}", info.root);
            println!(
                "Used:      {} ({} available)",
                bytes_human(info.total_bytes),
                bytes_human(info.available_bytes)
            );
            println!(
                "Counts:    {} backups, {} repositories, {} snapshots",
                info.backup_count, info.repo_count, info.snapshot_count
            );
            if !info.per_repo.is_empty() {
                println!("\nPer repository:");
                for r in info.per_repo.iter().take(30) {
                    println!(
                        "  {:<50} {:>10} ({} backups)",
                        truncate(&r.full_name, 50),
                        bytes_human(r.bytes),
                        r.backups
                    );
                }
            }
            Ok(ExitCode::Ok)
        }
        StorageCmd::SetRoot { path } => {
            let mut s = ctx.settings.clone();
            s.backup_root = path.to_string_lossy().to_string();
            s.save(&ctx.db).await?;
            StorageLayout::resolve(path.clone()).ensure()?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else {
                println!("Backup root set to {}", path.display());
            }
            Ok(ExitCode::Ok)
        }
        StorageCmd::Delete { backup_id, force } => {
            if !ctx.confirm(&format!("Permanently delete backup {backup_id}?")) {
                println!("Cancelled.");
                return Ok(ExitCode::Cancelled);
            }
            backup::delete_backup(&ctx.db, &ctx.layout, &backup_id, force).await?;
            if ctx.json {
                println!("{}", serde_json::json!({ "deleted": backup_id }));
            } else {
                println!("Deleted {backup_id}.");
            }
            Ok(ExitCode::Ok)
        }
        StorageCmd::Export { dir } => {
            std::fs::create_dir_all(&dir).map_err(|e| GbrError::io_at(dir.clone(), e))?;
            let records = backup::list_records(&ctx.db, 5000).await?;
            let mut n = 0;
            for r in &records {
                if let Ok(m) = backup::load_manifest(&ctx.db, &r.id).await {
                    let target = dir.join(format!("{}.json", r.id));
                    manifest::write_json_atomic(&target, &m)?;
                    n += 1;
                }
            }
            if ctx.json {
                println!("{}", serde_json::json!({ "exported": n, "dir": dir }));
            } else {
                println!("Exported {n} manifest(s) to {}", dir.display());
            }
            Ok(ExitCode::Ok)
        }
    }
}

// ---------------------------------------------------------------- publish

struct PublishArgs {
    path: PathBuf,
    name: Option<String>,
    description: Option<String>,
    public: bool,
    message: String,
    readme: bool,
    force: bool,
    remote: Option<String>,
    allow_secrets: bool,
}

async fn cmd_publish(ctx: &Ctx, a: PublishArgs) -> CmdResult {
    let preview = publish::preview(&a.path, &ctx.git).await?;

    if !ctx.json {
        println!("Project:  {}", preview.path);
        println!(
            "Git:      {}",
            if preview.is_git_repo {
                "repository detected"
            } else {
                "not a git repo (will init)"
            }
        );
        println!(
            "Files:    {} ({}), branch {}",
            preview.file_count,
            bytes_human(preview.total_bytes),
            preview.current_branch.as_deref().unwrap_or("-")
        );
        for w in &preview.warnings {
            println!("warning:  {w}");
        }
        if !preview.secret_hits.is_empty() {
            println!("\nFiles that look like they contain secrets:");
            for h in &preview.secret_hits {
                println!("  - {} ({})", h.path, h.kind);
            }
        }
        println!();
    }

    if !preview.secret_hits.is_empty()
        && !a.allow_secrets
        && !ctx.yes
        && !ctx.confirm("Publish these secret-looking files anyway?")
    {
        println!("Cancelled.");
        return Ok(ExitCode::Cancelled);
    }
    if a.force && !ctx.confirm("Force push may overwrite remote history. Continue?") {
        println!("Cancelled.");
        return Ok(ExitCode::Cancelled);
    }

    let gh = GitHubClient::connect().await?;
    let handle = TaskHandle::detached(uuid_str(), "publish");
    let req = PublishRequest {
        path: a.path.to_string_lossy().to_string(),
        repo_name: a.name.clone().unwrap_or_default(),
        description: a.description.clone(),
        private: !a.public,
        auto_init_readme: a.readme,
        commit_message: a.message.clone(),
        allow_force: a.force,
        existing_remote_url: a.remote.clone(),
        org: None,
    };

    let outcome = publish::run(&ctx.db, &ctx.git, &handle, &gh, &req).await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!("Published {}", outcome.repo_full_name);
        println!("  url:      {}", outcome.html_url);
        println!("  commits:  {}", outcome.committed_files);
        println!("  refs:     {}", outcome.pushed_refs);
    }
    Ok(ExitCode::Ok)
}

// ---------------------------------------------------------------- github

async fn cmd_github(ctx: &Ctx, cmd: GitHubCmd) -> CmdResult {
    let gh = GitHubClient::connect().await?;
    match cmd {
        GitHubCmd::CreateRepo {
            name,
            description,
            public,
            org,
        } => {
            let created = gh
                .create_repo(&name, description.as_deref(), public, false, org.as_deref())
                .await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&created)?);
            } else {
                println!(
                    "Created {} {}",
                    created
                        .get("full_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?"),
                    created
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                );
            }
            Ok(ExitCode::Ok)
        }
        GitHubCmd::RateLimit => {
            let r = gh.rate_limit().await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!("{}/{} requests remaining", r.remaining, r.limit);
                if r.reset_unix > 0 {
                    println!(
                        "resets {}",
                        chrono::DateTime::from_timestamp(r.reset_unix, 0)
                            .map(|d| d.to_rfc3339())
                            .unwrap_or_default()
                    );
                }
            }
            Ok(ExitCode::Ok)
        }
    }
}

// ---------------------------------------------------------------- tasks / logs / config

async fn cmd_tasks(ctx: &Ctx, cmd: TasksCmd) -> CmdResult {
    match cmd {
        TasksCmd::List { limit } => {
            let rows = gbr_core::tasks::history(&ctx.db, limit).await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if rows.is_empty() {
                println!("No tasks recorded yet.");
            } else {
                for t in rows {
                    println!(
                        "{:<38} {:<12} {:<10} {}",
                        truncate(&t.id, 38),
                        t.kind,
                        t.status,
                        t.title
                    );
                }
            }
            Ok(ExitCode::Ok)
        }
    }
}

async fn cmd_logs(
    ctx: &Ctx,
    level: Option<String>,
    search: Option<String>,
    limit: i64,
    export: Option<PathBuf>,
) -> CmdResult {
    if let Some(path) = export {
        let text = activity::export_text(&ctx.db, 5000).await?;
        std::fs::write(&path, text).map_err(|e| GbrError::io_at(path.clone(), e))?;
        println!("Exported redacted logs to {}", path.display());
        return Ok(ExitCode::Ok);
    }
    let entries = activity::list(&ctx.db, level.as_deref(), search.as_deref(), limit).await?;
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(ExitCode::Ok);
    }
    if entries.is_empty() {
        println!("No log entries.");
        return Ok(ExitCode::Ok);
    }
    for e in entries {
        println!("[{}] {:<6} {:<10} {}", e.ts, e.level, e.source, e.message);
    }
    Ok(ExitCode::Ok)
}

async fn cmd_config(ctx: &Ctx, cmd: ConfigCmd) -> CmdResult {
    match cmd {
        ConfigCmd::Show => {
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&ctx.settings)?);
            } else {
                let root = if ctx.settings.backup_root.trim().is_empty() {
                    format!("{} (default)", ctx.backup_root().display())
                } else {
                    ctx.settings.backup_root.clone()
                };
                println!("backup_root         = {root}");
                println!("default_mode        = {}", ctx.settings.default_mode);
                println!("default_format      = {}", ctx.settings.default_format);
                println!("compression_level   = {}", ctx.settings.compression_level);
                println!("snapshot_retention  = {}", ctx.settings.snapshot_retention);
                println!("git_path            = {}", ctx.settings.git_path);
                println!("github_client_id    = {}", ctx.settings.github_client_id);
                println!("log_level           = {}", ctx.settings.log_level);
                println!("notifications       = {}", ctx.settings.notifications);
                println!("encrypt_backups     = {}", ctx.settings.encrypt_backups);
            }
            Ok(ExitCode::Ok)
        }
        ConfigCmd::Set { key, value } => {
            let mut s: Settings = ctx.settings.clone();
            match key.as_str() {
                "backup_root" => s.backup_root = value,
                "default_mode" => s.default_mode = value,
                "default_format" => s.default_format = value,
                "compression_level" => {
                    s.compression_level = value.parse().map_err(|_| {
                        GbrError::InvalidInput("compression_level must be 1-22".into())
                    })?
                }
                "snapshot_retention" => {
                    s.snapshot_retention = value.parse().map_err(|_| {
                        GbrError::InvalidInput("snapshot_retention must be a number".into())
                    })?
                }
                "git_path" => s.git_path = value,
                "github_client_id" => s.github_client_id = value,
                "log_level" => s.log_level = value,
                "notifications" => s.notifications = value == "true" || value == "1",
                "encrypt_backups" => s.encrypt_backups = value == "true" || value == "1",
                other => return Err(GbrError::InvalidInput(format!("unknown setting: {other}"))),
            }
            s.save(&ctx.db).await?;
            println!("{key} updated.");
            Ok(ExitCode::Ok)
        }
        ConfigCmd::Completion { shell } => {
            use clap::CommandFactory;
            let mut cmd = <Cli as CommandFactory>::command();
            let bin = cmd.get_name().to_string();
            let mut out = std::io::stdout();
            match shell.as_str() {
                "bash" => {
                    clap_complete::generate(clap_complete::shells::Bash, &mut cmd, bin, &mut out)
                }
                "zsh" => {
                    clap_complete::generate(clap_complete::shells::Zsh, &mut cmd, bin, &mut out)
                }
                "fish" => {
                    clap_complete::generate(clap_complete::shells::Fish, &mut cmd, bin, &mut out)
                }
                "powershell" => clap_complete::generate(
                    clap_complete::shells::PowerShell,
                    &mut cmd,
                    bin,
                    &mut out,
                ),
                other => {
                    return Err(GbrError::InvalidInput(format!(
                        "unsupported shell: {other} (bash|zsh|fish|powershell)"
                    )))
                }
            }
            Ok(ExitCode::Ok)
        }
    }
}

async fn cmd_doctor(ctx: &Ctx) -> CmdResult {
    let mut problems = 0;
    let check = |ok: bool, label: &str, detail: &str, problems: &mut i32| {
        let mark = if ok { "ok  " } else { "FAIL" };
        if !ok {
            *problems += 1;
        }
        println!(" [{mark}] {label}: {detail}");
    };

    match ctx.git.version().await {
        Ok(v) => check(true, "git", &v, &mut problems),
        Err(e) => check(false, "git", &e.to_string(), &mut problems),
    }
    let lfs = ctx.git.lfs_available().await;
    check(
        lfs,
        "git-lfs",
        if lfs {
            "available"
        } else {
            "not installed (LFS objects will not be captured)"
        },
        &mut problems,
    );

    let db_ok = sqlx_ping(&ctx.db).await;
    check(db_ok, "database", &ctx.db_path_display(), &mut problems);

    let root = ctx.backup_root();
    let writable = std::fs::create_dir_all(&root).is_ok();
    check(
        writable,
        "storage",
        &format!(
            "{} ({})",
            root.display(),
            if writable { "writable" } else { "not writable" }
        ),
        &mut problems,
    );

    let has_token = auth::load_token()?.is_some();
    check(
        has_token,
        "credentials",
        if has_token {
            "stored in OS credential manager"
        } else {
            "not connected — run `gitbackup login`"
        },
        &mut problems,
    );

    if has_token {
        match GitHubClient::connect().await {
            Ok(gh) => match gh.rate_limit().await {
                Ok(r) => check(
                    true,
                    "github-api",
                    &format!("{}/{} requests remaining", r.remaining, r.limit),
                    &mut problems,
                ),
                Err(e) => check(false, "github-api", &e.to_string(), &mut problems),
            },
            Err(e) => check(false, "github-api", &e.to_string(), &mut problems),
        }
    }

    let storage = backup::storage_info(&ctx.db, &ctx.layout).await.unwrap();
    println!(
        "\nStorage root: {} ({} used, {} available)",
        storage.root,
        bytes_human(storage.total_bytes),
        bytes_human(storage.available_bytes)
    );
    println!("App version:  {}", gbr_core::VERSION);

    if problems == 0 {
        println!("\nAll checks passed.");
        Ok(ExitCode::Ok)
    } else {
        println!("\n{problems} check(s) failed.");
        Ok(ExitCode::Error)
    }
}

async fn sqlx_ping(db: &gbr_core::db::Db) -> bool {
    // Round-trip a query through the pool; a corrupted or locked database
    // fails here rather than at backup time.
    Settings::load(db).await.is_ok()
}

// ---------------------------------------------------------------- helpers

impl Ctx {
    fn db_path_display(&self) -> String {
        gbr_core::paths::AppPaths::default_root()
            .map(|p| p.db_path().display().to_string())
            .unwrap_or_else(|_| "unknown".into())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// Small local helper so the CLI does not need the `uuid` crate directly.
mod uuid_shim {
    pub fn new_v4_string() -> String {
        let mut buf = [0u8; 16];
        // Use the OS randomness source; good enough for task ids.
        getrandom_fill(&mut buf);
        buf[6] = (buf[6] & 0x0f) | 0x40;
        buf[8] = (buf[8] & 0x3f) | 0x80;
        let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }

    fn getrandom_fill(buf: &mut [u8]) {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open(if cfg!(windows) {
            r"\\.\NUL"
        } else {
            "/dev/urandom"
        }) {
            let _ = f.read_exact(buf);
        }
        // Fallback: time-based mixing (ids only, never for security).
        if buf.iter().all(|&b| b == 0) {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ((t >> (i % 16 * 8)) & 0xff) as u8;
            }
        }
    }
}

use self::uuid_shim as uuid;

fn uuid_str() -> String {
    uuid::new_v4_string()
}

// ---------------------------------------------------------------- update

async fn cmd_update(ctx: &Ctx, check_only: bool) -> CmdResult {
    println!("Checking for updates on GitHub...");
    let info = gbr_core::update::check_for_updates().await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&info).unwrap_or_default());
        return Ok(ExitCode::Ok);
    }

    println!("Current version: v{}", info.current_version);
    println!("Latest release:  v{} ({})", info.latest_version, info.release_name);

    if !info.update_available {
        println!("  ✓ You are running the latest version.");
        return Ok(ExitCode::Ok);
    }

    println!("\n★ New update available: v{} -> v{}", info.current_version, info.latest_version);
    if !info.release_notes.is_empty() {
        let snippet = info.release_notes.lines().take(12).collect::<Vec<_>>().join("\n");
        println!("\nRelease notes:\n{snippet}\n");
    }

    let download_url = match &info.download_url {
        Some(u) => u,
        None => {
            println!("No direct binary download found for this platform.");
            println!("Visit the release page to download: {}", info.release_url);
            return Ok(ExitCode::Ok);
        }
    };

    let asset_name = info.asset_name.as_deref().unwrap_or("update package");
    println!(
        "Asset: {} ({})",
        asset_name,
        gbr_core::bytes_human(info.asset_size.unwrap_or(0))
    );

    if check_only {
        println!("Run `gitbackup update` to download and install this update.");
        return Ok(ExitCode::Ok);
    }

    if !ctx.yes {
        print!("\nDownload and install update now? [y/N]: ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let mut buf = String::new();
        let _ = std::io::stdin().read_line(&mut buf);
        if !buf.trim().eq_ignore_ascii_case("y") {
            println!("Update cancelled.");
            return Ok(ExitCode::Cancelled);
        }
    }

    let dest_dir = gbr_core::update::download_dir();
    let target_file = dest_dir.join(asset_name);
    println!("Downloading {} to {}...", asset_name, target_file.display());
    gbr_core::update::download_asset(download_url, &target_file).await?;
    println!("Download complete.");

    println!("Launching installer...");
    gbr_core::update::launch_installer(&target_file)?;
    println!("Update launched! Please follow the installer prompts.");
    Ok(ExitCode::Ok)
}
