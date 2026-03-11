//! GitHub integration with safety rules

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubConfig {
    pub token: String,
    pub base_url: String,
    pub user_agent: String,
}

impl GitHubConfig {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            base_url: "https://api.github.com".to_string(),
            user_agent: "borgclaw/0.1".to_string(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RepoAccess {
    OwnedOnly,
    Allowlisted(Vec<String>),
    Any,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    DeleteBranch,
    ForcePush,
    ClosePR,
    MergePR,
    DeleteRepo,
    DeleteRelease,
    DeleteTag,
}

impl OperationType {
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            Self::DeleteBranch
                | Self::ForcePush
                | Self::ClosePR
                | Self::MergePR
                | Self::DeleteRepo
                | Self::DeleteRelease
                | Self::DeleteTag
        )
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::DeleteBranch => "delete branch",
            Self::ForcePush => "force push",
            Self::ClosePR => "close pull request",
            Self::MergePR => "merge pull request",
            Self::DeleteRepo => "delete repository",
            Self::DeleteRelease => "delete release",
            Self::DeleteTag => "delete tag",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubSafety {
    pub repo_access: RepoAccess,
    pub require_double_confirm_for: Vec<OperationType>,
}

impl Default for GitHubSafety {
    fn default() -> Self {
        Self {
            repo_access: RepoAccess::OwnedOnly,
            require_double_confirm_for: vec![
                OperationType::DeleteBranch,
                OperationType::ForcePush,
                OperationType::MergePR,
                OperationType::DeleteRepo,
                OperationType::DeleteRelease,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConfirmation {
    pub token: String,
    pub description: String,
    pub expires_at: DateTime<Utc>,
    pub operation: OperationType,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub full_name: String,
    pub owner: String,
    pub description: Option<String>,
    pub private: bool,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubBranch {
    pub name: String,
    pub sha: String,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPullRequest {
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub html_url: String,
    pub head: String,
    pub base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub number: u32,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub id: u32,
    pub tag_name: String,
    pub name: Option<String>,
    pub draft: bool,
    pub html_url: String,
}

pub struct GitHubClient {
    config: GitHubConfig,
    safety: GitHubSafety,
    http: reqwest::Client,
    pending_confirmations: Arc<RwLock<HashMap<String, PendingConfirmation>>>,
    authenticated_user: Arc<RwLock<Option<String>>>,
}

impl GitHubClient {
    pub fn new(config: GitHubConfig) -> Self {
        Self::with_safety(config, GitHubSafety::default())
    }

    pub fn with_safety(config: GitHubConfig, safety: GitHubSafety) -> Self {
        Self {
            config,
            safety,
            http: reqwest::Client::new(),
            pending_confirmations: Arc::new(RwLock::new(HashMap::new())),
            authenticated_user: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn get_authenticated_user(&self) -> Result<String, GitHubError> {
        {
            let cached = self.authenticated_user.read().await;
            if let Some(user) = cached.clone() {
                return Ok(user);
            }
        }

        let response = self
            .http
            .get(format!("{}/user", self.config.base_url))
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::AuthFailed(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct UserResponse {
            login: String,
        }

        let user: UserResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;
        let mut cached = self.authenticated_user.write().await;
        *cached = Some(user.login.clone());

        Ok(user.login)
    }

    pub async fn verify_ownership(&self, owner: &str, repo: &str) -> Result<bool, GitHubError> {
        match &self.safety.repo_access {
            RepoAccess::OwnedOnly => {
                let user = self.get_authenticated_user().await?;
                let response = self
                    .http
                    .get(format!("{}/repos/{}/{}", self.config.base_url, owner, repo))
                    .header("User-Agent", &self.config.user_agent)
                    .header("Authorization", format!("Bearer {}", self.config.token))
                    .send()
                    .await
                    .map_err(GitHubError::RequestFailed)?;

                if !response.status().is_success() {
                    return Err(GitHubError::NotFound(format!("{}/{}", owner, repo)));
                }

                #[derive(Deserialize)]
                struct RepoResponse {
                    owner: Owner,
                }

                #[derive(Deserialize)]
                struct Owner {
                    login: String,
                }

                let repo_resp: RepoResponse = response
                    .json()
                    .await
                    .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;
                Ok(repo_resp.owner.login == user)
            }
            RepoAccess::Allowlisted(allowlist) => {
                let full_name = format!("{}/{}", owner, repo);
                Ok(allowlist.contains(&full_name))
            }
            RepoAccess::Any => Ok(true),
        }
    }

    pub async fn check_repo_access(&self, owner: &str, repo: &str) -> Result<(), GitHubError> {
        let is_owned = self.verify_ownership(owner, repo).await?;

        match &self.safety.repo_access {
            RepoAccess::OwnedOnly if !is_owned => Err(GitHubError::AccessDenied(
                "Repository not owned by authenticated user".to_string(),
            )),
            RepoAccess::Allowlisted(allowlist) => {
                let full_name = format!("{}/{}", owner, repo);
                if !allowlist.contains(&full_name) {
                    Err(GitHubError::AccessDenied(format!(
                        "Repository {} not in allowlist",
                        full_name
                    )))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    pub fn requires_confirmation(&self, operation: OperationType) -> bool {
        self.safety.require_double_confirm_for.contains(&operation)
    }

    pub async fn begin_destructive_op(
        &self,
        operation: OperationType,
        target: impl Into<String>,
    ) -> Result<PendingConfirmation, GitHubError> {
        if !self.requires_confirmation(operation) {
            return Err(GitHubError::ConfirmationNotRequired);
        }

        let token = uuid::Uuid::new_v4().to_string();
        let target_str = target.into();
        let confirmation = PendingConfirmation {
            token: token.clone(),
            description: format!("{} on {}", operation.display_name(), target_str),
            expires_at: Utc::now() + chrono::Duration::seconds(60),
            operation,
            target: target_str,
        };

        let mut pending = self.pending_confirmations.write().await;
        pending.insert(token.clone(), confirmation.clone());

        Ok(confirmation)
    }

    pub async fn prepare_delete_branch(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
    ) -> Result<PendingConfirmation, GitHubError> {
        self.check_repo_access(owner, repo).await?;
        self.begin_destructive_op(
            OperationType::DeleteBranch,
            format!("{}/{}#{}", owner, repo, name),
        )
        .await
    }

    pub async fn prepare_merge_pr(
        &self,
        owner: &str,
        repo: &str,
        number: u32,
    ) -> Result<PendingConfirmation, GitHubError> {
        self.check_repo_access(owner, repo).await?;
        self.begin_destructive_op(
            OperationType::MergePR,
            format!("{}/{}#{}", owner, repo, number),
        )
        .await
    }

    pub async fn confirm_destructive_op(
        &self,
        token: &str,
    ) -> Result<PendingConfirmation, GitHubError> {
        let mut pending = self.pending_confirmations.write().await;

        let confirmation = pending
            .remove(token)
            .ok_or(GitHubError::ConfirmationExpired)?;

        if confirmation.expires_at < Utc::now() {
            return Err(GitHubError::ConfirmationExpired);
        }

        Ok(confirmation)
    }

    pub async fn list_repos(
        &self,
        visibility: Option<&str>,
    ) -> Result<Vec<GitHubRepo>, GitHubError> {
        let mut url = format!("{}/user/repos", self.config.base_url);
        if let Some(v) = visibility {
            url.push_str(&format!("?per_page=100&visibility={}", v));
        } else {
            url.push_str("?per_page=100");
        }

        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct RepoItem {
            name: String,
            full_name: String,
            owner: Owner,
            description: Option<String>,
            private: bool,
            html_url: String,
            default_branch: String,
        }

        #[derive(Deserialize)]
        struct Owner {
            login: String,
        }

        let repos: Vec<RepoItem> = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(repos
            .into_iter()
            .map(|r| GitHubRepo {
                name: r.name,
                full_name: r.full_name,
                owner: r.owner.login,
                description: r.description,
                private: r.private,
                html_url: r.html_url,
                default_branch: r.default_branch,
            })
            .collect())
    }

    pub async fn get_repo(&self, owner: &str, repo: &str) -> Result<GitHubRepo, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}", self.config.base_url, owner, repo);
        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::NotFound(format!("{}/{}", owner, repo)));
        }

        #[derive(Deserialize)]
        struct RepoResponse {
            name: String,
            full_name: String,
            owner: Owner,
            description: Option<String>,
            private: bool,
            html_url: String,
            default_branch: String,
        }

        #[derive(Deserialize)]
        struct Owner {
            login: String,
        }

        let r: RepoResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(GitHubRepo {
            name: r.name,
            full_name: r.full_name,
            owner: r.owner.login,
            description: r.description,
            private: r.private,
            html_url: r.html_url,
            default_branch: r.default_branch,
        })
    }

    pub async fn list_branches(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<GitHubBranch>, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}/branches", self.config.base_url, owner, repo);
        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct BranchItem {
            name: String,
            commit: Commit,
            protected: bool,
        }

        #[derive(Deserialize)]
        struct Commit {
            sha: String,
        }

        let branches: Vec<BranchItem> = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(branches
            .into_iter()
            .map(|b| GitHubBranch {
                name: b.name,
                sha: b.commit.sha,
                protected: b.protected,
            })
            .collect())
    }

    pub async fn create_branch(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
        from_sha: &str,
    ) -> Result<GitHubBranch, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}/git/refs", self.config.base_url, owner, repo);

        let body = serde_json::json!({
            "ref": format!("refs/heads/{}", name),
            "sha": from_sha
        });

        let response = self
            .http
            .post(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct RefResponse {
            #[serde(rename = "ref")]
            r#ref: String,
            object: Object,
        }

        #[derive(Deserialize)]
        struct Object {
            sha: String,
        }

        let r: RefResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(GitHubBranch {
            name: name.to_string(),
            sha: r.object.sha,
            protected: false,
        })
    }

    pub async fn delete_branch(
        &self,
        owner: &str,
        repo: &str,
        name: &str,
        confirmed_token: Option<&str>,
    ) -> Result<(), GitHubError> {
        self.check_repo_access(owner, repo).await?;

        if self.requires_confirmation(OperationType::DeleteBranch) {
            let token = confirmed_token.ok_or(GitHubError::ConfirmationRequired(
                "delete branch".to_string(),
            ))?;
            self.confirm_destructive_op(token).await?;
        }

        let url = format!(
            "{}/repos/{}/{}/git/refs/heads/{}",
            self.config.base_url, owner, repo, name
        );

        let response = self
            .http
            .delete(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        Ok(())
    }

    pub async fn list_prs(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<GitHubPullRequest>, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let mut url = format!("{}/repos/{}/{}/pulls", self.config.base_url, owner, repo);
        if let Some(s) = state {
            url.push_str(&format!("?state={}", s));
        }

        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct PRItem {
            number: u32,
            title: String,
            body: Option<String>,
            state: String,
            html_url: String,
            head: Head,
            base: Base,
        }

        #[derive(Deserialize)]
        struct Head {
            #[serde(rename = "ref")]
            r#ref: String,
        }

        #[derive(Deserialize)]
        struct Base {
            #[serde(rename = "ref")]
            r#ref: String,
        }

        let prs: Vec<PRItem> = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(prs
            .into_iter()
            .map(|pr| GitHubPullRequest {
                number: pr.number,
                title: pr.title,
                body: pr.body,
                state: pr.state,
                html_url: pr.html_url,
                head: pr.head.r#ref,
                base: pr.base.r#ref,
            })
            .collect())
    }

    pub async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        head: &str,
        base: &str,
    ) -> Result<GitHubPullRequest, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}/pulls", self.config.base_url, owner, repo);

        let pr_body = serde_json::json!({
            "title": title,
            "body": body,
            "head": head,
            "base": base
        });

        let response = self
            .http
            .post(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&pr_body)
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct PRResponse {
            number: u32,
            title: String,
            body: Option<String>,
            state: String,
            html_url: String,
            head: Head,
            base: Base,
        }

        #[derive(Deserialize)]
        struct Head {
            #[serde(rename = "ref")]
            r#ref: String,
        }

        #[derive(Deserialize)]
        struct Base {
            #[serde(rename = "ref")]
            r#ref: String,
        }

        let pr: PRResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(GitHubPullRequest {
            number: pr.number,
            title: pr.title,
            body: pr.body,
            state: pr.state,
            html_url: pr.html_url,
            head: pr.head.r#ref,
            base: pr.base.r#ref,
        })
    }

    pub async fn merge_pr(
        &self,
        owner: &str,
        repo: &str,
        number: u32,
        confirmed_token: Option<&str>,
    ) -> Result<bool, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        if self.requires_confirmation(OperationType::MergePR) {
            let token = confirmed_token.ok_or(GitHubError::ConfirmationRequired(
                "merge pull request".to_string(),
            ))?;
            self.confirm_destructive_op(token).await?;
        }

        let url = format!(
            "{}/repos/{}/{}/pulls/{}/merge",
            self.config.base_url, owner, repo, number
        );

        let body = serde_json::json!({
            "merge_method": "merge"
        });

        let response = self
            .http
            .put(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        Ok(response.status().is_success())
    }

    pub async fn list_issues(
        &self,
        owner: &str,
        repo: &str,
        state: Option<&str>,
    ) -> Result<Vec<GitHubIssue>, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let mut url = format!("{}/repos/{}/{}/issues", self.config.base_url, owner, repo);
        if let Some(s) = state {
            url.push_str(&format!("?state={}", s));
        }

        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct IssueItem {
            number: u32,
            title: String,
            body: Option<String>,
            state: String,
            html_url: String,
        }

        let issues: Vec<IssueItem> = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(issues
            .into_iter()
            .map(|i| GitHubIssue {
                number: i.number,
                title: i.title,
                body: i.body,
                state: i.state,
                html_url: i.html_url,
            })
            .collect())
    }

    pub async fn create_issue(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<GitHubIssue, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}/issues", self.config.base_url, owner, repo);

        let issue_body = serde_json::json!({
            "title": title,
            "body": body
        });

        let response = self
            .http
            .post(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&issue_body)
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct IssueResponse {
            number: u32,
            title: String,
            body: Option<String>,
            state: String,
            html_url: String,
        }

        let issue: IssueResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(GitHubIssue {
            number: issue.number,
            title: issue.title,
            body: issue.body,
            state: issue.state,
            html_url: issue.html_url,
        })
    }

    pub async fn list_releases(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<GitHubRelease>, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!("{}/repos/{}/{}/releases", self.config.base_url, owner, repo);

        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        #[derive(Deserialize)]
        struct ReleaseItem {
            id: u32,
            tag_name: String,
            name: Option<String>,
            draft: bool,
            html_url: String,
        }

        let releases: Vec<ReleaseItem> = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        Ok(releases
            .into_iter()
            .map(|r| GitHubRelease {
                id: r.id,
                tag_name: r.tag_name,
                name: r.name,
                draft: r.draft,
                html_url: r.html_url,
            })
            .collect())
    }

    pub async fn get_file(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        r#ref: &str,
    ) -> Result<String, GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!(
            "{}/repos/{}/{}/contents/{}?ref={}",
            self.config.base_url, owner, repo, path, r#ref
        );

        let response = self
            .http
            .get(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::NotFound(format!("{}/{}", path, r#ref)));
        }

        #[derive(Deserialize)]
        struct ContentResponse {
            content: Option<String>,
            encoding: Option<String>,
        }

        let content: ContentResponse = response
            .json()
            .await
            .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;

        if let (Some(encoded), Some("base64")) = (content.content, content.encoding.as_deref()) {
            use base64::Engine;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| GitHubError::ParseFailed(e.to_string()))?;
            String::from_utf8(decoded).map_err(|e| GitHubError::ParseFailed(e.to_string()))
        } else {
            Err(GitHubError::NotFound(format!("{}/{}", path, r#ref)))
        }
    }

    pub async fn create_file(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        content: &str,
        message: &str,
        branch: &str,
    ) -> Result<(), GitHubError> {
        self.check_repo_access(owner, repo).await?;

        let url = format!(
            "{}/repos/{}/{}/contents/{}",
            self.config.base_url, owner, repo, path
        );

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(content);

        let body = serde_json::json!({
            "message": message,
            "content": encoded,
            "branch": branch
        });

        let response = self
            .http
            .put(&url)
            .header("User-Agent", &self.config.user_agent)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(GitHubError::RequestFailed)?;

        if !response.status().is_success() {
            return Err(GitHubError::HttpError(response.status().as_u16()));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    #[error("Request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("HTTP error: {0}")]
    HttpError(u16),

    #[error("Authentication failed: {0}")]
    AuthFailed(u16),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Confirmation required: {0}")]
    ConfirmationRequired(String),

    #[error("Confirmation expired")]
    ConfirmationExpired,

    #[error("Confirmation not required for this operation")]
    ConfirmationNotRequired,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prepare_delete_branch_matches_documented_confirmation_flow() {
        let client = GitHubClient::with_safety(
            GitHubConfig::new("test-token"),
            GitHubSafety {
                repo_access: RepoAccess::Any,
                require_double_confirm_for: vec![OperationType::DeleteBranch],
            },
        );

        let confirmation = client
            .prepare_delete_branch("owner", "repo", "old-branch")
            .await
            .unwrap();

        assert_eq!(confirmation.operation, OperationType::DeleteBranch);
        assert!(confirmation.target.contains("owner/repo#old-branch"));
        assert!(!confirmation.token.is_empty());
    }

    #[tokio::test]
    async fn allowlisted_repo_access_does_not_require_auth_lookup() {
        let client = GitHubClient::with_safety(
            GitHubConfig::new("test-token"),
            GitHubSafety {
                repo_access: RepoAccess::Allowlisted(vec!["owner/repo".to_string()]),
                require_double_confirm_for: vec![],
            },
        );

        assert!(client.check_repo_access("owner", "repo").await.is_ok());
    }
}
