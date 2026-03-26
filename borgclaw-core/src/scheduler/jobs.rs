//! Jobs module - job definitions for scheduler

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// Policy for handling missed scheduled runs (e.g., after process restart)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatchUpPolicy {
    /// Skip all missed windows, advance to next future run (default)
    Skip,
    /// Coalesce: run once on recovery regardless of how many were missed
    RunOnce,
}

impl Default for CatchUpPolicy {
    fn default() -> Self {
        Self::Skip
    }
}

/// Job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID
    pub id: String,
    /// Job name
    pub name: String,
    /// Job description
    pub description: Option<String>,
    /// Job trigger
    pub trigger: JobTrigger,
    /// Action to execute (command or skill name)
    pub action: String,
    /// Current status
    pub status: JobStatus,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Last run timestamp
    pub last_run: Option<DateTime<Utc>>,
    /// Next scheduled run
    pub next_run: Option<DateTime<Utc>>,
    /// Number of times run
    pub run_count: u32,
    /// Maximum retry attempts after a failed run
    pub max_retries: u32,
    /// Number of retries already scheduled
    pub retry_count: u32,
    /// Delay before a retry attempt, in seconds
    pub retry_delay_seconds: u64,
    /// Timestamp when the job exhausted retries and entered dead-letter state
    pub dead_lettered_at: Option<DateTime<Utc>>,
    /// Recent execution history
    pub run_history: Vec<JobRun>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Policy for handling missed scheduled runs
    #[serde(default)]
    pub catch_up_policy: CatchUpPolicy,
    /// Count of detected missed runs during last recovery
    #[serde(default)]
    pub missed_runs: u32,
}

/// Recorded execution of a scheduled job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    /// Run start timestamp
    pub started_at: DateTime<Utc>,
    /// Run completion timestamp
    pub finished_at: DateTime<Utc>,
    /// Final run status
    pub status: JobStatus,
    /// Failure detail when present
    pub error: Option<String>,
    /// Retry number scheduled after this run when present
    pub retry_scheduled: Option<u32>,
}

/// Job trigger types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum JobTrigger {
    /// Cron expression
    Cron(String),
    /// Interval in seconds
    Interval(u64),
    /// One-shot at specific time
    OneShot(DateTime<Utc>),
}

impl JobTrigger {
    /// Get next run time from now
    pub fn next_run(&self) -> Option<DateTime<Utc>> {
        match self {
            JobTrigger::Cron(expr) => {
                let schedule = cron::Schedule::from_str(expr).ok()?;
                schedule.upcoming(Utc).next()
            }
            JobTrigger::Interval(secs) => Some(Utc::now() + Duration::seconds(*secs as i64)),
            JobTrigger::OneShot(dt) => {
                if *dt > Utc::now() {
                    Some(*dt)
                } else {
                    None
                }
            }
        }
    }
}

/// Job status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// Job is pending
    Pending,
    /// Job is running
    Running,
    /// Job completed successfully
    Completed,
    /// Job failed
    Failed,
    /// Job was cancelled
    Cancelled,
    /// Job is disabled
    Disabled,
}

impl Default for JobStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
            JobStatus::Disabled => write!(f, "disabled"),
        }
    }
}
