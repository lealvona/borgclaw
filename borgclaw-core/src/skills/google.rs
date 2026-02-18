//! Google Workspace integration - OAuth2, Gmail, Drive, Calendar, Docs, Sheets

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

impl Default for GoogleOAuthConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: "http://localhost:8085/oauth/callback".to_string(),
            scopes: vec![
                "https://www.googleapis.com/auth/gmail.readonly".to_string(),
                "https://www.googleapis.com/auth/gmail.send".to_string(),
                "https://www.googleapis.com/auth/drive.readonly".to_string(),
                "https://www.googleapis.com/auth/drive.file".to_string(),
                "https://www.googleapis.com/auth/calendar.readonly".to_string(),
                "https://www.googleapis.com/auth/calendar.events".to_string(),
            ],
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

#[derive(Clone)]
pub struct GoogleAuth {
    config: GoogleOAuthConfig,
    token: Arc<RwLock<Option<GoogleToken>>>,
    token_path: PathBuf,
    http: reqwest::Client,
}

impl GoogleAuth {
    pub fn new(config: GoogleOAuthConfig, token_path: PathBuf) -> Self {
        Self {
            config,
            token: Arc::new(RwLock::new(None)),
            token_path,
            http: reqwest::Client::new(),
        }
    }

    pub fn with_token_override(token: GoogleToken) -> Self {
        Self {
            config: GoogleOAuthConfig::default(),
            token: Arc::new(RwLock::new(Some(token))),
            token_path: PathBuf::from("/dev/null"),
            http: reqwest::Client::new(),
        }
    }

    pub fn build_auth_url(&self) -> String {
        let scopes = self.config.scopes.join(" ");
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&scopes)
        )
    }

    pub async fn exchange_code(&self, code: &str) -> Result<GoogleToken, GoogleError> {
        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ];

        let response = self.http
            .post("https://oauth2.googleapis.com/token")
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

        let token_resp: TokenResponse = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let token = GoogleToken {
            access_token: token_resp.access_token,
            refresh_token: token_resp.refresh_token,
            expires_at: chrono::Utc::now().timestamp() + token_resp.expires_in,
            scopes: token_resp.scope.split_whitespace().map(String::from).collect(),
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

        let response = self.http
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: i64,
            scope: Option<String>,
        }

        let token_resp: TokenResponse = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        let mut token = self.token.read().await.clone().ok_or(GoogleError::NotAuthenticated)?;
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
        let token: GoogleToken = serde_json::from_str(&content)
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;
        *self.token.write().await = Some(token);
        Ok(())
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

pub struct GmailClient {
    auth: Arc<GoogleAuth>,
}

impl GmailClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_messages(&self, query: &str, limit: u32) -> Result<Vec<GmailMessage>, GoogleError> {
        let token = self.auth.get_token().await?;
        
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults={}&q={}",
            limit,
            urlencoding::encode(query)
        );

        let response = self.auth.http
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
            thread_id: String,
        }

        let list: ListResponse = response.json().await
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
        
        let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}", id);

        let response = self.auth.http
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

        let msg: MsgResponse = response.json().await
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

    pub async fn send_message(&self, to: &str, subject: &str, body: &str) -> Result<String, GoogleError> {
        let token = self.auth.get_token().await?;

        let raw = format!(
            "To: {}\nSubject: {}\n\n{}",
            to, subject, body
        );

        use base64::Engine;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);

        let body = serde_json::json!({
            "raw": encoded
        });

        let response = self.auth.http
            .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
            .bearer_auth(&token.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct SendResponse {
            id: String,
        }

        let result: SendResponse = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size: Option<String>,
    pub parents: Option<Vec<String>>,
}

pub struct DriveClient {
    auth: Arc<GoogleAuth>,
}

impl DriveClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_files(&self, query: Option<&str>, page_size: u32) -> Result<Vec<DriveFile>, GoogleError> {
        let token = self.auth.get_token().await?;
        
        let mut url = format!(
            "https://www.googleapis.com/drive/v3/files?pageSize={}",
            page_size
        );
        
        if let Some(q) = query {
            url.push_str(&format!("&q={}", urlencoding::encode(q)));
        }

        let response = self.auth.http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            files: Vec<DriveFile>,
        }

        let result: ListResponse = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.files)
    }

    pub async fn download_file(&self, id: &str) -> Result<Vec<u8>, GoogleError> {
        let token = self.auth.get_token().await?;
        
        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}?alt=media",
            id
        );

        let response = self.auth.http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let bytes = response.bytes().await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?
            .to_vec();

        Ok(bytes)
    }

    pub async fn upload_file(&self, name: &str, content: Vec<u8>, mime_type: &str) -> Result<DriveFile, GoogleError> {
        let token = self.auth.get_token().await?;

        let metadata = serde_json::json!({
            "name": name,
            "mimeType": mime_type
        });

        let client = reqwest::Client::new();
        let multipart = reqwest::multipart::Part::bytes(content)
            .file_name(name.to_string())
            .mime_str(mime_type)
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let form = reqwest::multipart::Form::new()
            .text("metadata", metadata.to_string())
            .part("file", multipart);

        let response = client
            .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart")
            .bearer_auth(&token.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let file: DriveFile = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(file)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
}

pub struct CalendarClient {
    auth: Arc<GoogleAuth>,
}

impl CalendarClient {
    pub fn new(auth: Arc<GoogleAuth>) -> Self {
        Self { auth }
    }

    pub async fn list_events(&self, calendar_id: &str, time_min: &str, time_max: &str, max_results: u32) -> Result<Vec<CalendarEvent>, GoogleError> {
        let token = self.auth.get_token().await?;
        
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events?timeMin={}&timeMax={}&maxResults={}",
            urlencoding::encode(calendar_id),
            urlencoding::encode(time_min),
            urlencoding::encode(time_max),
            max_results
        );

        let response = self.auth.http
            .get(&url)
            .bearer_auth(&token.access_token)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        #[derive(Deserialize)]
        struct ListResponse {
            items: Option<Vec<CalendarEvent>>,
        }

        let result: ListResponse = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(result.items.unwrap_or_default())
    }

    pub async fn create_event(&self, calendar_id: &str, summary: &str, start: &str, end: &str) -> Result<CalendarEvent, GoogleError> {
        let token = self.auth.get_token().await?;
        
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events",
            urlencoding::encode(calendar_id)
        );

        let body = serde_json::json!({
            "summary": summary,
            "start": { "dateTime": start },
            "end": { "dateTime": end }
        });

        let response = self.auth.http
            .post(&url)
            .bearer_auth(&token.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GoogleError::RequestFailed(e.to_string()))?;

        let event: CalendarEvent = response.json().await
            .map_err(|e| GoogleError::ParseFailed(e.to_string()))?;

        Ok(event)
    }
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
