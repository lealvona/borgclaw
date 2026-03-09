//! Background sub-agents for parallel task execution

use crate::agent::{builtin_tools, Agent, AgentContext, SenderInfo, SessionId, SimpleAgent};
use crate::config::{AgentConfig, McpConfig, MemoryConfig, SecurityConfig, SkillsConfig};
use crate::memory::{new_entry_for_group, Memory, SqliteMemory};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input: String,
    pub tools_allowed: Vec<String>,
    pub memory_access: MemoryAccessType,
    pub priority: TaskPriority,
    pub timeout_seconds: u64,
    pub created_at: DateTime<Utc>,
    pub status: TaskStatus,
    pub result: Option<SubAgentResult>,
}

impl SubAgentTask {
    pub fn new(name: impl Into<String>, input: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: String::new(),
            input: input.into(),
            tools_allowed: Vec::new(),
            memory_access: MemoryAccessType::ReadOnly,
            priority: TaskPriority::Normal,
            timeout_seconds: 300,
            created_at: Utc::now(),
            status: TaskStatus::Pending,
            result: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_allowed = tools;
        self
    }

    pub fn with_memory_access(mut self, access: MemoryAccessType) -> Self {
        self.memory_access = access;
        self
    }

    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubAgentStatus {
    Pending,
    Running,
    Completed(super::AgentResponse),
    Failed(String),
    Cancelled,
    Timeout(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemoryAccessType {
    None,
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tools_used: Vec<String>,
    pub duration_ms: u64,
    pub memory_entries_created: usize,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct SubAgentCoordinator {
    config: AgentConfig,
    memory_config: MemoryConfig,
    skills_config: SkillsConfig,
    mcp_config: McpConfig,
    security_config: SecurityConfig,
    tasks: Arc<RwLock<HashMap<String, SubAgentTask>>>,
    result_sender: mpsc::Sender<SubAgentResult>,
    max_concurrent: usize,
}

impl SubAgentCoordinator {
    pub fn new(config: AgentConfig, result_sender: mpsc::Sender<SubAgentResult>) -> Self {
        Self::with_configs(
            config,
            MemoryConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            result_sender,
        )
    }

    pub fn with_configs(
        config: AgentConfig,
        memory_config: MemoryConfig,
        skills_config: SkillsConfig,
        mcp_config: McpConfig,
        security_config: SecurityConfig,
        result_sender: mpsc::Sender<SubAgentResult>,
    ) -> Self {
        Self {
            config,
            memory_config,
            skills_config,
            mcp_config,
            security_config,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            result_sender,
            max_concurrent: 5,
        }
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    pub async fn submit(&self, task: SubAgentTask) -> String {
        let id = task.id.clone();
        let mut tasks = self.tasks.write().await;
        tasks.insert(id.clone(), task);
        id
    }

    pub async fn spawn(
        &self,
        name: impl Into<String>,
        ctx: AgentContext,
        priority: TaskPriority,
    ) -> Result<String, SubAgentError> {
        let task = SubAgentTask::new(name, ctx.message).with_priority(priority);
        let task_id = self.submit(task).await;
        let coordinator = self.clone();
        let execute_id = task_id.clone();
        tokio::spawn(async move {
            let _ = coordinator.execute(&execute_id).await;
        });
        Ok(task_id)
    }

    pub async fn get_task(&self, id: &str) -> Option<SubAgentTask> {
        let tasks = self.tasks.read().await;
        tasks.get(id).cloned()
    }

    pub async fn status(&self, id: &str) -> SubAgentStatus {
        let Some(task) = self.get_task(id).await else {
            return SubAgentStatus::Failed(format!("Task not found: {}", id));
        };

        match task.status {
            TaskStatus::Pending => SubAgentStatus::Pending,
            TaskStatus::Running => SubAgentStatus::Running,
            TaskStatus::Completed => SubAgentStatus::Completed(agent_response_from_task(&task)),
            TaskStatus::Failed => SubAgentStatus::Failed(
                task.result
                    .as_ref()
                    .and_then(|result| result.error.clone())
                    .unwrap_or_else(|| "Task failed".to_string()),
            ),
            TaskStatus::Cancelled => SubAgentStatus::Cancelled,
            TaskStatus::Timeout => SubAgentStatus::Timeout(
                task.result
                    .as_ref()
                    .and_then(|result| result.error.clone())
                    .unwrap_or_else(|| "Task timed out".to_string()),
            ),
        }
    }

    pub async fn list_tasks(&self) -> Vec<SubAgentTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    pub async fn list_by_status(&self, status: TaskStatus) -> Vec<SubAgentTask> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.status == status)
            .cloned()
            .collect()
    }

    pub async fn cancel(&self, id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(id) {
            if task.status == TaskStatus::Pending || task.status == TaskStatus::Running {
                task.status = TaskStatus::Cancelled;
                return true;
            }
        }
        false
    }

    pub async fn execute(&self, task_id: &str) -> Result<SubAgentResult, SubAgentError> {
        let task = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(task_id)
                .ok_or_else(|| SubAgentError::NotFound(task_id.to_string()))?;

            if task.status != TaskStatus::Pending {
                return Err(SubAgentError::InvalidState(task.status));
            }

            task.status = TaskStatus::Running;
            task.clone()
        };

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(task.timeout_seconds);

        let result = tokio::time::timeout(timeout, self.execute_task_inner(&task))
            .await
            .unwrap_or_else(|_| Err(SubAgentError::Timeout(task.timeout_seconds)));

        let duration_ms = start.elapsed().as_millis() as u64;

        let (sub_result, status) = match result {
            Ok(output) => (
                SubAgentResult {
                    task_id: task_id.to_string(),
                    success: true,
                    output: output.output,
                    tools_used: output.tools_used,
                    duration_ms,
                    memory_entries_created: output.memory_entries_created,
                    error: None,
                },
                TaskStatus::Completed,
            ),
            Err(e) => {
                let status = match e {
                    SubAgentError::Timeout(_) => TaskStatus::Timeout,
                    _ => TaskStatus::Failed,
                };
                (
                    SubAgentResult {
                        task_id: task_id.to_string(),
                        success: false,
                        output: String::new(),
                        tools_used: vec![],
                        duration_ms,
                        memory_entries_created: 0,
                        error: Some(e.to_string()),
                    },
                    status,
                )
            }
        };

        {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(task_id) {
                t.status = status;
                t.result = Some(sub_result.clone());
            }
        }

        let _ = self.result_sender.send(sub_result.clone()).await;

        if sub_result.success {
            Ok(sub_result)
        } else {
            Err(SubAgentError::ExecutionFailed(
                sub_result
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    async fn execute_task_inner(
        &self,
        task: &SubAgentTask,
    ) -> Result<InnerTaskResult, SubAgentError> {
        let session_id = SessionId::new();
        let ctx = AgentContext {
            session_id: session_id.clone(),
            message: task.input.clone(),
            sender: SenderInfo {
                id: format!("subagent:{}", task.id),
                name: Some(task.name.clone()),
                channel: "subagent".to_string(),
            },
            metadata: HashMap::from([("group_id".to_string(), format!("subagent:{}", task.id))]),
        };

        let mut agent = SimpleAgent::new(
            self.config.clone(),
            Some(self.memory_config.clone()),
            Some(self.skills_config.clone()),
            Some(self.mcp_config.clone()),
            Some(self.security_config.clone()),
        );
        let tools = builtin_tools()
            .into_iter()
            .filter(|tool| {
                task.tools_allowed.is_empty()
                    || task
                        .tools_allowed
                        .iter()
                        .any(|allowed| allowed == &tool.name)
            })
            .collect::<Vec<_>>();
        for tool in tools {
            agent.register_tool(tool);
        }

        let response = agent.process(&ctx).await;
        let tools_used = response
            .tool_calls
            .iter()
            .map(|call| call.name.clone())
            .collect::<Vec<_>>();

        let memory_entries_created = match task.memory_access {
            MemoryAccessType::ReadWrite => {
                let memory = SqliteMemory::new(self.memory_config.database_path.clone());
                memory
                    .init()
                    .await
                    .map_err(|e| SubAgentError::ExecutionFailed(e.to_string()))?;
                memory
                    .store(new_entry_for_group(
                        format!("subagent:{}", task.name),
                        response.text.clone(),
                        format!("subagent:{}", task.id),
                    ))
                    .await
                    .map_err(|e| SubAgentError::ExecutionFailed(e.to_string()))?;
                1
            }
            MemoryAccessType::ReadOnly | MemoryAccessType::None => 0,
        };

        Ok(InnerTaskResult {
            output: response.text,
            tools_used,
            memory_entries_created,
        })
    }

    pub async fn run_pending(&self) -> Vec<Result<SubAgentResult, SubAgentError>> {
        let pending_ids: Vec<String> = {
            let tasks = self.tasks.read().await;
            tasks
                .values()
                .filter(|t| t.status == TaskStatus::Pending)
                .map(|t| t.id.clone())
                .collect()
        };

        let mut results = Vec::new();
        for id in pending_ids {
            results.push(self.execute(&id).await);
        }
        results
    }
}

struct InnerTaskResult {
    output: String,
    tools_used: Vec<String>,
    memory_entries_created: usize,
}

fn agent_response_from_task(task: &SubAgentTask) -> super::AgentResponse {
    let text = task
        .result
        .as_ref()
        .map(|result| result.output.clone())
        .unwrap_or_default();
    super::AgentResponse::text(text)
}

#[derive(Debug, thiserror::Error)]
pub enum SubAgentError {
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Invalid task state: {0:?}")]
    InvalidState(TaskStatus),
    #[error("Task timeout after {0} seconds")]
    Timeout(u64),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Memory access denied")]
    MemoryAccessDenied,
    #[error("Tool not allowed: {0}")]
    ToolNotAllowed(String),
}

pub struct SubAgentBuilder {
    name: String,
    description: String,
    tools_allowed: Vec<String>,
    memory_access: MemoryAccessType,
    priority: TaskPriority,
    timeout_seconds: u64,
}

impl SubAgentBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            tools_allowed: Vec::new(),
            memory_access: MemoryAccessType::ReadOnly,
            priority: TaskPriority::Normal,
            timeout_seconds: 300,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn allow_tool(mut self, tool: impl Into<String>) -> Self {
        self.tools_allowed.push(tool.into());
        self
    }

    pub fn allow_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_allowed = tools;
        self
    }

    pub fn memory_access(mut self, access: MemoryAccessType) -> Self {
        self.memory_access = access;
        self
    }

    pub fn priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }

    pub fn build(self, input: impl Into<String>) -> SubAgentTask {
        SubAgentTask {
            id: Uuid::new_v4().to_string(),
            name: self.name,
            description: self.description,
            input: input.into(),
            tools_allowed: self.tools_allowed,
            memory_access: self.memory_access,
            priority: self.priority,
            timeout_seconds: self.timeout_seconds,
            created_at: Utc::now(),
            status: TaskStatus::Pending,
            result: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subagent_executes_agent_and_records_memory_when_allowed() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_subagent_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let (sender, mut receiver) = mpsc::channel(4);
        let coordinator = SubAgentCoordinator::with_configs(
            AgentConfig {
                provider: "unsupported".to_string(),
                workspace: root.clone(),
                ..Default::default()
            },
            MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );
        let task_id = coordinator
            .submit(
                SubAgentBuilder::new("analysis")
                    .memory_access(MemoryAccessType::ReadWrite)
                    .build("hello"),
            )
            .await;

        let result = coordinator.execute(&task_id).await.unwrap();
        let emitted = receiver.recv().await.unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.memory_entries_created, 1);
        assert_eq!(emitted.task_id, task_id);
    }

    #[tokio::test]
    async fn subagent_spawn_runs_in_background_and_reports_status() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_spawn_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (sender, _receiver) = mpsc::channel(4);
        let coordinator = SubAgentCoordinator::with_configs(
            AgentConfig {
                provider: "unsupported".to_string(),
                workspace: root.clone(),
                ..Default::default()
            },
            MemoryConfig {
                database_path: root.join("memory"),
                ..Default::default()
            },
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );

        let task_id = coordinator
            .spawn(
                "background-summary",
                AgentContext {
                    session_id: SessionId::new(),
                    message: "hello from docs".to_string(),
                    sender: SenderInfo {
                        id: "user-1".to_string(),
                        name: Some("User".to_string()),
                        channel: "cli".to_string(),
                    },
                    metadata: HashMap::new(),
                },
                TaskPriority::Low,
            )
            .await
            .unwrap();

        let final_status = loop {
            match coordinator.status(&task_id).await {
                SubAgentStatus::Pending | SubAgentStatus::Running => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                other => break other,
            }
        };

        std::fs::remove_dir_all(&root).unwrap();
        match final_status {
            SubAgentStatus::Completed(response) => {
                assert!(response.text.contains("Provider 'unsupported'"));
            }
            other => panic!("unexpected status: {:?}", other),
        }
    }
}
