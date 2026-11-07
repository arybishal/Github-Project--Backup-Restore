use std::path::PathBuf;

use gbr_core::db::Db;
use gbr_core::git::GitCli;
use gbr_core::manifest::StorageLayout;
use gbr_core::settings::Settings;

/// Process exit codes (documented in CLI.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Ok = 0,
    Error = 1,
    Usage = 2,
    AuthRequired = 3,
    VerificationFailed = 4,
    Cancelled = 5,
}

/// Shared runtime context built once per invocation.
pub struct Ctx {
    pub json: bool,
    pub yes: bool,
    pub db: Db,
    pub settings: Settings,
    pub git: GitCli,
    pub layout: StorageLayout,
}

impl Ctx {
    pub async fn open(json: bool, yes: bool) -> anyhow::Result<Ctx> {
        let paths = gbr_core::paths::AppPaths::default_root()?;
        paths.ensure()?;
        let db = Db::open(&paths.db_path()).await?;
        let settings = Settings::load(&db).await?;
        let git = GitCli::new(settings.git_path.clone());
        let layout = StorageLayout::from_settings(&paths, &settings.backup_root);
        layout.ensure()?;
        Ok(Ctx {
            json,
            yes,
            db,
            settings,
            git,
            layout,
        })
    }

    pub fn backup_root(&self) -> PathBuf {
        self.layout.root().to_path_buf()
    }

    /// Yes/no confirmation. `--yes` and non-TTY sessions never block.
    pub fn confirm(&self, question: &str) -> bool {
        if self.yes {
            return true;
        }
        if !console::user_attended() {
            return false;
        }
        eprint!("{question} [y/N] ");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let answer = console::Term::stdout()
            .read_line()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        matches!(answer.as_str(), "y" | "yes")
    }

    /// Prompt for a line of input without echoing it (tokens, passwords).
    pub fn prompt_secret(&self, prompt: &str) -> Option<String> {
        if !console::user_attended() {
            return None;
        }
        eprint!("{prompt} ");
        let _ = std::io::Write::flush(&mut std::io::stderr());
        let line = console::Term::stdout().read_secure_line().ok()?;
        let line = line.trim().to_string();
        if line.is_empty() {
            None
        } else {
            Some(line)
        }
    }

    /// Human-readable progress line for a batch operation.
    pub fn progress(&self, index: usize, total: usize, label: &str) {
        if !self.json && console::user_attended() {
            eprint!("\r[{index}/{total}] {label}                          ");
        }
    }

    pub fn progress_done(&self) {
        if !self.json && console::user_attended() {
            eprintln!();
        }
    }
}
