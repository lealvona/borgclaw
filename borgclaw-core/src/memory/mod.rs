//! Memory module - hybrid vector + keyword search with per-group isolation

mod external;
mod heartbeat;
mod in_memory;
mod postgres;
mod session;
mod solution;
mod storage;

pub use external::ExternalMemoryAdapter;
pub use heartbeat::{HeartbeatEngine, HeartbeatResult, HeartbeatTask};
pub use in_memory::InMemoryMemory;
pub use postgres::PostgresMemory;
pub use session::{SessionCompactor, SessionMemory, SessionMessage};
pub use solution::{Solution, SolutionMemory, SolutionPattern};
pub use storage::SqliteMemory;

use crate::config::{
    MemoryAccessScopeConfig, MemoryBackend, MemoryConfig, MemorySensitivityConfig,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Entry ID
    pub id: String,
    /// Memory key/topic
    pub key: String,
    /// Memory content
    pub content: String,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Accessed at
    pub accessed_at: DateTime<Utc>,
    /// Access count
    pub access_count: u32,
    /// Importance score (0-1)
    pub importance: f32,
    /// Group ID for per-group isolation
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemorySensitivity {
    Public,
    Workspace,
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemoryAccessScope {
    Public,
    Workspace,
    #[default]
    Private,
}

impl From<MemorySensitivityConfig> for MemorySensitivity {
    fn from(value: MemorySensitivityConfig) -> Self {
        match value {
            MemorySensitivityConfig::Public => Self::Public,
            MemorySensitivityConfig::Workspace => Self::Workspace,
            MemorySensitivityConfig::Private => Self::Private,
        }
    }
}

impl From<MemoryAccessScopeConfig> for MemoryAccessScope {
    fn from(value: MemoryAccessScopeConfig) -> Self {
        match value {
            MemoryAccessScopeConfig::Public => Self::Public,
            MemoryAccessScopeConfig::Workspace => Self::Workspace,
            MemoryAccessScopeConfig::Private => Self::Private,
        }
    }
}

impl MemoryEntry {
    pub fn sensitivity(&self) -> MemorySensitivity {
        match self.metadata.get("sensitivity").map(String::as_str) {
            Some("public") => MemorySensitivity::Public,
            Some("private") => MemorySensitivity::Private,
            _ => MemorySensitivity::Workspace,
        }
    }

    pub fn set_sensitivity(&mut self, sensitivity: MemorySensitivity) {
        self.metadata.insert(
            "sensitivity".to_string(),
            match sensitivity {
                MemorySensitivity::Public => "public",
                MemorySensitivity::Workspace => "workspace",
                MemorySensitivity::Private => "private",
            }
            .to_string(),
        );
    }
}

impl MemoryAccessScope {
    pub fn allows(self, sensitivity: MemorySensitivity) -> bool {
        match self {
            Self::Public => matches!(sensitivity, MemorySensitivity::Public),
            Self::Workspace => !matches!(sensitivity, MemorySensitivity::Private),
            Self::Private => true,
        }
    }
}

/// Memory query
#[derive(Debug, Clone)]
pub struct MemoryQuery {
    /// Search query
    pub query: String,
    /// Max results
    pub limit: usize,
    /// Minimum relevance score
    pub min_score: f32,
    /// Filter by group ID
    pub group_id: Option<String>,
    /// Filter to entries created at or after this instant
    pub since: Option<DateTime<Utc>>,
    /// Filter to entries created at or before this instant
    pub until: Option<DateTime<Utc>>,
    /// Maximum readable sensitivity for this recall
    pub access_scope: MemoryAccessScope,
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: 5,
            min_score: 0.5,
            group_id: None,
            since: None,
            until: None,
            access_scope: MemoryAccessScope::Private,
        }
    }
}

impl MemoryQuery {
    pub fn for_group(query: impl Into<String>, group_id: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            group_id: Some(group_id.into()),
            ..Default::default()
        }
    }

    pub fn matches_entry(&self, entry: &MemoryEntry) -> bool {
        if entry.group_id != self.group_id {
            return false;
        }
        if self.since.is_some_and(|since| entry.created_at < since) {
            return false;
        }
        if self.until.is_some_and(|until| entry.created_at > until) {
            return false;
        }
        if !self.access_scope.allows(entry.sensitivity()) {
            return false;
        }
        true
    }
}

/// Memory result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryResult {
    pub entry: MemoryEntry,
    pub score: f32,
}

/// Memory trait - implemented by memory backends
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a memory
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError>;

    /// Recall memories
    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError>;

    /// List memory history ordered newest first
    async fn history(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError>;

    /// Get a specific memory
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError>;

    /// Delete a memory
    async fn delete(&self, id: &str) -> Result<(), MemoryError>;

    /// Update a memory
    async fn update(&self, entry: &MemoryEntry) -> Result<(), MemoryError>;

    /// List all keys
    async fn keys(&self) -> Result<Vec<String>, MemoryError>;

    /// Clear all memories
    async fn clear(&self) -> Result<(), MemoryError>;

    /// Clear memories for a specific group
    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError>;

    /// Get all group IDs
    async fn groups(&self) -> Result<Vec<String>, MemoryError>;

    /// Store a procedural memory entry.
    async fn store_procedural(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.store(entry).await
    }
}

/// Memory errors
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Query error: {0}")]
    QueryError(String),
    #[error("Compaction error: {0}")]
    CompactionError(String),
    #[error("Embedding error: {0}")]
    EmbeddingError(String),
}

/// Embedding provider for vector search
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError>;
}

/// HTTP-based embedding provider
pub struct HttpEmbeddingProvider {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpEmbeddingProvider {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for HttpEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, MemoryError> {
        #[derive(serde::Deserialize)]
        struct EmbedResponse {
            embedding: Vec<f32>,
        }

        let response = self
            .client
            .post(&self.endpoint)
            .json(&serde_json::json!({ "text": text }))
            .send()
            .await
            .map_err(|e| MemoryError::EmbeddingError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MemoryError::EmbeddingError(format!(
                "Embedding service returned status: {}",
                response.status()
            )));
        }

        let EmbedResponse { embedding } = response
            .json()
            .await
            .map_err(|e| MemoryError::EmbeddingError(e.to_string()))?;

        Ok(embedding)
    }
}

/// No-op embedding provider for when hybrid search is disabled
pub struct NoOpEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for NoOpEmbeddingProvider {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, MemoryError> {
        Ok(Vec::new())
    }
}

pub async fn create_memory_backend(config: &MemoryConfig) -> Result<Arc<dyn Memory>, MemoryError> {
    let embedding_provider = configured_embedding_provider(config);
    let hybrid_search = hybrid_search_runtime_enabled(config);

    let backend: Arc<dyn Memory> = match config.effective_backend() {
        MemoryBackend::Sqlite => {
            let memory = Arc::new(
                SqliteMemory::new(config.database_path.clone())
                    .with_embedding_provider(embedding_provider.clone(), hybrid_search),
            );
            memory.init().await?;
            memory
        }
        MemoryBackend::Postgres => {
            let connection_string = config.connection_string.clone().ok_or_else(|| {
                MemoryError::StorageError(
                    "memory.connection_string is required for postgres backend".to_string(),
                )
            })?;
            let memory = Arc::new(
                PostgresMemory::new(connection_string)
                    .with_embedding_provider(embedding_provider, hybrid_search),
            );
            memory.init().await?;
            memory
        }
        MemoryBackend::Memory => Arc::new(InMemoryMemory::new()),
    };

    if config.external.enabled {
        let adapter = ExternalMemoryAdapter::new(config)?;
        Ok(Arc::new(external::CompositeMemory::new(backend, adapter)))
    } else {
        Ok(backend)
    }
}

fn configured_embedding_provider(config: &MemoryConfig) -> Arc<dyn EmbeddingProvider> {
    if let Some(endpoint) = config.embedding_endpoint.as_deref() {
        if !endpoint.trim().is_empty() {
            return Arc::new(HttpEmbeddingProvider::new(endpoint.to_string()));
        }
    }

    Arc::new(NoOpEmbeddingProvider)
}

fn hybrid_search_runtime_enabled(config: &MemoryConfig) -> bool {
    config.hybrid_search
        && config
            .embedding_endpoint
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

/// Create a new memory entry
pub fn new_entry(key: impl Into<String>, content: impl Into<String>) -> MemoryEntry {
    let now = Utc::now();
    let mut entry = MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        key: key.into(),
        content: content.into(),
        metadata: HashMap::new(),
        created_at: now,
        accessed_at: now,
        access_count: 0,
        importance: 0.5,
        group_id: None,
    };
    entry.set_sensitivity(MemorySensitivity::Workspace);
    entry
}

/// Create a new memory entry for a group
pub fn new_entry_for_group(
    key: impl Into<String>,
    content: impl Into<String>,
    group_id: impl Into<String>,
) -> MemoryEntry {
    let mut entry = new_entry(key, content);
    entry.group_id = Some(group_id.into());
    entry
}

/// Create a low-importance procedural memory entry.
pub fn new_procedural_entry(key: impl Into<String>, content: impl Into<String>) -> MemoryEntry {
    let mut entry = new_entry(key, content);
    entry.importance = 0.3;
    entry
        .metadata
        .insert("memory_kind".to_string(), "procedural".to_string());
    entry
}

/// Create a procedural memory entry for a group.
pub fn new_procedural_entry_for_group(
    key: impl Into<String>,
    content: impl Into<String>,
    group_id: impl Into<String>,
) -> MemoryEntry {
    let mut entry = new_procedural_entry(key, content);
    entry.group_id = Some(group_id.into());
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_query_default_values() {
        let query = MemoryQuery::default();
        assert!(query.query.is_empty());
        assert_eq!(query.limit, 5);
        assert_eq!(query.min_score, 0.5);
        assert!(query.group_id.is_none());
        assert!(query.since.is_none());
        assert!(query.until.is_none());
    }

    #[test]
    fn memory_query_for_group_creates_query_with_group() {
        let query = MemoryQuery::for_group("search term", "group-123");
        assert_eq!(query.query, "search term");
        assert_eq!(query.group_id, Some("group-123".to_string()));
        assert_eq!(query.limit, 5); // Default
        assert_eq!(query.min_score, 0.5); // Default
    }

    #[test]
    fn memory_query_for_group_with_custom_values() {
        let query = MemoryQuery {
            query: "custom".to_string(),
            limit: 10,
            min_score: 0.8,
            group_id: Some("custom-group".to_string()),
            since: None,
            until: None,
            access_scope: MemoryAccessScope::Workspace,
        };

        assert_eq!(query.query, "custom");
        assert_eq!(query.limit, 10);
        assert_eq!(query.min_score, 0.8);
        assert_eq!(query.group_id, Some("custom-group".to_string()));
    }

    #[test]
    fn new_entry_creates_valid_entry() {
        let entry = new_entry("test-key", "test content");

        assert_eq!(entry.key, "test-key");
        assert_eq!(entry.content, "test content");
        assert_eq!(
            entry.metadata.get("sensitivity").map(String::as_str),
            Some("workspace")
        );
        assert_eq!(entry.access_count, 0);
        assert_eq!(entry.importance, 0.5);
        assert!(entry.group_id.is_none());
        assert_eq!(entry.sensitivity(), MemorySensitivity::Workspace);
        // ID should be a valid UUID
        assert!(!entry.id.is_empty());
        // Timestamps should be set and equal (fresh entry)
        assert_eq!(entry.created_at, entry.accessed_at);
    }

    #[test]
    fn new_entry_for_group_creates_entry_with_group() {
        let entry = new_entry_for_group("grouped-key", "grouped content", "my-group");

        assert_eq!(entry.key, "grouped-key");
        assert_eq!(entry.content, "grouped content");
        assert_eq!(entry.group_id, Some("my-group".to_string()));
        assert_eq!(
            entry.metadata.get("sensitivity").map(String::as_str),
            Some("workspace")
        );
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn new_procedural_entry_marks_entry_as_procedural() {
        let entry = new_procedural_entry("workflow", "run tests first");

        assert_eq!(
            entry.metadata.get("memory_kind").map(String::as_str),
            Some("procedural")
        );
        assert_eq!(entry.importance, 0.3);
    }

    #[test]
    fn workspace_scope_does_not_allow_private_entries() {
        let mut entry = new_entry("topic", "secret");
        entry.set_sensitivity(MemorySensitivity::Private);

        let query = MemoryQuery {
            access_scope: MemoryAccessScope::Workspace,
            ..Default::default()
        };

        assert!(!query.matches_entry(&entry));
    }

    #[test]
    fn memory_entry_with_metadata() {
        let mut entry = new_entry("meta-key", "meta content");
        entry
            .metadata
            .insert("source".to_string(), "test".to_string());
        entry
            .metadata
            .insert("priority".to_string(), "high".to_string());

        assert_eq!(entry.metadata.len(), 3);
        assert_eq!(entry.metadata.get("source"), Some(&"test".to_string()));
        assert_eq!(entry.metadata.get("priority"), Some(&"high".to_string()));
    }

    #[test]
    fn memory_result_creation() {
        let entry = new_entry("result-key", "result content");
        let result = MemoryResult {
            entry: entry.clone(),
            score: 0.95,
        };

        assert_eq!(result.entry.key, "result-key");
        assert_eq!(result.score, 0.95);
    }

    #[test]
    fn memory_error_variants() {
        let storage_err = MemoryError::StorageError("disk full".to_string());
        assert!(storage_err.to_string().contains("Storage error"));
        assert!(storage_err.to_string().contains("disk full"));

        let not_found_err = MemoryError::NotFound("entry-123".to_string());
        assert!(not_found_err.to_string().contains("Not found"));
        assert!(not_found_err.to_string().contains("entry-123"));

        let query_err = MemoryError::QueryError("invalid syntax".to_string());
        assert!(query_err.to_string().contains("Query error"));
        assert!(query_err.to_string().contains("invalid syntax"));

        let compaction_err = MemoryError::CompactionError("failed".to_string());
        assert!(compaction_err.to_string().contains("Compaction error"));

        let embedding_err = MemoryError::EmbeddingError("timeout".to_string());
        assert!(embedding_err.to_string().contains("Embedding error"));
        assert!(embedding_err.to_string().contains("timeout"));
    }

    #[test]
    fn http_embedding_provider_new() {
        let provider = HttpEmbeddingProvider::new("https://api.example.com/embed");
        assert_eq!(provider.endpoint, "https://api.example.com/embed");
    }

    #[tokio::test]
    async fn noop_embedding_provider_returns_empty() {
        let provider = NoOpEmbeddingProvider;
        let result = provider.embed("any text").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn configured_embedding_provider_uses_http_provider_when_endpoint_present() {
        let config = MemoryConfig {
            embedding_endpoint: Some("http://127.0.0.1:9000/embed".to_string()),
            ..Default::default()
        };

        assert!(hybrid_search_runtime_enabled(&config));
    }

    #[test]
    fn configured_embedding_provider_defaults_to_noop_without_endpoint() {
        let config = MemoryConfig::default();

        let provider = configured_embedding_provider(&config);

        assert!(!hybrid_search_runtime_enabled(&config));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(provider.embed("any text")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn memory_entry_importance_range() {
        let mut entry = new_entry("importance-test", "content");

        // Test different importance values
        entry.importance = 0.0;
        assert_eq!(entry.importance, 0.0);

        entry.importance = 0.5;
        assert_eq!(entry.importance, 0.5);

        entry.importance = 1.0;
        assert_eq!(entry.importance, 1.0);
    }

    #[test]
    fn memory_entry_access_tracking() {
        let mut entry = new_entry("access-test", "content");

        assert_eq!(entry.access_count, 0);

        entry.access_count += 1;
        assert_eq!(entry.access_count, 1);

        entry.access_count = 100;
        assert_eq!(entry.access_count, 100);
    }

    #[test]
    fn memory_entry_timestamps_are_datetime() {
        let before = Utc::now();
        let entry = new_entry("timestamp-test", "content");
        let after = Utc::now();

        // Timestamps should be within the test execution window
        assert!(entry.created_at >= before);
        assert!(entry.created_at <= after);
        assert!(entry.accessed_at >= before);
        assert!(entry.accessed_at <= after);
    }

    #[test]
    fn memory_result_serialization_roundtrip() {
        let entry = new_entry("result-key", "result content");
        let result = MemoryResult {
            entry: entry.clone(),
            score: 0.85,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: MemoryResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.entry.key, result.entry.key);
        assert_eq!(deserialized.score, result.score);
    }
}
