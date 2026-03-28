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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_up_policy_default_is_skip() {
        assert_eq!(CatchUpPolicy::default(), CatchUpPolicy::Skip);
    }

    #[test]
    fn catch_up_policy_variants() {
        assert_eq!(CatchUpPolicy::Skip, CatchUpPolicy::Skip);
        assert_eq!(CatchUpPolicy::RunOnce, CatchUpPolicy::RunOnce);
        assert_ne!(CatchUpPolicy::Skip, CatchUpPolicy::RunOnce);
    }

    #[test]
    fn catch_up_policy_serialization_roundtrip() {
        let skip = CatchUpPolicy::Skip;
        let json = serde_json::to_string(&skip).unwrap();
        assert_eq!(json, "\"skip\"");
        let deserialized: CatchUpPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CatchUpPolicy::Skip);

        let run_once = CatchUpPolicy::RunOnce;
        let json = serde_json::to_string(&run_once).unwrap();
        assert_eq!(json, "\"run_once\"");
        let deserialized: CatchUpPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, CatchUpPolicy::RunOnce);
    }

    #[test]
    fn job_status_default_is_pending() {
        assert_eq!(JobStatus::default(), JobStatus::Pending);
    }

    #[test]
    fn job_status_display_format() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Running.to_string(), "running");
        assert_eq!(JobStatus::Completed.to_string(), "completed");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
        assert_eq!(JobStatus::Cancelled.to_string(), "cancelled");
        assert_eq!(JobStatus::Disabled.to_string(), "disabled");
    }

    #[test]
    fn job_status_serialization_roundtrip() {
        for status in [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
            JobStatus::Disabled,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, status);
        }
    }

    #[test]
    fn job_trigger_cron_next_run() {
        // Valid cron expression: every minute
        let trigger = JobTrigger::Cron("0 * * * * *".to_string());
        let next = trigger.next_run();
        assert!(next.is_some());
        // Should be in the future
        assert!(next.unwrap() > Utc::now() - Duration::minutes(1));
    }

    #[test]
    fn job_trigger_cron_invalid_returns_none() {
        let trigger = JobTrigger::Cron("invalid cron".to_string());
        let next = trigger.next_run();
        assert!(next.is_none());
    }

    #[test]
    fn job_trigger_interval_next_run() {
        let trigger = JobTrigger::Interval(3600); // 1 hour
        let next = trigger.next_run();
        assert!(next.is_some());

        let expected = Utc::now() + Duration::seconds(3600);
        let actual = next.unwrap();
        // Allow 1 second tolerance for test execution time
        assert!((actual - expected).num_seconds().abs() <= 1);
    }

    #[test]
    fn job_trigger_oneshot_future_returns_datetime() {
        let future = Utc::now() + Duration::hours(1);
        let trigger = JobTrigger::OneShot(future);
        let next = trigger.next_run();
        assert_eq!(next, Some(future));
    }

    #[test]
    fn job_trigger_oneshot_past_returns_none() {
        let past = Utc::now() - Duration::hours(1);
        let trigger = JobTrigger::OneShot(past);
        let next = trigger.next_run();
        assert!(next.is_none());
    }

    #[test]
    fn job_creation_and_defaults() {
        let job = Job {
            id: "job-123".to_string(),
            name: "Test Job".to_string(),
            description: Some("A test job".to_string()),
            trigger: JobTrigger::Interval(60),
            action: "echo hello".to_string(),
            status: JobStatus::Pending,
            created_at: Utc::now(),
            last_run: None,
            next_run: None,
            run_count: 0,
            max_retries: 3,
            retry_count: 0,
            retry_delay_seconds: 60,
            dead_lettered_at: None,
            run_history: Vec::new(),
            metadata: HashMap::new(),
            catch_up_policy: CatchUpPolicy::default(),
            missed_runs: 0,
        };

        assert_eq!(job.id, "job-123");
        assert_eq!(job.name, "Test Job");
        assert_eq!(job.action, "echo hello");
        assert_eq!(job.max_retries, 3);
        assert_eq!(job.retry_delay_seconds, 60);
        assert!(job.run_history.is_empty());
        assert!(job.metadata.is_empty());
    }

    #[test]
    fn job_run_creation() {
        let now = Utc::now();
        let run = JobRun {
            started_at: now,
            finished_at: now + Duration::seconds(5),
            status: JobStatus::Completed,
            error: None,
            retry_scheduled: None,
        };

        assert_eq!(run.status, JobStatus::Completed);
        assert!(run.error.is_none());
        assert!(run.retry_scheduled.is_none());
    }

    #[test]
    fn job_run_with_error() {
        let now = Utc::now();
        let run = JobRun {
            started_at: now,
            finished_at: now + Duration::seconds(5),
            status: JobStatus::Failed,
            error: Some("Connection timeout".to_string()),
            retry_scheduled: Some(1),
        };

        assert_eq!(run.status, JobStatus::Failed);
        assert_eq!(run.error, Some("Connection timeout".to_string()));
        assert_eq!(run.retry_scheduled, Some(1));
    }

    #[test]
    fn job_serialization_roundtrip() {
        let job = Job {
            id: "job-456".to_string(),
            name: "Serialized Job".to_string(),
            description: None,
            trigger: JobTrigger::Cron("0 0 * * * *".to_string()),
            action: "backup".to_string(),
            status: JobStatus::Running,
            created_at: Utc::now(),
            last_run: Some(Utc::now()),
            next_run: Some(Utc::now() + Duration::hours(1)),
            run_count: 5,
            max_retries: 3,
            retry_count: 1,
            retry_delay_seconds: 300,
            dead_lettered_at: None,
            run_history: vec![],
            metadata: {
                let mut map = HashMap::new();
                map.insert("owner".to_string(), "admin".to_string());
                map
            },
            catch_up_policy: CatchUpPolicy::RunOnce,
            missed_runs: 2,
        };

        let json = serde_json::to_string(&job).unwrap();
        let deserialized: Job = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, job.id);
        assert_eq!(deserialized.name, job.name);
        assert_eq!(deserialized.status, job.status);
        assert_eq!(deserialized.run_count, job.run_count);
        assert_eq!(deserialized.catch_up_policy, CatchUpPolicy::RunOnce);
        assert_eq!(deserialized.missed_runs, 2);
    }

    #[test]
    fn job_trigger_variants() {
        let cron = JobTrigger::Cron("0 0 * * *".to_string());
        match cron {
            JobTrigger::Cron(expr) => assert_eq!(expr, "0 0 * * *"),
            _ => panic!("Expected Cron variant"),
        }

        let interval = JobTrigger::Interval(300);
        match interval {
            JobTrigger::Interval(secs) => assert_eq!(secs, 300),
            _ => panic!("Expected Interval variant"),
        }

        let now = Utc::now();
        let oneshot = JobTrigger::OneShot(now);
        match oneshot {
            JobTrigger::OneShot(dt) => assert_eq!(dt, now),
            _ => panic!("Expected OneShot variant"),
        }
    }
}
