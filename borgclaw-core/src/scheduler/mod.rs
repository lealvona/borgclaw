//! Scheduler module - cron jobs, heartbeat, background tasks

mod jobs;

pub use jobs::{Job, JobStatus, JobTrigger};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use futures_util::future::join_all;
use std::collections::HashMap;
use std::future::Future;
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
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(
        &self,
        poll_interval: Duration,
        handler: ScheduledJobHandler,
    ) -> Result<(), SchedulerError> {
        self.start_with_limit(poll_interval, 1, handler).await
    }

    pub async fn start_with_limit(
        &self,
        poll_interval: Duration,
        max_concurrent_jobs: usize,
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
                return Err(SchedulerError::Error("scheduler already running".to_string()));
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
                    .run_due_with_limit(max_concurrent_jobs, |job| {
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
        self.run_due_with_limit(1, handler).await
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
        }

        let mut results = Vec::with_capacity(due_jobs.len());
        for batch in due_jobs.chunks(max_concurrent_jobs) {
            let batch_results = join_all(batch.iter().cloned().map(|job| async {
                let result = handler(job.clone()).await;
                (job, result)
            }))
            .await;

            for (job, result) in batch_results {
                {
                    let mut jobs = self.jobs.write().await;
                    if let Some(stored) = jobs.get_mut(&job.id) {
                        stored.last_run = Some(Utc::now());
                        stored.run_count += 1;
                        match result.as_ref() {
                            Ok(()) => {
                                stored.status = JobStatus::Completed;
                                stored.next_run = match stored.trigger {
                                    JobTrigger::OneShot(_) => None,
                                    _ => stored.trigger.next_run(),
                                };
                            }
                            Err(_) => {
                                stored.status = JobStatus::Failed;
                            }
                        }
                    }
                }

                results.push(result.map(|_| job.id));
            }
        }

        results
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
        metadata: HashMap::new(),
    }
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
    }

    #[tokio::test]
    async fn scheduler_schedule_populates_missing_next_run() {
        let scheduler = Scheduler::new();
        let mut job = new_job("oneshot", JobTrigger::OneShot(Utc::now() + Duration::seconds(5)), "echo hi");
        job.next_run = None;
        let id = scheduler.schedule(job).await.unwrap();

        let stored = scheduler.get(&id).await.unwrap();
        assert!(stored.next_run.is_some());
    }

    #[tokio::test]
    async fn scheduler_next_runs_includes_non_cron_jobs() {
        let scheduler = Scheduler::new();
        let interval_id = scheduler
            .schedule(new_job("interval", JobTrigger::Interval(30), "echo interval"))
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
        let mut job = new_job("oneshot", JobTrigger::OneShot(Utc::now() + Duration::seconds(5)), "echo hi");
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
        let mut job = new_job("oneshot", JobTrigger::OneShot(Utc::now() + Duration::seconds(5)), "echo hi");
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
    }

    #[tokio::test]
    async fn scheduler_start_executes_due_jobs() {
        let scheduler = Scheduler::new();
        let mut job = new_job("oneshot", JobTrigger::OneShot(Utc::now() + Duration::seconds(5)), "echo hi");
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
}
