use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::db::Db;
use crate::error::Result;
use crate::github::{GitHubClient, Repo};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoFilter {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>, // all | public | private
    #[serde(default)]
    pub archived: Option<String>, // all | archived | active
    #[serde(default)]
    pub sort: Option<String>, // name | updated | size
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoSyncStats {
    pub fetched: u32,
    pub pages: u32,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RepoCounts {
    pub total: i64,
    pub public: i64,
    pub private: i64,
    pub organization: i64,
    pub archived: i64,
}

/// Persist one page of repositories. Uses a transaction so a partial sync can
/// never leave the local cache half-updated.
pub async fn upsert_page(pool: &SqlitePool, repos: &[Repo]) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    for r in repos {
        sqlx::query(
            "INSERT INTO repositories
             (github_id, node_id, owner, name, full_name, description, is_private, is_fork,
              is_archived, language, default_branch, size_kb, html_url, clone_url, pushed_at,
              updated_at, raw_json, synced_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)
             ON CONFLICT(github_id) DO UPDATE SET
               owner=excluded.owner, name=excluded.name, full_name=excluded.full_name,
               description=excluded.description, is_private=excluded.is_private,
               is_fork=excluded.is_fork, is_archived=excluded.is_archived,
               language=excluded.language, default_branch=excluded.default_branch,
               size_kb=excluded.size_kb, html_url=excluded.html_url,
               clone_url=excluded.clone_url, pushed_at=excluded.pushed_at,
               updated_at=excluded.updated_at, synced_at=excluded.synced_at",
        )
        .bind(r.github_id)
        .bind(&r.node_id)
        .bind(&r.owner)
        .bind(&r.name)
        .bind(&r.full_name)
        .bind(&r.description)
        .bind(r.is_private)
        .bind(r.is_fork)
        .bind(r.is_archived)
        .bind(&r.language)
        .bind(&r.default_branch)
        .bind(r.size_kb)
        .bind(&r.html_url)
        .bind(&r.clone_url)
        .bind(&r.pushed_at)
        .bind(&r.updated_at)
        .bind(serde_json::to_string(r).unwrap_or_default())
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Pull every accessible repository from GitHub into the local cache.
/// Pages are fetched sequentially (respecting rate limits) and flushed in
/// batches so a 5 000-repo account never needs to fit in memory at once.
pub async fn sync_from_github(db: &Db, gh: &GitHubClient) -> Result<RepoSyncStats> {
    let mut fetched = 0u32;
    let mut pages = 0u32;
    const PER_PAGE: u32 = 100;
    const MAX_PAGES: u32 = 50;

    loop {
        pages += 1;
        let batch = gh.list_repos_page(pages, PER_PAGE).await?;
        let n = batch.len() as u32;
        if n == 0 {
            pages -= 1;
            break;
        }
        upsert_page(db.pool(), &batch).await?;
        fetched += n;
        if n < PER_PAGE || pages >= MAX_PAGES {
            break;
        }
    }

    let stats = RepoSyncStats {
        fetched,
        pages,
        synced_at: Utc::now().to_rfc3339(),
    };

    crate::activity::log(
        db,
        crate::activity::LogLevel::Info,
        "repos",
        &format!(
            "synced {} repositories across {} page(s)",
            stats.fetched, stats.pages
        ),
        None,
    )
    .await;
    Ok(stats)
}

// ---------------------------------------------------------------- queries

pub async fn list(db: &Db, f: &RepoFilter) -> Result<Vec<Repo>> {
    let mut sql = String::from(
        "SELECT github_id, node_id, owner, name, full_name, description, is_private, is_fork,
                is_archived, language, default_branch, size_kb, html_url, clone_url, pushed_at,
                updated_at, last_backup_status, last_backup_at
         FROM repositories WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(s) = f.search.as_ref().filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND (full_name LIKE ? OR description LIKE ? OR name LIKE ?)");
        let pat = format!("%{}%", s.trim());
        binds.push(pat.clone());
        binds.push(pat.clone());
        binds.push(pat);
    }
    if let Some(o) = f.owner.as_ref().filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND owner = ?");
        binds.push(o.trim().to_string());
    }
    if let Some(l) = f.language.as_ref().filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND language = ?");
        binds.push(l.trim().to_string());
    }
    match f.visibility.as_deref() {
        Some("public") => sql.push_str(" AND is_private = 0"),
        Some("private") => sql.push_str(" AND is_private = 1"),
        _ => {}
    }
    match f.archived.as_deref() {
        Some("archived") => sql.push_str(" AND is_archived = 1"),
        Some("active") => sql.push_str(" AND is_archived = 0"),
        _ => {}
    }

    match f.sort.as_deref() {
        Some("updated") => sql.push_str(" ORDER BY updated_at DESC, full_name ASC"),
        Some("size") => sql.push_str(" ORDER BY size_kb DESC, full_name ASC"),
        _ => sql.push_str(" ORDER BY full_name ASC"),
    }

    let limit = f.limit.unwrap_or(200).clamp(1, 2000);
    let offset = f.offset.unwrap_or(0).max(0);
    sql.push_str(" LIMIT ? OFFSET ?");
    binds.push(limit.to_string());
    binds.push(offset.to_string());

    let mut query = sqlx::query(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    let rows = query.fetch_all(db.pool()).await?;
    Ok(rows.into_iter().map(row_to_repo).collect())
}

fn row_to_repo(r: sqlx::sqlite::SqliteRow) -> Repo {
    Repo {
        github_id: r.try_get("github_id").unwrap_or(0),
        node_id: r.try_get("node_id").unwrap_or(None),
        owner: r.try_get("owner").unwrap_or_default(),
        name: r.try_get("name").unwrap_or_default(),
        full_name: r.try_get("full_name").unwrap_or_default(),
        description: r.try_get("description").unwrap_or(None),
        is_private: r.try_get::<bool, _>("is_private").unwrap_or(false),
        is_fork: r.try_get::<bool, _>("is_fork").unwrap_or(false),
        is_archived: r.try_get::<bool, _>("is_archived").unwrap_or(false),
        language: r.try_get("language").unwrap_or(None),
        default_branch: r.try_get("default_branch").unwrap_or(None),
        size_kb: r.try_get("size_kb").unwrap_or(None),
        html_url: r.try_get("html_url").unwrap_or(None),
        clone_url: r.try_get("clone_url").unwrap_or(None),
        pushed_at: r.try_get("pushed_at").unwrap_or(None),
        updated_at: r.try_get("updated_at").unwrap_or(None),
        last_backup_status: r.try_get("last_backup_status").unwrap_or(None),
        last_backup_at: r.try_get("last_backup_at").unwrap_or(None),
    }
}

pub async fn counts(db: &Db) -> Result<RepoCounts> {
    let row = sqlx::query(
        "SELECT COUNT(*) as total,
                COALESCE(SUM(CASE WHEN is_private = 0 THEN 1 ELSE 0 END), 0) as public,
                COALESCE(SUM(CASE WHEN is_private = 1 THEN 1 ELSE 0 END), 0) as private,
                COALESCE(SUM(CASE WHEN is_archived = 1 THEN 1 ELSE 0 END), 0) as archived
         FROM repositories",
    )
    .fetch_one(db.pool())
    .await?;

    // Repositories owned by an organization the account belongs to. Org
    // membership is refreshed during sync and cached for offline use.
    let orgs = org_owners_list(db).await;
    let organization = if orgs.is_empty() {
        0
    } else {
        let placeholders = orgs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT COUNT(*) as c FROM repositories WHERE owner IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for o in &orgs {
            q = q.bind(o);
        }
        q.fetch_one(db.pool())
            .await
            .map(|r| r.try_get::<i64, _>("c").unwrap_or(0))
            .unwrap_or(0)
    };

    Ok(RepoCounts {
        total: row.try_get("total").unwrap_or(0),
        public: row.try_get("public").unwrap_or(0),
        private: row.try_get("private").unwrap_or(0),
        organization,
        archived: row.try_get("archived").unwrap_or(0),
    })
}

/// Organizations the authenticated account belongs to, refreshed on demand and
/// cached in settings so counts work offline.
async fn org_owners_list(db: &Db) -> Vec<String> {
    if let Ok(Some(v)) = crate::settings::get_raw::<Vec<String>>(db, "org_owners").await {
        return v;
    }
    Vec::new()
}

pub async fn cache_orgs(db: &Db, gh: &GitHubClient) -> Result<Vec<String>> {
    let orgs = gh.list_orgs().await?;
    crate::settings::set_raw(db, "org_owners", &orgs).await?;
    Ok(orgs)
}

pub async fn get_by_full_name(db: &Db, full_name: &str) -> Result<Option<Repo>> {
    let row = sqlx::query(
        "SELECT github_id, node_id, owner, name, full_name, description, is_private, is_fork,
                is_archived, language, default_branch, size_kb, html_url, clone_url, pushed_at,
                updated_at, last_backup_status, last_backup_at
         FROM repositories WHERE full_name = ?1",
    )
    .bind(full_name)
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(row_to_repo))
}

/// Distinct owners / languages for the filter dropdowns.
pub async fn facets(db: &Db) -> Result<(Vec<String>, Vec<String>)> {
    let owners = sqlx::query("SELECT DISTINCT owner FROM repositories ORDER BY owner")
        .fetch_all(db.pool())
        .await?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("owner").ok())
        .collect();
    let langs = sqlx::query(
        "SELECT DISTINCT language FROM repositories WHERE language IS NOT NULL ORDER BY language",
    )
    .fetch_all(db.pool())
    .await?
    .into_iter()
    .filter_map(|r| r.try_get::<String, _>("language").ok())
    .collect();
    Ok((owners, langs))
}
