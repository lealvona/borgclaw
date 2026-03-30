use super::{
    get_required_string, get_string, get_u64, optional_group_id, PropertySchema, Tool, ToolResult,
    ToolRuntime, ToolSchema,
};
use crate::memory::{
    new_entry, new_entry_for_group, new_procedural_entry, new_procedural_entry_for_group,
    MemoryQuery,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("memory_store", "Store information in long-term memory")
            .with_schema(ToolSchema::object(
                [
                    (
                        "key".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Memory key".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "value".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Information to store".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["key".to_string(), "value".to_string()],
            ))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_recall", "Recall information from long-term memory")
            .with_schema(ToolSchema::object(
                [
                    (
                        "query".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Search query".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Max results".to_string()),
                            default: Some(serde_json::json!(5)),
                            enum_values: None,
                        },
                    ),
                    (
                        "since".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Optional RFC3339 lower time bound".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "until".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Optional RFC3339 upper time bound".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_history", "List memory history newest-first")
            .with_schema(ToolSchema::object(
                [
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Max entries".to_string()),
                            default: Some(serde_json::json!(10)),
                            enum_values: None,
                        },
                    ),
                    (
                        "since".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Optional RFC3339 lower time bound".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "until".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Optional RFC3339 upper time bound".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["memory".to_string()]),
        Tool::new(
            "memory_store_procedural",
            "Store a procedural long-term memory",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "key".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Memory key".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
                (
                    "value".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Procedural memory content".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
            ]
            .into(),
            vec!["key".to_string(), "value".to_string()],
        ))
        .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_delete", "Delete a memory entry by id")
            .with_schema(ToolSchema::object(
                [(
                    "id".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Memory entry id".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_keys", "List all memory keys")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_groups", "List all memory groups")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["memory".to_string()]),
        Tool::new("memory_clear_group", "Clear all memories in a group")
            .with_schema(ToolSchema::object(
                [(
                    "group_id".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Group id to clear".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["group_id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["memory".to_string()]),
        Tool::new("solution_store", "Store a problem-solution pattern")
            .with_schema(ToolSchema::object(
                [
                    (
                        "problem".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Problem description".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "solution".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Solution description".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "tags".to_string(),
                        PropertySchema {
                            prop_type: "array".to_string(),
                            description: Some("Optional tags".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["problem".to_string(), "solution".to_string()],
            ))
            .with_tags(vec!["memory".to_string(), "solution".to_string()]),
        Tool::new("solution_find", "Find solutions by query")
            .with_schema(ToolSchema::object(
                [
                    (
                        "query".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Search query".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Max results".to_string()),
                            default: Some(serde_json::json!(5)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["memory".to_string(), "solution".to_string()]),
    ]);
}

pub async fn memory_store(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let key = match get_required_string(arguments, "key") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let value = match get_required_string(arguments, "value") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let group_id = optional_group_id(arguments, runtime);
    let entry = match group_id.clone() {
        Some(group_id) => new_entry_for_group(key, value, group_id),
        None => new_entry(key, value),
    };
    match runtime.memory.store(entry).await {
        Ok(()) => {
            let mut result = ToolResult::ok("stored");
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_recall(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let limit = get_u64(arguments, "limit").unwrap_or(5) as usize;
    let group_id = optional_group_id(arguments, runtime);
    let since = match parse_optional_rfc3339(arguments, "since") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let until = match parse_optional_rfc3339(arguments, "until") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime
        .memory
        .recall(&MemoryQuery {
            query,
            limit,
            min_score: 0.0,
            group_id: group_id.clone(),
            since,
            until,
        })
        .await
    {
        Ok(results) if results.is_empty() => {
            let mut result = ToolResult::ok("no matching memories");
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Ok(results) => {
            let mut result = ToolResult::ok(
                results
                    .into_iter()
                    .map(|result| format!("{}: {}", result.entry.key, result.entry.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_history(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let limit = get_u64(arguments, "limit").unwrap_or(10) as usize;
    let group_id = optional_group_id(arguments, runtime);
    let since = match parse_optional_rfc3339(arguments, "since") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let until = match parse_optional_rfc3339(arguments, "until") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime
        .memory
        .history(&MemoryQuery {
            limit,
            group_id: group_id.clone(),
            since,
            until,
            ..Default::default()
        })
        .await
    {
        Ok(entries) if entries.is_empty() => ToolResult::ok("no memory history"),
        Ok(entries) => {
            let mut result = ToolResult::ok(
                entries
                    .into_iter()
                    .map(|entry| {
                        format!(
                            "{} | {}: {}",
                            entry.created_at.to_rfc3339(),
                            entry.key,
                            entry.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_store_procedural(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let key = match get_required_string(arguments, "key") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let value = match get_required_string(arguments, "value") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let group_id = optional_group_id(arguments, runtime);

    let entry = match group_id.clone() {
        Some(group_id) => new_procedural_entry_for_group(key, value, group_id),
        None => new_procedural_entry(key, value),
    };

    match runtime.memory.store_procedural(entry).await {
        Ok(()) => {
            let mut result = ToolResult::ok("stored procedural memory");
            if let Some(group_id) = group_id {
                result = result.with_metadata("group_id", group_id);
            }
            result
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_delete(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let id = match get_required_string(arguments, "id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime.memory.delete(&id).await {
        Ok(()) => ToolResult::ok(format!("deleted {}", id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_keys(runtime: &ToolRuntime) -> ToolResult {
    match runtime.memory.keys().await {
        Ok(keys) if keys.is_empty() => ToolResult::ok("no memory keys"),
        Ok(keys) => ToolResult::ok(keys.join("\n")),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_groups(runtime: &ToolRuntime) -> ToolResult {
    match runtime.memory.groups().await {
        Ok(groups) if groups.is_empty() => ToolResult::ok("no groups"),
        Ok(groups) => ToolResult::ok(groups.join("\n")),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn memory_clear_group(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let group_id = match get_required_string(arguments, "group_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime.memory.clear_group(&group_id).await {
        Ok(()) => ToolResult::ok(format!("cleared group {}", group_id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn solution_store(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let problem = match get_required_string(arguments, "problem") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let solution = match get_required_string(arguments, "solution") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let tags = arguments
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let solution_data = serde_json::json!({
        "problem": problem,
        "solution": solution,
        "tags": tags,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "success_count": 0,
    });

    let entry = crate::memory::MemoryEntry {
        id: uuid::Uuid::new_v4().to_string(),
        key: format!("solution:{}", problem),
        content: solution_data.to_string(),
        metadata: {
            let mut m = std::collections::HashMap::new();
            m.insert("type".to_string(), "solution".to_string());
            m.insert("problem".to_string(), problem.clone());
            if !tags.is_empty() {
                m.insert("tags".to_string(), tags.join(","));
            }
            m
        },
        created_at: chrono::Utc::now(),
        accessed_at: chrono::Utc::now(),
        access_count: 0,
        importance: 0.8,
        group_id: Some("solutions".to_string()),
    };

    match runtime.memory.store(entry).await {
        Ok(()) => ToolResult::ok(format!("stored solution for: {}", problem)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn solution_find(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let limit = get_u64(arguments, "limit").unwrap_or(5) as usize;

    match runtime
        .memory
        .recall(&crate::memory::MemoryQuery {
            query: format!("solution:{}", query),
            limit,
            min_score: 0.0,
            group_id: Some("solutions".to_string()),
            ..Default::default()
        })
        .await
    {
        Ok(results) if results.is_empty() => ToolResult::ok("no matching solutions"),
        Ok(results) => {
            let formatted = results
                .into_iter()
                .filter_map(|r| {
                    let data: serde_json::Value = serde_json::from_str(&r.entry.content).ok()?;
                    let problem = data.get("problem")?.as_str()?;
                    let solution = data.get("solution")?.as_str()?;
                    Some(format!("Problem: {}\nSolution: {}", problem, solution))
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            ToolResult::ok(formatted)
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

fn parse_optional_rfc3339(
    arguments: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<DateTime<Utc>>, String> {
    get_string(arguments, key)
        .map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|parsed| parsed.with_timezone(&Utc))
                .map_err(|_| format!("invalid RFC3339 datetime for '{}'", key))
        })
        .transpose()
}
