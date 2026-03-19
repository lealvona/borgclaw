//! Scheduler module - cron jobs, heartbeat, background tasks

mod jobs;

pub use jobs::{Job, JobRun, JobStatus, JobTrigger};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use futures_util::future::join_all;
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

const SCHEDULER_LOOP_ID: &str = "__scheduler_loop__";

type BoxedJobFuture = Pin<Box<dyn Future<Output = Result<(), SchedulerError>> + Send>>;
pub type ScheduledJobHandler = Arc<dyn Fn(Job) -> BoxedJobFuture + Send + Sync>;

/// Scheduler trait - implemented by scheduler backends
#[async_trait]
pub trait SchedulerTrait: Send + Sync {
    /// Schedule a new job
    async fn schedule(&self, job: Job) -> Result<String, SchedulerError>;

    /// Unschedule a job
    async fn unschedule(&self, id: &str) -> Result<(), SchedulerError>;

    /// List all jobs
    async fn list(&self) -> Vec<Job>;

    /// Get job by ID
    async fn get(&self, id: &str) -> Option<Job>;
}

/// Scheduler - manages scheduled and background jobs
#[derive(Clone)]
pub struct Scheduler {
    jobs: Arc<RwLock<HashMap<String, Job>>>,
    running: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    state_path: Option<PathBuf>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(HashMap::new())),
            state_path: None,
        }
    }

    pub fn with_state_path(mut self, path: impl Into<PathBuf>) -> Self {
        let state_path = path.into();
        self.jobs = Arc::new(RwLock::new(load_jobs(&state_path)));
        self.state_path = Some(state_path);
        self
    }

    pub async fn start(
        &self,
        poll_interval: Duration,
        handler: ScheduledJobHandler,
    ) -> Result<(), SchedulerError> {
        self.start_with_policy(poll_interval, 1, None, handler)
            .await
    }

    pub async fn start_with_limit(
        &self,
        poll_interval: Duration,
        max_concurrent_jobs: usize,
        handler: ScheduledJobHandler,
    ) -> Result<(), SchedulerError> {
        self.start_with_policy(poll_interval, max_concurrent_jobs, None, handler)
            .await
    }

    pub async fn start_with_policy(
        &self,
        poll_interval: Duration,
        max_concurrent_jobs: usize,
        job_timeout: Option<Duration>,
        handler: ScheduledJobHandler,
    ) -> Result<(), SchedulerError> {
        if poll_interval.is_zero() {
            return Err(SchedulerError::Error(
                "scheduler poll interval must be greater than zero".to_string(),
            ));
        }
        let max_concurrent_jobs = max_concurrent_jobs.max(1);

        let mut running = self.running.write().await;
        if let Some(handle) = running.get(SCHEDULER_LOOP_ID) {
            if !handle.is_finished() {
                return Err(SchedulerError::Error(
                    "scheduler already running".to_string(),
                ));
            }
        }
        running.remove(SCHEDULER_LOOP_ID);

        let scheduler = self.clone();
        let loop_handler = handler.clone();
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            loop {
                ticker.tick().await;
                let _ = scheduler
                    .run_due_with_policy(max_concurrent_jobs, job_timeout, |job| {
                        let loop_handler = loop_handler.clone();
                        async move { loop_handler(job).await }
                    })
                    .await;
            }
        });
        running.insert(SCHEDULER_LOOP_ID.to_string(), handle);

        Ok(())
    }

    pub async fn stop(&self) -> bool {
        let mut running = self.running.write().await;
        if let Some(handle) = running.remove(SCHEDULER_LOOP_ID) {
            handle.abort();
            true
        } else {
            false
        }
    }

    pub async fn is_running(&self) -> bool {
        let mut running = self.running.write().await;
        if let Some(handle) = running.get(SCHEDULER_LOOP_ID) {
            if handle.is_finished() {
                running.remove(SCHEDULER_LOOP_ID);
                return false;
            }
            return true;
        }

        false
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SchedulerTrait for Scheduler {
    async fn schedule(&self, job: Job) -> Result<String, SchedulerError> {
        let id = job.id.clone();
        let mut job = job;

        if let JobTrigger::Cron(cron_expr) = &job.trigger {
            Schedule::from_str(cron_expr)
                .map_err(|e| SchedulerError::InvalidSchedule(e.to_string()))?;
        }

        if job.next_run.is_none() {
            job.next_run = job.trigger.next_run();
        }

        let mut jobs = self.jobs.write().await;
        jobs.insert(id.clone(), job);
        self.persist_state(&jobs);

        Ok(id)
    }

    async fn unschedule(&self, id: &str) -> Result<(), SchedulerError> {
        {
            let mut running = self.running.write().await;
            if let Some(handle) = running.remove(id) {
                handle.abort();
            }
        }

        let mut jobs = self.jobs.write().await;
        jobs.remove(id)
            .ok_or_else(|| SchedulerError::JobNotFound(id.to_string()))?;
        self.persist_state(&jobs);

        Ok(())
    }

    async fn list(&self) -> Vec<Job> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    async fn get(&self, id: &str) -> Option<Job> {
        let jobs = self.jobs.read().await;
        jobs.get(id).cloned()
    }
}

impl Scheduler {
    /// Get jobs matching a trigger type
    pub async fn get_by_trigger(&self, trigger_type: TriggerType) -> Vec<Job> {
        let jobs = self.jobs.read().await;
        jobs.values()
            .filter(|j| match (&j.trigger, trigger_type) {
                (JobTrigger::Cron(_), TriggerType::Cron) => true,
                (JobTrigger::Interval(_), TriggerType::Interval) => true,
                (JobTrigger::OneShot(_), TriggerType::OneShot) => true,
                _ => false,
            })
            .cloned()
            .collect()
    }

    /// Update job status
    pub async fn update_status(&self, id: &str, status: JobStatus) -> Result<(), SchedulerError> {
        let mut jobs = self.jobs.write().await;
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| SchedulerError::JobNotFound(id.to_string()))?;
        job.status = status;
        self.persist_state(&jobs);
        Ok(())
    }

    /// Get next run time for cron jobs
    pub async fn next_runs(&self) -> HashMap<String, DateTime<Utc>> {
        let jobs = self.jobs.read().await;
        let mut next_runs = HashMap::new();

        for (id, job) in jobs.iter() {
            if let Some(next) = job.next_run.or_else(|| job.trigger.next_run()) {
                next_runs.insert(id.clone(), next);
            }
        }

        next_runs
    }

    /// Execute all due jobs using the supplied handler.
    pub async fn run_due<F, Fut>(&self, handler: F) -> Vec<Result<String, SchedulerError>>
    where
        F: Fn(Job) -> Fut + Send + Sync,
        Fut: Future<Output = Result<(), SchedulerError>>,
    {
        self.run_due_with_policy(1, None, handler).await
    }

    pub async fn run_due_with_limit<F, Fut>(
        &self,
        max_concurrent_jobs: usize,
        handler: F,
    ) -> Vec<Result<String, SchedulerError>>
    where
        F: Fn(Job) -> Fut + Send + Sync,
        Fut: Future<Output = Result<(), SchedulerError>>,
    {
        self.run_due_with_policy(max_concurrent_jobs, None, handler)
            .await
    }

    pub async fn run_due_with_policy<F, Fut>(
        &self,
        max_concurrent_jobs: usize,
        job_timeout: Option<Duration>,
        handler: F,
    ) -> Vec<Result<String, SchedulerError>>
    where
        F: Fn(Job) -> Fut + Send + Sync,
        Fut: Future<Output = Result<(), SchedulerError>>,
    {
        let max_concurrent_jobs = max_concurrent_jobs.max(1);
        let due_jobs: Vec<Job> = {
            let now = Utc::now();
            let jobs = self.jobs.read().await;
            jobs.values()
                .filter(|job| {
                    job.status != JobStatus::Disabled
                        && job.next_run.map(|next| next <= now).unwrap_or(false)
                })
                .cloned()
                .collect()
        };

        if due_jobs.is_empty() {
            return Vec::new();
        }

        {
            let mut jobs = self.jobs.write().await;
            for job in &due_jobs {
                if let Some(stored) = jobs.get_mut(&job.id) {
                    stored.status = JobStatus::Running;
                }
            }
            self.persist_state(&jobs);
        }

        let mut results = Vec::with_capacity(due_jobs.len());
        for batch in due_jobs.chunks(max_concurrent_jobs) {
            let batch_results = join_all(batch.iter().cloned().map(|job| async {
                let started_at = Utc::now();
                let result = match job_timeout {
                    Some(timeout) => {
                        match tokio::time::timeout(timeout, handler(job.clone())).await {
                            Ok(result) => result,
                            Err(_) => Err(SchedulerError::JobFailed(format!(
                                "job timed out after {} seconds",
                                timeout.as_secs()
                            ))),
                        }
                    }
                    None => handler(job.clone()).await,
                };
                (job, started_at, result)
            }))
            .await;

            for (job, started_at, result) in batch_results {
                let finished_at = Utc::now();
                {
                    let mut jobs = self.jobs.write().await;
                    if let Some(stored) = jobs.get_mut(&job.id) {
                        stored.last_run = Some(finished_at);
                        stored.run_count += 1;
                        let (status, error, retry_scheduled) = match result.as_ref() {
                            Ok(()) => {
                                stored.status = JobStatus::Completed;
                                stored.retry_count = 0;
                                stored.dead_lettered_at = None;
                                stored.next_run = match stored.trigger {
                                    JobTrigger::OneShot(_) => None,
                                    _ => stored.trigger.next_run(),
                                };
                                (JobStatus::Completed, None, None)
                            }
                            Err(err) => {
                                let error = Some(err.to_string());
                                if stored.retry_count < stored.max_retries {
                                    stored.retry_count += 1;
                                    stored.status = JobStatus::Pending;
                                    stored.next_run = Some(
                                        finished_at
                                            + chrono::Duration::seconds(
                                                stored.retry_delay_seconds.max(1) as i64,
                                            ),
                                    );
                                    (JobStatus::Failed, error, Some(stored.retry_count))
                                } else {
                                    stored.status = JobStatus::Failed;
                                    stored.next_run = None;
                                    stored.dead_lettered_at = Some(finished_at);
                                    (JobStatus::Failed, error, None)
                                }
                            }
                        };
                        stored.run_history.push(JobRun {
                            started_at,
                            finished_at,
                            status,
                            error,
                            retry_scheduled,
                        });
                        if stored.run_history.len() > 20 {
                            let excess = stored.run_history.len() - 20;
                            stored.run_history.drain(0..excess);
                        }
                    }
                    self.persist_state(&jobs);
                }

                results.push(result.map(|_| job.id));
            }
        }

        results
    }

    fn persist_state(&self, jobs: &HashMap<String, Job>) {
        let Some(path) = &self.state_path else {
            return;
        };

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let Ok(serialized) = serde_json::to_string_pretty(jobs) else {
            return;
        };

        let temp_path = path.with_extension("json.tmp");
        if std::fs::write(&temp_path, serialized).is_ok() {
            let _ = std::fs::rename(temp_path, path);
        }
    }
}

/// Trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerType {
    Cron,
    Interval,
    OneShot,
}

/// Scheduler errors
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Invalid schedule: {0}")]
    InvalidSchedule(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Job failed: {0}")]
    JobFailed(String),
    #[error("Scheduler error: {0}")]
    Error(String),
}

/// Create a new job
pub fn new_job(name: impl Into<String>, trigger: JobTrigger, action: impl Into<String>) -> Job {
    let next_run = trigger.next_run();
    Job {
        id: Uuid::new_v4().to_string(),
        name: name.into(),
        description: None,
        trigger,
        action: action.into(),
        status: JobStatus::Pending,
        created_at: Utc::now(),
        last_run: None,
        next_run,
        run_count: 0,
        max_retries: 0,
        retry_count: 0,
        retry_delay_seconds: 60,
        dead_lettered_at: None,
        run_history: Vec::new(),
        metadata: HashMap::new(),
    }
}

pub fn with_retry_policy(mut job: Job, max_retries: u32, retry_delay_seconds: u64) -> Job {
    job.max_retries = max_retries;
    job.retry_delay_seconds = retry_delay_seconds.max(1);
    job
}

fn load_jobs(path: &PathBuf) -> HashMap<String, Job> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    let Ok(mut jobs) = serde_json::from_str::<HashMap<String, Job>>(&contents) else {
        return HashMap::new();
    };

    for job in jobs.values_mut() {
        if job.status == JobStatus::Running {
            job.status = JobStatus::Pending;
            if job.next_run.is_none() {
                job.next_run = Some(Utc::now());
            }
        }
    }

    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn new_job_initializes_next_run_from_trigger() {
        let job = new_job("interval", JobTrigger::Interval(30), "echo hi");
        assert!(job.next_run.is_some());
        assert_eq!(job.max_retries, 0);
        assert_eq!(job.retry_count, 0);
        assert_eq!(job.retry_delay_seconds, 60);
        assert!(job.dead_lettered_at.is_none());
    }

    #[tokio::test]
    async fn scheduler_schedule_populates_missing_next_run() {
        let scheduler = Scheduler::new();
        let mut job = new_job(
            "oneshot",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = None;
        let id = scheduler.schedule(job).await.unwrap();

        let stored = scheduler.get(&id).await.unwrap();
        assert!(stored.next_run.is_some());
    }

    #[tokio::test]
    async fn scheduler_next_runs_includes_non_cron_jobs() {
        let scheduler = Scheduler::new();
        let interval_id = scheduler
            .schedule(new_job(
                "interval",
                JobTrigger::Interval(30),
                "echo interval",
            ))
            .await
            .unwrap();
        let oneshot_id = scheduler
            .schedule(new_job(
                "oneshot",
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo oneshot",
            ))
            .await
            .unwrap();

        let next_runs = scheduler.next_runs().await;
        assert!(next_runs.contains_key(&interval_id));
        assert!(next_runs.contains_key(&oneshot_id));
    }

    #[tokio::test]
    async fn scheduler_run_due_executes_due_jobs_and_updates_state() {
        let scheduler = Scheduler::new();
        let mut job = new_job(
            "oneshot",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let results = scheduler
            .run_due(|job| async move {
                assert_eq!(job.action, "echo hi");
                Ok(())
            })
            .await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
        assert_eq!(stored.status, JobStatus::Completed);
        assert_eq!(stored.run_count, 1);
        assert!(stored.last_run.is_some());
        assert!(stored.next_run.is_none());
        assert_eq!(stored.run_history.len(), 1);
        assert_eq!(stored.run_history[0].status, JobStatus::Completed);
        assert!(stored.run_history[0].error.is_none());
        assert!(stored.run_history[0].retry_scheduled.is_none());
    }

    #[tokio::test]
    async fn scheduler_run_due_recalculates_repeating_job_next_run() {
        let scheduler = Scheduler::new();
        let mut job = new_job("interval", JobTrigger::Interval(30), "echo tick");
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let _ = scheduler.run_due(|_| async { Ok(()) }).await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(stored.status, JobStatus::Completed);
        assert_eq!(stored.run_count, 1);
        assert!(stored.next_run.is_some());
    }

    #[tokio::test]
    async fn scheduler_run_due_marks_failures() {
        let scheduler = Scheduler::new();
        let mut job = new_job(
            "oneshot",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let results = scheduler
            .run_due(|_| async { Err(SchedulerError::JobFailed("boom".to_string())) })
            .await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        assert_eq!(stored.status, JobStatus::Failed);
        assert_eq!(stored.run_count, 1);
        assert!(stored.last_run.is_some());
        assert_eq!(stored.run_history.len(), 1);
        assert_eq!(stored.run_history[0].status, JobStatus::Failed);
        assert_eq!(
            stored.run_history[0].error.as_deref(),
            Some("Job failed: boom")
        );
        assert!(stored.run_history[0].retry_scheduled.is_none());
    }

    #[tokio::test]
    async fn scheduler_run_due_reschedules_retryable_failures() {
        let scheduler = Scheduler::new();
        let mut job = with_retry_policy(
            new_job(
                "retry-once",
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo retry",
            ),
            2,
            30,
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let results = scheduler
            .run_due(|_| async { Err(SchedulerError::JobFailed("boom".to_string())) })
            .await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        assert_eq!(stored.status, JobStatus::Pending);
        assert_eq!(stored.retry_count, 1);
        assert!(stored.next_run.is_some());
        assert!(stored.dead_lettered_at.is_none());
        assert_eq!(stored.run_history.len(), 1);
        assert_eq!(stored.run_history[0].retry_scheduled, Some(1));
    }

    #[tokio::test]
    async fn scheduler_run_due_dead_letters_after_retries_exhausted() {
        let scheduler = Scheduler::new();
        let mut job = with_retry_policy(
            new_job(
                "retry-exhausted",
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo retry",
            ),
            1,
            5,
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let _ = scheduler
            .run_due(|_| async { Err(SchedulerError::JobFailed("boom".to_string())) })
            .await;

        {
            let mut jobs = scheduler.jobs.write().await;
            let stored = jobs.get_mut(&id).unwrap();
            stored.next_run = Some(Utc::now() - Duration::seconds(1));
        }

        let results = scheduler
            .run_due(|_| async { Err(SchedulerError::JobFailed("boom again".to_string())) })
            .await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        assert_eq!(stored.status, JobStatus::Failed);
        assert_eq!(stored.retry_count, 1);
        assert!(stored.next_run.is_none());
        assert!(stored.dead_lettered_at.is_some());
        assert_eq!(stored.run_history.len(), 2);
        assert_eq!(stored.run_history[0].retry_scheduled, Some(1));
        assert!(stored.run_history[1].retry_scheduled.is_none());
    }

    #[tokio::test]
    async fn scheduler_start_executes_due_jobs() {
        let scheduler = Scheduler::new();
        let mut job = new_job(
            "oneshot",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();
        let runs = Arc::new(AtomicUsize::new(0));
        let handler_runs = runs.clone();

        scheduler
            .start(
                std::time::Duration::from_millis(10),
                Arc::new(move |_| {
                    let handler_runs = handler_runs.clone();
                    Box::pin(async move {
                        handler_runs.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                }),
            )
            .await
            .unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if runs.load(Ordering::SeqCst) > 0 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        assert!(scheduler.is_running().await);
        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(stored.status, JobStatus::Completed);
        assert_eq!(stored.run_count, 1);

        assert!(scheduler.stop().await);
        assert!(!scheduler.is_running().await);
    }

    #[tokio::test]
    async fn scheduler_start_rejects_duplicate_loop() {
        let scheduler = Scheduler::new();
        let handler: ScheduledJobHandler = Arc::new(|_| Box::pin(async { Ok(()) }));

        scheduler
            .start(std::time::Duration::from_millis(50), handler.clone())
            .await
            .unwrap();

        let err = scheduler
            .start(std::time::Duration::from_millis(50), handler)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("already running"));

        assert!(scheduler.stop().await);
    }

    #[tokio::test]
    async fn scheduler_run_due_with_limit_honors_concurrency_cap() {
        let scheduler = Scheduler::new();
        for idx in 0..2 {
            let mut job = new_job(
                format!("job-{idx}"),
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo hi",
            );
            job.next_run = Some(Utc::now() - Duration::seconds(1));
            scheduler.schedule(job).await.unwrap();
        }

        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let current_for_handler = current.clone();
        let peak_for_handler = peak.clone();

        let results = scheduler
            .run_due_with_limit(2, move |_| {
                let current = current_for_handler.clone();
                let peak = peak_for_handler.clone();
                async move {
                    let active = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(active, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert_eq!(results.len(), 2);
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn scheduler_run_due_with_limit_can_force_serial_execution() {
        let scheduler = Scheduler::new();
        for idx in 0..2 {
            let mut job = new_job(
                format!("job-{idx}"),
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo hi",
            );
            job.next_run = Some(Utc::now() - Duration::seconds(1));
            scheduler.schedule(job).await.unwrap();
        }

        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let current_for_handler = current.clone();
        let peak_for_handler = peak.clone();

        let results = scheduler
            .run_due_with_limit(1, move |_| {
                let current = current_for_handler.clone();
                let peak = peak_for_handler.clone();
                async move {
                    let active = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(active, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;

        assert_eq!(results.len(), 2);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn scheduler_run_due_with_policy_marks_timeouts_failed() {
        let scheduler = Scheduler::new();
        let mut job = new_job(
            "slow-job",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let results = scheduler
            .run_due_with_policy(1, Some(std::time::Duration::from_secs(1)), |_| async {
                tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                Ok(())
            })
            .await;

        let stored = scheduler.get(&id).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
        assert_eq!(stored.status, JobStatus::Failed);
        assert_eq!(stored.run_count, 1);
    }

    #[tokio::test]
    async fn scheduler_with_state_path_reloads_persisted_jobs() {
        let state_path =
            std::env::temp_dir().join(format!("borgclaw_scheduler_state_{}.json", Uuid::new_v4()));
        let scheduler = Scheduler::new().with_state_path(state_path.clone());
        let id = scheduler
            .schedule(new_job(
                "interval",
                JobTrigger::Interval(30),
                "echo persisted",
            ))
            .await
            .unwrap();

        let reloaded = Scheduler::new().with_state_path(state_path);
        let stored = reloaded.get(&id).await.unwrap();
        assert_eq!(stored.name, "interval");
        assert_eq!(stored.action, "echo persisted");
    }

    #[tokio::test]
    async fn scheduler_with_state_path_persists_run_history() {
        let state_path =
            std::env::temp_dir().join(format!("borgclaw_scheduler_runs_{}.json", Uuid::new_v4()));
        let scheduler = Scheduler::new().with_state_path(state_path.clone());
        let mut job = new_job(
            "oneshot",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo hi",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();

        let results = scheduler.run_due(|_| async { Ok(()) }).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());

        let reloaded = Scheduler::new().with_state_path(state_path);
        let stored = reloaded.get(&id).await.unwrap();
        assert_eq!(stored.status, JobStatus::Completed);
        assert_eq!(stored.run_count, 1);
        assert_eq!(stored.run_history.len(), 1);
    }

    #[tokio::test]
    async fn scheduler_with_state_path_persists_unschedule() {
        let state_path = std::env::temp_dir().join(format!(
            "borgclaw_scheduler_unschedule_{}.json",
            Uuid::new_v4()
        ));
        let scheduler = Scheduler::new().with_state_path(state_path.clone());
        let id = scheduler
            .schedule(new_job(
                "oneshot",
                JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
                "echo hi",
            ))
            .await
            .unwrap();

        scheduler.unschedule(&id).await.unwrap();

        let reloaded = Scheduler::new().with_state_path(state_path);
        assert!(reloaded.get(&id).await.is_none());
    }

    #[tokio::test]
    async fn scheduler_with_state_path_recovers_running_jobs_as_pending() {
        let state_path = std::env::temp_dir().join(format!(
            "borgclaw_scheduler_recover_{}.json",
            Uuid::new_v4()
        ));
        let scheduler = Scheduler::new().with_state_path(state_path.clone());
        let mut job = new_job(
            "recover-running",
            JobTrigger::OneShot(Utc::now() + Duration::seconds(5)),
            "echo recover",
        );
        job.next_run = Some(Utc::now() - Duration::seconds(1));
        let id = scheduler.schedule(job).await.unwrap();
        scheduler
            .update_status(&id, JobStatus::Running)
            .await
            .unwrap();

        let reloaded = Scheduler::new().with_state_path(state_path);
        let stored = reloaded.get(&id).await.unwrap();
        assert_eq!(stored.status, JobStatus::Pending);
        assert!(stored.next_run.is_some());
    }
}
