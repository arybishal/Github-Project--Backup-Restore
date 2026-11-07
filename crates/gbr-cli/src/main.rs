//! `gitbackup` — terminal interface for Git Backup Restore.
//!
//! Shares **100%** of its backup/restore/auth logic with the desktop app via
//! `gbr-core`. The CLI never requires the GUI to be running.

mod cmds;
mod ctx;

use clap::{Parser, Subcommand};

use ctx::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "gitbackup",
    version,
    about = "Cross-platform GitHub backup, restore & publishing",
    long_about = "Git Backup Restore — back up, verify, restore and publish GitHub repositories.\n\
                  Developer: Bishal Aryal (https://bishalaryal.com.np)",
    propagate_version = true,
    disable_help_subcommand = false
)]
pub struct Cli {
    /// Machine-readable output (where supported)
    #[arg(long, global = true, help_heading = "Global")]
    pub json: bool,

    /// Never prompt; assume yes for confirmations (non-interactive mode)
    #[arg(short = 'y', long, global = true, help_heading = "Global")]
    pub yes: bool,

    /// Increase log verbosity (-v info, -vv debug)
    #[arg(short, long, action = clap::ArgAction::Count, global = true, help_heading = "Global")]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Connect a GitHub account (token or device flow)
    Login {
        /// Use a personal access token instead of the browser device flow
        #[arg(long, conflicts_with = "device")]
        token: Option<String>,
        /// Use the GitHub device authorization flow (needs a client id in config)
        #[arg(long)]
        device: bool,
    },
    /// Disconnect the stored GitHub credentials
    Logout,
    /// Show authentication, git and storage status
    Status,
    /// Repository discovery
    Repos {
        #[command(subcommand)]
        cmd: ReposCmd,
    },
    /// Back up repositories
    Backup {
        /// Repository full name (owner/name). Omit when using --all.
        repository: Option<String>,
        /// Back up every accessible repository
        #[arg(long)]
        all: bool,
        /// Backup mode: full (git mirror) or selective (files)
        #[arg(long, value_parser = ["full", "selective"], default_value = "full")]
        mode: String,
        /// Archive format for selective backups
        #[arg(long, value_parser = ["folder", "zip", "tar", "tar.zst"], default_value = "zip")]
        format: String,
        /// Destination storage root (defaults to the configured backup root)
        #[arg(long)]
        dest: Option<String>,
        /// Include glob patterns (selective mode), repeatable
        #[arg(long = "include", value_name = "GLOB")]
        include: Vec<String>,
        /// Exclude glob patterns (selective mode), repeatable
        #[arg(long = "exclude", value_name = "GLOB")]
        exclude: Vec<String>,
        /// Include hidden files (selective mode)
        #[arg(long)]
        hidden: bool,
        /// Include .gitignore-excluded files (selective mode)
        #[arg(long)]
        ignored: bool,
        /// Also capture git-lfs objects (full mode, default on)
        #[arg(long, default_value_t = true)]
        lfs: bool,
        /// Snapshots to keep per repository
        #[arg(long)]
        retention: Option<u32>,
    },
    /// Restore a backup
    Restore {
        /// Backup id (see `history`)
        backup_id: Option<String>,
        /// Restore the newest backup of this repository
        #[arg(long, value_name = "REPO")]
        latest: Option<String>,
        /// Destination folder
        #[arg(long)]
        dest: Option<String>,
        /// Conflict handling: new_folder | merge | replace
        #[arg(long, value_parser = ["new_folder", "merge", "replace"], default_value = "new_folder")]
        conflict: String,
        /// Restore a full backup as a checked-out working tree (default true)
        #[arg(long, default_value_t = true)]
        worktree: bool,
    },
    /// Verify a backup against its manifest
    Verify {
        /// Backup id, or --all
        backup_id: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Backup history
    History {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
    /// Storage statistics
    Storage {
        #[command(subcommand)]
        cmd: StorageCmd,
    },
    /// Publish a local project folder to GitHub
    Publish {
        /// Local project directory
        path: std::path::PathBuf,
        /// Repository name (defaults to the folder name)
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        /// Create a public repository (default: private)
        #[arg(long)]
        public: bool,
        /// Commit message for the initial commit
        #[arg(long, default_value = "Initial commit")]
        message: String,
        /// Write a README.md if the project has none
        #[arg(long)]
        readme: bool,
        /// Allow force-with-lease when the remote has diverged
        #[arg(long)]
        force: bool,
        /// Connect to an existing remote URL instead of creating a repository
        #[arg(long)]
        remote: Option<String>,
        /// Allow publishing files that look like they contain secrets
        #[arg(long)]
        allow_secrets: bool,
    },
    /// GitHub repository management
    GitHub {
        #[command(subcommand)]
        cmd: GitHubCmd,
    },
    /// Background task management
    Tasks {
        #[command(subcommand)]
        cmd: TasksCmd,
    },
    /// Activity logs
    Logs {
        #[arg(long, value_parser = ["debug", "info", "warn", "error"])]
        level: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: i64,
        /// Write the (redacted) log to a file
        #[arg(long)]
        export: Option<std::path::PathBuf>,
    },
    /// Configuration
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Diagnose the installation (git, storage, database, credentials)
    Doctor,
}

#[derive(Subcommand, Debug)]
pub enum ReposCmd {
    /// List repositories (paginated, filterable)
    List {
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        language: Option<String>,
        #[arg(long, value_parser = ["all", "public", "private"], default_value = "all")]
        visibility: String,
        #[arg(long, value_parser = ["all", "archived", "active"], default_value = "all")]
        archived: String,
        #[arg(long, value_parser = ["name", "updated", "size"], default_value = "name")]
        sort: String,
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long, default_value_t = 0)]
        offset: i64,
    },
    /// Search repositories by keyword
    Search { keyword: String },
    /// Refresh the local repository cache from GitHub
    Sync,
    /// Show the filter facets (owners, languages)
    Facets,
}

#[derive(Subcommand, Debug)]
pub enum StorageCmd {
    /// Show storage usage, backup counts and per-repository sizes
    Info,
    /// Choose the backup root directory
    SetRoot { path: std::path::PathBuf },
    /// Delete a backup (requires --yes unless interactive)
    Delete {
        backup_id: String,
        /// Required when this is the repository's only backup
        #[arg(long)]
        force: bool,
    },
    /// Export all manifests to a directory
    Export {
        /// Target directory for the exported .json manifests
        dir: std::path::PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub enum GitHubCmd {
    /// Create a new GitHub repository
    CreateRepo {
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        public: bool,
        /// Organization to create it under
        #[arg(long)]
        org: Option<String>,
    },
    /// Show API rate-limit status
    RateLimit,
}

#[derive(Subcommand, Debug)]
pub enum TasksCmd {
    /// List task history
    List {
        #[arg(long, default_value_t = 20)]
        limit: i64,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print the effective configuration
    Show,
    /// Set a configuration value (`gitbackup config set backup_root /path`)
    Set { key: String, value: String },
    /// Print shell completion script for bash|zsh|fish|powershell
    Completion { shell: String },
}

fn main() {
    let cli = Cli::parse();
    let code = run(cli);
    std::process::exit(code as i32);
}

fn run(cli: Cli) -> ExitCode {
    // `-v` / `-vv` surface engine logs on stderr; nothing is ever logged at
    // warn or above unless something actually went wrong.
    let level = match cli.verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: could not start async runtime: {e}");
            return ExitCode::Error;
        }
    };
    rt.block_on(async move { cmds::dispatch(cli).await })
}
