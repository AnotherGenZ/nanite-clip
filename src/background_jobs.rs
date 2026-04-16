use std::collections::{BTreeMap, HashMap, VecDeque};
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use chrono::{DateTime, Utc};
use tokio::sync::{Semaphore, mpsc};
use tracing::{info, warn};

use crate::db::ClipAudioTrackDraft;
use crate::post_process::PostProcessPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackgroundJobId(pub u64);

impl std::fmt::Display for BackgroundJobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "job-{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackgroundJobKind {
    StorageTiering,
    Upload,
    Montage,
    DiscordWebhook,
    PostProcess,
}

impl BackgroundJobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StorageTiering => "storage_tiering",
            Self::Upload => "upload",
            Self::Montage => "montage",
            Self::DiscordWebhook => "discord_webhook",
            Self::PostProcess => "post_process",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "upload" => Self::Upload,
            "montage" => Self::Montage,
            "discord_webhook" => Self::DiscordWebhook,
            "post_process" => Self::PostProcess,
            _ => Self::StorageTiering,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::StorageTiering => "Storage Tiering",
            Self::Upload => "Upload",
            Self::Montage => "Montage",
            Self::DiscordWebhook => "Discord Webhook",
            Self::PostProcess => "Post Process",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundJobState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl BackgroundJobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "cancelled" => Self::Cancelled,
            _ => Self::Failed,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobProgress {
    pub current_step: u32,
    pub total_steps: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BackgroundJobRecord {
    pub id: BackgroundJobId,
    pub kind: BackgroundJobKind,
    pub label: String,
    pub state: BackgroundJobState,
    pub related_clip_ids: Vec<i64>,
    pub progress: Option<BackgroundJobProgress>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub detail: Option<String>,
    pub cancellable: bool,
}

#[derive(Debug, Clone)]
pub enum BackgroundJobSuccess {
    StorageTiering {
        moved_clip_ids: Vec<i64>,
        message: String,
    },
    Upload {
        clip_id: i64,
        provider_label: String,
        clip_url: Option<String>,
        message: String,
    },
    Montage {
        output_path: String,
        source_clip_ids: Vec<i64>,
        message: String,
    },
    DiscordWebhook {
        clip_id: i64,
        message: String,
    },
    PostProcess {
        clip_id: i64,
        final_path: String,
        plan: PostProcessPlan,
        tracks: Vec<ClipAudioTrackDraft>,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum BackgroundJobNotification {
    Updated(BackgroundJobRecord),
    Finished {
        record: BackgroundJobRecord,
        success: Option<BackgroundJobSuccess>,
        error: Option<String>,
    },
}

#[derive(Debug)]
pub struct BackgroundJobContext {
    id: BackgroundJobId,
    tx: mpsc::UnboundedSender<ManagerEvent>,
    cancelled: Arc<AtomicBool>,
}

impl BackgroundJobContext {
    pub fn id(&self) -> BackgroundJobId {
        self.id
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn check_cancelled(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Job cancelled.".into())
        } else {
            Ok(())
        }
    }

    pub fn progress(
        &self,
        current_step: u32,
        total_steps: u32,
        message: impl Into<String>,
    ) -> Result<(), String> {
        self.check_cancelled()?;
        let _ = self.tx.send(ManagerEvent::Progress {
            id: self.id,
            progress: BackgroundJobProgress {
                current_step,
                total_steps: total_steps.max(1),
                message: message.into(),
            },
        });
        Ok(())
    }
}

pub struct BackgroundJobManager {
    next_id: u64,
    tx: mpsc::UnboundedSender<ManagerEvent>,
    rx: mpsc::UnboundedReceiver<ManagerEvent>,
    jobs: BTreeMap<BackgroundJobId, JobEntry>,
    history: VecDeque<BackgroundJobRecord>,
    kind_limits: HashMap<BackgroundJobKind, std::sync::Arc<Semaphore>>,
}

struct JobEntry {
    record: BackgroundJobRecord,
    cancelled: Arc<AtomicBool>,
}

enum ManagerEvent {
    Started {
        id: BackgroundJobId,
    },
    Progress {
        id: BackgroundJobId,
        progress: BackgroundJobProgress,
    },
    Finished {
        id: BackgroundJobId,
        success: BackgroundJobSuccess,
    },
    Failed {
        id: BackgroundJobId,
        error: String,
    },
    Cancelled {
        id: BackgroundJobId,
    },
}

impl BackgroundJobManager {
    pub const HISTORY_LIMIT: usize = 24;

    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            next_id: 1,
            tx,
            rx,
            jobs: BTreeMap::new(),
            history: VecDeque::new(),
            kind_limits: default_kind_limits(),
        }
    }

    pub fn active_jobs(&self) -> Vec<BackgroundJobRecord> {
        self.jobs
            .values()
            .map(|entry| entry.record.clone())
            .collect()
    }

    pub fn recent_jobs(&self) -> Vec<BackgroundJobRecord> {
        self.history.iter().cloned().collect()
    }

    pub fn record(&self, id: BackgroundJobId) -> Option<BackgroundJobRecord> {
        self.jobs
            .get(&id)
            .map(|entry| entry.record.clone())
            .or_else(|| self.history.iter().find(|record| record.id == id).cloned())
    }

    pub fn is_active(&self, id: BackgroundJobId) -> bool {
        self.jobs.contains_key(&id)
    }

    pub fn replace_recent_jobs(&mut self, jobs: Vec<BackgroundJobRecord>) {
        self.history.clear();
        let mut max_id = 0;
        for record in jobs.into_iter().take(Self::HISTORY_LIMIT) {
            max_id = max_id.max(record.id.0);
            self.history.push_back(record);
        }
        self.next_id = self.next_id.max(max_id + 1);
    }

    pub fn remove_history(&mut self, id: BackgroundJobId) -> Option<BackgroundJobRecord> {
        let index = self.history.iter().position(|record| record.id == id)?;
        self.history.remove(index)
    }

    pub fn start<F, Fut>(
        &mut self,
        kind: BackgroundJobKind,
        label: impl Into<String>,
        related_clip_ids: Vec<i64>,
        job: F,
    ) -> BackgroundJobId
    where
        F: FnOnce(BackgroundJobContext) -> Fut + Send + 'static,
        Fut: Future<Output = Result<BackgroundJobSuccess, String>> + Send + 'static,
    {
        let id = BackgroundJobId(self.next_id);
        self.next_id += 1;

        let now = Utc::now();
        let label = label.into();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.jobs.insert(
            id,
            JobEntry {
                record: BackgroundJobRecord {
                    id,
                    kind,
                    label: label.clone(),
                    state: BackgroundJobState::Queued,
                    related_clip_ids,
                    progress: None,
                    started_at: now,
                    updated_at: now,
                    finished_at: None,
                    detail: None,
                    cancellable: true,
                },
                cancelled: cancelled.clone(),
            },
        );
        info!(job_id = %id, kind = %kind.label(), label = %label, "Queued background job");

        let tx = self.tx.clone();
        let permit = self.kind_limits.get(&kind).cloned();
        tokio::spawn(async move {
            let _guard = match permit {
                Some(semaphore) => Some(
                    semaphore
                        .acquire_owned()
                        .await
                        .expect("background job semaphore should remain open"),
                ),
                None => None,
            };
            info!(job_id = %id, kind = %kind.label(), label = %label, "Starting background job");
            let _ = tx.send(ManagerEvent::Started { id });
            let context = BackgroundJobContext {
                id,
                tx: tx.clone(),
                cancelled: cancelled.clone(),
            };

            if cancelled.load(Ordering::SeqCst) {
                let _ = tx.send(ManagerEvent::Cancelled { id });
                let _ = tx.send(ManagerEvent::Failed {
                    id,
                    error: "Job cancelled.".into(),
                });
                return;
            }

            match job(context).await {
                Ok(_) if cancelled.load(Ordering::SeqCst) => {
                    warn!(job_id = %id, kind = %kind.label(), label = %label, "Background job completed after cancellation");
                    let _ = tx.send(ManagerEvent::Cancelled { id });
                    let _ = tx.send(ManagerEvent::Failed {
                        id,
                        error: "Job cancelled.".into(),
                    });
                }
                Ok(success) => {
                    info!(
                        job_id = %id,
                        kind = %kind.label(),
                        label = %label,
                        success = ?success,
                        "Background job succeeded"
                    );
                    let _ = tx.send(ManagerEvent::Finished { id, success });
                }
                Err(error) if cancelled.load(Ordering::SeqCst) => {
                    warn!(
                        job_id = %id,
                        kind = %kind.label(),
                        label = %label,
                        error = %error,
                        "Background job failed after cancellation"
                    );
                    let _ = tx.send(ManagerEvent::Cancelled { id });
                    let _ = tx.send(ManagerEvent::Failed {
                        id,
                        error: if error.trim().is_empty() {
                            "Job cancelled.".into()
                        } else {
                            error
                        },
                    });
                }
                Err(error) => {
                    warn!(
                        job_id = %id,
                        kind = %kind.label(),
                        label = %label,
                        error = %error,
                        "Background job failed"
                    );
                    let _ = tx.send(ManagerEvent::Failed { id, error });
                }
            }
        });

        id
    }

    pub fn cancel(&mut self, id: BackgroundJobId) -> bool {
        if let Some(entry) = self.jobs.get_mut(&id) {
            entry.cancelled.store(true, Ordering::SeqCst);
            entry.record.detail = Some("Cancellation requested.".into());
            entry.record.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    pub fn drain_notifications(&mut self) -> Vec<BackgroundJobNotification> {
        let mut notifications = Vec::new();

        while let Ok(event) = self.rx.try_recv() {
            match event {
                ManagerEvent::Started { id } => {
                    if let Some(entry) = self.jobs.get_mut(&id) {
                        entry.record.state = BackgroundJobState::Running;
                        entry.record.updated_at = Utc::now();
                        notifications
                            .push(BackgroundJobNotification::Updated(entry.record.clone()));
                    }
                }
                ManagerEvent::Progress { id, progress } => {
                    if let Some(entry) = self.jobs.get_mut(&id) {
                        entry.record.progress = Some(progress);
                        entry.record.updated_at = Utc::now();
                        notifications
                            .push(BackgroundJobNotification::Updated(entry.record.clone()));
                    }
                }
                ManagerEvent::Cancelled { id } => {
                    if let Some(entry) = self.jobs.get_mut(&id) {
                        entry.record.state = BackgroundJobState::Cancelled;
                        entry.record.updated_at = Utc::now();
                        notifications
                            .push(BackgroundJobNotification::Updated(entry.record.clone()));
                    }
                }
                ManagerEvent::Finished { id, success } => {
                    if let Some(mut entry) = self.jobs.remove(&id) {
                        entry.record.state = if entry.cancelled.load(Ordering::SeqCst) {
                            BackgroundJobState::Cancelled
                        } else {
                            BackgroundJobState::Succeeded
                        };
                        entry.record.detail = Some(match &success {
                            BackgroundJobSuccess::StorageTiering { message, .. }
                            | BackgroundJobSuccess::Upload { message, .. }
                            | BackgroundJobSuccess::Montage { message, .. }
                            | BackgroundJobSuccess::DiscordWebhook { message, .. }
                            | BackgroundJobSuccess::PostProcess { message, .. } => message.clone(),
                        });
                        entry.record.updated_at = Utc::now();
                        entry.record.finished_at = Some(entry.record.updated_at);
                        let record = entry.record.clone();
                        self.push_history(entry.record);
                        notifications.push(BackgroundJobNotification::Finished {
                            record,
                            success: Some(success),
                            error: None,
                        });
                    }
                }
                ManagerEvent::Failed { id, error } => {
                    if let Some(mut entry) = self.jobs.remove(&id) {
                        entry.record.state = if entry.cancelled.load(Ordering::SeqCst) {
                            BackgroundJobState::Cancelled
                        } else {
                            BackgroundJobState::Failed
                        };
                        entry.record.detail = Some(error.clone());
                        entry.record.updated_at = Utc::now();
                        entry.record.finished_at = Some(entry.record.updated_at);
                        let record = entry.record.clone();
                        self.push_history(entry.record);
                        notifications.push(BackgroundJobNotification::Finished {
                            record,
                            success: None,
                            error: Some(error),
                        });
                    }
                }
            }
        }

        notifications
    }

    fn push_history(&mut self, record: BackgroundJobRecord) {
        self.history.push_front(record);
        while self.history.len() > Self::HISTORY_LIMIT {
            self.history.pop_back();
        }
    }
}

impl Default for BackgroundJobManager {
    fn default() -> Self {
        Self::new()
    }
}

fn default_kind_limits() -> HashMap<BackgroundJobKind, std::sync::Arc<Semaphore>> {
    let mut kind_limits = HashMap::new();
    kind_limits.insert(
        BackgroundJobKind::PostProcess,
        std::sync::Arc::new(Semaphore::new(1)),
    );
    kind_limits.insert(
        BackgroundJobKind::Upload,
        std::sync::Arc::new(Semaphore::new(2)),
    );
    kind_limits.insert(
        BackgroundJobKind::Montage,
        std::sync::Arc::new(Semaphore::new(1)),
    );
    kind_limits.insert(
        BackgroundJobKind::StorageTiering,
        std::sync::Arc::new(Semaphore::new(1)),
    );
    kind_limits.insert(
        BackgroundJobKind::DiscordWebhook,
        std::sync::Arc::new(Semaphore::new(4)),
    );
    kind_limits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn completes_job_and_records_history() {
        let mut manager = BackgroundJobManager::new();
        let id = manager.start(
            BackgroundJobKind::StorageTiering,
            "Tier clips",
            vec![1, 2],
            |ctx| async move {
                ctx.progress(1, 2, "Scanning").unwrap();
                ctx.progress(2, 2, "Moving").unwrap();
                Ok(BackgroundJobSuccess::StorageTiering {
                    moved_clip_ids: vec![1],
                    message: "Moved 1 clip.".into(),
                })
            },
        );

        let mut saw_finished = false;
        for _ in 0..20 {
            let notifications = manager.drain_notifications();
            if notifications.iter().any(|notification| {
                matches!(
                    notification,
                    BackgroundJobNotification::Finished {
                        record,
                        success: Some(BackgroundJobSuccess::StorageTiering { .. }),
                        ..
                    } if record.id == id
                )
            }) {
                saw_finished = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(saw_finished);
        assert!(manager.active_jobs().is_empty());
        assert_eq!(manager.recent_jobs().len(), 1);
        assert_eq!(
            manager.recent_jobs()[0].state,
            BackgroundJobState::Succeeded
        );
    }

    #[tokio::test]
    async fn cancellation_marks_job_cancelled() {
        let mut manager = BackgroundJobManager::new();
        let id = manager.start(
            BackgroundJobKind::Upload,
            "Upload clip",
            vec![7],
            |ctx| async move {
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                ctx.check_cancelled()?;
                Ok(BackgroundJobSuccess::Upload {
                    clip_id: 7,
                    provider_label: "YouTube".into(),
                    clip_url: Some("https://example.invalid/watch?v=abc123".into()),
                    message: "uploaded".into(),
                })
            },
        );

        assert!(manager.cancel(id));
        let mut saw_cancelled = false;
        for _ in 0..20 {
            for notification in manager.drain_notifications() {
                if let BackgroundJobNotification::Finished { record, .. } = notification {
                    if record.id == id {
                        saw_cancelled = true;
                        assert_eq!(record.state, BackgroundJobState::Cancelled);
                    }
                }
            }
            if saw_cancelled {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(saw_cancelled);
    }

    #[test]
    fn remove_history_deletes_finished_record() {
        let mut manager = BackgroundJobManager::new();
        let now = Utc::now();
        manager.replace_recent_jobs(vec![BackgroundJobRecord {
            id: BackgroundJobId(7),
            kind: BackgroundJobKind::Upload,
            label: "Upload clip #7 to YouTube".into(),
            state: BackgroundJobState::Failed,
            related_clip_ids: vec![7],
            progress: None,
            started_at: now - chrono::Duration::minutes(1),
            updated_at: now,
            finished_at: Some(now),
            detail: Some("Upload failed.".into()),
            cancellable: false,
        }]);

        let removed = manager.remove_history(BackgroundJobId(7)).unwrap();

        assert_eq!(removed.id, BackgroundJobId(7));
        assert!(manager.record(BackgroundJobId(7)).is_none());
        assert!(manager.recent_jobs().is_empty());
    }
}
