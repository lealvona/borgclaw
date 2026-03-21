//! Memory module - hybrid vector + keyword search with per-group isolation

mod heartbeat;
mod session;
mod solution;
mod storage;

pub use heartbeat::{HeartbeatEngine, HeartbeatResult, HeartbeatTask};
pub use session::{SessionCompactor, SessionMemory, SessionMessage};
pub use solution::{Solution, SolutionMemory, SolutionPattern};
pub use storage::SqliteMemory;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: 5,
            min_score: 0.5,
            group_id: None,
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

/// Create a new memory entry
pub fn new_entry(key: impl Into<String>, content: impl Into<String>) -> MemoryEntry {
    let now = Utc::now();
    MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        key: key.into(),
        content: content.into(),
        metadata: HashMap::new(),
        created_at: now,
        accessed_at: now,
        access_count: 0,
        importance: 0.5,
        group_id: None,
    }
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
