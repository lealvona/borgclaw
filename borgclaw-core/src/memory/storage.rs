//! Memory storage implementation with SQLite + FTS5

use super::{Memory, MemoryEntry, MemoryError, MemoryQuery, MemoryResult};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// SQLite-based memory storage with FTS5
pub struct SqliteMemory {
    conn: Arc<RwLock<Option<sqlx::SqlitePool>>>,
    path: PathBuf,
}

impl SqliteMemory {
    pub fn new(path: PathBuf) -> Self {
        Self {
            conn: Arc::new(RwLock::new(None)),
            path,
        }
    }
    
    pub async fn init(&self) -> Result<(), MemoryError> {
        let db_path = self.path.join("memory.db");
        
        let pool = sqlx::pool::PoolOptions::<sqlx::Sqlite>::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}", db_path.display()))
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                key TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT DEFAULT '{}',
                created_at TEXT NOT NULL,
                accessed_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 0,
                importance REAL DEFAULT 0.5,
                group_id TEXT
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        // Create FTS5 virtual table for full-text search
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                key,
                content,
                content='memories',
                content_rowid='rowid'
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        // Create triggers to keep FTS in sync
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                INSERT INTO memories_fts(rowid, key, content) VALUES (NEW.rowid, NEW.key, NEW.content);
            END
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, key, content) VALUES('delete', OLD.rowid, OLD.key, OLD.content);
            END
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        sqlx::query(
            r#"
            CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                INSERT INTO memories_fts(memories_fts, rowid, key, content) VALUES('delete', OLD.rowid, OLD.key, OLD.content);
                INSERT INTO memories_fts(rowid, key, content) VALUES (NEW.rowid, NEW.key, NEW.content);
            END
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        // Create vector table for semantic search (using simple embeddings)
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_embeddings (
                memory_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        *self.conn.write().await = Some(pool);
        Ok(())
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO memories (id, key, content, metadata, created_at, accessed_at, access_count, importance, group_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&entry.id)
        .bind(&entry.key)
        .bind(&entry.content)
        .bind(serde_json::to_string(&entry.metadata).unwrap_or_default())
        .bind(entry.created_at.to_rfc3339())
        .bind(entry.accessed_at.to_rfc3339())
        .bind(entry.access_count)
        .bind(entry.importance)
        .bind(&entry.group_id)
        .execute(pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        // Use FTS5 for keyword search
        let rows: Vec<(String, String, String, String, u32)> = sqlx::query_as(
            r#"
            SELECT m.id, m.key, m.content, m.accessed_at, m.access_count
            FROM memories m
            JOIN memories_fts fts ON m.rowid = fts.rowid
            WHERE memories_fts MATCH ?
            ORDER BY rank
            LIMIT ?
            "#,
        )
        .bind(&query.query)
        .bind(query.limit as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;
        
        let results: Vec<MemoryResult> = rows
            .into_iter()
            .enumerate()
            .map(|(i, (id, key, content, accessed_at, access_count))| {
                let score = 1.0 - (i as f32 * 0.1); // Simple scoring
                MemoryResult {
                    entry: MemoryEntry {
                        id,
                        key,
                        content,
                        metadata: std::collections::HashMap::new(),
                        created_at: chrono::Utc::now(),
                        accessed_at: chrono::DateTime::parse_from_rfc3339(&accessed_at)
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now()),
                        access_count,
                        importance: 0.5,
                        group_id: None,
                    },
                    score: score.max(query.min_score),
                }
            })
            .collect();
        
        Ok(results)
    }
    
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        let row: Option<(String, String, String, String, String, String, u32, f32)> = sqlx::query_as(
            r#"SELECT id, key, content, metadata, created_at, accessed_at, access_count, importance FROM memories WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(row.map(|(id, key, content, metadata, created_at, accessed_at, access_count, importance)| {
            MemoryEntry {
                id,
                key,
                content,
                metadata: serde_json::from_str(&metadata).unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                accessed_at: chrono::DateTime::parse_from_rfc3339(&accessed_at)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
                access_count,
                importance,
                group_id: None,
            }
        }))
    }
    
    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn update(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.store(entry.clone()).await
    }
    
    async fn keys(&self) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT key FROM memories ORDER BY key")
            .fetch_all(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(rows.into_iter().map(|(k,)| k).collect())
    }
    
    async fn clear(&self) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        sqlx::query("DELETE FROM memories")
            .execute(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        sqlx::query("DELETE FROM memories WHERE group_id = ?")
            .bind(group_id)
            .execute(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(())
    }
    
    async fn groups(&self) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn.as_ref().ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;
        
        let rows: Vec<(Option<String>,)> = sqlx::query_as("SELECT DISTINCT group_id FROM memories WHERE group_id IS NOT NULL")
            .fetch_all(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        
        Ok(rows.into_iter().filter_map(|(g,)| g).collect())
    }
}
