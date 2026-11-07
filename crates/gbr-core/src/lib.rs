//! # gbr-core
//!
//! Shared engine for **Git Backup Restore**. Both the Tauri desktop app and
//! the `gitbackup` CLI drive this crate — no backup, restore, authentication
//! or repository logic is duplicated in either front end.
//!
//! ## Module map
//!
//! | module | responsibility |
//! |---|---|
//! | [`auth`] | GitHub login (PAT + device flow), OS credential manager |
//! | [`github`] | GitHub REST client (pagination, rate limits, repo creation) |
//! | [`repo_store`] | local repository cache + search/filter facets |
//! | [`git`] | safe `git` subprocess wrapper (no shell interpolation) |
//! | [`backup`] | full mirror backup, snapshots, verification, retention |
//! | [`selective`] | file selection, sensitive-file scan, zip/tar/tar.zst |
//! | [`restore`] | validated restore with conflict + zip-slip protection |
//! | [`publish`] | local folder → GitHub repository |
//! | [`manifest`] | on-disk layout, atomic manifests, SHA-256 helpers |
//! | [`tasks`] | cancellable background job manager with progress |
//! | [`db`] | SQLite (SQLx) connection + migrations |
//! | [`settings`] | persisted application settings |
//! | [`activity`] | redacted activity log store |
//! | [`dashboard`] | aggregate statistics for the dashboard page |

pub mod activity;
pub mod auth;
pub mod backup;
pub mod dashboard;
pub mod db;
pub mod error;
pub mod git;
pub mod github;
pub mod manifest;
pub mod paths;
pub mod publish;
pub mod repo_store;
pub mod restore;
pub mod secretscan;
pub mod selective;
pub mod settings;
pub mod tasks;

pub use error::{GbrError, Result};
pub use manifest::bytes_human;

/// Application version baked in at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Honest, user-facing description of what a git backup does *not* cover.
/// Shown in the About page and in every manifest.
pub const LIMITATIONS: &[&str] = &[
    "Git backups capture repository data (commits, branches, tags, and LFS objects when git-lfs is available).",
    "GitHub platform data outside git is NOT captured: Issues, Pull Requests, Discussions, Projects, Wikis, Actions secrets and variables, Environments, Deploy keys, webhooks, branch protection rules, repository settings, and GitHub Release assets.",
    "Hidden GitHub-only references such as refs/pull/* may be absent from a mirror clone.",
    "A git backup is not a complete GitHub account backup.",
];

#[cfg(test)]
mod integration_tests {
    use super::*;
    use tempfile::tempdir;

    /// End-to-end deterministic test: build a local "remote" repo with real
    /// commits, branches and tags, mirror-backup it, verify, mutate the
    /// source, update the mirror, then restore and check history survived.
    #[tokio::test]
    async fn full_backup_preserves_branches_tags_and_history() {
        let tmp = tempdir().unwrap();
        let remote = tmp.path().join("origin.git");
        let work = tmp.path().join("work");

        let git = git::GitCli::new("git");

        // Build the source repository.
        std::fs::create_dir_all(&work).unwrap();
        git.init(&work, false).await.unwrap();
        // Committer identity must not depend on the machine's git config.
        for (k, v) in [
            ("user.email", "gbr-test@example.com"),
            ("user.name", "GBR Test"),
        ] {
            git.checked(&["config".into(), k.into(), v.into()], Some(&work), None)
                .await
                .unwrap();
        }
        git.checked(
            &[
                "symbolic-ref".into(),
                "HEAD".into(),
                "refs/heads/main".into(),
            ],
            Some(&work),
            None,
        )
        .await
        .unwrap();
        std::fs::write(work.join("file.txt"), "v1\n").unwrap();
        git.checked(&["add".into(), "-A".into()], Some(&work), None)
            .await
            .unwrap();
        git.checked(
            &["commit".into(), "-m".into(), "first".into()],
            Some(&work),
            None,
        )
        .await
        .unwrap();
        git.checked(&["tag".into(), "v1.0".into()], Some(&work), None)
            .await
            .unwrap();
        git.checked(
            &["checkout".into(), "-b".into(), "feature".into()],
            Some(&work),
            None,
        )
        .await
        .unwrap();
        std::fs::write(work.join("file.txt"), "v2\n").unwrap();
        git.checked(&["add".into(), "-A".into()], Some(&work), None)
            .await
            .unwrap();
        git.checked(
            &["commit".into(), "-m".into(), "second".into()],
            Some(&work),
            None,
        )
        .await
        .unwrap();

        // Publish it as a bare "remote" so clone --mirror has a real source.
        git.clone_mirror(&work.to_string_lossy(), &remote, None)
            .await
            .expect("mirror clone");

        // --- backup ---
        let storage = tmp.path().join("storage");
        let layout = manifest::StorageLayout::resolve(storage.clone());
        layout.ensure().unwrap();

        let db = db::Db::open(&tmp.path().join("test.sqlite3"))
            .await
            .unwrap();
        let engine = backup::BackupEngine::new(db.clone(), git.clone(), layout.clone());

        let (_mgr, _rx) = tasks::TaskManager::new(db.clone());
        let handle = tasks::TaskHandle::detached("test", "test");

        let repo = github::Repo {
            github_id: 1,
            node_id: None,
            owner: "local".into(),
            name: "demo".into(),
            full_name: "local/demo".into(),
            description: None,
            is_private: false,
            is_fork: false,
            is_archived: false,
            language: None,
            default_branch: Some("main".into()),
            size_kb: None,
            html_url: None,
            clone_url: Some(remote.to_string_lossy().to_string()),
            pushed_at: None,
            updated_at: None,
            last_backup_status: None,
            last_backup_at: None,
        };

        let opts = backup::FullBackupOptions {
            include_lfs: false,
            retention: 5,
            snapshot: true,
        };

        let out = engine
            .run_full(&handle, &repo, "", &opts, None)
            .await
            .expect("backup should succeed");
        assert!(out.verified, "mirror must verify: {out:?}");
        assert!(
            out.refs >= 3,
            "expected main + feature + tag, got {}",
            out.refs
        );
        assert!(!out.updated_existing, "first run must be a fresh clone");

        // Second run must be an incremental update, not a conflict.
        let out2 = engine
            .run_full(&handle, &repo, "", &opts, None)
            .await
            .unwrap();
        assert!(out2.updated_existing, "second run must update in place");

        // Restore and prove branches/tags/history survived.
        let dest = tmp.path().join("restored");
        let request = restore::RestoreRequest {
            backup_id: out.backup_id.clone(),
            destination: dest.to_string_lossy().to_string(),
            conflict_mode: restore::ConflictMode::NewFolder,
            as_worktree: true,
        };
        let plan = restore::plan(&db, &layout, &request).await.unwrap();
        assert!(plan.can_proceed, "plan blockers: {:?}", plan.blockers);

        let outcome = restore::run(&engine, &handle, &request, false)
            .await
            .unwrap();
        assert!(outcome.verified);

        let restored_work = dest.with_extension("");
        let log = git
            .checked(
                &["log".into(), "--oneline".into(), "--all".into()],
                Some(&restored_work),
                None,
            )
            .await
            .unwrap();
        assert!(
            log.contains("first") && log.contains("second"),
            "history lost: {log}"
        );

        let tags = git
            .checked(&["tag".into(), "--list".into()], Some(&restored_work), None)
            .await
            .unwrap();
        assert!(tags.contains("v1.0"), "tag lost: {tags}");

        let branches = git
            .checked(
                &["branch".into(), "--list".into(), "--all".into()],
                Some(&restored_work),
                None,
            )
            .await
            .unwrap();
        assert!(branches.contains("feature"), "branch lost: {branches}");

        // Verification through the public API must agree.
        let v = backup::verify_backup(&git, &layout, &db, &out.backup_id)
            .await
            .unwrap();
        assert!(v.ok, "verify failed: {}", v.detail);
    }

    #[tokio::test]
    async fn database_migrations_apply_to_fresh_and_existing_db() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("m.sqlite3");
        let d1 = db::Db::open(&path).await.expect("fresh migrate");
        // Re-opening an already-migrated database must be a no-op.
        d1.close().await;
        let d2 = db::Db::open(&path).await.expect("re-open migrate");
        use sqlx::Row;
        let row: i64 = sqlx::query("SELECT COUNT(*) as c FROM sqlite_master WHERE type='table'")
            .fetch_one(d2.pool())
            .await
            .unwrap()
            .try_get("c")
            .unwrap();
        assert!(row >= 12, "expected at least 12 tables, got {row}");
        d2.close().await;
    }

    #[test]
    fn path_traversal_is_rejected_everywhere() {
        assert!(paths::validate_relative_path("../../etc").is_err());
        assert!(paths::validate_relative_path("C:\\Windows\\System32").is_err());
        assert!(!manifest::sanitize_component("..").is_empty());
    }
}
