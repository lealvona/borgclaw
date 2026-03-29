use super::{
    get_required_string, get_u64, github_client, number_property, string_property, truncate_output,
    Tool, ToolResult, ToolRuntime, ToolSchema,
};
use crate::skills::github::UpdateFileRequest;
use std::collections::HashMap;

pub fn register(tools: &mut Vec<Tool>) {
    tools.extend([
        Tool::new("github_list_repos", "List accessible GitHub repositories")
            .with_schema(ToolSchema::object(
                [(
                    "visibility".to_string(),
                    super::PropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional visibility filter".to_string()),
                        default: None,
                        enum_values: None,
                    },
                )]
                .into(),
                Vec::new(),
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_get_repo", "Get GitHub repository details")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_branches", "List GitHub repository branches")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_branch", "Create a GitHub branch")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("branch".to_string(), string_property("Branch name")),
                    ("from_sha".to_string(), string_property("Source commit SHA")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "branch".to_string(),
                    "from_sha".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_prs", "List GitHub pull requests")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "state".to_string(),
                        string_property("Optional pull request state filter"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_pr", "Create a GitHub pull request")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("title".to_string(), string_property("Pull request title")),
                    ("head".to_string(), string_property("Head branch")),
                    ("base".to_string(), string_property("Base branch")),
                    ("body".to_string(), string_property("Pull request body")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "title".to_string(),
                    "head".to_string(),
                    "base".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new(
            "github_prepare_delete_branch",
            "Prepare a GitHub branch deletion confirmation",
        )
        .with_schema(ToolSchema::object(
            [
                ("owner".to_string(), string_property("Repository owner")),
                ("repo".to_string(), string_property("Repository name")),
                ("branch".to_string(), string_property("Branch name")),
            ]
            .into(),
            vec![
                "owner".to_string(),
                "repo".to_string(),
                "branch".to_string(),
            ],
        ))
        .with_tags(vec!["github".to_string(), "security".to_string()]),
        Tool::new("github_delete_branch", "Delete a GitHub branch")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("branch".to_string(), string_property("Branch name")),
                    (
                        "confirmation_token".to_string(),
                        string_property("Confirmation token from preparation step"),
                    ),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "branch".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new(
            "github_prepare_merge_pr",
            "Prepare a GitHub pull request merge confirmation",
        )
        .with_schema(ToolSchema::object(
            [
                ("owner".to_string(), string_property("Repository owner")),
                ("repo".to_string(), string_property("Repository name")),
                (
                    "number".to_string(),
                    number_property("Pull request number", serde_json::json!(1)),
                ),
            ]
            .into(),
            vec![
                "owner".to_string(),
                "repo".to_string(),
                "number".to_string(),
            ],
        ))
        .with_tags(vec!["github".to_string(), "security".to_string()]),
        Tool::new("github_merge_pr", "Merge a GitHub pull request")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "number".to_string(),
                        number_property("Pull request number", serde_json::json!(1)),
                    ),
                    (
                        "confirmation_token".to_string(),
                        string_property("Confirmation token from preparation step"),
                    ),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "number".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_issues", "List GitHub issues")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "state".to_string(),
                        string_property("Optional issue state filter"),
                    ),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_issue", "Create a GitHub issue")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("title".to_string(), string_property("Issue title")),
                    ("body".to_string(), string_property("Issue body")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "title".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_list_releases", "List GitHub releases")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_get_file", "Read a file from a GitHub repository")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("path".to_string(), string_property("Repository file path")),
                    ("ref".to_string(), string_property("Git ref or branch name")),
                ]
                .into(),
                vec!["owner".to_string(), "repo".to_string(), "path".to_string()],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_create_file", "Create a file in a GitHub repository")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("path".to_string(), string_property("Repository file path")),
                    ("content".to_string(), string_property("File content")),
                    ("message".to_string(), string_property("Commit message")),
                    ("branch".to_string(), string_property("Target branch")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "path".to_string(),
                    "content".to_string(),
                    "message".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_update_file", "Update a file in a GitHub repository")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    ("path".to_string(), string_property("Repository file path")),
                    ("content".to_string(), string_property("New file content")),
                    ("message".to_string(), string_property("Commit message")),
                    (
                        "sha".to_string(),
                        string_property("File SHA (required for update)"),
                    ),
                    ("branch".to_string(), string_property("Target branch")),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "path".to_string(),
                    "content".to_string(),
                    "message".to_string(),
                    "sha".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new(
            "github_delete_file",
            "Delete a file from a GitHub repository",
        )
        .with_schema(ToolSchema::object(
            [
                ("owner".to_string(), string_property("Repository owner")),
                ("repo".to_string(), string_property("Repository name")),
                ("path".to_string(), string_property("Repository file path")),
                ("message".to_string(), string_property("Commit message")),
                (
                    "sha".to_string(),
                    string_property("File SHA (required for deletion)"),
                ),
                ("branch".to_string(), string_property("Target branch")),
            ]
            .into(),
            vec![
                "owner".to_string(),
                "repo".to_string(),
                "path".to_string(),
                "message".to_string(),
                "sha".to_string(),
            ],
        ))
        .with_tags(vec!["github".to_string(), "integration".to_string()]),
        Tool::new("github_close_issue", "Close a GitHub issue")
            .with_schema(ToolSchema::object(
                [
                    ("owner".to_string(), string_property("Repository owner")),
                    ("repo".to_string(), string_property("Repository name")),
                    (
                        "issue_number".to_string(),
                        number_property("Issue number to close", serde_json::json!(1)),
                    ),
                ]
                .into(),
                vec![
                    "owner".to_string(),
                    "repo".to_string(),
                    "issue_number".to_string(),
                ],
            ))
            .with_tags(vec!["github".to_string(), "integration".to_string()]),
    ]);
}

pub async fn github_list_repos(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let visibility = arguments.get("visibility").and_then(|value| value.as_str());

    match client.list_repos(visibility).await {
        Ok(repos) if repos.is_empty() => ToolResult::ok("no repositories"),
        Ok(repos) => ToolResult::ok(
            repos
                .into_iter()
                .map(|repo| format!("{} ({})", repo.full_name, repo.default_branch))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_get_repo(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.get_repo(&owner, &repo).await {
        Ok(repo) => ToolResult::ok(format!(
            "{} | default={} | private={}",
            repo.full_name, repo.default_branch, repo.private
        )),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_list_branches(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.list_branches(&owner, &repo).await {
        Ok(branches) if branches.is_empty() => ToolResult::ok("no branches"),
        Ok(branches) => ToolResult::ok(
            branches
                .into_iter()
                .map(|branch| {
                    format!(
                        "{} | {} | protected={}",
                        branch.name, branch.sha, branch.protected
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_create_branch(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = match get_required_string(arguments, "branch") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let from_sha = match get_required_string(arguments, "from_sha") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client
        .create_branch(&owner, &repo, &branch, &from_sha)
        .await
    {
        Ok(created) => ToolResult::ok(format!("{} | {}", created.name, created.sha))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_list_prs(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let state = arguments.get("state").and_then(|value| value.as_str());

    match client.list_prs(&owner, &repo, state).await {
        Ok(prs) if prs.is_empty() => ToolResult::ok("no pull requests"),
        Ok(prs) => ToolResult::ok(
            prs.into_iter()
                .map(|pr| format!("{} #{} [{}]", pr.title, pr.number, pr.state))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_create_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let title = match get_required_string(arguments, "title") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let head = match get_required_string(arguments, "head") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let base = match get_required_string(arguments, "base") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let body = arguments
        .get("body")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    match client
        .create_pr(&owner, &repo, &title, body, &head, &base)
        .await
    {
        Ok(pr) => ToolResult::ok(format!("{} #{}", pr.html_url, pr.number))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_prepare_delete_branch(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = match get_required_string(arguments, "branch") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.prepare_delete_branch(&owner, &repo, &branch).await {
        Ok(confirmation) => ToolResult::ok(confirmation.description)
            .with_metadata("confirmation_token", confirmation.token)
            .with_metadata("expires_at", confirmation.expires_at.to_rfc3339()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_delete_branch(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = match get_required_string(arguments, "branch") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let confirmation_token = arguments
        .get("confirmation_token")
        .and_then(|value| value.as_str());

    match client
        .delete_branch(&owner, &repo, &branch, confirmation_token)
        .await
    {
        Ok(()) => ToolResult::ok(format!("deleted branch {}", branch)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_prepare_merge_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let number = match get_u64(arguments, "number") {
        Some(value) => value as u32,
        None => return ToolResult::err("missing number argument 'number'"),
    };

    match client.prepare_merge_pr(&owner, &repo, number).await {
        Ok(confirmation) => ToolResult::ok(confirmation.description)
            .with_metadata("confirmation_token", confirmation.token)
            .with_metadata("expires_at", confirmation.expires_at.to_rfc3339()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_merge_pr(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let number = match get_u64(arguments, "number") {
        Some(value) => value as u32,
        None => return ToolResult::err("missing number argument 'number'"),
    };
    let confirmation_token = arguments
        .get("confirmation_token")
        .and_then(|value| value.as_str());

    match client
        .merge_pr(&owner, &repo, number, confirmation_token)
        .await
    {
        Ok(true) => ToolResult::ok(format!("merged pull request {}", number)),
        Ok(false) => ToolResult::err("merge request was not accepted"),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_list_issues(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let state = arguments.get("state").and_then(|value| value.as_str());

    match client.list_issues(&owner, &repo, state).await {
        Ok(issues) if issues.is_empty() => ToolResult::ok("no issues"),
        Ok(issues) => ToolResult::ok(
            issues
                .into_iter()
                .map(|issue| format!("{} #{} [{}]", issue.title, issue.number, issue.state))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_create_issue(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let title = match get_required_string(arguments, "title") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let body = arguments.get("body").and_then(|value| value.as_str());

    match client.create_issue(&owner, &repo, &title, body).await {
        Ok(issue) => ToolResult::ok(format!("{} #{}", issue.html_url, issue.number))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_list_releases(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };

    match client.list_releases(&owner, &repo).await {
        Ok(releases) if releases.is_empty() => ToolResult::ok("no releases"),
        Ok(releases) => ToolResult::ok(
            releases
                .into_iter()
                .map(|release| {
                    format!(
                        "{} | {} | draft={}",
                        release.tag_name,
                        release.name.unwrap_or_default(),
                        release.draft
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_get_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let git_ref = arguments
        .get("ref")
        .and_then(|value| value.as_str())
        .unwrap_or("HEAD");

    match client.get_file(&owner, &repo, &path, git_ref).await {
        Ok(content) => ToolResult::ok(truncate_output(&content)),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_create_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let content = match get_required_string(arguments, "content") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let message = match get_required_string(arguments, "message") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = arguments
        .get("branch")
        .and_then(|value| value.as_str())
        .unwrap_or("main");

    match client
        .create_file(&owner, &repo, &path, &content, &message, branch)
        .await
    {
        Ok(()) => ToolResult::ok(format!("created {}", path))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo)
            .with_metadata("branch", branch.to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_update_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let content = match get_required_string(arguments, "content") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let message = match get_required_string(arguments, "message") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let sha = match get_required_string(arguments, "sha") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = arguments
        .get("branch")
        .and_then(|value| value.as_str())
        .unwrap_or("main");

    let request = UpdateFileRequest {
        owner: owner.clone(),
        repo: repo.clone(),
        path: path.clone(),
        content,
        message,
        sha,
        branch: branch.to_string(),
    };

    match client.update_file(&request).await {
        Ok(()) => ToolResult::ok(format!("updated {}", path))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo)
            .with_metadata("branch", branch.to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_delete_file(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let path = match get_required_string(arguments, "path") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let message = match get_required_string(arguments, "message") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let sha = match get_required_string(arguments, "sha") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let branch = arguments
        .get("branch")
        .and_then(|value| value.as_str())
        .unwrap_or("main");

    match client
        .delete_file(&owner, &repo, &path, &message, &sha, branch)
        .await
    {
        Ok(()) => ToolResult::ok(format!("deleted {}", path))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo)
            .with_metadata("branch", branch.to_string()),
        Err(err) => ToolResult::err(err.to_string()),
    }
}

pub async fn github_close_issue(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult {
    let client = match github_client(runtime) {
        Ok(client) => client,
        Err(err) => return ToolResult::err(err),
    };
    let owner = match get_required_string(arguments, "owner") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let repo = match get_required_string(arguments, "repo") {
        Ok(value) => value,
        Err(err) => return ToolResult::err(err),
    };
    let issue_number = match get_u64(arguments, "issue_number") {
        Some(value) => value as u32,
        None => return ToolResult::err("missing issue_number argument"),
    };

    match client.close_issue(&owner, &repo, issue_number).await {
        Ok(()) => ToolResult::ok(format!("closed issue #{}", issue_number))
            .with_metadata("owner", owner)
            .with_metadata("repo", repo),
        Err(err) => ToolResult::err(err.to_string()),
    }
}
