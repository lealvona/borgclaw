//! Session memory with auto-compaction support

use super::MemoryError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const DEFAULT_COMPACTION_THRESHOLD: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub token_count: usize,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

impl SessionMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: content.into(),
            timestamp: Utc::now(),
            token_count: 0,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: Utc::now(),
            token_count: 0,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::System,
            content: content.into(),
            timestamp: Utc::now(),
            token_count: 0,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_tokens(mut self, count: usize) -> Self {
        self.token_count = count;
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn mark_important(mut self) -> Self {
        self.metadata
            .insert("important".to_string(), "true".to_string());
        self
    }

    pub fn is_important(&self) -> bool {
        self.metadata
            .get("important")
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }
}

pub struct SessionMemory {
    session_id: String,
    messages: VecDeque<SessionMessage>,
    compaction_threshold: usize,
    total_tokens: usize,
    compactions: u32,
    summary: Option<String>,
}

impl SessionMemory {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            messages: VecDeque::new(),
            compaction_threshold: DEFAULT_COMPACTION_THRESHOLD,
            total_tokens: 0,
            compactions: 0,
            summary: None,
        }
    }

    pub fn with_compaction_threshold(mut self, threshold: usize) -> Self {
        self.compaction_threshold = threshold;
        self
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    pub fn compactions(&self) -> u32 {
        self.compactions
    }

    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    pub fn push(&mut self, message: SessionMessage) {
        self.total_tokens += message.token_count;
        self.messages.push_back(message);
    }

    pub fn push_user(&mut self, content: impl Into<String>) {
        self.push(SessionMessage::user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<String>) {
        self.push(SessionMessage::assistant(content));
    }

    pub fn messages(&self) -> impl Iterator<Item = &SessionMessage> {
        self.messages.iter()
    }

    pub fn last_n(&self, n: usize) -> impl Iterator<Item = &SessionMessage> {
        let start = self.messages.len().saturating_sub(n);
        self.messages.iter().skip(start)
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.total_tokens = 0;
    }

    pub fn set_summary(&mut self, summary: impl Into<String>) {
        self.summary = Some(summary.into());
    }
}

pub struct SessionCompactor {
    keep_recent: usize,
    keep_important: bool,
}

impl SessionCompactor {
    pub fn new() -> Self {
        Self {
            keep_recent: 10,
            keep_important: true,
        }
    }

    pub fn with_keep_recent(mut self, n: usize) -> Self {
        self.keep_recent = n;
        self
    }

    pub fn keep_recent(self, n: usize) -> Self {
        self.with_keep_recent(n)
    }

    pub fn with_keep_important(mut self, keep_important: bool) -> Self {
        self.keep_important = keep_important;
        self
    }

    pub fn keep_important(self, keep_important: bool) -> Self {
        self.with_keep_important(keep_important)
    }

    pub fn should_compact(&self, session: &SessionMemory) -> bool {
        session.len() > session.compaction_threshold
    }

    pub fn compact(&self, session: &mut SessionMemory) -> Result<CompactionResult, MemoryError> {
        if session.len() <= self.keep_recent {
            return Ok(CompactionResult {
                messages_removed: 0,
                tokens_saved: 0,
                summary: None,
            });
        }

        let mut tokens_saved = 0;
        let recent_start = session.len().saturating_sub(self.keep_recent);
        let mut removed_messages = Vec::new();
        let mut preserved_messages = Vec::new();

        for (index, message) in session.messages.drain(..).enumerate() {
            let preserve = index >= recent_start || (self.keep_important && message.is_important());
            if preserve {
                preserved_messages.push(message);
            } else {
                tokens_saved += message.token_count;
                removed_messages.push(message);
            }
        }
        let messages_removed = removed_messages.len();
        session.messages = preserved_messages.into();

        let summary = if !removed_messages.is_empty() {
            Some(self.generate_summary(&removed_messages))
        } else {
            None
        };

        session.total_tokens = session.total_tokens.saturating_sub(tokens_saved);
        session.compactions += 1;
        if let Some(ref s) = summary {
            session.set_summary(s);
        }

        Ok(CompactionResult {
            messages_removed,
            tokens_saved,
            summary,
        })
    }

    fn generate_summary(&self, messages: &[SessionMessage]) -> String {
        let user_msgs: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect();

        let assistant_msgs: Vec<&str> = messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .map(|m| m.content.as_str())
            .collect();

        format!(
            "Session context: {} user messages, {} assistant responses. Recent topics: {}",
            user_msgs.len(),
            assistant_msgs.len(),
            user_msgs
                .iter()
                .take(3)
                .map(|s| {
                    let words: Vec<&str> = s.split_whitespace().take(5).collect();
                    words.join(" ")
                })
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

impl Default for SessionCompactor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub messages_removed: usize,
    pub tokens_saved: usize,
    pub summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compactor_supports_documented_builder_aliases() {
        let mut session = SessionMemory::new("session").with_compaction_threshold(3);
        session.push_user("old 1");
        session.push_user("old 2");
        session.push_user("recent 1");
        session.push_user("recent 2");

        let result = SessionCompactor::new()
            .keep_recent(2)
            .keep_important(false)
            .compact(&mut session)
            .unwrap();

        assert_eq!(result.messages_removed, 2);
        assert_eq!(session.len(), 2);
    }

    #[test]
    fn compactor_preserves_important_messages_when_enabled() {
        let mut session = SessionMemory::new("session").with_compaction_threshold(3);
        session.push(SessionMessage::user("old important").mark_important());
        session.push(SessionMessage::assistant("old normal"));
        session.push(SessionMessage::user("recent 1"));
        session.push(SessionMessage::assistant("recent 2"));

        let result = SessionCompactor::new()
            .keep_recent(2)
            .keep_important(true)
            .compact(&mut session)
            .unwrap();

        let contents = session
            .messages()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>();

        assert_eq!(result.messages_removed, 1);
        assert!(contents.iter().any(|content| content == "old important"));
        assert!(contents.iter().any(|content| content == "recent 1"));
        assert!(contents.iter().any(|content| content == "recent 2"));
    }
}
