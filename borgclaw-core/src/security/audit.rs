//! Security audit logging for compliance and monitoring

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Types of security events that can be audited
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Tool was executed
    ToolExecution,
    /// Tool execution was approved
    ToolApproved,
    /// Tool execution was denied
    ToolDenied,
    /// Command was executed
    CommandExecution,
    /// Command was blocked by policy
    CommandBlocked,
    /// MCP server was called
    McpCall,
    /// Plugin was invoked
    PluginInvocation,
    /// Secret was accessed
    SecretAccess,
    /// Pairing request
    PairingRequest,
    /// Pairing completed
    PairingCompleted,
    /// Authentication failure
    AuthFailure,
    /// Rate limit hit
    RateLimitHit,
}

/// A single audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Type of event
    pub event_type: AuditEventType,
    /// User or session that triggered the event
    pub actor: String,
    /// Resource being acted upon (tool name, command, etc.)
    pub resource: String,
    /// Action taken
    pub action: String,
    /// Whether the action succeeded
    pub success: bool,
    /// Additional context
    pub metadata: HashMap<String, String>,
}

impl AuditEntry {
    pub fn new(
        event_type: AuditEventType,
        actor: impl Into<String>,
        resource: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            actor: actor.into(),
            resource: resource.into(),
            action: action.into(),
            success: true,
            metadata: HashMap::new(),
        }
    }

    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Configuration for audit logging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    /// Whether audit logging is enabled
    pub enabled: bool,
    /// Path to audit log file
    pub log_path: PathBuf,
    /// Maximum number of entries to keep in memory before flushing
    pub buffer_size: usize,
    /// Whether to include full metadata in logs
    pub verbose: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_path: PathBuf::from(".local/logs/audit.jsonl"),
            buffer_size: 100,
            verbose: false,
        }
    }
}

/// Audit logger for security events
pub struct AuditLogger {
    config: AuditConfig,
    buffer: Arc<Mutex<Vec<AuditEntry>>>,
}

impl AuditLogger {
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn disabled() -> Self {
        Self::new(AuditConfig {
            enabled: false,
            ..Default::default()
        })
    }

    /// Log a security event
    pub async fn log(&self, entry: AuditEntry) {
        if !self.config.enabled {
            return;
        }

        let mut buffer = self.buffer.lock().await;
        buffer.push(entry);

        if buffer.len() >= self.config.buffer_size {
            drop(buffer);
            let _ = self.flush().await;
        }
    }

    /// Log tool execution
    pub async fn log_tool_execution(
        &self,
        actor: &str,
        tool_name: &str,
        success: bool,
        details: Option<&str>,
    ) {
        let entry = AuditEntry::new(
            AuditEventType::ToolExecution,
            actor,
            tool_name,
            if success { "executed" } else { "failed" },
        )
        .with_success(success);

        let entry = if let Some(details) = details {
            entry.with_metadata("details", details)
        } else {
            entry
        };

        self.log(entry).await;
    }

    /// Log approval decision
    pub async fn log_approval_decision(
        &self,
        actor: &str,
        tool_name: &str,
        approved: bool,
        reason: Option<&str>,
    ) {
        let entry = AuditEntry::new(
            if approved {
                AuditEventType::ToolApproved
            } else {
                AuditEventType::ToolDenied
            },
            actor,
            tool_name,
            if approved { "approved" } else { "denied" },
        )
        .with_success(approved);

        let entry = if let Some(reason) = reason {
            entry.with_metadata("reason", reason)
        } else {
            entry
        };

        self.log(entry).await;
    }

    /// Log command execution
    pub async fn log_command(&self, actor: &str, command: &str, blocked: bool, success: bool) {
        let entry = AuditEntry::new(
            if blocked {
                AuditEventType::CommandBlocked
            } else {
                AuditEventType::CommandExecution
            },
            actor,
            command,
            if blocked {
                "blocked"
            } else if success {
                "executed"
            } else {
                "failed"
            },
        )
        .with_success(!blocked && success);

        self.log(entry).await;
    }

    /// Flush buffered entries to disk
    pub async fn flush(&self) -> Result<(), AuditError> {
        if !self.config.enabled {
            return Ok(());
        }

        let mut buffer = self.buffer.lock().await;
        if buffer.is_empty() {
            return Ok(());
        }

        // Ensure parent directory exists
        if let Some(parent) = self.config.log_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AuditError::IoError(e.to_string()))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log_path)
            .await
            .map_err(|e| AuditError::IoError(e.to_string()))?;

        for entry in buffer.drain(..) {
            let line = serde_json::to_string(&entry)
                .map_err(|e| AuditError::SerializeError(e.to_string()))?;
            file.write_all(line.as_bytes())
                .await
                .map_err(|e| AuditError::IoError(e.to_string()))?;
            file.write_all(b"\n")
                .await
                .map_err(|e| AuditError::IoError(e.to_string()))?;
        }

        file.flush()
            .await
            .map_err(|e| AuditError::IoError(e.to_string()))?;

        Ok(())
    }

    /// Get recent audit entries from buffer
    pub async fn recent_entries(&self, limit: usize) -> Vec<AuditEntry> {
        let buffer = self.buffer.lock().await;
        buffer.iter().rev().take(limit).cloned().collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    #[error("IO error: {0}")]
    IoError(String),

    #[error("Serialization error: {0}")]
    SerializeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_entry_creation() {
        let entry = AuditEntry::new(
            AuditEventType::ToolExecution,
            "user123",
            "execute_command",
            "executed",
        )
        .with_success(true)
        .with_metadata("command", "ls -la");

        assert_eq!(entry.event_type, AuditEventType::ToolExecution);
        assert_eq!(entry.actor, "user123");
        assert_eq!(entry.resource, "execute_command");
        assert_eq!(entry.action, "executed");
        assert!(entry.success);
        assert_eq!(entry.metadata.get("command"), Some(&"ls -la".to_string()));
    }

    #[tokio::test]
    async fn audit_logger_buffers_entries() {
        let config = AuditConfig {
            enabled: true,
            log_path: PathBuf::from("/tmp/test_audit.jsonl"),
            buffer_size: 10,
            verbose: false,
        };

        let logger = AuditLogger::new(config);

        logger
            .log_tool_execution("user1", "test_tool", true, None)
            .await;

        let recent = logger.recent_entries(5).await;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].actor, "user1");
    }
}
