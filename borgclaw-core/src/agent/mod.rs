//! Agent core module - handles agent loop, tools, and session management

mod provider;
mod session;
mod subagent;
mod tools;

pub use provider::{ChatMessage, ChatProvider, ProviderFactory, ProviderRequest};
pub use session::{Message, MessageRole, Session, SessionId};
pub use subagent::{
    SubAgentBuilder, SubAgentCoordinator, SubAgentError, SubAgentResult, SubAgentTask,
    MemoryAccessType, TaskPriority, TaskStatus,
};
pub use tools::{builtin_tools, execute_tool, parse_tool_command, Tool, ToolCall, ToolResult, ToolRuntime, ToolSchema};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent trait - implemented by agent backends
#[async_trait]
pub trait Agent: Send + Sync {
    /// Process a message and return response
    async fn process(&mut self, ctx: &AgentContext) -> AgentResponse;
    
    /// Get available tools
    fn tools(&self) -> &[Tool];
    
    /// Update agent configuration
    async fn configure(&mut self, config: &super::config::AgentConfig) -> Result<(), AgentError>;
    
    /// Get agent state
    fn state(&self) -> AgentState;
    
    /// Shutdown agent
    async fn shutdown(&mut self) -> Result<(), AgentError>;
}

/// Agent context - input for agent processing
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// Session ID
    pub session_id: SessionId,
    /// User message
    pub message: String,
    /// Channel sender info
    pub sender: SenderInfo,
    /// Additional context
    pub metadata: HashMap<String, String>,
}

/// Sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderInfo {
    /// Unique sender ID
    pub id: String,
    /// Display name
    pub name: Option<String>,
    /// Channel type
    pub channel: String,
}

/// Agent response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// Response text
    pub text: String,
    /// Tool calls made
    pub tool_calls: Vec<ToolCall>,
    /// Session updates
    pub session_updates: HashMap<String, String>,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

impl AgentResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tool_calls: Vec::new(),
            session_updates: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Agent state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Processing,
    WaitingForApproval,
    Error(String),
}

/// Agent events for monitoring
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Message received
    MessageReceived(AgentContext),
    /// Tool execution started
    ToolStarted(String),
    /// Tool execution completed
    ToolCompleted(String, ToolResult),
    /// Error occurred
    Error(String),
    /// Session created
    SessionCreated(SessionId),
    /// Session ended
    SessionEnded(SessionId),
}

/// Agent errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Tool execution failed: {0}")]
    ToolFailed(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
    #[error("Rate limited: {0}")]
    RateLimited(String),
}

/// Tool call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: HashMap<String, serde_json::Value>,
    /// Approval token (if required)
    pub approval_token: Option<String>,
}

/// Simple in-memory agent implementation
pub struct SimpleAgent {
    config: super::config::AgentConfig,
    memory_config: super::config::MemoryConfig,
    security_config: super::config::SecurityConfig,
    tools: Vec<Tool>,
    state: AgentState,
    sessions: HashMap<String, Session>,
    provider: Option<Box<dyn ChatProvider>>,
    tool_runtime: Option<ToolRuntime>,
}

impl SimpleAgent {
    pub fn new(
        config: super::config::AgentConfig,
        memory_config: Option<super::config::MemoryConfig>,
        security_config: Option<super::config::SecurityConfig>,
    ) -> Self {
        let provider = ProviderFactory::create(&config).ok();
        Self {
            config,
            memory_config: memory_config.unwrap_or_default(),
            security_config: security_config.unwrap_or_default(),
            tools: Vec::new(),
            state: AgentState::Idle,
            sessions: HashMap::new(),
            provider,
            tool_runtime: None,
        }
    }
    
    pub fn register_tool(&mut self, tool: Tool) {
        self.tools.push(tool);
    }

    fn system_prompt(&self) -> Option<String> {
        let path = self.config.soul_path.as_ref()?;
        std::fs::read_to_string(path).ok()
    }

    fn ensure_session(&mut self, session_id: &SessionId, group_id: Option<String>) -> &mut Session {
        self.sessions
            .entry(session_id.0.clone())
            .or_insert_with(|| Session::new(group_id, self.config.max_tokens.clamp(32, 512) as usize))
    }

    async fn ensure_tool_runtime(&mut self) -> Result<&ToolRuntime, AgentError> {
        if self.tool_runtime.is_none() {
            let runtime = ToolRuntime::from_config(
                &self.config,
                &self.memory_config,
                &self.security_config,
            )
            .await
            .map_err(AgentError::ConfigError)?;
            self.tool_runtime = Some(runtime);
        }

        Ok(self.tool_runtime.as_ref().expect("tool runtime initialized"))
    }

    fn compact_session_if_needed(threshold: usize, session: &mut Session) {
        let threshold = threshold.max(4);
        if session.len() <= threshold {
            return;
        }

        let non_system: Vec<_> = session
            .messages()
            .iter()
            .filter(|msg| msg.role != MessageRole::System)
            .cloned()
            .collect();
        if non_system.len() <= 4 {
            return;
        }

        let keep_recent = (threshold / 2).max(4);
        let removed = non_system.len().saturating_sub(keep_recent);
        if removed == 0 {
            return;
        }

        let summary = format!(
            "{} prior messages summarized. Recent topics: {}",
            removed,
            non_system
                .iter()
                .take(3)
                .map(|msg| msg.content.split_whitespace().take(6).collect::<Vec<_>>().join(" "))
                .filter(|topic| !topic.is_empty())
                .collect::<Vec<_>>()
                .join("; ")
        );

        session.compact_with_recent(&summary, keep_recent);
    }
}

#[async_trait]
impl Agent for SimpleAgent {
    async fn process(&mut self, ctx: &AgentContext) -> AgentResponse {
        self.state = AgentState::Processing;
        if self.provider.is_none() {
            self.state = AgentState::Error("provider initialization failed".to_string());
            return AgentResponse::text(format!(
                "Provider '{}' is not configured or is unsupported in the current runtime.",
                self.config.provider
            ));
        }

        let parsed_tool_call = parse_tool_command(&ctx.message, &self.tools);
        let system_prompt = self.system_prompt();
        let model = self.config.model.clone();
        let temperature = self.config.temperature;
        let max_tokens = self.config.max_tokens;
        let session = self.ensure_session(&ctx.session_id, ctx.metadata.get("group_id").cloned());
        if let Some(prompt) = system_prompt {
            let has_system = session
                .messages()
                .iter()
                .any(|msg| msg.role == MessageRole::System && msg.content == prompt);
            if !has_system {
                session.add_message(Message::system(prompt));
            }
        }
        session.add_message(Message::user(ctx.message.clone()));

        if let Some(call) = parsed_tool_call {
            let runtime = match self.ensure_tool_runtime().await {
                Ok(runtime) => runtime,
                Err(err) => {
                    self.state = AgentState::Error(err.to_string());
                    return AgentResponse::text(format!("Tool runtime error: {}", err));
                }
            };
            let result = execute_tool(&call, runtime).await;
            let response_text = result.output.clone();
            let session = self.ensure_session(&ctx.session_id, ctx.metadata.get("group_id").cloned());
            session.add_message(Message::assistant(response_text.clone()));
            self.state = AgentState::Idle;
            return AgentResponse {
                text: response_text,
                tool_calls: vec![call.with_result(result)],
                session_updates: HashMap::new(),
                metadata: HashMap::new(),
            };
        }

        {
            let threshold = self.memory_config.session_compaction_threshold;
            let session = self.ensure_session(&ctx.session_id, ctx.metadata.get("group_id").cloned());
            Self::compact_session_if_needed(threshold, session);
        }
        let request_messages = self
            .ensure_session(&ctx.session_id, ctx.metadata.get("group_id").cloned())
            .messages()
            .iter()
            .map(ChatMessage::from)
            .collect();
        let request = ProviderRequest {
            model,
            temperature,
            max_tokens,
            messages: request_messages,
        };
        let provider = self.provider.as_ref().expect("provider checked above");

        let response = match provider.complete(&request).await {
            Ok(text) => {
                let session = self.ensure_session(&ctx.session_id, ctx.metadata.get("group_id").cloned());
                session.add_message(Message::assistant(text.clone()));
                self.state = AgentState::Idle;
                AgentResponse::text(text)
            }
            Err(err) => {
                self.state = AgentState::Error(err.to_string());
                AgentResponse::text(format!("Provider error: {}", err))
            }
        };

        if matches!(self.state, AgentState::Processing) {
            self.state = AgentState::Idle;
        }
        response
    }
    
    fn tools(&self) -> &[Tool] {
        &self.tools
    }
    
    async fn configure(&mut self, config: &super::config::AgentConfig) -> Result<(), AgentError> {
        self.config = config.clone();
        self.provider = ProviderFactory::create(config).ok();
        Ok(())
    }
    
    fn state(&self) -> AgentState {
        self.state.clone()
    }
    
    async fn shutdown(&mut self) -> Result<(), AgentError> {
        self.state = AgentState::Idle;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_keeps_recent_messages_and_summary() {
        let mut session = Session::new(None, 64);
        session.add_message(Message::system("system"));
        for index in 0..8 {
            session.add_message(Message::user(format!("user {}", index)));
            session.add_message(Message::assistant(format!("assistant {}", index)));
        }

        SimpleAgent::compact_session_if_needed(6, &mut session);

        let messages = session.messages().iter().collect::<Vec<_>>();
        assert!(messages.iter().any(|msg| msg.content.contains("summarized")));
        assert!(messages.iter().any(|msg| msg.content == "assistant 7"));
        assert!(messages.iter().any(|msg| msg.content == "user 7"));
        assert!(!messages.iter().any(|msg| msg.content == "user 0"));
    }
}
