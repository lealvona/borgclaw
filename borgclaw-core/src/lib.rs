//! BorgClaw Core - Personal AI Agent Framework
//!
//! A modular, secure personal AI assistant combining the best features
//! from OpenClaw-family frameworks.

pub mod agent;
pub mod channel;
pub mod config;
pub mod constants;
pub mod fallback;
pub mod mcp;
pub mod memory;
pub mod scheduler;
pub mod security;
pub mod skills;

pub use constants::*;

pub use agent::{builtin_tools, Agent, AgentEvent, SimpleAgent, Tool, ToolResult};
pub use channel::{Channel, ChannelSender};
pub use config::{AppConfig, ChannelConfig, SecurityConfig};
pub use fallback::FallbackDeliverable;
pub use memory::{Memory, MemoryEntry};
pub use scheduler::{Job, Scheduler, SchedulerTrait};
pub use security::{
    BitwardenClient, BitwardenConfig, OnePasswordClient, OnePasswordConfig, PairingManager,
    SecretStore, SecurityLayer, VaultClient, VaultError,
};

use std::sync::Arc;
use tokio::sync::RwLock;

/// BorgClaw application state
pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub agent: Arc<RwLock<Option<Box<dyn Agent>>>>,
    pub memory: Arc<RwLock<Option<Box<dyn Memory>>>>,
    pub scheduler: Arc<RwLock<Option<Box<dyn SchedulerTrait>>>>,
    pub security: Arc<SecurityLayer>,
    pub channels: Arc<RwLock<Vec<Box<dyn Channel>>>>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            agent: Arc::new(RwLock::new(None)),
            memory: Arc::new(RwLock::new(None)),
            scheduler: Arc::new(RwLock::new(None)),
            security: Arc::new(SecurityLayer::new()),
            channels: Arc::new(RwLock::new(Vec::new())),
        }
    }
}
