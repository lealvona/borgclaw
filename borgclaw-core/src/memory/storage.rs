//! Memory storage implementation with SQLite + FTS5

use super::{Memory, MemoryEntry, MemoryError, MemoryQuery, MemoryResult};
use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use std::path::PathBuf;
use std::str::FromStr;
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
        std::fs::create_dir_all(&self.path)
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        let db_path = self.path.join("memory.db");

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
            .map_err(|e| MemoryError::StorageError(e.to_string()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);

        let pool = sqlx::pool::PoolOptions::<sqlx::Sqlite>::new()
            .max_connections(1)
            .connect_with(options)
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
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

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
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        let rows = if let Some(group_id) = &query.group_id {
            sqlx::query_as::<_, MemoryRow>(
                r#"
                SELECT m.id, m.key, m.content, m.metadata, m.created_at, m.accessed_at, m.access_count, m.importance, m.group_id
                FROM memories m
                JOIN memories_fts fts ON m.rowid = fts.rowid
                WHERE memories_fts MATCH ? AND m.group_id = ?
                ORDER BY bm25(memories_fts)
                LIMIT ?
                "#,
            )
            .bind(&query.query)
            .bind(group_id)
            .bind(query.limit as i64)
            .fetch_all(pool)
            .await
        } else {
            sqlx::query_as::<_, MemoryRow>(
                r#"
                SELECT m.id, m.key, m.content, m.metadata, m.created_at, m.accessed_at, m.access_count, m.importance, m.group_id
                FROM memories m
                JOIN memories_fts fts ON m.rowid = fts.rowid
                WHERE memories_fts MATCH ? AND m.group_id IS NULL
                ORDER BY bm25(memories_fts)
                LIMIT ?
                "#,
            )
            .bind(&query.query)
            .bind(query.limit as i64)
            .fetch_all(pool)
            .await
        }
        .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        let results: Vec<MemoryResult> = rows
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let score = 1.0 - (i as f32 * 0.1); // Simple scoring
                MemoryResult {
                    entry: row.into_memory_entry(),
                    score: score.max(query.min_score),
                }
            })
            .collect();

        Ok(results)
    }

    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        let row: Option<MemoryRow> = sqlx::query_as(
            r#"SELECT id, key, content, metadata, created_at, accessed_at, access_count, importance, group_id FROM memories WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(row.map(MemoryRow::into_memory_entry))
    }

    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

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
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT key FROM memories ORDER BY key")
            .fetch_all(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(rows.into_iter().map(|(k,)| k).collect())
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        sqlx::query("DELETE FROM memories")
            .execute(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        sqlx::query("DELETE FROM memories WHERE group_id = ?")
            .bind(group_id)
            .execute(pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(())
    }

    async fn groups(&self) -> Result<Vec<String>, MemoryError> {
        let conn = self.conn.read().await;
        let pool = conn
            .as_ref()
            .ok_or_else(|| MemoryError::StorageError("Not initialized".to_string()))?;

        let rows: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT DISTINCT group_id FROM memories WHERE group_id IS NOT NULL")
                .fetch_all(pool)
                .await
                .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(rows.into_iter().filter_map(|(g,)| g).collect())
    }
}

#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    key: String,
    content: String,
    metadata: String,
    created_at: String,
    accessed_at: String,
    access_count: u32,
    importance: f32,
    group_id: Option<String>,
}

impl MemoryRow {
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
            access_count: self.access_count,
            importance: self.importance,
            group_id: self.group_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{new_entry, new_entry_for_group, Memory, MemoryQuery};

    #[tokio::test]
    async fn recall_round_trips_metadata_and_group() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_memory_test_{}", uuid::Uuid::new_v4()));
        let memory = SqliteMemory::new(root.clone());
        memory.init().await.unwrap();

        let mut entry = new_entry_for_group("deadline", "Quarterly report due friday", "work");
        entry
            .metadata
            .insert("source".to_string(), "calendar".to_string());
        memory.store(entry.clone()).await.unwrap();

        let results = memory
            .recall(&MemoryQuery {
                query: "report".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: Some("work".to_string()),
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.group_id.as_deref(), Some("work"));
        assert_eq!(
            results[0].entry.metadata.get("source").map(String::as_str),
            Some("calendar")
        );
    }

    #[tokio::test]
    async fn recall_respects_group_isolation() {
        let root =
            std::env::temp_dir().join(format!("borgclaw_memory_test_{}", uuid::Uuid::new_v4()));
        let memory = SqliteMemory::new(root.clone());
        memory.init().await.unwrap();

        memory
            .store(new_entry("shared", "deploy runbook"))
            .await
            .unwrap();
        memory
            .store(new_entry_for_group("shared", "deploy checklist", "ops"))
            .await
            .unwrap();

        let no_group = memory
            .recall(&MemoryQuery {
                query: "deploy".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: None,
            })
            .await
            .unwrap();
        let ops_group = memory
            .recall(&MemoryQuery {
                query: "deploy".to_string(),
                limit: 5,
                min_score: 0.0,
                group_id: Some("ops".to_string()),
            })
            .await
            .unwrap();

        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(no_group.len(), 1);
        assert_eq!(no_group[0].entry.group_id, None);
        assert_eq!(ops_group.len(), 1);
        assert_eq!(ops_group[0].entry.group_id.as_deref(), Some("ops"));
    }
}
