use super::{
    get_bool, get_required_string, get_string, get_u64, google_client, number_property,
    require_tool_approval, resolve_workspace_path, string_property, PropertySchema, Tool,
    ToolResult, ToolRuntime, ToolSchema,
};
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("google_list_messages", "List Gmail messages")
            .with_schema(ToolSchema::object(
                [
                    ("query".to_string(), string_property("Optional Gmail query")),
                    (
                        "limit".to_string(),
                        number_property("Maximum messages", serde_json::json!(10)),
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_get_message", "Get a Gmail message by id")
            .with_schema(ToolSchema::object(
                [("id".to_string(), string_property("Gmail message id"))].into(),
                vec!["id".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_send_email", "Send an email via Gmail")
            .with_schema(ToolSchema::object(
                [
                    ("to".to_string(), string_property("Recipient email address")),
                    ("subject".to_string(), string_property("Email subject")),
                    ("body".to_string(), string_property("Email body")),
                ]
                .into(),
                vec!["to".to_string(), "subject".to_string(), "body".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_search_files", "Search Google Drive files")
            .with_schema(ToolSchema::object(
                [("query".to_string(), string_property("Drive search query"))].into(),
                vec!["query".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_download_file", "Download a Google Drive file")
            .with_schema(ToolSchema::object(
                [("id".to_string(), string_property("Drive file id"))].into(),
                vec!["id".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_list_events", "List Google Calendar events")
            .with_schema(ToolSchema::object(
                [
                    (
                        "calendar_id".to_string(),
                        string_property("Calendar identifier"),
                    ),
                    (
                        "days".to_string(),
                        number_property("Days ahead to query", serde_json::json!(7)),
                    ),
                ]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_upload_file", "Upload a file to Google Drive")
            .with_schema(ToolSchema::object(
                [
                    ("path".to_string(), string_property("Local file path")),
                    ("mime_type".to_string(), string_property("File MIME type")),
                    (
                        "folder_id".to_string(),
                        string_property("Optional Drive folder id"),
                    ),
                ]
                .into(),
                vec!["path".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_create_event", "Create a Google Calendar event")
            .with_schema(ToolSchema::object(
                [
                    ("summary".to_string(), string_property("Event summary")),
                    (
                        "description".to_string(),
                        string_property("Optional event description"),
                    ),
                    ("start".to_string(), string_property("RFC3339 start time")),
                    ("end".to_string(), string_property("RFC3339 end time")),
                ]
                .into(),
                vec![
                    "summary".to_string(),
                    "start".to_string(),
                    "end".to_string(),
                ],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_delete_email", "Permanently delete a Gmail message")
            .with_schema(ToolSchema::object(
                [(
                    "message_id".to_string(),
                    string_property("Gmail message id"),
                )]
                .into(),
                vec!["message_id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_trash_email", "Move a Gmail message to trash")
            .with_schema(ToolSchema::object(
                [(
                    "message_id".to_string(),
                    string_property("Gmail message id"),
                )]
                .into(),
                vec!["message_id".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_update_event", "Update a Google Calendar event")
            .with_schema(ToolSchema::object(
                [
                    (
                        "event_id".to_string(),
                        string_property("Event id to update"),
                    ),
                    ("summary".to_string(), string_property("New event summary")),
                    (
                        "description".to_string(),
                        string_property("Optional event description"),
                    ),
                    ("start".to_string(), string_property("RFC3339 start time")),
                    ("end".to_string(), string_property("RFC3339 end time")),
                ]
                .into(),
                vec![
                    "event_id".to_string(),
                    "summary".to_string(),
                    "start".to_string(),
                    "end".to_string(),
                ],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_delete_event", "Delete a Google Calendar event")
            .with_schema(ToolSchema::object(
                [(
                    "event_id".to_string(),
                    string_property("Event id to delete"),
                )]
                .into(),
                vec!["event_id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_create_folder",
            "Create a new folder in Google Drive",
        )
        .with_schema(ToolSchema::object(
            [
                ("name".to_string(), string_property("Folder name")),
                (
                    "parent_id".to_string(),
                    string_property("Optional parent folder ID"),
                ),
            ]
            .into(),
            vec!["name".to_string()],
        ))
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_list_folders", "List folders in Google Drive")
            .with_schema(ToolSchema::object(
                [(
                    "parent_id".to_string(),
                    string_property("Optional parent folder ID"),
                )]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_share_file", "Share a Google Drive file")
            .with_schema(ToolSchema::object(
                [
                    ("file_id".to_string(), string_property("File ID to share")),
                    (
                        "email".to_string(),
                        string_property("Optional email to share with (omit for public)"),
                    ),
                    (
                        "role".to_string(),
                        string_property("Permission role: reader, commenter, or writer"),
                    ),
                    (
                        "allow_discovery".to_string(),
                        PropertySchema {
                            prop_type: "boolean".to_string(),
                            description: Some("Allow file discovery if public".to_string()),
                            default: Some(serde_json::json!(false)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["file_id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_list_permissions",
            "List permissions for a Google Drive file",
        )
        .with_schema(ToolSchema::object(
            [("file_id".to_string(), string_property("File ID"))].into(),
            vec!["file_id".to_string()],
        ))
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_remove_permission",
            "Remove a permission from a Google Drive file",
        )
        .with_schema(ToolSchema::object(
            [
                ("file_id".to_string(), string_property("File ID")),
                (
                    "permission_id".to_string(),
                    string_property("Permission ID to remove"),
                ),
            ]
            .into(),
            vec!["file_id".to_string(), "permission_id".to_string()],
        ))
        .with_approval(true)
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_move_file",
            "Move a file to a different folder in Google Drive",
        )
        .with_schema(ToolSchema::object(
            [
                ("file_id".to_string(), string_property("File ID to move")),
                (
                    "new_folder_id".to_string(),
                    string_property("Destination folder ID"),
                ),
                (
                    "old_folder_id".to_string(),
                    string_property("Optional source folder ID to remove from"),
                ),
            ]
            .into(),
            vec!["file_id".to_string(), "new_folder_id".to_string()],
        ))
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_copy_file", "Copy a file in Google Drive")
            .with_schema(ToolSchema::object(
                [
                    ("file_id".to_string(), string_property("File ID to copy")),
                    ("new_name".to_string(), string_property("Name for the copy")),
                ]
                .into(),
                vec!["file_id".to_string(), "new_name".to_string()],
            ))
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new("google_delete_file", "Delete a file from Google Drive")
            .with_schema(ToolSchema::object(
                [
                    ("file_id".to_string(), string_property("File ID to delete")),
                    (
                        "permanent".to_string(),
                        PropertySchema {
                            prop_type: "boolean".to_string(),
                            description: Some("Permanently delete instead of trash".to_string()),
                            default: Some(serde_json::json!(false)),
                            enum_values: None,
                        },
                    ),
                ]
                .into(),
                vec!["file_id".to_string()],
            ))
            .with_approval(true)
            .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_get_file_details",
            "Get detailed information about a Google Drive file",
        )
        .with_schema(ToolSchema::object(
            [("file_id".to_string(), string_property("File ID"))].into(),
            vec!["file_id".to_string()],
        ))
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
        Tool::new(
            "google_authenticate",
            "Authenticate with Google OAuth to enable Gmail/Drive/Calendar access. Returns a URL for the user to visit.",
        )
        .with_schema(ToolSchema::object(
            [(
                "force".to_string(),
                PropertySchema {
                    prop_type: "boolean".to_string(),
                    description: Some("Force re-authentication even if already authenticated".to_string()),
                    default: Some(serde_json::json!(false)),
                    enum_values: None,
                },
            )]
            .into(),
            Vec::new(),
        ))
        .with_tags(vec!["google".to_string(), "integration".to_string()]),
    ]);
}

pub async fn google_authenticate(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let force = arguments
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let client = google_client(runtime);

    // Check if already authenticated and not forcing re-auth
    if !force && client.auth().get_token().await.is_ok() {
        return ToolResult::ok(
            "Already authenticated with Google. Use force=true to re-authenticate.",
        );
    }

    // Get the context information from runtime
    let session_id = runtime
        .invocation
        .as_ref()
        .map(|ctx| ctx.session_id.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let user_id = runtime
        .invocation
        .as_ref()
        .map(|ctx| ctx.sender.id.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let channel = runtime
        .invocation
        .as_ref()
        .map(|ctx| ctx.sender.channel.clone())
        .unwrap_or_else(|| "cli".to_string());

    let group_id = runtime
        .invocation
        .as_ref()
        .and_then(|ctx| ctx.metadata.get("group_id").cloned());

    // Generate a unique state parameter
    let state = uuid::Uuid::new_v4().to_string();

    // Store the OAuth state for later retrieval
    let oauth_state = crate::skills::google::OAuthState {
        session_id: session_id.clone(),
        user_id: user_id.clone(),
        channel: channel.clone(),
        created_at: chrono::Utc::now(),
        group_id: group_id.clone(),
    };

    client
        .auth()
        .pending_store()
        .insert(state.clone(), oauth_state)
        .await;

    // Build the auth URL with state
    let auth_url = client.auth().build_auth_url_with_state(&state);

    // Return instructions and URL
    let message = format!(
        "Please authenticate with Google by visiting this URL:\n\n{}\n\n\
        This link is specific to your session and will expire in 10 minutes.\n\
        Once you complete the authentication, I'll be able to access your Gmail, Google Drive, and Calendar.",
        auth_url
    );

    ToolResult::ok(message)
}

pub async fn google_list_messages(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let query = arguments.get("query").and_then(|value| value.as_str());
    let limit = get_u64(arguments, "limit").unwrap_or(10) as u32;

    match client.list_messages(query, limit).await {
        Ok(messages) if messages.is_empty() => ToolResult::ok("no messages"),
        Ok(messages) => ToolResult::ok(
            messages
                .into_iter()
                .map(|msg| {
                    format!(
                        "{} | {} | {}",
                        msg.id,
                        msg.from.unwrap_or_default(),
                        msg.subject.unwrap_or_default()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_get_message(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let id = match get_required_string(arguments, "id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let gmail = crate::skills::GmailClient::new(client.auth());
    match gmail.get_message(&id).await {
        Ok(message) => ToolResult::ok(format!(
            "{} | {} | {}",
            message.id,
            message.from.unwrap_or_default(),
            message.subject.unwrap_or_default()
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_send_email(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let to = match get_required_string(arguments, "to") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let subject = match get_required_string(arguments, "subject") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let body = match get_required_string(arguments, "body") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.send_email(&to, &subject, &body).await {
        Ok(id) => ToolResult::ok(id),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_search_files(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let query = match get_required_string(arguments, "query") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.search_files(&query).await {
        Ok(files) if files.is_empty() => ToolResult::ok("no files"),
        Ok(files) => ToolResult::ok(
            files
                .into_iter()
                .map(|file| format!("{} | {} | {}", file.id, file.name, file.mime_type))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_download_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let id = match get_required_string(arguments, "id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.download_file(&id).await {
        Ok(bytes) => {
            ToolResult::ok(format!("downloaded {} bytes", bytes.len())).with_metadata("file_id", id)
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_list_events(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let calendar_id = arguments
        .get("calendar_id")
        .and_then(|value| value.as_str())
        .unwrap_or("primary");
    let days = get_u64(arguments, "days").unwrap_or(7) as i64;
    let now = chrono::Utc::now();

    match client
        .list_events(calendar_id, now, now + chrono::Duration::days(days))
        .await
    {
        Ok(events) if events.is_empty() => ToolResult::ok("no events"),
        Ok(events) => ToolResult::ok(
            events
                .into_iter()
                .map(|event| format!("{} | {}", event.start.to_rfc3339(), event.summary))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_upload_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let mime_type = arguments
        .get("mime_type")
        .and_then(|value| value.as_str())
        .unwrap_or("application/octet-stream")
        .to_string();
    let folder_id = arguments.get("folder_id").and_then(|value| value.as_str());
    let resolved =
        match resolve_workspace_path(&runtime.workspace_root, &runtime.workspace_policy, &path) {
            Ok(path) => path,
            Err(err) => return ToolResult::err(err),
        };
    let bytes = match std::fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let name = resolved
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload.bin")
        .to_string();

    match client
        .upload_file(&name, bytes, &mime_type, folder_id)
        .await
    {
        Ok(file) => ToolResult::ok(format!("{} | {}", file.id, file.name)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_create_event(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let summary = match get_required_string(arguments, "summary") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match get_required_string(arguments, "start") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let end = match get_required_string(arguments, "end") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match chrono::DateTime::parse_from_rfc3339(&start) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let end = match chrono::DateTime::parse_from_rfc3339(&end) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let description = arguments
        .get("description")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    match client
        .create_event(crate::skills::CalendarEvent {
            summary,
            start,
            end,
            description,
            ..Default::default()
        })
        .await
    {
        Ok(event) => ToolResult::ok(format!("{} | {}", event.id, event.summary)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_delete_email(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let message_id = match get_required_string(arguments, "message_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.delete_email(&message_id).await {
        Ok(()) => ToolResult::ok(format!("deleted {}", message_id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_trash_email(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let message_id = match get_required_string(arguments, "message_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.trash_email(&message_id).await {
        Ok(()) => ToolResult::ok(format!("trashed {}", message_id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_update_event(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let event_id = match get_required_string(arguments, "event_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let summary = match get_required_string(arguments, "summary") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match get_required_string(arguments, "start") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let end = match get_required_string(arguments, "end") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let start = match chrono::DateTime::parse_from_rfc3339(&start) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let end = match chrono::DateTime::parse_from_rfc3339(&end) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(err) => return ToolResult::err(err.to_string()),
    };
    let description = arguments
        .get("description")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);

    match client
        .update_event(
            &event_id,
            crate::skills::CalendarEvent {
                summary,
                start,
                end,
                description,
                ..Default::default()
            },
        )
        .await
    {
        Ok(event) => ToolResult::ok(format!("{} | {}", event.id, event.summary)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_delete_event(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let event_id = match get_required_string(arguments, "event_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.delete_event(&event_id).await {
        Ok(()) => ToolResult::ok(format!("deleted {}", event_id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_create_folder(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let name = match get_required_string(arguments, "name") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let parent_id = get_string(arguments, "parent_id");

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.create_folder(&name, parent_id.as_deref()).await {
        Ok(folder) => ToolResult::ok(format!("created folder {} ({})", folder.name, folder.id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_list_folders(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let parent_id = get_string(arguments, "parent_id");

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.list_folders(parent_id.as_deref(), 20).await {
        Ok(folders) if folders.is_empty() => ToolResult::ok("no folders"),
        Ok(folders) => ToolResult::ok(
            folders
                .into_iter()
                .map(|f| format!("{} | {}", f.id, f.name))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_share_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    if let Some(result) = require_tool_approval("google_share_file", arguments, runtime).await {
        return result;
    }

    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let email = get_string(arguments, "email");
    let role = get_string(arguments, "role").unwrap_or_else(|| "reader".to_string());
    let allow_discovery = get_bool(arguments, "allow_discovery").unwrap_or(false);

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive
        .share_file(&file_id, email.as_deref(), &role, allow_discovery)
        .await
    {
        Ok(perm) => {
            let msg = if let Some(email) = email {
                format!("shared with {} as {}", email, perm.role)
            } else {
                format!("shared publicly as {}", perm.role)
            };
            ToolResult::ok(msg)
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_list_permissions(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.list_permissions(&file_id).await {
        Ok(perms) if perms.is_empty() => ToolResult::ok("no permissions"),
        Ok(perms) => ToolResult::ok(
            perms
                .into_iter()
                .map(|p| {
                    let email = p.email_address.as_deref().unwrap_or("public");
                    format!("{} | {} | {} | {}", p.id, p.permission_type, p.role, email)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_remove_permission(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    if let Some(result) =
        require_tool_approval("google_remove_permission", arguments, runtime).await
    {
        return result;
    }

    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let permission_id = match get_required_string(arguments, "permission_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.remove_permission(&file_id, &permission_id).await {
        Ok(()) => ToolResult::ok("permission removed"),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_move_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let new_folder_id = match get_required_string(arguments, "new_folder_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let old_folder_id = get_string(arguments, "old_folder_id");

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive
        .move_file(&file_id, &new_folder_id, old_folder_id.as_deref())
        .await
    {
        Ok(file) => ToolResult::ok(format!("moved {} to {}", file.name, new_folder_id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_copy_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let new_name = match get_required_string(arguments, "new_name") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.copy_file(&file_id, &new_name).await {
        Ok(file) => ToolResult::ok(format!("copied to {} ({})", file.name, file.id)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_delete_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    if let Some(result) = require_tool_approval("google_delete_file", arguments, runtime).await {
        return result;
    }

    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let permanent = get_bool(arguments, "permanent").unwrap_or(false);

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.delete_file(&file_id, permanent).await {
        Ok(()) => {
            let msg = if permanent {
                "file permanently deleted"
            } else {
                "file moved to trash"
            };
            ToolResult::ok(msg)
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn google_get_file_details(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = google_client(runtime);
    let file_id = match get_required_string(arguments, "file_id") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    let drive = crate::skills::DriveClient::new(client.auth());
    match drive.get_file_details(&file_id).await {
        Ok(details) => {
            let mut result = format!(
                "Name: {}\nID: {}\nType: {}\nSize: {:?}\n",
                details.file.name, details.file.id, details.file.mime_type, details.file.size
            );
            if let Some(link) = details.web_view_link {
                result.push_str(&format!("View: {}\n", link));
            }
            if let Some(created) = details.created_time {
                result.push_str(&format!("Created: {}\n", created));
            }
            if let Some(modified) = details.modified_time {
                result.push_str(&format!("Modified: {}\n", modified));
            }
            ToolResult::ok(result)
        }
        Err(err) => ToolResult::err(err.to_string()),
    }
}
