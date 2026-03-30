use super::{
    get_required_string, get_u64, require_tool_approval, truncate_output, PropertySchema, Tool,
    ToolResult, ToolRuntime, ToolSchema,
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
            timeout_secs,
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
        ToolResult::ok(truncate_output(&execution.output)).with_metadata(
            "execution_mode",
            match execution.mode {
                crate::security::CommandExecutionMode::Host => "host",
                crate::security::CommandExecutionMode::Docker => "docker",
            },
        )
    } else {
        ToolResult::err(truncate_output(&execution.output)).with_metadata(
            "execution_mode",
            match execution.mode {
                crate::security::CommandExecutionMode::Host => "host",
                crate::security::CommandExecutionMode::Docker => "docker",
            },
        )
    }
}
