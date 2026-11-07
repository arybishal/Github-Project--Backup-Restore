use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::sync::{mpsc, watch, Mutex};

use crate::activity::{self, LogLevel};
use crate::db::Db;
use crate::error::{GbrError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub title: String,
    pub stage: String,
    pub current_repo: Option<String>,
    pub percent: Option<f32>,
    pub detail: Option<String>,
    pub transferred_bytes: Option<u64>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskDto {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub progress: i64,
    pub detail: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
}

/// Handle handed to every long-running operation: cooperative cancellation
/// plus honest progress reporting (percent is `None` when not measurable —
/// it is never fabricated).
#[derive(Clone)]
pub struct TaskHandle {
    pub id: String,
    pub title: String,
    cancel: watch::Sender<bool>,
    tx: mpsc::UnboundedSender<TaskProgress>,
    started: Instant,
}

impl TaskHandle {
    /// Handle for work that runs without a registry entry (unit tests, one-off
    /// CLI operations). Progress events go nowhere and cancellation is inert.
    pub fn detached(id: impl Into<String>, title: impl Into<String>) -> Self {
        let (cancel, _) = watch::channel(false);
        let (tx, _rx) = mpsc::unbounded_channel();
        Self {
            id: id.into(),
            title: title.into(),
            cancel,
            tx,
            started: Instant::now(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancel.borrow()
    }

    pub fn ensure_active(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(GbrError::Other("operation cancelled".into()))
        } else {
            Ok(())
        }
    }

    /// Report a stage change. `percent` must be a real measurement or `None`.
    pub fn report(&self, stage: &str, percent: Option<f32>, detail: Option<String>) {
        let _ = self.tx.send(TaskProgress {
            task_id: self.id.clone(),
            title: self.title.clone(),
            stage: stage.to_string(),
            current_repo: None,
            percent,
            detail,
            transferred_bytes: None,
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        });
    }

    pub fn report_repo(
        &self,
        stage: &str,
        repo: Option<String>,
        percent: Option<f32>,
        detail: Option<String>,
    ) {
        let _ = self.tx.send(TaskProgress {
            task_id: self.id.clone(),
            title: self.title.clone(),
            stage: stage.to_string(),
            current_repo: repo,
            percent,
            detail,
            transferred_bytes: None,
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        });
    }

    pub fn report_bytes(
        &self,
        stage: &str,
        repo: Option<String>,
        bytes: u64,
        detail: Option<String>,
    ) {
        let _ = self.tx.send(TaskProgress {
            task_id: self.id.clone(),
            title: self.title.clone(),
            stage: stage.to_string(),
            current_repo: repo,
            percent: None,
            detail,
            transferred_bytes: Some(bytes),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        });
    }
}

struct LiveTask {
    cancel: watch::Sender<bool>,
}

/// Registry of running tasks. Started work is spawned onto the Tokio runtime
/// and reports through an unbounded channel; history is persisted to SQLite.
pub struct TaskManager {
    live: Mutex<HashMap<String, LiveTask>>,
    progress_tx: mpsc::UnboundedSender<TaskProgress>,
    db: Db,
}

pub type ProgressReceiver = mpsc::UnboundedReceiver<TaskProgress>;

impl TaskManager {
    pub fn new(db: Db) -> (Arc<Self>, ProgressReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                live: Mutex::new(HashMap::new()),
                progress_tx: tx,
                db,
            }),
            rx,
        )
    }

    pub async fn cancel(&self, id: &str) -> Result<bool> {
        let live = self.live.lock().await;
        match live.get(id) {
            Some(t) => {
                let _ = t.cancel.send(true);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub async fn active_ids(&self) -> Vec<String> {
        self.live.lock().await.keys().cloned().collect()
    }

    /// Allocate a task id + handle, persist the `running` row, and register it.
    pub async fn begin(&self, kind: &str, title: &str) -> TaskHandle {
        let id = uuid::Uuid::new_v4().to_string();
        let (cancel, _) = watch::channel(false);
        let created_at = Utc::now().to_rfc3339();
        {
            let mut live = self.live.lock().await;
            live.insert(
                id.clone(),
                LiveTask {
                    cancel: cancel.clone(),
                },
            );
        }
        if let Err(e) = sqlx::query(
            "INSERT INTO task_history (id, kind, title, status, progress, created_at) VALUES (?1, ?2, ?3, 'running', 0, ?4)",
        )
        .bind(&id)
        .bind(kind)
        .bind(title)
        .bind(&created_at)
        .execute(self.db.pool())
        .await
        {
            tracing::warn!("could not record task start: {e}");
        }

        TaskHandle {
            id,
            title: title.to_string(),
            cancel,
            tx: self.progress_tx.clone(),
            started: Instant::now(),
        }
    }

    pub async fn finish(&self, handle: &TaskHandle, error: Option<String>) {
        let now = Utc::now().to_rfc3339();
        let (status, level, progress) = match &error {
            None => ("succeeded", LogLevel::Info, 100),
            Some(e) if e.contains("cancelled") => ("cancelled", LogLevel::Warn, 0),
            Some(_) => ("failed", LogLevel::Error, 0),
        };
        {
            let mut live = self.live.lock().await;
            live.remove(&handle.id);
        }
        if let Err(e) = sqlx::query(
            "UPDATE task_history SET status = ?2, progress = ?3, finished_at = ?4, error = ?5 WHERE id = ?1",
        )
        .bind(&handle.id)
        .bind(status)
        .bind(progress)
        .bind(&now)
        .bind(&error)
        .execute(self.db.pool())
        .await
        {
            tracing::warn!("could not record task end: {e}");
        }

        let msg = match (&handle.title, &error) {
            (t, None) => format!("{t}: completed"),
            (t, Some(e)) => format!("{t}: {status} — {e}"),
        };
        activity::log(&self.db, level, "task", &msg, None).await;
    }

    pub async fn history(&self, limit: i64) -> Result<Vec<TaskDto>> {
        history(&self.db, limit).await
    }

    pub async fn pending_count(&self) -> i64 {
        sqlx::query("SELECT COUNT(*) as c FROM task_history WHERE status IN ('running','queued')")
            .fetch_one(self.db.pool())
            .await
            .ok()
            .and_then(|r| r.try_get::<i64, _>("c").ok())
            .unwrap_or(0)
    }
}

/// Task history for callers that only hold a database handle (the CLI).
pub async fn history(db: &Db, limit: i64) -> Result<Vec<TaskDto>> {
    let rows = sqlx::query(
        "SELECT id, kind, title, status, progress, detail, created_at, finished_at, error
         FROM task_history ORDER BY created_at DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(db.pool())
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TaskDto {
            id: r.try_get("id").unwrap_or_default(),
            kind: r.try_get("kind").unwrap_or_default(),
            title: r.try_get("title").unwrap_or_default(),
            status: r.try_get("status").unwrap_or_default(),
            progress: r.try_get("progress").unwrap_or(0),
            detail: r.try_get("detail").unwrap_or(None),
            created_at: r.try_get("created_at").unwrap_or_default(),
            finished_at: r.try_get("finished_at").unwrap_or(None),
            error: r.try_get("error").unwrap_or(None),
        })
        .collect())
}
