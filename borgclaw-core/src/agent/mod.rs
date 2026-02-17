//! Agent core module - handles agent loop, tools, and session management

mod session;
mod tools;

pub use session::{Session, SessionId};
pub use tools::{builtin_tools, Tool, ToolCall, ToolResult, ToolSchema};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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
    tools: Vec<Tool>,
    state: AgentState,
    session: Option<Session>,
}

impl SimpleAgent {
    pub fn new(config: super::config::AgentConfig) -> Self {
        Self {
            config,
            tools: Vec::new(),
            state: AgentState::Idle,
            session: None,
        }
    }
    
    pub fn register_tool(&mut self, tool: Tool) {
        self.tools.push(tool);
    }
}

#[async_trait]
impl Agent for SimpleAgent {
    async fn process(&mut self, ctx: &AgentContext) -> AgentResponse {
        self.state = AgentState::Processing;
        
        // Simple echo response for now - will integrate with LLM later
        let response = AgentResponse::text(format!("Received: {}", ctx.message));
        
        self.state = AgentState::Idle;
        response
    }
    
    fn tools(&self) -> &[Tool] {
        &self.tools
    }
    
    async fn configure(&mut self, config: &super::config::AgentConfig) -> Result<(), AgentError> {
        self.config = config.clone();
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
