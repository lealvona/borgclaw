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
    pub tool_calls: Vec<crate::agent::ToolCall>,
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

    pub fn with_tool_call(mut self, tool_call: crate::agent::ToolCall) -> Self {
        self.tool_calls.push(tool_call);
        self
    }

    pub fn is_important(&self) -> bool {
        !self.tool_calls.is_empty()
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
    /// Compaction pass (incremented each time session is summarized)
    pub compaction_pass: u32,
    /// Running summary from previous conversation context
    pub running_summary: Option<String>,
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
            compaction_pass: 0,
            running_summary: None,
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

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
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
        self.compact_with_recent_and_important(summary, keep_recent, false);
    }

    pub fn compact_with_recent_and_important(
        &mut self,
        summary: &str,
        keep_recent: usize,
        keep_important: bool,
    ) {
        let system = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect::<Vec<_>>();
        let non_system = self
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .cloned()
            .collect::<Vec<_>>();
        let recent_start = non_system.len().saturating_sub(keep_recent);
        let preserved = non_system
            .into_iter()
            .enumerate()
            .filter_map(|(index, message)| {
                let preserve = index >= recent_start || (keep_important && message.is_important());
                preserve.then_some(message)
            })
            .collect::<Vec<_>>();

        self.messages.clear();

        for msg in system {
            self.messages.push_back(msg);
        }

        self.messages.push_back(Message::system(format!(
            "[Previous conversation summarized]: {}",
            summary
        )));

        for msg in preserved {
            self.messages.push_back(msg);
        }
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count().max(1) * 4 / 3
    }

    pub fn total_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| self.count_tokens(&m.content))
            .sum()
    }

    pub fn needs_compaction(&self, token_threshold: usize) -> bool {
        self.total_tokens() > token_threshold
    }

    pub fn get_history_for_summary(&self) -> String {
        self.messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .map(|m| format!("{}: {}", m.role.as_str(), m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn get_context_for_llm(&self) -> Vec<(MessageRole, String)> {
        let mut context = Vec::new();

        if let Some(ref summary) = self.running_summary {
            context.push((
                MessageRole::System,
                format!(
                    "[Previous conversation summary (pass {}): {}]",
                    self.compaction_pass, summary
                ),
            ));
        }

        for msg in self
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
        {
            context.push((msg.role, msg.content.clone()));
        }

        context
    }

    pub fn apply_compaction(&mut self, summary: String, keep_recent: usize, keep_important: bool) {
        self.running_summary = Some(summary);
        self.compaction_pass += 1;
        self.compact_with_recent_and_important("", keep_recent, keep_important);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_new_generates_uuid() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();

        // Should be different UUIDs
        assert_ne!(id1.0, id2.0);

        // Should be valid UUID format (36 chars with dashes)
        assert_eq!(id1.0.len(), 36);
        assert!(id1.0.contains('-'));
    }

    #[test]
    fn session_id_from_string() {
        let id = SessionId::from_string("custom-id-123");
        assert_eq!(id.0, "custom-id-123");
    }

    #[test]
    fn session_id_default_generates_new() {
        let id1: SessionId = Default::default();
        let id2: SessionId = Default::default();

        assert_ne!(id1.0, id2.0);
        assert_eq!(id1.0.len(), 36);
    }

    #[test]
    fn session_id_clone() {
        let id = SessionId::new();
        let cloned = id.clone();
        assert_eq!(id.0, cloned.0);
    }

    #[test]
    fn session_id_equality() {
        let id1 = SessionId::from_string("same");
        let id2 = SessionId::from_string("same");
        let id3 = SessionId::from_string("different");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn message_user_creation() {
        let msg = Message::user("Hello, world!");

        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello, world!");
        assert!(msg.tool_calls.is_empty());
        assert!(!msg.id.is_empty());
        // Timestamp should be recent
        assert!(msg.timestamp <= Utc::now());
    }

    #[test]
    fn message_assistant_creation() {
        let msg = Message::assistant("I'm here to help.");

        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "I'm here to help.");
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn message_system_creation() {
        let msg = Message::system("You are a helpful assistant.");

        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "You are a helpful assistant.");
    }

    #[test]
    fn message_with_tool_call() {
        let mut args = std::collections::HashMap::new();
        args.insert("arg".to_string(), serde_json::json!("value"));

        let tool_call = crate::agent::ToolCall::new("test_tool", args);

        let msg = Message::assistant("Using tool").with_tool_call(tool_call);

        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].name, "test_tool");
    }

    #[test]
    fn message_is_important_with_tool_calls() {
        let tool_call = crate::agent::ToolCall::new("test_tool", std::collections::HashMap::new());

        let msg_with_tool = Message::assistant("Using tool").with_tool_call(tool_call);
        assert!(msg_with_tool.is_important());

        let msg_without_tool = Message::assistant("Just chatting");
        assert!(!msg_without_tool.is_important());
    }

    #[test]
    fn message_role_as_str() {
        assert_eq!(MessageRole::User.as_str(), "User");
        assert_eq!(MessageRole::Assistant.as_str(), "Assistant");
        assert_eq!(MessageRole::System.as_str(), "System");
    }

    #[test]
    fn session_new_creation() {
        let session = Session::new(Some("group-123".to_string()), 100);

        assert!(!session.id.0.is_empty());
        assert_eq!(session.group_id, Some("group-123".to_string()));
        assert!(session.is_empty());
        assert_eq!(session.len(), 0);
        assert_eq!(session.message_count, 0);
        assert_eq!(session.compaction_pass, 0);
        assert!(session.running_summary.is_none());
        assert!(session.metadata.is_empty());
    }

    #[test]
    fn session_new_without_group() {
        let session = Session::new(None, 50);
        assert!(session.group_id.is_none());
    }

    #[test]
    fn session_add_message_increments_count() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::user("Hello"));
        assert_eq!(session.len(), 1);
        assert_eq!(session.message_count, 1);
        assert!(!session.is_empty());

        session.add_message(Message::assistant("Hi there"));
        assert_eq!(session.len(), 2);
        assert_eq!(session.message_count, 2);
    }

    #[test]
    fn session_respects_max_messages() {
        let mut session = Session::new(None, 3);

        session.add_message(Message::user("Message 1"));
        session.add_message(Message::user("Message 2"));
        session.add_message(Message::user("Message 3"));
        assert_eq!(session.len(), 3);

        // Adding a 4th message should evict the oldest
        session.add_message(Message::user("Message 4"));
        assert_eq!(session.len(), 3);

        // First message should be gone
        let messages: Vec<_> = session.messages().iter().collect();
        assert_eq!(messages[0].content, "Message 2");
        assert_eq!(messages[2].content, "Message 4");
    }

    #[test]
    fn session_system_prompt_extraction() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::system("System prompt 1"));
        session.add_message(Message::user("Hello"));
        session.add_message(Message::system("System prompt 2"));

        let prompt = session.system_prompt();
        assert!(prompt.contains("System prompt 1"));
        assert!(prompt.contains("System prompt 2"));
        assert!(!prompt.contains("Hello"));
    }

    #[test]
    fn session_conversation_history() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::system("System"));
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi!"));

        let history = session.conversation_history();
        assert!(!history.contains("System"));
        assert!(history.contains("User: Hello"));
        assert!(history.contains("Assistant: Hi!"));
    }

    #[test]
    fn session_compact_clears_non_system() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::system("System prompt"));
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi!"));

        session.compact("Summary of conversation");

        // Should have system prompt + summary
        assert_eq!(session.len(), 2);

        let messages: Vec<_> = session.messages().iter().collect();
        assert_eq!(messages[0].content, "System prompt");
        assert!(messages[1].content.contains("Summary of conversation"));
    }

    #[test]
    fn session_compact_with_recent_preserves_recent() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::user("Old 1"));
        session.add_message(Message::user("Old 2"));
        session.add_message(Message::user("Recent 1"));
        session.add_message(Message::user("Recent 2"));

        session.compact_with_recent("Summary", 2);

        let messages: Vec<_> = session.messages().iter().collect();
        // Summary + 2 recent messages
        assert_eq!(messages.len(), 3);
        assert!(messages[0].content.contains("Summary"));
        assert_eq!(messages[1].content, "Recent 1");
        assert_eq!(messages[2].content, "Recent 2");
    }

    #[test]
    fn session_count_tokens() {
        let session = Session::new(None, 10);

        // Rough estimation: words * 4/3
        let tokens = session.count_tokens("Hello world");
        assert!(tokens > 0);

        // Empty string should return at least 1
        let tokens = session.count_tokens("");
        assert_eq!(tokens, 1);
    }

    #[test]
    fn session_total_tokens() {
        let mut session = Session::new(None, 10);

        let initial_tokens = session.total_tokens();

        session.add_message(Message::user("Hello world this is a test"));
        let tokens_with_one = session.total_tokens();
        assert!(tokens_with_one > initial_tokens);

        session.add_message(Message::assistant("This is a response message"));
        let tokens_with_two = session.total_tokens();
        assert!(tokens_with_two > tokens_with_one);
    }

    #[test]
    fn session_needs_compaction() {
        let mut session = Session::new(None, 100);

        // Add many messages to exceed threshold
        for i in 0..50 {
            session.add_message(Message::user(&format!(
                "Message {} with lots of words to increase token count",
                i
            )));
        }

        let tokens = session.total_tokens();
        assert!(session.needs_compaction(tokens - 1));
        assert!(!session.needs_compaction(tokens + 1000));
    }

    #[test]
    fn session_get_history_for_summary() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::system("System"));
        session.add_message(Message::user("Question"));
        session.add_message(Message::assistant("Answer"));

        let history = session.get_history_for_summary();
        assert!(!history.contains("System"));
        assert!(history.contains("User: Question"));
        assert!(history.contains("Assistant: Answer"));
    }

    #[test]
    fn session_get_context_for_llm() {
        let mut session = Session::new(None, 10);

        session.running_summary = Some("Previous summary".to_string());
        session.compaction_pass = 1;

        session.add_message(Message::system("System"));
        session.add_message(Message::user("Hello"));
        session.add_message(Message::assistant("Hi!"));

        let context = session.get_context_for_llm();

        // First item should be the summary
        assert_eq!(context[0].0, MessageRole::System);
        assert!(context[0].1.contains("Previous summary"));
        assert!(context[0].1.contains("pass 1"));

        // Should include conversation messages (not system)
        assert_eq!(context.len(), 3); // Summary + user + assistant
    }

    #[test]
    fn session_apply_compaction() {
        let mut session = Session::new(None, 10);

        session.add_message(Message::user("Old message"));
        session.compaction_pass = 0;

        session.apply_compaction("New summary".to_string(), 0, false);

        assert_eq!(session.compaction_pass, 1);
        assert_eq!(session.running_summary, Some("New summary".to_string()));
    }

    #[test]
    fn session_serialization_roundtrip() {
        let mut session = Session::new(Some("group-abc".to_string()), 50);
        session.add_message(Message::system("System prompt"));
        session.add_message(Message::user("Hello"));

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id.0, session.id.0);
        assert_eq!(deserialized.group_id, session.group_id);
        assert_eq!(deserialized.len(), session.len());
        assert_eq!(deserialized.message_count, session.message_count);
    }

    #[test]
    fn message_serialization_roundtrip() {
        let msg = Message::user("Test message");

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.role, msg.role);
        assert_eq!(deserialized.content, msg.content);
        assert_eq!(deserialized.id, msg.id);
    }

    #[test]
    fn message_role_serialization_roundtrip() {
        for role in [
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::System,
        ] {
            let json = serde_json::to_string(&role).unwrap();
            let deserialized: MessageRole = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, role);
        }
    }
}
