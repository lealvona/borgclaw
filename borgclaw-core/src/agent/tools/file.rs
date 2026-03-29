use super::{
    get_required_string, get_u64, resolve_workspace_path, PropertySchema, Tool, ToolResult,
    ToolRuntime, ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("read_file", "Read a file from the filesystem")
            .with_schema(ToolSchema::object(
                [
                    (
                        "path".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("File path".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "offset".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Line offset".to_string()),
                            default: Some(serde_json::json!(0)),
                            enum_values: None,
                        },
                    ),
                    (
                        "limit".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Number of lines".to_string()),
                            default: Some(serde_json::json!(100)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["filesystem".to_string()]),
        Tool::new("list_directory", "List files in a directory")
            .with_schema(ToolSchema::object(
                [(
                    "path".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Directory path".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["filesystem".to_string()]),
    ]);
}

pub async fn read_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let offset = get_u64(arguments, "offset").unwrap_or(0) as usize;
    let limit = get_u64(arguments, "limit").unwrap_or(100) as usize;

    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };

    let content = match std::fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let lines = content
        .lines()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(idx, line)| format!("{:>4}: {}", offset + idx + 1, line))
        .collect::<Vec<_>>()
        .join("\n");

    ToolResult::ok(lines)
}

pub async fn list_directory(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let path = arguments
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or(".");

    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };

    let entries = match std::fs::read_dir(&resolved) {
        Ok(entries) => entries,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    let mut names = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            let file_type = entry.file_type().ok();
            let suffix = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                "/"
            } else {
                ""
            };
            format!("{}{}", entry.file_name().to_string_lossy(), suffix)
        })
        .collect::<Vec<_>>();
    names.sort();

    ToolResult::ok(names.join("\n"))
}
