//! Scheduler module - cron jobs, heartbeat, background tasks

mod jobs;

pub use jobs::{Job, JobStatus, JobTrigger};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

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
        
        if let JobTrigger::Cron(cron_expr) = &job.trigger {
            Schedule::from_str(cron_expr)
                .map_err(|e| SchedulerError::InvalidSchedule(e.to_string()))?;
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
            if let JobTrigger::Cron(cron_expr) = &job.trigger {
                if let Ok(schedule) = Schedule::from_str(cron_expr) {
                    if let Some(next) = schedule.upcoming(Utc).next() {
                        next_runs.insert(id.clone(), next);
                    }
                }
            }
        }
        
        next_runs
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
pub fn new_job(
    name: impl Into<String>,
    trigger: JobTrigger,
    action: impl Into<String>,
) -> Job {
    Job {
        id: Uuid::new_v4().to_string(),
        name: name.into(),
        description: None,
        trigger,
        action: action.into(),
        status: JobStatus::Pending,
        created_at: Utc::now(),
        last_run: None,
        next_run: None,
        run_count: 0,
        metadata: HashMap::new(),
    }
}
