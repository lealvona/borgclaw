use super::{
    EmbeddingProvider, Memory, MemoryEntry, MemoryError, MemoryQuery, MemoryResult,
    NoOpEmbeddingProvider,
};
use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

const RRF_K: f32 = 60.0;

pub struct PostgresMemory {
    conn: Arc<RwLock<Option<sqlx::PgPool>>>,
    connection_string: String,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    hybrid_search: bool,
}

#[derive(sqlx::FromRow)]
struct PgMemoryRow {
    id: String,
    key: String,
    content: String,
    metadata: String,
    created_at: String,
    accessed_at: String,
    access_count: i64,
    importance: f32,
    group_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PgRankedMemoryRow {
    id: String,
    key: String,
    content: String,
    metadata: String,
    created_at: String,
    accessed_at: String,
    access_count: i64,
    importance: f32,
    group_id: Option<String>,
    score: f32,
}

impl PgMemoryRow {
    fn into_memory_entry(self) -> MemoryEntry {
        MemoryEntry {
            id: self.id,
            key: self.key,
            content: self.content,
            metadata: serde_json::from_str(&self.metadata).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            accessed_at: chrono::DateTime::parse_from_rfc3339(&self.accessed_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            access_count: self.access_count.max(0) as u32,
            importance: self.importance,
            group_id: self.group_id,
        }
    }
}

impl PgRankedMemoryRow {
    fn into_memory_result(self) -> MemoryResult {
        MemoryResult {
            entry: PgMemoryRow {
                id: self.id,
                key: self.key,
                content: self.content,
                metadata: self.metadata,
                created_at: self.created_at,
                accessed_at: self.accessed_at,
                access_count: self.access_count,
                importance: self.importance,
                group_id: self.group_id,
            }
            .into_memory_entry(),
            score: self.score,
        }
    }
}

impl PostgresMemory {
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            conn: Arc::new(RwLock::new(None)),
            connection_string: connection_string.into(),
            embedding_provider: Arc::new(NoOpEmbeddingProvider),
            hybrid_search: false,
        }
    }

    pub fn with_embedding_provider(
        mut self,
        provider: Arc<dyn EmbeddingProvider>,
        hybrid_search: bool,
    ) -> Self {
        self.embedding_provider = provider;
        self.hybrid_search = hybrid_search;
        self
    }

    pub async fn init(&self) -> Result<(), MemoryError> {
        let options = PgConnectOptions::from_str(&self.connection_string)
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL,
                accessed_at TIMESTAMPTZ NOT NULL,
                access_count INTEGER NOT NULL DEFAULT 0,
                importance REAL NOT NULL DEFAULT 0.5,
                group_id TEXT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query(
            r#"
            ALTER TABLE memories
            ADD COLUMN IF NOT EXISTS search_vector tsvector
            GENERATED ALWAYS AS (
                to_tsvector('english', coalesce(key, '') || ' ' || coalesce(content, ''))
            ) STORED
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memories_group_id ON memories(group_id)")
            .execute(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_memories_search_vector ON memories USING GIN(search_vector)",
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        if self.hybrid_search {
            sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
                .execute(&pool)
                .await
                .map_err(|e| MemoryError::StorageError(e.to_string()))?;

            sqlx::query(
                r#"
                CREATE TABLE IF NOT EXISTS memory_embeddings (
                    memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
                    embedding vector NOT NULL
                )
                "#,
            )
            .execute(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        }

        *self.conn.write().await = Some(pool);
        Ok(())
    }

    async fn pool(&self) -> Result<sqlx::PgPool, MemoryError> {
        self.conn
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))
    }

    async fn recall_text(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let trimmed = query.query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool().await?;
        let rows: Vec<PgRankedMemoryRow> = if let Some(group_id) = &query.group_id {
            sqlx::query_as::<_, PgRankedMemoryRow>(
                r#"
                SELECT id, key, content, metadata::text AS metadata, created_at::text AS created_at,
                       accessed_at::text AS accessed_at, access_count, importance, group_id,
                       ts_rank_cd(search_vector, websearch_to_tsquery('english', $2)) AS score
                FROM memories
                WHERE group_id = $1
                  AND search_vector @@ websearch_to_tsquery('english', $2)
                ORDER BY score DESC, importance DESC, accessed_at DESC
                LIMIT $3
                "#,
            )
            .bind(group_id)
            .bind(trimmed)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        } else {
            sqlx::query_as::<_, PgRankedMemoryRow>(
                r#"
                SELECT id, key, content, metadata::text AS metadata, created_at::text AS created_at,
                       accessed_at::text AS accessed_at, access_count, importance, group_id,
                       ts_rank_cd(search_vector, websearch_to_tsquery('english', $1)) AS score
                FROM memories
                WHERE group_id IS NULL
                  AND search_vector @@ websearch_to_tsquery('english', $1)
                ORDER BY score DESC, importance DESC, accessed_at DESC
                LIMIT $2
                "#,
            )
            .bind(trimmed)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        }
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(PgRankedMemoryRow::into_memory_result)
            .filter(|result| result.score >= query.min_score)
            .collect())
    }

    async fn recall_semantic(
        &self,
        query: &MemoryQuery,
        query_embedding: &[f32],
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        if query_embedding.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool().await?;
        let query_vector = vector_literal(query_embedding);
        let rows: Vec<PgRankedMemoryRow> = if let Some(group_id) = &query.group_id {
            sqlx::query_as::<_, PgRankedMemoryRow>(
                r#"
                SELECT m.id, m.key, m.content, m.metadata::text AS metadata, m.created_at::text AS created_at,
                       m.accessed_at::text AS accessed_at, m.access_count, m.importance, m.group_id,
                       CAST(1 - (me.embedding <=> $2::vector) AS real) AS score
                FROM memory_embeddings me
                JOIN memories m ON m.id = me.memory_id
                WHERE m.group_id = $1
                ORDER BY me.embedding <=> $2::vector
                LIMIT $3
                "#,
            )
            .bind(group_id)
            .bind(&query_vector)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        } else {
            sqlx::query_as::<_, PgRankedMemoryRow>(
                r#"
                SELECT m.id, m.key, m.content, m.metadata::text AS metadata, m.created_at::text AS created_at,
                       m.accessed_at::text AS accessed_at, m.access_count, m.importance, m.group_id,
                       CAST(1 - (me.embedding <=> $1::vector) AS real) AS score
                FROM memory_embeddings me
                JOIN memories m ON m.id = me.memory_id
                WHERE m.group_id IS NULL
                ORDER BY me.embedding <=> $1::vector
                LIMIT $2
                "#,
            )
            .bind(&query_vector)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        }
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(PgRankedMemoryRow::into_memory_result)
            .filter(|result| result.score >= query.min_score)
            .collect())
    }
}

#[async_trait]
impl Memory for PostgresMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        let pool = self.pool().await?;
        sqlx::query(
            r#"
            INSERT INTO memories (id, key, content, metadata, created_at, accessed_at, access_count, importance, group_id)
            VALUES ($1, $2, $3, $4::jsonb, $5::timestamptz, $6::timestamptz, $7, $8, $9)
            ON CONFLICT (id) DO UPDATE SET
                key = EXCLUDED.key,
                content = EXCLUDED.content,
                metadata = EXCLUDED.metadata,
                created_at = EXCLUDED.created_at,
                accessed_at = EXCLUDED.accessed_at,
                access_count = EXCLUDED.access_count,
                importance = EXCLUDED.importance,
                group_id = EXCLUDED.group_id
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.key)
        .bind(&entry.content)
        .bind(serde_json::to_string(&entry.metadata).unwrap_or_else(|_| "{}".to_string()))
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.accessed_at.to_rfc3339())
        .bind(entry.access_count as i32)
        .bind(entry.importance)
        .bind(&entry.group_id)
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        if self.hybrid_search {
            if let Ok(embedding) = self.embedding_provider.embed(&entry.content).await {
                let embedding_literal = vector_literal(&embedding);
                sqlx::query(
                    r#"
                    INSERT INTO memory_embeddings (memory_id, embedding)
                    VALUES ($1, $2::vector)
                    ON CONFLICT (memory_id) DO UPDATE SET embedding = EXCLUDED.embedding
                    "#,
                )
                .bind(&entry.id)
                .bind(embedding_literal)
                .execute(&pool)
                .await
                .map_err(|e| MemoryError::StorageError(e.to_string()))?;
            }
        }

        Ok(())
    }

    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let text_results = self.recall_text(query).await?;

        if self.hybrid_search {
            if let Ok(query_embedding) = self.embedding_provider.embed(&query.query).await {
                let semantic_results = self.recall_semantic(query, &query_embedding).await?;
                return Ok(reciprocal_rank_fuse(
                    text_results,
                    semantic_results,
                    query.limit,
                ));
            }
        }

        Ok(text_results)
    }

    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let pool = self.pool().await?;
        let row: Option<PgMemoryRow> = sqlx::query_as::<_, PgMemoryRow>(
            r#"
            SELECT id, key, content, metadata::text AS metadata, created_at::text AS created_at,
                   accessed_at::text AS accessed_at, access_count, importance, group_id
            FROM memories
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(row.map(PgMemoryRow::into_memory_entry))
    }

    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn update(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.store(entry.clone()).await
    }

    async fn keys(&self) -> Result<Vec<String>, MemoryError> {
        let pool = self.pool().await?;
        let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT key FROM memories ORDER BY key")
            .fetch_all(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(rows.into_iter().map(|(key,)| key).collect())
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM memories")
            .execute(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError> {
        let pool = self.pool().await?;
        sqlx::query("DELETE FROM memories WHERE group_id = $1")
            .bind(group_id)
            .execute(&pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn groups(&self) -> Result<Vec<String>, MemoryError> {
        let pool = self.pool().await?;
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT DISTINCT group_id FROM memories WHERE group_id IS NOT NULL")
                .fetch_all(&pool)
                .await
                .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(rows.into_iter().map(|(group,)| group).collect())
    }
}

fn vector_literal(values: &[f32]) -> String {
    let formatted = values
        .iter()
        .map(|value| {
            if value.is_finite() {
                value.to_string()
            } else {
                "0".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", formatted)
}

fn reciprocal_rank_fuse(
    text_results: Vec<MemoryResult>,
    semantic_results: Vec<MemoryResult>,
    limit: usize,
) -> Vec<MemoryResult> {
    let mut merged: HashMap<String, MemoryResult> = HashMap::new();

    for (rank, result) in text_results.into_iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f32 + 1.0);
        merged
            .entry(result.entry.id.clone())
            .and_modify(|existing| existing.score += score)
            .or_insert(MemoryResult {
                entry: result.entry,
                score,
            });
    }

    for (rank, result) in semantic_results.into_iter().enumerate() {
        let score = 1.0 / (RRF_K + rank as f32 + 1.0);
        merged
            .entry(result.entry.id.clone())
            .and_modify(|existing| existing.score += score)
            .or_insert(MemoryResult {
                entry: result.entry,
                score,
            });
    }

    let mut results: Vec<_> = merged.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::new_entry;

    #[test]
    fn vector_literal_formats_pgvector_syntax() {
        let vector = vector_literal(&[1.0, 2.5, -3.25]);
        assert_eq!(vector, "[1,2.5,-3.25]");
    }

    #[test]
    fn vector_literal_replaces_non_finite_values() {
        let vector = vector_literal(&[1.0, f32::NAN, f32::INFINITY]);
        assert_eq!(vector, "[1,0,0]");
    }

    #[test]
    fn reciprocal_rank_fuse_prefers_items_ranked_in_both_lists() {
        let mut alpha = new_entry("alpha", "shared");
        alpha.id = "alpha".to_string();
        let mut beta = new_entry("beta", "text-only");
        beta.id = "beta".to_string();
        let mut gamma = new_entry("gamma", "semantic-only");
        gamma.id = "gamma".to_string();

        let fused = reciprocal_rank_fuse(
            vec![
                MemoryResult {
                    entry: alpha.clone(),
                    score: 0.9,
                },
                MemoryResult {
                    entry: beta,
                    score: 0.8,
                },
            ],
            vec![
                MemoryResult {
                    entry: alpha,
                    score: 0.95,
                },
                MemoryResult {
                    entry: gamma,
                    score: 0.85,
                },
            ],
            3,
        );

        assert_eq!(
            fused.first().map(|item| item.entry.id.as_str()),
            Some("alpha")
        );
        assert_eq!(fused.len(), 3);
    }
}
