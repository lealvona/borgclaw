use super::{
    get_required_string, require_tool_approval, PropertySchema, Tool, ToolResult, ToolRuntime,
    ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("plugin_list", "List loaded WASM plugins")
            .with_schema(ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["plugin".to_string()]),
        Tool::new("plugin_invoke", "Invoke a loaded WASM plugin")
            .with_schema(ToolSchema::object(
                [
                    (
                        "plugin".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Plugin name".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "function".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Exported function to call".to_string()),
                            default: Some(serde_json::json!("invoke")),
                            enum_values: None,
                        },
                    ),
                    (
                        "input".to_string(),
                        PropertySchema {
                            prop_type: "object".to_string(),
                            description: Some("JSON input passed to the plugin".to_string()),
                            default: Some(serde_json::json!({})),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["plugin".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["plugin".to_string()]),
    ]);
}

pub async fn plugin_list(runtime: &ToolRuntime) -> ToolResult {
    let plugins = runtime.plugins.list().await;
    if plugins.is_empty() {
        return ToolResult::ok("no plugins loaded");
    }

    ToolResult::ok(
        plugins
            .into_iter()
            .map(|plugin| {
                format!(
                    "{} {} - {}",
                    plugin.name, plugin.version, plugin.description
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub async fn plugin_invoke(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    if let Some(result) = require_tool_approval("plugin_invoke", arguments, runtime).await {
        return result;
    }

    let plugin = match get_required_string(arguments, "plugin") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let function =
        if let Some(function) = arguments.get("function").and_then(|value| value.as_str()) {
            function.to_string()
        } else {
            runtime
                .plugins
                .get(&plugin)
                .await
                .map(|manifest| manifest.entry_point)
                .unwrap_or_else(|| "invoke".to_string())
        };
    let input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let input_json = match serde_json::to_string(&input) {
        Ok(json) => json,
        Err(err) => return ToolResult::err(err.to_string()),
    };

    match runtime
        .plugins
        .invoke(&plugin, &function, &input_json)
        .await
    {
        Ok(output) => ToolResult::ok(output)
            .with_metadata("plugin", plugin)
            .with_metadata("function", function),
        Err(err) => ToolResult::err(err.to_string()),
    }
}
