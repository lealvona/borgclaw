use super::{
    get_bool, get_required_string, get_u64, require_tool_approval, truncate_output, PropertySchema,
    Tool, ToolResult, ToolRuntime, ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.push(
        Tool::new("execute_command", "Execute a shell command")
            .with_schema(ToolSchema::object(
                [
                    (
                        "command".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Command to execute".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "timeout".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some("Timeout in seconds".to_string()),
                            default: Some(serde_json::json!(60)),
                            enum_values: None,
                        },
                    ),
                    (
                        "approval_token".to_string(),
                        PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Approval token for protected commands".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "pty".to_string(),
                        PropertySchema {
                            prop_type: "boolean".to_string(),
                            description: Some("Run the foreground command in a PTY".to_string()),
                            default: Some(serde_json::json!(false)),
                            enum_values: None,
                        },
                    ),
                    (
                        "background".to_string(),
                        PropertySchema {
                            prop_type: "boolean".to_string(),
                            description: Some("Run the command in the background".to_string()),
                            default: Some(serde_json::json!(false)),
                            enum_values: None,
                        },
                    ),
                    (
                        "yield_ms".to_string(),
                        PropertySchema {
                            prop_type: "number".to_string(),
                            description: Some(
                                "When background=true, wait this many milliseconds for a quick completion before returning".to_string(),
                            ),
                            default: Some(serde_json::json!(250)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["command".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["system".to_string()]),
    );
}

pub async fn execute_command(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let command = match get_required_string(arguments, "command") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let timeout_secs = get_u64(arguments, "timeout").unwrap_or(60);
    let pty = get_bool(arguments, "pty").unwrap_or(false);
    let background = get_bool(arguments, "background").unwrap_or(false);
    let yield_ms = get_u64(arguments, "yield_ms");
    let context = command_execution_context(runtime, background);
    if let Some(result) = require_tool_approval("execute_command", arguments, runtime).await {
        return result;
    }

    let execution = match runtime
        .security
        .execute_command(
            "execute_command",
            &command,
            &runtime.workspace_root,
            &runtime.workspace_policy,
            crate::security::CommandExecutionOptions {
                timeout_secs,
                pty,
                background,
                yield_ms,
                context,
            },
        )
        .await
    {
        Ok(result) => result,
        Err(err) => {
            let actor = runtime
                .invocation
                .as_ref()
                .map(|i| i.sender.id.clone())
                .unwrap_or_else(|| "system".to_string());
            let err_string = match err {
                crate::security::SecurityError::ExecutionError(msg) => msg,
                other => other.to_string(),
            };
            let blocked = err_string.contains("blocked command by policy");
            let event_type = if blocked {
                crate::security::AuditEventType::CommandBlocked
            } else {
                crate::security::AuditEventType::CommandExecution
            };
            runtime
                .audit
                .log(
                    crate::security::AuditEntry::new(
                        event_type,
                        &actor,
                        &command,
                        if blocked { "blocked" } else { "failed" },
                    )
                    .with_success(false)
                    .with_metadata("reason", err_string.clone()),
                )
                .await;
            return ToolResult::err(err_string);
        }
    };

    let actor = runtime
        .invocation
        .as_ref()
        .map(|i| i.sender.id.clone())
        .unwrap_or_else(|| "system".to_string());
    let audit_entry = crate::security::AuditEntry::new(
        crate::security::AuditEventType::CommandExecution,
        &actor,
        &command,
        if execution.success {
            "executed"
        } else {
            "failed"
        },
    )
    .with_success(execution.success)
    .with_metadata(
        "execution_mode",
        match execution.mode {
            crate::security::CommandExecutionMode::Host => "host",
            crate::security::CommandExecutionMode::Docker => "docker",
        },
    );
    let audit_entry = if let Some(image) = &execution.image {
        audit_entry.with_metadata("docker_image", image)
    } else {
        audit_entry
    };
    runtime.audit.log(audit_entry).await;

    if execution.success {
        let mut result = ToolResult::ok(truncate_output(&execution.output)).with_metadata(
            "execution_mode",
            match execution.mode {
                crate::security::CommandExecutionMode::Host => "host",
                crate::security::CommandExecutionMode::Docker => "docker",
            },
        );
        if let Some(process_id) = execution.process_id {
            result = result
                .with_metadata("process_id", process_id)
                .with_metadata("background", execution.background.to_string());
        }
        if let Some(pid) = execution.pid {
            result = result.with_metadata("pid", pid.to_string());
        }
        if execution.pty {
            result = result.with_metadata("pty", "true");
        }
        result
    } else {
        let mut result = ToolResult::err(truncate_output(&execution.output)).with_metadata(
            "execution_mode",
            match execution.mode {
                crate::security::CommandExecutionMode::Host => "host",
                crate::security::CommandExecutionMode::Docker => "docker",
            },
        );
        if let Some(process_id) = execution.process_id {
            result = result
                .with_metadata("process_id", process_id)
                .with_metadata("background", execution.background.to_string());
        }
        if let Some(pid) = execution.pid {
            result = result.with_metadata("pid", pid.to_string());
        }
        if execution.pty {
            result = result.with_metadata("pty", "true");
        }
        result
    }
}

fn command_execution_context(
    runtime: &ToolRuntime,
    background: bool,
) -> crate::security::CommandExecutionContext {
    if background {
        return crate::security::CommandExecutionContext::Background;
    }

    if let Some(invocation) = &runtime.invocation {
        if invocation.metadata.contains_key("heartbeat_task_id")
            || invocation.sender.channel == "scheduler"
            || invocation.sender.channel == "subagent"
        {
            return crate::security::CommandExecutionContext::Background;
        }
        if invocation.sender.channel == "cli" {
            return crate::security::CommandExecutionContext::LocalInteractive;
        }
        return crate::security::CommandExecutionContext::RemoteInteractive;
    }

    crate::security::CommandExecutionContext::LocalInteractive
}
