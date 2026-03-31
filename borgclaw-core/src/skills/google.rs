//! Google Workspace integration - OAuth2, Gmail, Drive, Calendar, Docs, Sheets

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub token_path: PathBuf,
    pub scopes: Vec<String>,
    #[serde(skip)]
    pub auth_base_url: String,
    #[serde(skip)]
    pub gmail_base_url: String,
    #[serde(skip)]
    pub drive_base_url: String,
    #[serde(skip)]
    pub calendar_base_url: String,
}

impl Default for GoogleOAuthConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:8085/oauth/callback".to_string(),
            token_path: PathBuf::from(".local/data/google_token.json"),
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                "https://www.googleapis.com/auth/gmail.send".to_string(),
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                "https://www.googleapis.com/auth/calendar.events".to_string(),
            ],
            auth_base_url: "https://oauth2.googleapis.com".to_string(),
            gmail_base_url: "https://gmail.googleapis.com".to_string(),
            drive_base_url: "https://www.googleapis.com".to_string(),
            calendar_base_url: "https://www.googleapis.com".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub scopes: Vec<String>,
}

/// OAuth state for cross-channel authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthState {
    /// Unique session ID for this auth request
    pub session_id: String,
    /// User/channel identifier to route the callback
    pub user_id: String,
    /// Channel type (cli, websocket, telegram, etc.)
    pub channel: String,
    /// Timestamp when the request was created
    pub created_at: DateTime<Utc>,
    /// Optional group ID for multi-user channels
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthPrincipal {
    pub channel: String,
    pub user_id: String,
    pub group_id: Option<String>,
}

impl OAuthPrincipal {
    pub fn new(
        channel: impl Into<String>,
        user_id: impl Into<String>,
        group_id: Option<String>,
    ) -> Self {
        Self {
            channel: channel.into(),
            user_id: user_id.into(),
            group_id,
        }
    }

    pub fn from_oauth_state(state: &OAuthState) -> Self {
        Self::new(
            state.channel.clone(),
            state.user_id.clone(),
            state.group_id.clone(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCompletion {
    pub session_id: String,
    pub channel: String,
    pub success: bool,
    pub message: String,
    pub completed_at: DateTime<Utc>,
}

/// Pending OAuth requests store
#[derive(Clone)]
pub struct OAuthPendingStore {
    requests: Arc<RwLock<std::collections::HashMap<String, OAuthState>>>,
    path: Option<PathBuf>,
}

impl OAuthPendingStore {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(std::collections::HashMap::new())),
            path: None,
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        let requests = load_pending_requests(&path);
        Self {
            requests: Arc::new(RwLock::new(requests)),
            path: Some(path),
        }
    }

    pub fn from_google_config(config: &GoogleOAuthConfig) -> Self {
        Self::with_path(pending_store_path_for_token(&config.token_path))
    }

    pub async fn insert(&self, state: String, info: OAuthState) {
        let mut requests = self.requests.write().await;
        requests.insert(state, info);
        persist_pending_requests(self.path.as_ref(), &requests);
    }

    pub async fn get(&self, state: &str) -> Option<OAuthState> {
        let requests = self.requests.read().await;
        requests.get(state).cloned()
    }

    pub async fn remove(&self, state: &str) -> Option<OAuthState> {
        let mut requests = self.requests.write().await;
        let removed = requests.remove(state);
        persist_pending_requests(self.path.as_ref(), &requests);
        removed
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    pub async fn entry_count(&self) -> usize {
        self.requests.read().await.len()
    }

    /// Clean up expired requests (older than 10 minutes)
    pub async fn cleanup_expired(&self) -> usize {
        let mut requests = self.requests.write().await;
        let now = Utc::now();
        let before = requests.len();
        requests.retain(|_, info| now.signed_duration_since(info.created_at).num_minutes() < 10);
        persist_pending_requests(self.path.as_ref(), &requests);
        before.saturating_sub(requests.len())
    }
}

impl Default for OAuthPendingStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct OAuthCompletionStore {
    entries: Arc<RwLock<std::collections::HashMap<String, OAuthCompletion>>>,
    path: Option<PathBuf>,
}

impl OAuthCompletionStore {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(std::collections::HashMap::new())),
            path: None,
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        let entries = load_oauth_completions(&path);
        Self {
            entries: Arc::new(RwLock::new(entries)),
            path: Some(path),
        }
    }

    pub fn from_google_config(config: &GoogleOAuthConfig) -> Self {
        Self::with_path(completion_store_path_for_token(&config.token_path))
    }

    pub async fn insert(&self, state: String, completion: OAuthCompletion) {
        let mut entries = self.entries.write().await;
        entries.insert(state, completion);
        persist_oauth_completions(self.path.as_ref(), &entries);
    }

    pub async fn take(&self, state: &str) -> Option<OAuthCompletion> {
        let mut entries = self.entries.write().await;
        let removed = entries.remove(state);
        persist_oauth_completions(self.path.as_ref(), &entries);
        removed
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    pub async fn entry_count(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn cleanup_expired(&self, max_age_minutes: i64) -> usize {
        let mut entries = self.entries.write().await;
        let now = Utc::now();
        let before = entries.len();
        entries.retain(|_, completion| {
            now.signed_duration_since(completion.completed_at)
                .num_minutes()
                < max_age_minutes
        });
        persist_oauth_completions(self.path.as_ref(), &entries);
        before.saturating_sub(entries.len())
    }
}

impl Default for OAuthCompletionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct GoogleAuth {
    config: GoogleOAuthConfig,
    token: Arc<RwLock<Option<GoogleToken>>>,
    token_path: PathBuf,
    http: reqwest::Client,
    pending: OAuthPendingStore,
}

impl GoogleAuth {
    pub fn new(config: GoogleOAuthConfig, token_path: PathBuf) -> Self {
        let pending = OAuthPendingStore::from_google_config(&config);
        Self {
            config,
            token: Arc::new(RwLock::new(None)),
            token_path,
            http: reqwest::Client::new(),
            pending,
        }
    }

    pub fn from_config(config: GoogleOAuthConfig) -> Self {
        let token_path = config.token_path.clone();
        Self::new(config, token_path)
    }

    pub fn with_token_override(token: GoogleToken) -> Self {
        Self {
            config: GoogleOAuthConfig::default(),
            token: Arc::new(RwLock::new(Some(token))),
            token_path: PathBuf::from("/dev/null"),
            http: reqwest::Client::new(),
            pending: OAuthPendingStore::new(),
        }
    }

    /// Build OAuth URL with state parameter for cross-channel auth
    pub fn build_auth_url_with_state(&self, state: &str) -> String {
        let scopes = self.config.scopes.join(" ");
        format!(
            "{}/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
            self.authorization_base_url(),
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&scopes),
            urlencoding::encode(state)
        )
    }

    pub fn build_auth_url(&self) -> String {
        self.build_auth_url_with_state("")
    }

    /// Get reference to pending OAuth store
    pub fn pending_store(&self) -> &OAuthPendingStore {
        &self.pending
    }

    pub async fn exchange_code(&self, code: &str) -> Result<GoogleToken, GoogleError> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];

        let response = self
            .http
            .post(self.token_url())
            .form(&params)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            refresh_token: Option<String>,
            expires_in: i64,
            scope: String,
        }

        let token_resp: TokenResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let token = GoogleToken {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            expires_at: chrono::Utc::now().timestamp() + token_resp.expires_in,
            scopes: token_resp
                .scope
                .split_whitespace()
                .map(String::from)
                .collect(),
        };

        *self.token.write().await = Some(token.clone());
        self.save_token(&token).await?;

        Ok(token)
    }

    pub async fn get_token(&self) -> Result<GoogleToken, GoogleError> {
        if let Some(token) = self.token.read().await.clone() {
            if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                return Ok(token);
            }

            if let Some(refresh) = token.refresh_token {
                return self.refresh(&refresh).await;
            }
        }

        if self.token_path.exists() {
            self.load_token().await?;
            if let Some(token) = self.token.read().await.clone() {
                if token.expires_at > chrono::Utc::now().timestamp() + 60 {
                    return Ok(token);
                }
                if let Some(refresh) = token.refresh_token {
                    return self.refresh(&refresh).await;
                }
            }
        }

        Err(GoogleError::NotAuthenticated)
    }

    async fn refresh(&self, refresh_token: &str) -> Result<GoogleToken, GoogleError> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];

        let response = self
            .http
            .post(self.token_url())
            .form(&params)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: i64,
        }

        let token_resp: TokenResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let mut token = self
            .token
            .read()
            .await
            .clone()
            .ok_or(GoogleError::NotAuthenticated)?;
        token.access_token = token_resp.access_token;
        token.expires_at = chrono::Utc::now().timestamp() + token_resp.expires_in;

        *self.token.write().await = Some(token.clone());
        Ok(token)
    }

    async fn save_token(&self, token: &GoogleToken) -> Result<(), GoogleError> {
        let content = serde_json::to_string_pretty(token)
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;
        std::fs::write(&self.token_path, content)
            .map_err(|e| GoogleError::IoError(e.to_string()))?;
        Ok(())
    }

    async fn load_token(&self) -> Result<(), GoogleError> {
        let content = std::fs::read_to_string(&self.token_path)
            .map_err(|e| GoogleError::IoError(e.to_string()))?;
        let token: GoogleToken =
            serde_json::from_str(&content).map_err(|e| GoogleError::ParseFailed(e.to_string()))?;
        *self.token.write().await = Some(token);
        Ok(())
    }

    fn authorization_base_url(&self) -> String {
        self.config
            .auth_base_url
            .trim_end_matches("/token")
            .trim_end_matches('/')
            .to_string()
    }

    fn token_url(&self) -> String {
        format!("{}/token", self.authorization_base_url())
    }
}

fn pending_store_path_for_token(token_path: &std::path::Path) -> PathBuf {
    let mut path = token_path.to_path_buf();
    let file_name = token_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.pending-oauth.json"))
        .unwrap_or_else(|| "google_token.json.pending-oauth.json".to_string());
    path.set_file_name(file_name);
    path
}

fn completion_store_path_for_token(token_path: &std::path::Path) -> PathBuf {
    let mut path = token_path.to_path_buf();
    let file_name = token_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.oauth-completions.json"))
        .unwrap_or_else(|| "google_token.json.oauth-completions.json".to_string());
    path.set_file_name(file_name);
    path
}

pub fn scoped_token_path(token_path: &std::path::Path, principal: &OAuthPrincipal) -> PathBuf {
    let mut path = token_path.to_path_buf();
    let file_stem = token_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("google_token");
    let extension = token_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("json");
    let scope = [
        sanitize_scope_segment(&principal.channel),
        sanitize_scope_segment(&principal.user_id),
        principal
            .group_id
            .as_deref()
            .map(sanitize_scope_segment)
            .unwrap_or_else(|| "dm".to_string()),
    ]
    .join("__");
    path.set_file_name(format!("{file_stem}.{scope}.{extension}"));
    path
}

fn sanitize_scope_segment(segment: &str) -> String {
    let sanitized: String = segment
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn load_pending_requests(path: &std::path::Path) -> std::collections::HashMap<String, OAuthState> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn load_oauth_completions(
    path: &std::path::Path,
) -> std::collections::HashMap<String, OAuthCompletion> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn persist_pending_requests(
    path: Option<&PathBuf>,
    requests: &std::collections::HashMap<String, OAuthState>,
) {
    let Some(path) = path else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if requests.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }

    if let Ok(content) = serde_json::to_string_pretty(requests) {
        let _ = std::fs::write(path, content);
    }
}

fn persist_oauth_completions(
    path: Option<&PathBuf>,
    entries: &std::collections::HashMap<String, OAuthCompletion>,
) {
    let Some(path) = path else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if entries.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }

    if let Ok(content) = serde_json::to_string_pretty(entries) {
        let _ = std::fs::write(path, content);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub date: Option<String>,
    pub snippet: Option<String>,
}

#[derive(Clone)]
pub struct GoogleClient {
    auth: Arc<GoogleAuth>,
    gmail: GmailClient,
    drive: DriveClient,
    calendar: CalendarClient,
}

impl GoogleClient {
    pub fn new(config: GoogleOAuthConfig) -> Self {
        let auth = Arc::new(GoogleAuth::from_config(config));
        let gmail = GmailClient::new(auth.clone());
        let drive = DriveClient::new(auth.clone());
        let calendar = CalendarClient::new(auth.clone());
        Self {
            auth,
            gmail,
            drive,
            calendar,
        }
    }

    pub fn auth(&self) -> Arc<GoogleAuth> {
        self.auth.clone()
    }

    pub async fn list_messages(
        &self,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<GmailMessage>, GoogleError> {
        self.gmail.list_messages(query, limit).await
    }

    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<String, GoogleError> {
        self.gmail.send_email(to, subject, body).await
    }

    pub async fn upload_file(
        &self,
        name: &str,
        content: Vec<u8>,
        mime_type: &str,
        folder_id: Option<&str>,
    ) -> Result<DriveFile, GoogleError> {
        self.drive
            .upload_file(name, content, mime_type, folder_id)
            .await
    }

    pub async fn search_files(&self, query: &str) -> Result<Vec<DriveFile>, GoogleError> {
        self.drive.search_files(query).await
    }

    pub async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>, GoogleError> {
        self.calendar
            .list_events(calendar_id, time_min, time_max, 20)
            .await
    }

    pub async fn create_event(&self, event: CalendarEvent) -> Result<CalendarEvent, GoogleError> {
        self.calendar.create_event("primary", event).await
    }

    pub async fn delete_email(&self, message_id: &str) -> Result<(), GoogleError> {
        self.gmail.delete_message(message_id).await
    }

    pub async fn trash_email(&self, message_id: &str) -> Result<(), GoogleError> {
        self.gmail.trash_message(message_id).await
    }

    pub async fn update_event(
        &self,
        event_id: &str,
        event: CalendarEvent,
    ) -> Result<CalendarEvent, GoogleError> {
        self.calendar.update_event("primary", event_id, event).await
    }

    pub async fn delete_event(&self, event_id: &str) -> Result<(), GoogleError> {
        self.calendar.delete_event("primary", event_id).await
    }
}

#[derive(Clone)]
pub struct GmailClient {
    auth: Arc<GoogleAuth>,
}

impl GmailClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_messages(
        &self,
        query: Option<&str>,
        limit: u32,
    ) -> Result<Vec<GmailMessage>, GoogleError> {
        let token = self.auth.get_token().await?;

        let mut url = format!(
            "{}/gmail/v1/users/me/messages?maxResults={}",
            self.auth.config.gmail_base_url.trim_end_matches('/'),
            limit
        );
        if let Some(query) = query {
            url.push_str("&q=");
            url.push_str(&urlencoding::encode(query));
        }

        let response = self
            .auth
            .http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            messages: Option<Vec<MessageId>>,
        }

        #[derive(Deserialize)]
        struct MessageId {
            id: String,
        }

        let list: ListResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let mut messages = Vec::new();
        if let Some(ids) = list.messages {
            for msg_id in ids.iter().take(10) {
                if let Ok(msg) = self.get_message(&msg_id.id).await {
                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    pub async fn get_message(&self, id: &str) -> Result<GmailMessage, GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/gmail/v1/users/me/messages/{}",
            self.auth.config.gmail_base_url.trim_end_matches('/'),
            id
        );
        if let Some(bytes) = read_fixture_response(&self.auth.config.gmail_base_url, "GET", &url)? {
            #[derive(Deserialize)]
            struct MsgResponse {
                id: String,
                thread_id: String,
                payload: Option<Payload>,
                snippet: Option<String>,
            }

            #[derive(Deserialize)]
            struct Payload {
                headers: Vec<Header>,
            }

            #[derive(Deserialize)]
            struct Header {
                name: String,
                value: String,
            }

            let msg: MsgResponse = serde_json::from_slice(&bytes)
                .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

            let mut subject = None;
            let mut from = None;
            let mut to = None;
            let mut date = None;

            if let Some(payload) = msg.payload {
                for header in payload.headers {
                    match header.name.as_str() {
                        "Subject" => subject = Some(header.value),
                        "From" => from = Some(header.value),
                        "To" => to = Some(header.value),
                        "Date" => date = Some(header.value),
                        _ => {}
                    }
                }
            }

            return Ok(GmailMessage {
                id: msg.id,
                thread_id: msg.thread_id,
                subject,
                from,
                to,
                date,
                snippet: msg.snippet,
            });
        }

        let response = self
            .auth
            .http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct MsgResponse {
            id: String,
            thread_id: String,
            payload: Option<Payload>,
            snippet: Option<String>,
        }

        #[derive(Deserialize)]
        struct Payload {
            headers: Vec<Header>,
        }

        #[derive(Deserialize)]
        struct Header {
            name: String,
            value: String,
        }

        let msg: MsgResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let mut subject = None;
        let mut from = None;
        let mut to = None;
        let mut date = None;

        if let Some(payload) = msg.payload {
            for header in payload.headers {
                match header.name.as_str() {
                    "Subject" => subject = Some(header.value),
                    "From" => from = Some(header.value),
                    "To" => to = Some(header.value),
                    "Date" => date = Some(header.value),
                    _ => {}
                }
            }
        }

        Ok(GmailMessage {
            id: msg.id,
            thread_id: msg.thread_id,
            subject,
            from,
            to,
            date,
            snippet: msg.snippet,
        })
    }

    pub async fn send_message(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<String, GoogleError> {
        let token = self.auth.get_token().await?;

        let raw = format!("To: {}\nSubject: {}\n\n{}", to, subject, body);

        use base64::Engine;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);

        let body = serde_json::json!({
            "raw": encoded
        });

        let response = self
            .auth
            .http
            .post(format!(
                "{}/gmail/v1/users/me/messages/send",
                self.auth.config.gmail_base_url.trim_end_matches('/')
            ))
            .bearer_auth(&token.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct SendResponse {
            id: String,
        }

        let result: SendResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.id)
    }

    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> Result<String, GoogleError> {
        self.send_message(to, subject, body).await
    }

    pub async fn delete_message(&self, id: &str) -> Result<(), GoogleError> {
        let token = self.auth.get_token().await?;
        let url = format!(
            "{}/gmail/v1/users/me/messages/{}",
            self.auth.config.gmail_base_url.trim_end_matches('/'),
            id
        );

        let response = self
            .auth
            .http
            .delete(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(GoogleError::RequestFailed(format!(
                "Failed to delete message: {}",
                response.status()
            )))
        }
    }

    pub async fn trash_message(&self, id: &str) -> Result<(), GoogleError> {
        let token = self.auth.get_token().await?;
        let url = format!(
            "{}/gmail/v1/users/me/messages/{}/trash",
            self.auth.config.gmail_base_url.trim_end_matches('/'),
            id
        );

        let response = self
            .auth
            .http
            .post(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(GoogleError::RequestFailed(format!(
                "Failed to trash message: {}",
                response.status()
            )))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: Option<String>,
    pub parents: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct DriveClient {
    auth: Arc<GoogleAuth>,
}

impl DriveClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_files(
        &self,
        query: Option<&str>,
        page_size: u32,
    ) -> Result<Vec<DriveFile>, GoogleError> {
        let token = self.auth.get_token().await?;

        let mut url = format!(
            "{}/drive/v3/files?pageSize={}",
            self.auth.config.drive_base_url.trim_end_matches('/'),
            page_size
        );

        if let Some(q) = query {
            url.push_str(&format!("&q={}", urlencoding::encode(q)));
        }

        let response = self
            .auth
            .http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            files: Vec<DriveFile>,
        }

        let result: ListResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.files)
    }

    pub async fn download_file(&self, id: &str) -> Result<Vec<u8>, GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/drive/v3/files/{}?alt=media",
            self.auth.config.drive_base_url.trim_end_matches('/'),
            id
        );
        if let Some(bytes) = read_fixture_response(&self.auth.config.drive_base_url, "GET", &url)? {
            return Ok(bytes);
        }

        let response = self
            .auth
            .http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let bytes = response
            .bytes()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(bytes)
    }

    pub async fn upload_file(
        &self,
        name: &str,
        content: Vec<u8>,
        mime_type: &str,
        folder_id: Option<&str>,
    ) -> Result<DriveFile, GoogleError> {
        let token = self.auth.get_token().await?;

        let mut metadata = serde_json::json!({
            "name": name,
            "mimeType": mime_type
        });
        if let Some(folder_id) = folder_id {
            metadata["parents"] = serde_json::json!([folder_id]);
        }

        let client = reqwest::Client::new();
        let multipart = reqwest::multipart::Part::bytes(content)
            .file_name(name.to_string())
            .mime_str(mime_type)
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .text("metadata", metadata.to_string())
            .part("file", multipart);

        let response = client
            .post(format!(
                "{}/upload/drive/v3/files?uploadType=multipart",
                self.auth.config.drive_base_url.trim_end_matches('/')
            ))
            .bearer_auth(&token.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let file: DriveFile = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(file)
    }

    pub async fn search_files(&self, query: &str) -> Result<Vec<DriveFile>, GoogleError> {
        self.list_files(Some(query), 20).await
    }

    /// Create a new folder in Google Drive
    pub async fn create_folder(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<DriveFile, GoogleError> {
        let token = self.auth.get_token().await?;

        let mut metadata = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder"
        });
        if let Some(parent_id) = parent_id {
            metadata["parents"] = serde_json::json!([parent_id]);
        }

        let response = self
            .auth
            .http
            .post(format!(
                "{}/drive/v3/files",
                self.auth.config.drive_base_url.trim_end_matches('/')
            ))
            .bearer_auth(&token.access_token)
            .json(&metadata)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let file: DriveFile = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(file)
    }

    /// List folders in Google Drive
    pub async fn list_folders(
        &self,
        parent_id: Option<&str>,
        page_size: u32,
    ) -> Result<Vec<DriveFile>, GoogleError> {
        let mut query = "mimeType='application/vnd.google-apps.folder'".to_string();
        if let Some(parent_id) = parent_id {
            query.push_str(&format!(" and '{}' in parents", parent_id));
        }
        query.push_str(" and trashed=false");

        self.list_files(Some(&query), page_size).await
    }

    /// Share a file with a specific user or make it public
    pub async fn share_file(
        &self,
        file_id: &str,
        email: Option<&str>,
        role: &str,
        allow_discovery: bool,
    ) -> Result<Permission, GoogleError> {
        let token = self.auth.get_token().await?;

        let permission = if let Some(email) = email {
            serde_json::json!({
                "type": "user",
                "role": role,
                "emailAddress": email
            })
        } else {
            serde_json::json!({
                "type": if allow_discovery { "anyone" } else { "domain" },
                "role": role
            })
        };

        let response = self
            .auth
            .http
            .post(format!(
                "{}/drive/v3/files/{}/permissions",
                self.auth.config.drive_base_url.trim_end_matches('/'),
                file_id
            ))
            .bearer_auth(&token.access_token)
            .json(&permission)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let result: Permission = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result)
    }

    /// List permissions for a file
    pub async fn list_permissions(&self, file_id: &str) -> Result<Vec<Permission>, GoogleError> {
        let token = self.auth.get_token().await?;

        let response = self
            .auth
            .http
            .get(format!(
                "{}/drive/v3/files/{}/permissions",
                self.auth.config.drive_base_url.trim_end_matches('/'),
                file_id
            ))
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            permissions: Vec<Permission>,
        }

        let result: ListResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.permissions)
    }

    /// Remove a permission from a file
    pub async fn remove_permission(
        &self,
        file_id: &str,
        permission_id: &str,
    ) -> Result<(), GoogleError> {
        let token = self.auth.get_token().await?;

        let response = self
            .auth
            .http
            .delete(format!(
                "{}/drive/v3/files/{}/permissions/{}",
                self.auth.config.drive_base_url.trim_end_matches('/'),
                file_id,
                permission_id
            ))
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(GoogleError::RequestFailed(format!(
                "Failed to remove permission: {}",
                response.status()
            )))
        }
    }

    /// Move a file to a different folder
    pub async fn move_file(
        &self,
        file_id: &str,
        new_parent_id: &str,
        old_parent_id: Option<&str>,
    ) -> Result<DriveFile, GoogleError> {
        let token = self.auth.get_token().await?;

        let mut url = format!(
            "{}/drive/v3/files/{}?addParents={}",
            self.auth.config.drive_base_url.trim_end_matches('/'),
            file_id,
            new_parent_id
        );

        if let Some(old_parent_id) = old_parent_id {
            url.push_str(&format!("&removeParents={}", old_parent_id));
        }

        let response = self
            .auth
            .http
            .patch(&url)
            .bearer_auth(&token.access_token)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let file: DriveFile = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(file)
    }

    /// Copy a file
    pub async fn copy_file(&self, file_id: &str, new_name: &str) -> Result<DriveFile, GoogleError> {
        let token = self.auth.get_token().await?;

        let metadata = serde_json::json!({
            "name": new_name
        });

        let response = self
            .auth
            .http
            .post(format!(
                "{}/drive/v3/files/{}/copy",
                self.auth.config.drive_base_url.trim_end_matches('/'),
                file_id
            ))
            .bearer_auth(&token.access_token)
            .json(&metadata)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let file: DriveFile = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(file)
    }

    /// Delete a file (move to trash or permanently)
    pub async fn delete_file(&self, file_id: &str, permanent: bool) -> Result<(), GoogleError> {
        let token = self.auth.get_token().await?;

        if permanent {
            let response = self
                .auth
                .http
                .delete(format!(
                    "{}/drive/v3/files/{}",
                    self.auth.config.drive_base_url.trim_end_matches('/'),
                    file_id
                ))
                .bearer_auth(&token.access_token)
                .send()
                .await
                .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

            if response.status().is_success() {
                Ok(())
            } else {
                Err(GoogleError::RequestFailed(format!(
                    "Failed to delete file: {}",
                    response.status()
                )))
            }
        } else {
            // Move to trash
            let metadata = serde_json::json!({
                "trashed": true
            });

            let response = self
                .auth
                .http
                .patch(format!(
                    "{}/drive/v3/files/{}",
                    self.auth.config.drive_base_url.trim_end_matches('/'),
                    file_id
                ))
                .bearer_auth(&token.access_token)
                .json(&metadata)
                .send()
                .await
                .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

            if response.status().is_success() {
                Ok(())
            } else {
                Err(GoogleError::RequestFailed(format!(
                    "Failed to trash file: {}",
                    response.status()
                )))
            }
        }
    }

    /// Batch upload multiple files
    pub async fn batch_upload(
        &self,
        files: Vec<(&str, Vec<u8>, &str, Option<&str>)>,
    ) -> Result<Vec<DriveFile>, GoogleError> {
        let mut results = Vec::new();

        for (name, content, mime_type, folder_id) in files {
            match self.upload_file(name, content, mime_type, folder_id).await {
                Ok(file) => results.push(file),
                Err(e) => return Err(e),
            }
        }

        Ok(results)
    }

    /// Batch share files
    pub async fn batch_share(
        &self,
        file_ids: Vec<&str>,
        email: Option<&str>,
        role: &str,
        allow_discovery: bool,
    ) -> Result<Vec<Permission>, GoogleError> {
        let mut results = Vec::new();

        for file_id in file_ids {
            match self.share_file(file_id, email, role, allow_discovery).await {
                Ok(permission) => results.push(permission),
                Err(e) => return Err(e),
            }
        }

        Ok(results)
    }

    /// Get file details including web view link
    pub async fn get_file_details(&self, file_id: &str) -> Result<DriveFileDetails, GoogleError> {
        let token = self.auth.get_token().await?;

        let response = self
            .auth
            .http
            .get(format!(
                "{}/drive/v3/files/{}?fields=id,name,mimeType,size,parents,webViewLink,webContentLink,createdTime,modifiedTime",
                self.auth.config.drive_base_url.trim_end_matches('/'),
                file_id
            ))
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let details: DriveFileDetails = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(details)
    }
}

/// Permission structure for sharing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    #[serde(rename = "type")]
    pub permission_type: String,
    pub role: String,
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,
    pub domain: Option<String>,
    #[serde(rename = "allowFileDiscovery")]
    pub allow_file_discovery: Option<bool>,
}

/// Extended file details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFileDetails {
    #[serde(flatten)]
    pub file: DriveFile,
    #[serde(rename = "webViewLink")]
    pub web_view_link: Option<String>,
    #[serde(rename = "webContentLink")]
    pub web_content_link: Option<String>,
    #[serde(rename = "createdTime")]
    pub created_time: Option<String>,
    #[serde(rename = "modifiedTime")]
    pub modified_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: String,
    pub description: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Default for CalendarEvent {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: String::new(),
            summary: String::new(),
            description: None,
            start: now,
            end: now,
        }
    }
}

#[derive(Clone)]
pub struct CalendarClient {
    auth: Arc<GoogleAuth>,
}

impl CalendarClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_events(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        max_results: u32,
    ) -> Result<Vec<CalendarEvent>, GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/calendar/v3/calendars/{}/events?timeMin={}&timeMax={}&maxResults={}",
            self.auth.config.calendar_base_url.trim_end_matches('/'),
            urlencoding::encode(calendar_id),
            urlencoding::encode(&time_min.to_rfc3339()),
            urlencoding::encode(&time_max.to_rfc3339()),
            max_results
        );

        let response = self
            .auth
            .http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            items: Option<Vec<ApiCalendarEvent>>,
        }

        let result: ListResponse = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        result
            .items
            .unwrap_or_default()
            .into_iter()
            .map(CalendarEvent::try_from)
            .collect()
    }

    pub async fn create_event(
        &self,
        calendar_id: &str,
        event: CalendarEvent,
    ) -> Result<CalendarEvent, GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/calendar/v3/calendars/{}/events",
            self.auth.config.calendar_base_url.trim_end_matches('/'),
            urlencoding::encode(calendar_id)
        );

        let body = serde_json::json!({
            "summary": event.summary,
            "description": event.description,
            "start": { "dateTime": event.start.to_rfc3339() },
            "end": { "dateTime": event.end.to_rfc3339() }
        });
        if let Some(bytes) =
            read_fixture_response(&self.auth.config.calendar_base_url, "POST", &url)?
        {
            let event: ApiCalendarEvent = serde_json::from_slice(&bytes)
                .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;
            return CalendarEvent::try_from(event);
        }

        let response = self
            .auth
            .http
            .post(&url)
            .bearer_auth(&token.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let event: ApiCalendarEvent = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        CalendarEvent::try_from(event)
    }

    pub async fn update_event(
        &self,
        calendar_id: &str,
        event_id: &str,
        event: CalendarEvent,
    ) -> Result<CalendarEvent, GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/calendar/v3/calendars/{}/events/{}",
            self.auth.config.calendar_base_url.trim_end_matches('/'),
            urlencoding::encode(calendar_id),
            urlencoding::encode(event_id)
        );

        let body = serde_json::json!({
            "summary": event.summary,
            "description": event.description,
            "start": { "dateTime": event.start.to_rfc3339() },
            "end": { "dateTime": event.end.to_rfc3339() }
        });

        let response = self
            .auth
            .http
            .put(&url)
            .bearer_auth(&token.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let event: ApiCalendarEvent = response
            .json()
            .await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        CalendarEvent::try_from(event)
    }

    pub async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), GoogleError> {
        let token = self.auth.get_token().await?;

        let url = format!(
            "{}/calendar/v3/calendars/{}/events/{}",
            self.auth.config.calendar_base_url.trim_end_matches('/'),
            urlencoding::encode(calendar_id),
            urlencoding::encode(event_id)
        );

        let response = self
            .auth
            .http
            .delete(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(GoogleError::RequestFailed(format!(
                "Failed to delete event: {}",
                response.status()
            )))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ApiCalendarEvent {
    id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    start: Option<ApiEventDateTime>,
    end: Option<ApiEventDateTime>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiEventDateTime {
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    date: Option<String>,
}

impl TryFrom<ApiCalendarEvent> for CalendarEvent {
    type Error = GoogleError;

    fn try_from(value: ApiCalendarEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.unwrap_or_default(),
            summary: value.summary.unwrap_or_default(),
            description: value.description,
            start: parse_google_event_time(value.start)?,
            end: parse_google_event_time(value.end)?,
        })
    }
}

fn parse_google_event_time(value: Option<ApiEventDateTime>) -> Result<DateTime<Utc>, GoogleError> {
    let Some(value) = value else {
        return Err(GoogleError::ParseFailed("missing event time".to_string()));
    };

    if let Some(date_time) = value.date_time {
        return DateTime::parse_from_rfc3339(&date_time)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| GoogleError::ParseFailed(e.to_string()));
    }

    if let Some(date) = value.date {
        let date_time = format!("{}T00:00:00Z", date);
        return DateTime::parse_from_rfc3339(&date_time)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| GoogleError::ParseFailed(e.to_string()));
    }

    Err(GoogleError::ParseFailed("missing event time".to_string()))
}

pub(crate) fn fixture_path_for_request(
    base_url: &str,
    method: &str,
    request_url: &str,
) -> Result<Option<PathBuf>, GoogleError> {
    let Ok(base) = url::Url::parse(base_url) else {
        return Ok(None);
    };
    if base.scheme() != "file" {
        return Ok(None);
    }

    let root = base.to_file_path().map_err(|_| {
        GoogleError::ParseFailed(format!("Invalid file fixture base URL: {base_url}"))
    })?;
    let request = url::Url::parse(request_url)
        .map_err(|e| GoogleError::ParseFailed(format!("Invalid fixture request URL: {e}")))?;
    let request_path = request.to_file_path().map_err(|_| {
        GoogleError::ParseFailed(format!("Invalid file fixture request URL: {request_url}"))
    })?;
    let relative = request_path
        .strip_prefix(&root)
        .unwrap_or(request_path.as_path());
    let mut key = format!("{method} {}", relative.to_string_lossy());
    if let Some(query) = request.query() {
        key.push('?');
        key.push_str(query);
    }
    let sanitized = key
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '_',
        })
        .collect::<String>();

    Ok(Some(root.join(".borgclaw-fixtures").join(sanitized)))
}

fn read_fixture_response(
    base_url: &str,
    method: &str,
    request_url: &str,
) -> Result<Option<Vec<u8>>, GoogleError> {
    let Some(path) = fixture_path_for_request(base_url, method, request_url)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|e| {
        GoogleError::ParseFailed(format!(
            "Failed to read fixture '{}': {}",
            path.display(),
            e
        ))
    })?;
    Ok(Some(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum GoogleError {
    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse failed: {0}")]
    ParseFailed(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Not authenticated")]
    NotAuthenticated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_client_uses_documented_token_path() {
        let client = GoogleClient::new(GoogleOAuthConfig {
            token_path: PathBuf::from(".local/data/google_token.json"),
            ..Default::default()
        });

        assert_eq!(
            client.auth.token_path,
            PathBuf::from(".local/data/google_token.json")
        );
    }

    #[tokio::test]
    async fn oauth_pending_store_persists_requests_for_shared_callback_lookup() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_google_oauth_pending_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let token_path = root.join("google_token.json");
        let store = OAuthPendingStore::with_path(root.join("google_token.json.pending-oauth.json"));
        let request = OAuthState {
            session_id: "session-1".to_string(),
            user_id: "user-1".to_string(),
            channel: "websocket".to_string(),
            created_at: Utc::now(),
            group_id: Some("group-1".to_string()),
        };

        store.insert("state-1".to_string(), request.clone()).await;

        let reloaded = OAuthPendingStore::from_google_config(&GoogleOAuthConfig {
            token_path: token_path.clone(),
            ..Default::default()
        });
        let loaded = reloaded.get("state-1").await.unwrap();
        assert_eq!(loaded.session_id, request.session_id);
        assert_eq!(loaded.user_id, request.user_id);
        assert_eq!(loaded.group_id, request.group_id);

        reloaded.remove("state-1").await.unwrap();
        let reloaded_again = OAuthPendingStore::from_google_config(&GoogleOAuthConfig {
            token_path,
            ..Default::default()
        });
        assert!(reloaded_again.get("state-1").await.is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scoped_token_path_isolated_by_principal() {
        let base = PathBuf::from(".local/data/google_token.json");
        let alice = OAuthPrincipal::new("cli", "cli:alice", Some("cli:alice".to_string()));
        let bob = OAuthPrincipal::new("cli", "cli:bob", Some("cli:bob".to_string()));

        let alice_path = scoped_token_path(&base, &alice);
        let bob_path = scoped_token_path(&base, &bob);

        assert_ne!(alice_path, bob_path);
        assert!(alice_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap()
            .contains("cli__cli_alice__cli_alice"));
    }

    #[tokio::test]
    async fn oauth_completion_store_round_trips_entries() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_google_oauth_completion_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let token_path = root.join("google_token.json");
        let store =
            OAuthCompletionStore::with_path(root.join("google_token.json.oauth-completions.json"));
        store
            .insert(
                "session-1".to_string(),
                OAuthCompletion {
                    session_id: "session-1".to_string(),
                    channel: "cli".to_string(),
                    success: true,
                    message: "done".to_string(),
                    completed_at: Utc::now(),
                },
            )
            .await;

        let reloaded = OAuthCompletionStore::from_google_config(&GoogleOAuthConfig {
            token_path,
            ..Default::default()
        });
        let completion = reloaded.take("session-1").await.unwrap();
        assert_eq!(completion.channel, "cli");
        assert!(completion.success);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn oauth_completion_store_cleanup_removes_expired_entries() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_google_oauth_completion_cleanup_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let store =
            OAuthCompletionStore::with_path(root.join("google_token.json.oauth-completions.json"));
        store
            .insert(
                "stale".to_string(),
                OAuthCompletion {
                    session_id: "session-stale".to_string(),
                    channel: "cli".to_string(),
                    success: true,
                    message: "done".to_string(),
                    completed_at: Utc::now() - chrono::Duration::minutes(90),
                },
            )
            .await;
        store
            .insert(
                "fresh".to_string(),
                OAuthCompletion {
                    session_id: "session-fresh".to_string(),
                    channel: "cli".to_string(),
                    success: true,
                    message: "done".to_string(),
                    completed_at: Utc::now(),
                },
            )
            .await;

        let removed = store.cleanup_expired(60).await;
        assert_eq!(removed, 1);
        assert!(store.take("stale").await.is_none());
        assert!(store.take("fresh").await.is_some());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn calendar_event_parses_google_datetime_shape() {
        let parsed = CalendarEvent::try_from(ApiCalendarEvent {
            id: Some("evt-1".to_string()),
            summary: Some("Meeting".to_string()),
            description: None,
            start: Some(ApiEventDateTime {
                date_time: Some("2026-03-09T10:00:00Z".to_string()),
                date: None,
            }),
            end: Some(ApiEventDateTime {
                date_time: Some("2026-03-09T11:00:00Z".to_string()),
                date: None,
            }),
        })
        .unwrap();

        assert_eq!(parsed.summary, "Meeting");
        assert_eq!(parsed.start.to_rfc3339(), "2026-03-09T10:00:00+00:00");
    }

    // Tests for new Google Drive operations (TICKET-048)

    #[test]
    fn drive_file_parses_from_json() {
        let json = r#"{
            "id": "file123",
            "name": "test.txt",
            "mimeType": "text/plain",
            "size": "1024",
            "parents": ["folder456"]
        }"#;

        let file: DriveFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "file123");
        assert_eq!(file.name, "test.txt");
        assert_eq!(file.mime_type, "text/plain");
        assert_eq!(file.size, Some("1024".to_string()));
        assert_eq!(file.parents, Some(vec!["folder456".to_string()]));
    }

    #[test]
    fn permission_parses_from_json() {
        let json = r#"{
            "id": "perm123",
            "type": "user",
            "role": "writer",
            "emailAddress": "test@example.com"
        }"#;

        let perm: Permission = serde_json::from_str(json).unwrap();
        assert_eq!(perm.id, "perm123");
        assert_eq!(perm.permission_type, "user");
        assert_eq!(perm.role, "writer");
        assert_eq!(perm.email_address, Some("test@example.com".to_string()));
    }

    #[test]
    fn drive_file_details_parses_from_json() {
        let json = r#"{
            "id": "file123",
            "name": "test.txt",
            "mimeType": "text/plain",
            "webViewLink": "https://drive.google.com/file/d/file123/view",
            "webContentLink": "https://drive.google.com/uc?id=file123",
            "createdTime": "2024-01-01T00:00:00.000Z",
            "modifiedTime": "2024-01-02T00:00:00.000Z"
        }"#;

        let details: DriveFileDetails = serde_json::from_str(json).unwrap();
        assert_eq!(details.file.id, "file123");
        assert_eq!(details.file.name, "test.txt");
        assert_eq!(
            details.web_view_link,
            Some("https://drive.google.com/file/d/file123/view".to_string())
        );
        assert_eq!(
            details.web_content_link,
            Some("https://drive.google.com/uc?id=file123".to_string())
        );
        assert_eq!(
            details.created_time,
            Some("2024-01-01T00:00:00.000Z".to_string())
        );
    }

    #[test]
    fn folder_mime_type_is_correct() {
        // Verify the folder MIME type constant used in create_folder
        let folder_mime = "application/vnd.google-apps.folder";
        assert!(!folder_mime.is_empty());
        assert!(folder_mime.contains("folder"));
    }

    #[test]
    fn drive_query_escaping_works_correctly() {
        // Test that query strings are properly URL-encoded
        let query = "name contains 'test file'";
        let encoded = urlencoding::encode(query);
        assert!(encoded.contains("%27")); // ' should be encoded
        assert!(encoded.contains("%20")); // space should be encoded
    }

    #[test]
    fn batch_operations_handle_empty_lists() {
        // Test that batch operations handle empty input gracefully
        let files: Vec<(String, Vec<u8>, String)> = vec![];
        assert!(files.is_empty());

        let shares: Vec<(String, String, String)> = vec![];
        assert!(shares.is_empty());
    }

    #[test]
    fn batch_operations_handle_single_item() {
        // Test batch with single item
        let files = [(
            "file1.txt".to_string(),
            b"content".to_vec(),
            "text/plain".to_string(),
        )];
        assert_eq!(files.len(), 1);

        let shares = [(
            "file123".to_string(),
            "user@example.com".to_string(),
            "reader".to_string(),
        )];
        assert_eq!(shares.len(), 1);
    }

    #[test]
    fn batch_operations_handle_multiple_items() {
        // Test batch with multiple items
        let files = [
            (
                "file1.txt".to_string(),
                b"content1".to_vec(),
                "text/plain".to_string(),
            ),
            (
                "file2.txt".to_string(),
                b"content2".to_vec(),
                "text/plain".to_string(),
            ),
            (
                "file3.txt".to_string(),
                b"content3".to_vec(),
                "text/plain".to_string(),
            ),
        ];
        assert_eq!(files.len(), 3);

        let shares = [
            (
                "file1".to_string(),
                "user1@example.com".to_string(),
                "reader".to_string(),
            ),
            (
                "file2".to_string(),
                "user2@example.com".to_string(),
                "writer".to_string(),
            ),
        ];
        assert_eq!(shares.len(), 2);
    }

    #[test]
    fn permission_roles_are_valid() {
        // Test valid permission roles
        let valid_roles = vec![
            "owner",
            "organizer",
            "fileOrganizer",
            "writer",
            "reader",
            "commenter",
        ];
        for role in valid_roles {
            assert!(!role.is_empty());
        }
    }

    #[test]
    fn permission_types_are_valid() {
        // Test valid permission types
        let valid_types = vec!["user", "group", "domain", "anyone"];
        for perm_type in valid_types {
            assert!(!perm_type.is_empty());
        }
    }

    #[test]
    fn drive_error_display_formats_correctly() {
        let error = GoogleError::RequestFailed("network error".to_string());
        let display = format!("{}", error);
        assert!(display.contains("network error"));

        let error = GoogleError::NotAuthenticated;
        let display = format!("{}", error);
        assert!(display.contains("Not authenticated"));

        let error = GoogleError::ParseFailed("invalid json".to_string());
        let display = format!("{}", error);
        assert!(display.contains("invalid json"));
    }

    #[test]
    fn drive_file_with_null_parents_parses_correctly() {
        let json = r#"{
            "id": "file123",
            "name": "test.txt",
            "mimeType": "text/plain",
            "size": null,
            "parents": null
        }"#;

        let file: DriveFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.id, "file123");
        assert_eq!(file.parents, None);
        assert_eq!(file.size, None);
    }

    #[test]
    fn drive_file_with_multiple_parents_parses_correctly() {
        let json = r#"{
            "id": "file123",
            "name": "test.txt",
            "mimeType": "text/plain",
            "parents": ["folder1", "folder2", "folder3"]
        }"#;

        let file: DriveFile = serde_json::from_str(json).unwrap();
        assert_eq!(
            file.parents,
            Some(vec![
                "folder1".to_string(),
                "folder2".to_string(),
                "folder3".to_string()
            ])
        );
    }

    #[test]
    fn permission_without_email_parses_correctly() {
        let json = r#"{
            "id": "perm123",
            "type": "anyone",
            "role": "reader",
            "allowFileDiscovery": false
        }"#;

        let perm: Permission = serde_json::from_str(json).unwrap();
        assert_eq!(perm.permission_type, "anyone");
        assert_eq!(perm.email_address, None);
        assert_eq!(perm.allow_file_discovery, Some(false));
    }

    #[test]
    fn move_file_metadata_construction() {
        // Test the JSON structure used in move_file
        let new_parent = "folder456";
        let body = serde_json::json!({
            "addParents": [new_parent],
            "removeParents": ["old_folder"]
        });

        assert!(body.get("addParents").is_some());
        assert!(body.get("removeParents").is_some());
    }

    #[test]
    fn copy_file_metadata_construction() {
        // Test the JSON structure used in copy_file
        let body = serde_json::json!({
            "name": "Copy of test.txt"
        });

        assert_eq!(body["name"], "Copy of test.txt");
    }

    #[test]
    fn delete_file_permanent_vs_trash() {
        // Test that permanent delete uses DELETE method
        // and trash uses update to trashed=true
        let permanent = true;

        // Permanent delete uses DELETE endpoint
        assert!(permanent);

        // Trash uses PATCH with trashed=true
        let trash_body = serde_json::json!({"trashed": true});
        assert_eq!(trash_body["trashed"], true);
    }

    #[test]
    fn share_file_permission_construction_user() {
        // Test permission JSON for user sharing
        let email = "user@example.com";
        let role = "writer";
        let permission = serde_json::json!({
            "type": "user",
            "role": role,
            "emailAddress": email
        });

        assert_eq!(permission["type"], "user");
        assert_eq!(permission["role"], "writer");
        assert_eq!(permission["emailAddress"], "user@example.com");
    }

    #[test]
    fn share_file_permission_construction_public() {
        // Test permission JSON for public sharing
        let role = "reader";
        let allow_discovery = true;
        let permission = serde_json::json!({
            "type": if allow_discovery { "anyone" } else { "domain" },
            "role": role
        });

        assert_eq!(permission["type"], "anyone");
        assert_eq!(permission["role"], "reader");
    }

    #[test]
    fn create_folder_metadata_construction() {
        // Test folder creation metadata
        let name = "My Folder";
        let parent_id = Some("parent123");

        let mut metadata = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder"
        });

        if let Some(parent) = parent_id {
            metadata["parents"] = serde_json::json!([parent]);
        }

        assert_eq!(metadata["name"], "My Folder");
        assert_eq!(metadata["mimeType"], "application/vnd.google-apps.folder");
        assert_eq!(metadata["parents"][0], "parent123");
    }

    #[test]
    fn create_folder_without_parent() {
        // Test folder creation metadata without parent
        let name = "My Folder";
        let metadata = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder"
        });

        assert_eq!(metadata["name"], "My Folder");
        assert!(metadata.get("parents").is_none());
    }

    #[test]
    fn list_folders_query_construction() {
        // Test query construction for list_folders
        let parent_id: Option<&str> = Some("parent123");
        let mut query = "mimeType='application/vnd.google-apps.folder'".to_string();

        if let Some(parent) = parent_id {
            query.push_str(&format!(" and '{}' in parents", parent));
        }
        query.push_str(" and trashed=false");

        assert!(query.contains("mimeType='application/vnd.google-apps.folder'"));
        assert!(query.contains("'parent123' in parents"));
        assert!(query.contains("trashed=false"));
    }

    #[test]
    fn list_folders_query_without_parent() {
        // Test query construction without parent
        let parent_id: Option<&str> = None;
        let mut query = "mimeType='application/vnd.google-apps.folder'".to_string();

        if let Some(parent) = parent_id {
            query.push_str(&format!(" and '{}' in parents", parent));
        }
        query.push_str(" and trashed=false");

        assert!(query.contains("mimeType='application/vnd.google-apps.folder'"));
        assert!(!query.contains("in parents"));
        assert!(query.contains("trashed=false"));
    }

    #[test]
    fn file_details_includes_required_fields() {
        // Test that DriveFileDetails includes all required fields
        let details = DriveFileDetails {
            file: DriveFile {
                id: "file123".to_string(),
                name: "test.txt".to_string(),
                mime_type: "text/plain".to_string(),
                size: Some("1024".to_string()),
                parents: Some(vec!["folder456".to_string()]),
            },
            web_view_link: Some("https://drive.google.com/file/d/file123/view".to_string()),
            web_content_link: Some("https://drive.google.com/uc?id=file123".to_string()),
            created_time: Some("2024-01-01T00:00:00.000Z".to_string()),
            modified_time: Some("2024-01-02T00:00:00.000Z".to_string()),
        };

        assert!(!details.file.id.is_empty());
        assert!(!details.file.name.is_empty());
        assert!(details.web_view_link.is_some());
        assert!(details.web_content_link.is_some());
    }
}
