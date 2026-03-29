use super::{
    get_required_string, require_tool_approval, PropertySchema, Tool, ToolResult, ToolRuntime,
    ToolSchema,
};
use crate::mcp::client::{McpClient, McpClientConfig};
use crate::mcp::transport::{
    McpTransportConfig, SseTransportConfig, StdioTransportConfig, WebSocketTransportConfig,
};
use crate::security::{CommandCheck, SecurityLayer};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new(
            "mcp_list_tools",
            "List tools exposed by a configured MCP server",
        )
        .with_schema(ToolSchema::object(
            [(
                "server".to_string(),
                PropertySchema {
                    prop_type: "string".to_string(),
                    description: Some("Configured MCP server name".to_string()),
                    default: None,
                    enum_values: None,
                },
            )]
            .into(),
            vec!["server".to_string()],
        ))
        .with_tags(vec!["mcp".to_string(), "integration".to_string()]),
        Tool::new(
            "mcp_call_tool",
            "Call a tool exposed by a configured MCP server",
        )
        .with_schema(ToolSchema::object(
            [
                (
                    "server".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Configured MCP server name".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
                (
                    "tool".to_string(),
                    PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Remote MCP tool name".to_string()),
                        default: None,
                        enum_values: None,
                    },
                ),
                (
                    "input".to_string(),
                    PropertySchema {
                        prop_type: "object".to_string(),
                        description: Some(
                            "JSON input object passed to the remote tool".to_string(),
                        ),
                        default: Some(serde_json::json!({})),
                        enum_values: None,
                    },
                ),
            ]
            .into(),
            vec!["server".to_string(), "tool".to_string()],
        ))
        .with_approval(true)
        .with_tags(vec!["mcp".to_string(), "integration".to_string()]),
    ]);
}

pub async fn mcp_list_tools(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let server = match get_required_string(arguments, "server") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mut client = match mcp_client_for_server(runtime, &server).await {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    if let Err(err) = client.initialize().await {
        return ToolResult::err(err.to_string());
    }
    let result = client.list_tools().await;
    let _ = client.disconnect().await;

    match result {
        Ok(tools) if tools.is_empty() => ToolResult::ok("no tools"),
        Ok(tools) => ToolResult::ok(
            tools
                .into_iter()
                .map(|tool| format!("{} - {}", tool.name, tool.description.unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn mcp_call_tool(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    if let Some(result) = require_tool_approval("mcp_call_tool", arguments, runtime).await {
        return result;
    }

    let server = match get_required_string(arguments, "server") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let tool = match get_required_string(arguments, "tool") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let mut client = match mcp_client_for_server(runtime, &server).await {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    if let Err(err) = client.initialize().await {
        return ToolResult::err(err.to_string());
    }
    let result = client.call_tool(&tool, input).await;
    let _ = client.disconnect().await;

    match result {
        Ok(result) => {
            let output = result
                .content
                .into_iter()
                .map(|content| match content {
                    crate::mcp::types::McpContent::Text { text } => text,
                    crate::mcp::types::McpContent::Image { mime_type, .. } => {
                        format!("[image:{}]", mime_type)
                    }
                    crate::mcp::types::McpContent::Resource { resource } => {
                        format!("[resource:{}]", resource)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if result.is_error.unwrap_or(false) {
                ToolResult::err(output)
            } else {
                ToolResult::ok(output)
                    .with_metadata("server", server)
                    .with_metadata("tool", tool)
            }
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn mcp_client_for_server(
    runtime: &ToolRuntime,
    server: &str,
) -> Result<McpClient, String> {
    let transport_config = mcp_transport_config_for_server(runtime, server).await?;

    Ok(McpClient::new(McpClientConfig {
        name: "borgclaw".to_string(),
        transport_config,
        protocol_version: "2024-11-05".to_string(),
    }))
}

pub(super) async fn mcp_transport_config_for_server(
    runtime: &ToolRuntime,
    server: &str,
) -> Result<McpTransportConfig, String> {
    let config = runtime
        .mcp_servers
        .get(server)
        .ok_or_else(|| format!("unknown mcp server '{}'", server))?;
    Ok(match config.transport.as_str() {
        "stdio" => {
            let command = config
                .command
                .clone()
                .ok_or_else(|| "missing MCP command".to_string())?;
            match runtime.security.check_command(&command) {
                CommandCheck::Blocked(pattern) => {
                    return Err(format!("blocked MCP command by policy: {}", pattern));
                }
                CommandCheck::Allowed => {}
            }
            let mut env = runtime.security.secret_env().await;
            for (key, value) in &config.env {
                env.insert(
                    key.clone(),
                    resolve_secret_or_env_reference(value, &runtime.security).await,
                );
            }
            McpTransportConfig::Stdio(StdioTransportConfig {
                command,
                args: config.args.clone(),
                env,
            })
        }
        "sse" => McpTransportConfig::Sse(SseTransportConfig {
            url: config
                .url
                .clone()
                .ok_or_else(|| "missing MCP url".to_string())?,
            post_url: config
                .url
                .clone()
                .ok_or_else(|| "missing MCP url".to_string())?,
            headers: config.headers.clone(),
        }),
        "websocket" => McpTransportConfig::WebSocket(WebSocketTransportConfig {
            url: config
                .url
                .clone()
                .ok_or_else(|| "missing MCP url".to_string())?,
        }),
        other => return Err(format!("unsupported MCP transport '{}'", other)),
    })
}

async fn resolve_secret_or_env_reference(value: &str, security: &SecurityLayer) -> String {
    if let Some(var) = value.strip_prefix("${").and_then(|v| v.strip_suffix('}')) {
        if let Some(secret) = security.get_secret(var).await {
            return secret;
        }
        return std::env::var(var).unwrap_or_default();
    }
    value.to_string()
}
