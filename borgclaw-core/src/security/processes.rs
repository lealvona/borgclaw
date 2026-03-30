use super::{CommandExecutionMode, SecurityError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const PROCESS_STATE_FILE: &str = "processes.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandProcessStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandProcessRecord {
    pub id: String,
    pub command: String,
    pub pid: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: CommandProcessStatus,
    pub exit_code: Option<i32>,
    pub output: String,
    pub pty: bool,
    pub timeout_secs: u64,
    pub yield_ms: Option<u64>,
    pub execution_mode: CommandExecutionMode,
    pub image: Option<String>,
}

pub fn process_state_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(PROCESS_STATE_FILE)
}

pub fn load_process_records(
    path: &Path,
) -> Result<HashMap<String, CommandProcessRecord>, SecurityError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let contents = std::fs::read_to_string(path)
        .map_err(|err| SecurityError::ExecutionError(err.to_string()))?;
    serde_json::from_str(&contents).map_err(|err| SecurityError::ExecutionError(err.to_string()))
}

pub fn save_process_records(
    path: &Path,
    records: &HashMap<String, CommandProcessRecord>,
) -> Result<(), SecurityError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| SecurityError::ExecutionError(err.to_string()))?;
    }
    let payload = serde_json::to_string_pretty(records)
        .map_err(|err| SecurityError::ExecutionError(err.to_string()))?;
    std::fs::write(path, payload).map_err(|err| SecurityError::ExecutionError(err.to_string()))
}

pub fn upsert_process_record(
    path: &Path,
    record: &CommandProcessRecord,
) -> Result<(), SecurityError> {
    let mut records = load_process_records(path)?;
    records.insert(record.id.clone(), record.clone());
    save_process_records(path, &records)
}

pub fn get_process_record(
    path: &Path,
    id: &str,
) -> Result<Option<CommandProcessRecord>, SecurityError> {
    let records = load_process_records(path)?;
    Ok(records.get(id).cloned())
}

#[cfg(unix)]
fn terminate_pid(pid: u32) -> Result<(), SecurityError> {
    let status = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map_err(|err| SecurityError::ExecutionError(err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(SecurityError::ExecutionError(format!(
            "failed to terminate process {}",
            pid
        )))
    }
}

#[cfg(windows)]
fn terminate_pid(pid: u32) -> Result<(), SecurityError> {
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status()
        .map_err(|err| SecurityError::ExecutionError(err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(SecurityError::ExecutionError(format!(
            "failed to terminate process {}",
            pid
        )))
    }
}

pub fn cancel_process_record(path: &Path, id: &str) -> Result<bool, SecurityError> {
    let mut records = load_process_records(path)?;
    let Some(record) = records.get_mut(id) else {
        return Ok(false);
    };

    if record.status != CommandProcessStatus::Running {
        return Ok(false);
    }

    if let Some(pid) = record.pid {
        terminate_pid(pid)?;
    }

    record.status = CommandProcessStatus::Cancelled;
    record.finished_at = Some(Utc::now());
    save_process_records(path, &records)?;
    Ok(true)
}
