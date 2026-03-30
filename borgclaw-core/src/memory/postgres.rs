use super::{
    storage::cosine_similarity, EmbeddingProvider, Memory, MemoryEntry, MemoryError, MemoryQuery,
    MemoryResult, NoOpEmbeddingProvider,
};
use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

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
            CREATE TABLE IF NOT EXISTS memory_embeddings (
                memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
                embedding BYTEA NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_memories_group_id ON memories(group_id)
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

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
        let pool = self.pool().await?;
        let like = format!("%{}%", query.query);
        let rows: Vec<PgMemoryRow> = if let Some(group_id) = &query.group_id {
            sqlx::query_as::<_, PgMemoryRow>(
                r#"
                SELECT id, key, content, metadata::text AS metadata, created_at::text AS created_at,
                       accessed_at::text AS accessed_at, access_count, importance, group_id
                FROM memories
                WHERE group_id = $1 AND (key ILIKE $2 OR content ILIKE $2)
                ORDER BY
                    CASE
                        WHEN key ILIKE $2 THEN 0
                        WHEN content ILIKE $2 THEN 1
                        ELSE 2
                    END,
                    importance DESC,
                    accessed_at DESC
                LIMIT $3
                "#,
            )
            .bind(group_id)
            .bind(&like)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        } else {
            sqlx::query_as::<_, PgMemoryRow>(
                r#"
                SELECT id, key, content, metadata::text AS metadata, created_at::text AS created_at,
                       accessed_at::text AS accessed_at, access_count, importance, group_id
                FROM memories
                WHERE group_id IS NULL AND (key ILIKE $1 OR content ILIKE $1)
                ORDER BY
                    CASE
                        WHEN key ILIKE $1 THEN 0
                        WHEN content ILIKE $1 THEN 1
                        ELSE 2
                    END,
                    importance DESC,
                    accessed_at DESC
                LIMIT $2
                "#,
            )
            .bind(&like)
            .bind(query.limit as i64)
            .fetch_all(&pool)
            .await
        }
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| MemoryResult {
                entry: row.into_memory_entry(),
                score: (1.0 - (index as f32 * 0.1)).max(query.min_score),
            })
            .collect())
    }

    async fn recall_semantic(
        &self,
        query: &MemoryQuery,
        query_embedding: &[f32],
    ) -> Result<Vec<MemoryResult>, MemoryError> {
        let pool = self.pool().await?;
        let rows: Vec<(String, Vec<u8>)> = if let Some(group_id) = &query.group_id {
            sqlx::query_as(
                r#"
                SELECT me.memory_id, me.embedding
                FROM memory_embeddings me
                JOIN memories m ON m.id = me.memory_id
                WHERE m.group_id = $1
                "#,
            )
            .bind(group_id)
            .fetch_all(&pool)
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT me.memory_id, me.embedding
                FROM memory_embeddings me
                JOIN memories m ON m.id = me.memory_id
                WHERE m.group_id IS NULL
                "#,
            )
            .fetch_all(&pool)
            .await
        }
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        let mut results = Vec::new();
        for (memory_id, blob) in rows {
            let embedding: Vec<f32> = blob
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            let similarity = cosine_similarity(query_embedding, &embedding);
            if similarity < query.min_score {
                continue;
            }
            if let Some(entry) = self.get(&memory_id).await? {
                results.push(MemoryResult {
                    entry,
                    score: similarity,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.limit);
        Ok(results)
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
                let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                sqlx::query(
                    r#"
                    INSERT INTO memory_embeddings (memory_id, embedding)
                    VALUES ($1, $2)
                    ON CONFLICT (memory_id) DO UPDATE SET embedding = EXCLUDED.embedding
                    "#,
                )
                .bind(&entry.id)
                .bind(blob)
                .execute(&pool)
                .await
                .map_err(|e| MemoryError::StorageError(e.to_string()))?;
            }
        }

        Ok(())
    }

    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        if self.hybrid_search {
            match self.embedding_provider.embed(&query.query).await {
                Ok(query_embedding) => {
                    let mut combined = std::collections::HashMap::new();
                    for result in self.recall_text(query).await? {
                        combined.insert(result.entry.id.clone(), result);
                    }
                    for result in self.recall_semantic(query, &query_embedding).await? {
                        if let Some(existing) = combined.get_mut(&result.entry.id) {
                            existing.score = (existing.score + result.score) / 2.0;
                        } else {
                            combined.insert(result.entry.id.clone(), result);
                        }
                    }
                    let mut results: Vec<_> = combined.into_values().collect();
                    results.sort_by(|a, b| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    results.truncate(query.limit);
                    return Ok(results);
                }
                Err(_) => {}
            }
        }

        self.recall_text(query).await
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
