use super::{
    get_required_string, get_u64, require_tool_approval, truncate_output, PropertySchema, Tool,
    ToolResult, ToolRuntime, ToolSchema,
};
use crate::security::CommandCheck;
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

    match runtime.security.check_command(&command) {
        CommandCheck::Blocked(pattern) => {
            let actor = runtime
                .invocation
                .as_ref()
                .map(|i| i.sender.id.clone())
                .unwrap_or_else(|| "system".to_string());
            runtime
                .audit
                .log_command(&actor, &command, true, false)
                .await;
            return ToolResult::err(format!("blocked command by policy: {}", pattern));
        }
        CommandCheck::Allowed => {}
    }

    let mut cmd = tokio::process::Command::new("sh");
    let secret_env = runtime.security.secret_env().await;
    cmd.arg("-lc")
        .arg(&command)
        .current_dir(&runtime.workspace_root)
        .envs(secret_env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let output = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.output(),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => return ToolResult::err(err.to_string()),
        Err(_) => return ToolResult::err("command timed out"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    let actor = runtime
        .invocation
        .as_ref()
        .map(|i| i.sender.id.clone())
        .unwrap_or_else(|| "system".to_string());
    let success = output.status.success();
    runtime
        .audit
        .log_command(&actor, &command, false, success)
        .await;

    if success {
        ToolResult::ok(truncate_output(&combined))
    } else {
        ToolResult::err(truncate_output(&combined))
    }
}
