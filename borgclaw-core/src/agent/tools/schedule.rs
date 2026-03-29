use super::{execute_tool, get_required_string, message, ToolCall, ToolResult, ToolRuntime};
use crate::scheduler::{new_job, JobTrigger, SchedulerError, SchedulerTrait};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<super::Tool>) {
    tools.extend([
        super::Tool::new("schedule_task", "Schedule a task to run later")
            .with_schema(super::ToolSchema::object(
                [
                    (
                        "message".to_string(),
                        super::PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Task description".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "tool".to_string(),
                        super::PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Built-in tool name to execute later".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "arguments".to_string(),
                        super::PropertySchema {
                            prop_type: "object".to_string(),
                            description: Some("Arguments for the scheduled tool call".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "cron".to_string(),
                        super::PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Cron expression".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["scheduling".to_string()]),
        super::Tool::new("run_scheduled_tasks", "Execute due scheduled tasks")
            .with_schema(super::ToolSchema::object(HashMap::new(), Vec::new()))
            .with_tags(vec!["scheduling".to_string()]),
        super::Tool::new("approve", "Approve a pending protected tool operation")
            .with_schema(super::ToolSchema::object(
                [
                    (
                        "tool".to_string(),
                        super::PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Tool name being approved".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                    (
                        "token".to_string(),
                        super::PropertySchema {
                            prop_type: "string".to_string(),
                            description: Some("Approval token".to_string()),
                            default: None,
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["tool".to_string(), "token".to_string()],
            ))
            .with_tags(vec!["security".to_string()]),
    ]);
}

pub async fn schedule_task(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let scheduled = match scheduled_action(arguments) {
        Ok(action) => action,
        Err(err) => return ToolResult::err(err),
    };

    let trigger = match arguments.get("cron").and_then(|value| value.as_str()) {
        Some(cron) => JobTrigger::Cron(cron.to_string()),
        None => JobTrigger::OneShot(chrono::Utc::now() + chrono::Duration::seconds(1)),
    };

    let mut job = new_job(scheduled.job_name(), trigger, scheduled.job_action());
    scheduled.apply_metadata(&mut job.metadata);
    apply_invocation_metadata(runtime, &mut job.metadata);
    let id = {
        let scheduler = runtime.scheduler.lock().await;
        match scheduler.schedule(job).await {
            Ok(id) => id,
            Err(err) => return ToolResult::err(err.to_string()),
        }
    };

    ToolResult::ok(format!("scheduled {}", id))
}

pub async fn run_scheduled_tasks(runtime: &ToolRuntime) -> ToolResult {
    let scheduler = runtime.scheduler.lock().await;
    let results = scheduler
        .run_due(|job| {
            let runtime = runtime.clone();
            async move { execute_scheduled_job(&job, &runtime).await }
        })
        .await;

    if results.is_empty() {
        return ToolResult::ok("no due jobs");
    }

    let success_count = results.iter().filter(|result| result.is_ok()).count();
    let failure_count = results.len() - success_count;
    ToolResult::ok(format!(
        "executed {} scheduled jobs ({} ok, {} failed)",
        results.len(),
        success_count,
        failure_count
    ))
    .with_metadata("executed", results.len().to_string())
    .with_metadata("succeeded", success_count.to_string())
    .with_metadata("failed", failure_count.to_string())
}

pub(super) async fn execute_scheduled_job(
    job: &crate::scheduler::Job,
    runtime: &ToolRuntime,
) -> Result<(), SchedulerError> {
    match job.metadata.get("action_tool").map(String::as_str) {
        Some("message") => {
            let result = message(&HashMap::from([(
                "text".to_string(),
                serde_json::json!(job
                    .metadata
                    .get("text")
                    .cloned()
                    .unwrap_or_else(|| job.action.clone())),
            )]));
            if result.success {
                Ok(())
            } else {
                Err(SchedulerError::JobFailed(result.output))
            }
        }
        Some("tool_call") => {
            let tool_name = job.metadata.get("tool_name").cloned().ok_or_else(|| {
                SchedulerError::Error("scheduled job missing tool_name".to_string())
            })?;
            if matches!(tool_name.as_str(), "schedule_task" | "run_scheduled_tasks") {
                return Err(SchedulerError::Error(format!(
                    "scheduled tool not allowed: {}",
                    tool_name
                )));
            }
            if runtime
                .security
                .needs_approval(&tool_name, runtime.security.approval_mode())
            {
                return Err(SchedulerError::Error(format!(
                    "scheduled background execution cannot satisfy approval for {}",
                    tool_name
                )));
            }
            let arguments = job
                .metadata
                .get("tool_arguments")
                .map(|raw| serde_json::from_str::<HashMap<String, serde_json::Value>>(raw))
                .transpose()
                .map_err(|err| SchedulerError::Error(err.to_string()))?
                .unwrap_or_default();
            let scheduled_runtime = runtime.with_scheduled_job_context(job);
            let result = execute_tool(
                &ToolCall::new(tool_name.clone(), arguments),
                &scheduled_runtime,
            )
            .await;
            if result.success {
                Ok(())
            } else {
                Err(SchedulerError::JobFailed(result.output))
            }
        }
        Some(other) => Err(SchedulerError::Error(format!(
            "unsupported scheduled action tool: {}",
            other
        ))),
        None => Err(SchedulerError::Error(
            "scheduled job missing action_tool metadata".to_string(),
        )),
    }
}

enum ScheduledAction {
    Message(String),
    ToolCall {
        tool_name: String,
        arguments: HashMap<String, serde_json::Value>,
    },
}

impl ScheduledAction {
    fn job_name(&self) -> String {
        match self {
            Self::Message(message) => message.clone(),
            Self::ToolCall { tool_name, .. } => format!("tool:{}", tool_name),
        }
    }

    fn job_action(&self) -> String {
        match self {
            Self::Message(message) => message.clone(),
            Self::ToolCall { tool_name, .. } => format!("tool:{}", tool_name),
        }
    }

    fn apply_metadata(&self, metadata: &mut HashMap<String, String>) {
        match self {
            Self::Message(message) => {
                metadata.insert("action_tool".to_string(), "message".to_string());
                metadata.insert("text".to_string(), message.clone());
            }
            Self::ToolCall {
                tool_name,
                arguments,
            } => {
                metadata.insert("action_tool".to_string(), "tool_call".to_string());
                metadata.insert("tool_name".to_string(), tool_name.clone());
                let raw_arguments =
                    serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string());
                metadata.insert("tool_arguments".to_string(), raw_arguments);
            }
        }
    }
}

fn scheduled_action(
    arguments: &HashMap<String, serde_json::Value>,
) -> Result<ScheduledAction, String> {
    match (
        arguments.get("message").and_then(|value| value.as_str()),
        arguments.get("tool").and_then(|value| value.as_str()),
    ) {
        (Some(message), None) => Ok(ScheduledAction::Message(message.to_string())),
        (None, Some(tool_name)) => {
            let tool_arguments = arguments
                .get("arguments")
                .and_then(|value| value.as_object())
                .map(|object| {
                    object
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default();
            Ok(ScheduledAction::ToolCall {
                tool_name: tool_name.to_string(),
                arguments: tool_arguments,
            })
        }
        (Some(_), Some(_)) => Err("provide either message or tool, not both".to_string()),
        (None, None) => Err("missing required argument: message or tool".to_string()),
    }
}

fn apply_invocation_metadata(runtime: &ToolRuntime, metadata: &mut HashMap<String, String>) {
    let Some(invocation) = runtime.invocation.as_ref() else {
        return;
    };

    metadata.insert(
        "scheduled_session_id".to_string(),
        invocation.session_id.0.clone(),
    );
    metadata.insert(
        "scheduled_sender_id".to_string(),
        invocation.sender.id.clone(),
    );
    metadata.insert(
        "scheduled_sender_channel".to_string(),
        invocation.sender.channel.clone(),
    );
    if let Some(name) = &invocation.sender.name {
        metadata.insert("scheduled_sender_name".to_string(), name.clone());
    }
    for (key, value) in &invocation.metadata {
        metadata.insert(format!("scheduled_meta_{}", key), value.clone());
    }
}

pub fn scheduled_metadata(job: &crate::scheduler::Job) -> HashMap<String, String> {
    job.metadata
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix("scheduled_meta_")
                .map(|key| (key.to_string(), value.clone()))
        })
        .collect()
}

pub async fn approve_tool(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let tool = match get_required_string(arguments, "tool") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let token = match get_required_string(arguments, "token") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match runtime.security.approve_pending(&tool, &token).await {
        Ok(()) => ToolResult::ok(format!(
            "approval recorded for {}; rerun the original command with \"approval_token\":\"{}\"",
            tool, token
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}
