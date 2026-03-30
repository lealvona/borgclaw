use super::{Memory, MemoryEntry, MemoryError, MemoryQuery, MemoryResult};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct InMemoryMemory {
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl InMemoryMemory {
    pub fn new() -> Self {
        Self::default()
    }

    fn score(entry: &MemoryEntry, query: &str) -> f32 {
        if query.trim().is_empty() {
            return 0.0;
        }

        let haystack = format!("{} {}", entry.key, entry.content).to_ascii_lowercase();
        let needle = query.to_ascii_lowercase();
        if haystack == needle {
            1.0
        } else if haystack.contains(&needle) {
            0.9
        } else {
            let overlap = needle
                .split_whitespace()
                .filter(|token| haystack.contains(token))
                .count();
            if overlap == 0 {
                0.0
            } else {
                overlap as f32 / needle.split_whitespace().count().max(1) as f32
            }
        }
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.entries.write().await.insert(entry.id.clone(), entry);
        Ok(())
    }

    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let entries = self.entries.read().await;
        let mut results: Vec<MemoryResult> = entries
            .values()
            .filter(|entry| query.matches_entry(entry))
            .filter_map(|entry| {
                let score = Self::score(entry, &query.query);
                (score >= query.min_score).then(|| MemoryResult {
                    entry: entry.clone(),
                    score,
                })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.limit);
        Ok(results)
    }

    async fn history(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let mut entries: Vec<MemoryEntry> = self
            .entries
            .read()
            .await
            .values()
            .filter(|entry| query.matches_entry(entry))
            .cloned()
            .collect();
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        entries.truncate(query.limit);
        Ok(entries)
    }

    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        Ok(self.entries.read().await.get(id).cloned())
    }

    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        self.entries.write().await.remove(id);
        Ok(())
    }

    async fn update(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.entries
            .write()
            .await
            .insert(entry.id.clone(), entry.clone());
        Ok(())
    }

    async fn keys(&self) -> Result<Vec<String>, MemoryError> {
        let mut keys: Vec<String> = self
            .entries
            .read()
            .await
            .values()
            .map(|entry| entry.key.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        keys.sort();
        Ok(keys)
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        self.entries.write().await.clear();
        Ok(())
    }

    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError> {
        self.entries
            .write()
            .await
            .retain(|_, entry| entry.group_id.as_deref() != Some(group_id));
        Ok(())
    }

    async fn groups(&self) -> Result<Vec<String>, MemoryError> {
        let mut groups: Vec<String> = self
            .entries
            .read()
            .await
            .values()
            .filter_map(|entry| entry.group_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        groups.sort();
        Ok(groups)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{new_entry, new_entry_for_group};
    use chrono::{Duration, Utc};

    #[tokio::test]
    async fn in_memory_memory_round_trips_entries() {
        let memory = InMemoryMemory::new();
        let entry = new_entry("topic", "alpha beta");
        let id = entry.id.clone();
        memory.store(entry).await.unwrap();

        let loaded = memory.get(&id).await.unwrap().unwrap();
        assert_eq!(loaded.key, "topic");
        assert_eq!(loaded.content, "alpha beta");
    }

    #[tokio::test]
    async fn in_memory_memory_respects_group_isolation() {
        let memory = InMemoryMemory::new();
        memory
            .store(new_entry("shared", "public note"))
            .await
            .unwrap();
        memory
            .store(new_entry_for_group("shared", "private note", "ops"))
            .await
            .unwrap();

        let public = memory
            .recall(&MemoryQuery {
                query: "note".to_string(),
                limit: 10,
                min_score: 0.0,
                group_id: None,
                ..Default::default()
            })
            .await
            .unwrap();
        let ops = memory
            .recall(&MemoryQuery::for_group("note", "ops"))
            .await
            .unwrap();

        assert_eq!(public.len(), 1);
        assert_eq!(public[0].entry.group_id, None);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].entry.group_id.as_deref(), Some("ops"));
    }

    #[tokio::test]
    async fn in_memory_memory_history_respects_time_filters() {
        let memory = InMemoryMemory::new();
        let now = Utc::now();

        let mut older = new_entry("topic", "older");
        older.created_at = now - Duration::days(3);
        older.accessed_at = older.created_at;
        memory.store(older).await.unwrap();

        let mut newer = new_entry("topic", "newer");
        newer.created_at = now;
        newer.accessed_at = newer.created_at;
        memory.store(newer).await.unwrap();

        let entries = memory
            .history(&MemoryQuery {
                limit: 10,
                since: Some(now - Duration::days(1)),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content, "newer");
    }
}
