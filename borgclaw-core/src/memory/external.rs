use super::{Memory, MemoryEntry, MemoryError, MemoryQuery, MemoryResult};
use crate::config::MemoryConfig;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct ExternalMemoryAdapter {
    endpoint: String,
    mirror_writes: bool,
    client: reqwest::Client,
}

impl ExternalMemoryAdapter {
    pub fn new(config: &MemoryConfig) -> Result<Self, MemoryError> {
        let endpoint = config.external.endpoint.clone().ok_or_else(|| {
            MemoryError::StorageError("memory.external.endpoint is required".to_string())
        })?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.external.timeout_seconds.max(1)))
            .build()
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            mirror_writes: config.external.mirror_writes,
            client,
        })
    }

    pub fn mirror_writes(&self) -> bool {
        self.mirror_writes
    }

    pub async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let response = self
            .client
            .post(format!("{}/search", self.endpoint))
            .json(&ExternalRecallRequest::from_query(query))
            .send()
            .await
            .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MemoryError::QueryError(format!(
                "external memory search failed with status {}",
                response.status()
            )));
        }

        let payload: ExternalRecallResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::QueryError(e.to_string()))?;
        Ok(payload.results)
    }

    pub async fn history(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let response = self
            .client
            .post(format!("{}/history", self.endpoint))
            .json(&ExternalHistoryRequest::from_query(query))
            .send()
            .await
            .map_err(|e| MemoryError::QueryError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(MemoryError::QueryError(format!(
                "external memory history failed with status {}",
                response.status()
            )));
        }

        let payload: ExternalHistoryResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::QueryError(e.to_string()))?;
        Ok(payload.entries)
    }

    pub async fn store(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.post_entry("memories", entry).await
    }

    pub async fn store_procedural(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.post_entry("memories/procedural", entry).await
    }

    async fn post_entry(&self, path: &str, entry: &MemoryEntry) -> Result<(), MemoryError> {
        let response = self
            .client
            .post(format!("{}/{}", self.endpoint, path))
            .json(entry)
            .send()
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(MemoryError::StorageError(format!(
                "external memory write failed with status {}",
                response.status()
            )))
        }
    }
}

pub struct CompositeMemory {
    local: Arc<dyn Memory>,
    external: ExternalMemoryAdapter,
}

impl CompositeMemory {
    pub fn new(local: Arc<dyn Memory>, external: ExternalMemoryAdapter) -> Self {
        Self { local, external }
    }
}

#[async_trait]
impl Memory for CompositeMemory {
    async fn store(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.local.store(entry.clone()).await?;
        if self.external.mirror_writes() {
            let _ = self.external.store(&entry).await;
        }
        Ok(())
    }

    async fn recall(&self, query: &MemoryQuery) -> Result<Vec<MemoryResult>, MemoryError> {
        let local_results = self.local.recall(query).await?;
        let external_results = self.external.recall(query).await.unwrap_or_default();
        Ok(merge_memory_results(
            local_results,
            external_results,
            query.limit,
        ))
    }

    async fn history(&self, query: &MemoryQuery) -> Result<Vec<MemoryEntry>, MemoryError> {
        let local_entries = self.local.history(query).await?;
        let external_entries = self.external.history(query).await.unwrap_or_default();
        Ok(merge_history_entries(
            local_entries,
            external_entries,
            query.limit,
        ))
    }

    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError> {
        self.local.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), MemoryError> {
        self.local.delete(id).await
    }

    async fn update(&self, entry: &MemoryEntry) -> Result<(), MemoryError> {
        self.local.update(entry).await
    }

    async fn keys(&self) -> Result<Vec<String>, MemoryError> {
        self.local.keys().await
    }

    async fn clear(&self) -> Result<(), MemoryError> {
        self.local.clear().await
    }

    async fn clear_group(&self, group_id: &str) -> Result<(), MemoryError> {
        self.local.clear_group(group_id).await
    }

    async fn groups(&self) -> Result<Vec<String>, MemoryError> {
        self.local.groups().await
    }

    async fn store_procedural(&self, entry: MemoryEntry) -> Result<(), MemoryError> {
        self.local.store_procedural(entry.clone()).await?;
        if self.external.mirror_writes() {
            let _ = self.external.store_procedural(&entry).await;
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ExternalRecallRequest {
    query: String,
    limit: usize,
    min_score: f32,
    group_id: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

impl ExternalRecallRequest {
    fn from_query(query: &MemoryQuery) -> Self {
        Self {
            query: query.query.clone(),
            limit: query.limit,
            min_score: query.min_score,
            group_id: query.group_id.clone(),
            since: query.since.map(|value| value.to_rfc3339()),
            until: query.until.map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Serialize)]
struct ExternalHistoryRequest {
    limit: usize,
    group_id: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

impl ExternalHistoryRequest {
    fn from_query(query: &MemoryQuery) -> Self {
        Self {
            limit: query.limit,
            group_id: query.group_id.clone(),
            since: query.since.map(|value| value.to_rfc3339()),
            until: query.until.map(|value| value.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ExternalRecallResponse {
    #[serde(default)]
    results: Vec<MemoryResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExternalHistoryResponse {
    #[serde(default)]
    entries: Vec<MemoryEntry>,
}

fn merge_memory_results(
    local_results: Vec<MemoryResult>,
    external_results: Vec<MemoryResult>,
    limit: usize,
) -> Vec<MemoryResult> {
    let mut merged = HashMap::new();

    for result in local_results {
        merged.insert(result.entry.id.clone(), result);
    }

    for result in external_results {
        merged
            .entry(result.entry.id.clone())
            .and_modify(|existing: &mut MemoryResult| {
                if result.score > existing.score {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
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

fn merge_history_entries(
    local_entries: Vec<MemoryEntry>,
    external_entries: Vec<MemoryEntry>,
    limit: usize,
) -> Vec<MemoryEntry> {
    let mut merged = HashMap::new();

    for entry in local_entries {
        merged.insert(entry.id.clone(), entry);
    }

    for entry in external_entries {
        merged.entry(entry.id.clone()).or_insert(entry);
    }

    let mut entries: Vec<_> = merged.into_values().collect();
    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    entries.truncate(limit);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::new_entry;
    use chrono::{Duration, Utc};

    #[test]
    fn merge_memory_results_prefers_higher_scored_duplicate_and_keeps_external_hits() {
        let mut shared_local = new_entry("shared", "local");
        shared_local.id = "shared".to_string();
        let mut shared_external = new_entry("shared", "external");
        shared_external.id = "shared".to_string();
        let mut external_only = new_entry("external-only", "external");
        external_only.id = "external-only".to_string();

        let merged = merge_memory_results(
            vec![MemoryResult {
                entry: shared_local,
                score: 0.5,
            }],
            vec![
                MemoryResult {
                    entry: shared_external,
                    score: 0.9,
                },
                MemoryResult {
                    entry: external_only,
                    score: 0.7,
                },
            ],
            10,
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].entry.content, "external");
        assert!(merged
            .iter()
            .any(|result| result.entry.key == "external-only"));
    }

    #[test]
    fn merge_history_entries_deduplicates_and_sorts_newest_first() {
        let now = Utc::now();
        let mut local = new_entry("local", "local");
        local.id = "shared".to_string();
        local.created_at = now - Duration::days(2);
        local.accessed_at = local.created_at;

        let mut external = new_entry("external", "external");
        external.id = "external".to_string();
        external.created_at = now;
        external.accessed_at = external.created_at;

        let merged = merge_history_entries(vec![local.clone()], vec![local, external], 10);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].key, "external");
    }

    #[test]
    fn external_recall_request_serializes_since_and_until_bounds() {
        let now = Utc::now();
        let request = ExternalRecallRequest::from_query(&MemoryQuery {
            query: "note".to_string(),
            since: Some(now - Duration::hours(1)),
            until: Some(now),
            ..Default::default()
        });

        assert_eq!(request.query, "note");
        assert!(request.since.is_some());
        assert!(request.until.is_some());
    }
}
