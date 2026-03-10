//! Background sub-agents for parallel task execution

use crate::agent::{builtin_tools, Agent, AgentContext, SenderInfo, SessionId, SimpleAgent};
use crate::config::{
    AgentConfig, McpConfig, MemoryConfig, SchedulerConfig, SecurityConfig, SkillsConfig,
};
use crate::memory::{new_entry_for_group, Memory, SqliteMemory};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentTask {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input: String,
    pub parent_session_id: Option<SessionId>,
    pub parent_sender: Option<SenderInfo>,
    pub parent_metadata: HashMap<String, String>,
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
            parent_session_id: None,
            parent_sender: None,
            parent_metadata: HashMap::new(),
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

    pub fn with_parent_context(mut self, ctx: &AgentContext) -> Self {
        self.parent_session_id = Some(ctx.session_id.clone());
        self.parent_sender = Some(ctx.sender.clone());
        self.parent_metadata = ctx.metadata.clone();
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
    scheduler_config: SchedulerConfig,
    skills_config: SkillsConfig,
    mcp_config: McpConfig,
    security_config: SecurityConfig,
    tasks: Arc<RwLock<HashMap<String, SubAgentTask>>>,
    result_sender: mpsc::Sender<SubAgentResult>,
    max_concurrent: usize,
    permits: Arc<Semaphore>,
    state_path: PathBuf,
}

impl SubAgentCoordinator {
    pub fn new(config: AgentConfig, result_sender: mpsc::Sender<SubAgentResult>) -> Self {
        Self::with_configs(
            config,
            MemoryConfig::default(),
            SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            result_sender,
        )
    }

    pub fn with_configs(
        config: AgentConfig,
        memory_config: MemoryConfig,
        scheduler_config: SchedulerConfig,
        skills_config: SkillsConfig,
        mcp_config: McpConfig,
        security_config: SecurityConfig,
        result_sender: mpsc::Sender<SubAgentResult>,
    ) -> Self {
        let state_path = config.workspace.join("subagents.json");
        Self {
            config,
            memory_config,
            scheduler_config,
            skills_config,
            mcp_config,
            security_config,
            tasks: Arc::new(RwLock::new(load_tasks(&state_path))),
            result_sender,
            max_concurrent: 5,
            permits: Arc::new(Semaphore::new(5)),
            state_path,
        }
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max.max(1);
        self.permits = Arc::new(Semaphore::new(self.max_concurrent));
        self
    }

    pub async fn submit(&self, task: SubAgentTask) -> String {
        let id = task.id.clone();
        let mut tasks = self.tasks.write().await;
        tasks.insert(id.clone(), task);
        persist_tasks(&self.state_path, &tasks);
        id
    }

    pub async fn spawn(
        &self,
        name: impl Into<String>,
        ctx: AgentContext,
        priority: TaskPriority,
    ) -> Result<String, SubAgentError> {
        let task = SubAgentTask::new(name, ctx.message.clone())
            .with_priority(priority)
            .with_parent_context(&ctx);
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
                persist_tasks(&self.state_path, &tasks);
                return true;
            }
        }
        false
    }

    pub async fn execute(&self, task_id: &str) -> Result<SubAgentResult, SubAgentError> {
        let _permit = self
            .permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SubAgentError::ExecutionFailed("Sub-agent limiter closed".to_string()))?;

        let task = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(task_id)
                .ok_or_else(|| SubAgentError::NotFound(task_id.to_string()))?;

            if task.status == TaskStatus::Cancelled {
                return Err(SubAgentError::Cancelled);
            }

            if task.status != TaskStatus::Pending {
                return Err(SubAgentError::InvalidState(task.status));
            }

            task.status = TaskStatus::Running;
            task.clone()
        };
        self.persist_state().await;

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

        let was_cancelled = {
            let mut tasks = self.tasks.write().await;
            if let Some(t) = tasks.get_mut(task_id) {
                if t.status == TaskStatus::Cancelled {
                    t.result = Some(SubAgentResult {
                        task_id: task_id.to_string(),
                        success: false,
                        output: String::new(),
                        tools_used: vec![],
                        duration_ms,
                        memory_entries_created: 0,
                        error: Some("Task cancelled".to_string()),
                    });
                    true
                } else {
                    t.status = status;
                    t.result = Some(sub_result.clone());
                    false
                }
            } else {
                false
            }
        };
        self.persist_state().await;

        if was_cancelled {
            let cancelled = SubAgentResult {
                task_id: task_id.to_string(),
                success: false,
                output: String::new(),
                tools_used: vec![],
                duration_ms,
                memory_entries_created: 0,
                error: Some("Task cancelled".to_string()),
            };
            let _ = self.result_sender.send(cancelled).await;
            return Err(SubAgentError::Cancelled);
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
        if let Some(delay_ms) = test_delay_ms(task) {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }

        let session_id = task.parent_session_id.clone().unwrap_or_default();
        let ctx = AgentContext {
            session_id,
            message: task.input.clone(),
            sender: task.parent_sender.clone().unwrap_or_else(|| SenderInfo {
                id: format!("subagent:{}", task.id),
                name: Some(task.name.clone()),
                channel: "subagent".to_string(),
            }),
            metadata: task_metadata(task),
        };

        let mut agent = SimpleAgent::new(
            self.config.clone(),
            Some(self.memory_config.clone()),
            Some(self.scheduler_config.clone()),
            Some(self.skills_config.clone()),
            Some(self.mcp_config.clone()),
            Some(self.security_config.clone()),
        );
        let tools = allowed_builtin_tools(task);
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
                        task_group_id(task),
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

    async fn persist_state(&self) {
        let tasks = self.tasks.read().await;
        persist_tasks(&self.state_path, &tasks);
    }
}

struct InnerTaskResult {
    output: String,
    tools_used: Vec<String>,
    memory_entries_created: usize,
}

fn allowed_builtin_tools(task: &SubAgentTask) -> Vec<crate::agent::Tool> {
    builtin_tools()
        .into_iter()
        .filter(|tool| task_allows_tool(task, &tool.name))
        .collect()
}

fn task_metadata(task: &SubAgentTask) -> HashMap<String, String> {
    let mut metadata = task.parent_metadata.clone();
    metadata
        .entry("subagent_task_id".to_string())
        .or_insert_with(|| task.id.clone());
    metadata
        .entry("subagent_task_name".to_string())
        .or_insert_with(|| task.name.clone());
    metadata
        .entry("group_id".to_string())
        .or_insert_with(|| task_group_id(task));
    metadata
}

fn task_group_id(task: &SubAgentTask) -> String {
    task.parent_metadata
        .get("group_id")
        .cloned()
        .unwrap_or_else(|| format!("subagent:{}", task.id))
}

fn task_allows_tool(task: &SubAgentTask, tool_name: &str) -> bool {
    if !task.tools_allowed.is_empty()
        && !task
            .tools_allowed
            .iter()
            .any(|allowed| allowed == tool_name)
    {
        return false;
    }

    match task.memory_access {
        MemoryAccessType::None => !matches!(tool_name, "memory_store" | "memory_recall"),
        MemoryAccessType::ReadOnly => tool_name != "memory_store",
        MemoryAccessType::ReadWrite => true,
    }
}

fn test_delay_ms(task: &SubAgentTask) -> Option<u64> {
    task.description
        .strip_prefix("__borgclaw_test_delay_ms=")
        .and_then(|value| value.parse::<u64>().ok())
}

fn agent_response_from_task(task: &SubAgentTask) -> super::AgentResponse {
    let text = task
        .result
        .as_ref()
        .map(|result| result.output.clone())
        .unwrap_or_default();
    super::AgentResponse::text(text)
}

fn load_tasks(path: &PathBuf) -> HashMap<String, SubAgentTask> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}

fn persist_tasks(path: &PathBuf, tasks: &HashMap<String, SubAgentTask>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let Ok(serialized) = serde_json::to_string_pretty(tasks) else {
        return;
    };
    let temp_path = path.with_extension("json.tmp");
    if std::fs::write(&temp_path, serialized).is_ok() {
        let _ = std::fs::rename(temp_path, path);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SubAgentError {
    #[error("Task not found: {0}")]
    NotFound(String),
    #[error("Invalid task state: {0:?}")]
    InvalidState(TaskStatus),
    #[error("Task cancelled")]
    Cancelled,
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
            parent_session_id: None,
            parent_sender: None,
            parent_metadata: HashMap::new(),
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
    use crate::memory::MemoryQuery;

    #[test]
    fn subagent_memory_access_filters_memory_tools() {
        let none = SubAgentBuilder::new("none")
            .memory_access(MemoryAccessType::None)
            .build("hello");
        let read_only = SubAgentBuilder::new("read_only")
            .memory_access(MemoryAccessType::ReadOnly)
            .build("hello");
        let read_write = SubAgentBuilder::new("read_write")
            .memory_access(MemoryAccessType::ReadWrite)
            .build("hello");

        let none_tools = allowed_builtin_tools(&none)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        let read_only_tools = allowed_builtin_tools(&read_only)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();
        let read_write_tools = allowed_builtin_tools(&read_write)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(!none_tools.iter().any(|name| name == "memory_store"));
        assert!(!none_tools.iter().any(|name| name == "memory_recall"));
        assert!(!read_only_tools.iter().any(|name| name == "memory_store"));
        assert!(read_only_tools.iter().any(|name| name == "memory_recall"));
        assert!(read_write_tools.iter().any(|name| name == "memory_store"));
        assert!(read_write_tools.iter().any(|name| name == "memory_recall"));
    }

    #[test]
    fn subagent_explicit_tool_allowlist_still_respects_memory_policy() {
        let read_only = SubAgentBuilder::new("read_only")
            .memory_access(MemoryAccessType::ReadOnly)
            .allow_tools(vec!["memory_store".to_string(), "memory_recall".to_string()])
            .build("hello");

        let allowed = allowed_builtin_tools(&read_only)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>();

        assert!(!allowed.iter().any(|name| name == "memory_store"));
        assert!(allowed.iter().any(|name| name == "memory_recall"));
    }

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
            crate::config::SchedulerConfig::default(),
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
            crate::config::SchedulerConfig::default(),
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

    #[tokio::test]
    async fn subagent_spawn_inherits_parent_context() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_context_test_{}",
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
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );
        let session_id = SessionId::new();
        let task_id = coordinator
            .spawn(
                "background-summary",
                AgentContext {
                    session_id: session_id.clone(),
                    message: "hello from docs".to_string(),
                    sender: SenderInfo {
                        id: "user-1".to_string(),
                        name: Some("User".to_string()),
                        channel: "cli".to_string(),
                    },
                    metadata: HashMap::from([
                        ("group_id".to_string(), "parent-group".to_string()),
                        ("source".to_string(), "cli".to_string()),
                    ]),
                },
                TaskPriority::Low,
            )
            .await
            .unwrap();

        let task = coordinator.get_task(&task_id).await.unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(task.parent_session_id, Some(session_id));
        assert_eq!(task.parent_sender.unwrap().id, "user-1");
        assert_eq!(
            task.parent_metadata.get("group_id").map(String::as_str),
            Some("parent-group")
        );
    }

    #[tokio::test]
    async fn subagent_enforces_max_concurrent_limit() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_limit_test_{}",
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
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        )
        .with_max_concurrent(1);

        let first = coordinator
            .submit(
                SubAgentBuilder::new("slow")
                    .description("__borgclaw_test_delay_ms=150")
                    .build("first"),
            )
            .await;
        let second = coordinator
            .submit(SubAgentBuilder::new("fast").build("second"))
            .await;

        let first_task = {
            let coordinator = coordinator.clone();
            let first = first.clone();
            tokio::spawn(async move { coordinator.execute(&first).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let second_task = {
            let coordinator = coordinator.clone();
            let second = second.clone();
            tokio::spawn(async move { coordinator.execute(&second).await })
        };

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(matches!(
            coordinator.status(&first).await,
            SubAgentStatus::Running
        ));
        assert!(matches!(
            coordinator.status(&second).await,
            SubAgentStatus::Pending
        ));

        let _ = first_task.await.unwrap().unwrap();
        let _ = second_task.await.unwrap().unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn subagent_cancelled_task_keeps_cancelled_status() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_cancel_test_{}",
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
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );

        let task_id = coordinator
            .submit(
                SubAgentBuilder::new("slow")
                    .description("__borgclaw_test_delay_ms=150")
                    .build("hello"),
            )
            .await;

        let handle = {
            let coordinator = coordinator.clone();
            let task_id = task_id.clone();
            tokio::spawn(async move { coordinator.execute(&task_id).await })
        };

        loop {
            if matches!(coordinator.status(&task_id).await, SubAgentStatus::Running) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(coordinator.cancel(&task_id).await);
        let result = handle.await.unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(matches!(result, Err(SubAgentError::Cancelled)));
        assert!(matches!(
            coordinator.status(&task_id).await,
            SubAgentStatus::Cancelled
        ));
    }

    #[tokio::test]
    async fn subagent_readwrite_memory_uses_parent_group_id_when_present() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_parent_group_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (sender, _receiver) = mpsc::channel(4);
        let memory_path = root.join("memory");
        let coordinator = SubAgentCoordinator::with_configs(
            AgentConfig {
                provider: "unsupported".to_string(),
                workspace: root.clone(),
                ..Default::default()
            },
            MemoryConfig {
                database_path: memory_path.clone(),
                ..Default::default()
            },
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );
        let task_id = coordinator
            .submit(
                SubAgentBuilder::new("analysis")
                    .memory_access(MemoryAccessType::ReadWrite)
                    .build("hello")
                    .with_parent_context(&AgentContext {
                        session_id: SessionId::new(),
                        message: "hello".to_string(),
                        sender: SenderInfo {
                            id: "user-1".to_string(),
                            name: Some("User".to_string()),
                            channel: "cli".to_string(),
                        },
                        metadata: HashMap::from([(
                            "group_id".to_string(),
                            "parent-group".to_string(),
                        )]),
                    }),
            )
            .await;

        let result = coordinator.execute(&task_id).await.unwrap();
        let memory = SqliteMemory::new(memory_path);
        memory.init().await.unwrap();
        let recalled = memory
            .recall(&MemoryQuery {
                query: "Provider unsupported".to_string(),
                group_id: Some("parent-group".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.success);
        assert_eq!(result.memory_entries_created, 1);
        assert_eq!(recalled.len(), 1);
    }

    #[tokio::test]
    async fn subagent_persists_tasks_across_reconstruction() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_subagent_persist_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let (sender, _receiver) = mpsc::channel(4);
        let config = AgentConfig {
            provider: "unsupported".to_string(),
            workspace: root.clone(),
            ..Default::default()
        };
        let memory = MemoryConfig {
            database_path: root.join("memory"),
            ..Default::default()
        };

        let coordinator = SubAgentCoordinator::with_configs(
            config.clone(),
            memory.clone(),
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender,
        );
        let task_id = coordinator
            .submit(SubAgentBuilder::new("persisted").build("hello"))
            .await;
        let _ = coordinator.execute(&task_id).await;

        let (sender2, _receiver2) = mpsc::channel(4);
        let reconstructed = SubAgentCoordinator::with_configs(
            config,
            memory,
            crate::config::SchedulerConfig::default(),
            SkillsConfig::default(),
            McpConfig::default(),
            SecurityConfig::default(),
            sender2,
        );

        let task = reconstructed.get_task(&task_id).await.unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.result.is_some());
    }
}
