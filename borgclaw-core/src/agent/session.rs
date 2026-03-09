//! Session management for agent conversations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

/// Session ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Conversation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,
    /// Role (user/assistant/system)
    pub role: MessageRole,
    /// Content
    pub content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Tool calls
    pub tool_calls: Vec<super::ToolCall>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.into(),
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: content.into(),
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
        }
    }
}

/// Message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ID
    pub id: SessionId,
    /// Group/conversation ID
    pub group_id: Option<String>,
    /// Messages
    messages: VecDeque<Message>,
    /// Metadata
    pub metadata: std::collections::HashMap<String, String>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Last activity
    pub last_activity: DateTime<Utc>,
    /// Message count
    pub message_count: usize,
    /// Max messages to keep
    max_messages: usize,
}

impl Session {
    pub fn new(group_id: Option<String>, max_messages: usize) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            group_id,
            messages: VecDeque::with_capacity(max_messages),
            metadata: std::collections::HashMap::new(),
            created_at: now,
            last_activity: now,
            message_count: 0,
            max_messages,
        }
    }

    pub fn add_message(&mut self, msg: Message) {
        if self.messages.len() >= self.max_messages {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
        self.last_activity = Utc::now();
        self.message_count += 1;
    }

    pub fn messages(&self) -> &VecDeque<Message> {
        &self.messages
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn system_prompt(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn conversation_history(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| format!("{}: {}", m.role.as_str(), m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Compact session by summarizing old messages
    pub fn compact(&mut self, summary: &str) {
        let system = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect::<Vec<_>>();

        self.messages.clear();

        // Add system messages
        for msg in system {
            self.messages.push_back(msg);
        }

        // Add summary as system message
        self.messages.push_back(Message::system(format!(
            "[Previous conversation summarized]: {}",
            summary
        )));
    }

    pub fn compact_with_recent(&mut self, summary: &str, keep_recent: usize) {
        let system = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect::<Vec<_>>();
        let recent = self
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .rev()
            .take(keep_recent)
            .cloned()
            .collect::<Vec<_>>();

        self.messages.clear();

        for msg in system {
            self.messages.push_back(msg);
        }

        self.messages.push_back(Message::system(format!(
            "[Previous conversation summarized]: {}",
            summary
        )));

        for msg in recent.into_iter().rev() {
            self.messages.push_back(msg);
        }
    }
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::System => "System",
        }
    }
}
