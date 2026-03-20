//! BorgClaw Gateway - WebSocket gateway for remote connections

use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Path, State,
    },
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use borgclaw_core::{
    channel::{
        ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender, WebhookChannel,
        WebhookError, WebhookTrigger,
    },
    config::load_config,
    security::SecurityLayer,
    AppConfig,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{net::SocketAddr, path::PathBuf};
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

const DEFAULT_WEBHOOK_BODY_LIMIT_BYTES: usize = 1024 * 1024;

/// Gateway metrics for observability
#[derive(Default)]
struct GatewayMetrics {
    /// Total WebSocket connections accepted
    connections_total: AtomicU64,
    /// Current active WebSocket connections
    connections_active: AtomicU64,
    /// Total messages received via WebSocket
    messages_received: AtomicU64,
    /// Total messages sent via WebSocket
    messages_sent: AtomicU64,
    /// Total pairing requests
    pairing_requests: AtomicU64,
    /// Total successful authentications
    auth_success: AtomicU64,
    /// Total failed authentications
    auth_failure: AtomicU64,
    /// Gateway start time
    started_at: chrono::DateTime<chrono::Utc>,
}

impl GatewayMetrics {
    fn new() -> Self {
        Self {
            started_at: chrono::Utc::now(),
            ..Default::default()
        }
    }

    fn increment_connections(&self) {
        self.connections_total.fetch_add(1, Ordering::SeqCst);
        self.connections_active.fetch_add(1, Ordering::SeqCst);
    }

    fn decrement_connections(&self) {
        self.connections_active.fetch_sub(1, Ordering::SeqCst);
    }

    fn increment_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::SeqCst);
    }

    fn increment_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::SeqCst);
    }

    fn increment_pairing_requests(&self) {
        self.pairing_requests.fetch_add(1, Ordering::SeqCst);
    }

    fn increment_auth_success(&self) {
        self.auth_success.fetch_add(1, Ordering::SeqCst);
    }

    fn increment_auth_failure(&self) {
        self.auth_failure.fetch_add(1, Ordering::SeqCst);
    }

    fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            connections_total: self.connections_total.load(Ordering::SeqCst),
            connections_active: self.connections_active.load(Ordering::SeqCst),
            messages_received: self.messages_received.load(Ordering::SeqCst),
            messages_sent: self.messages_sent.load(Ordering::SeqCst),
            pairing_requests: self.pairing_requests.load(Ordering::SeqCst),
            auth_success: self.auth_success.load(Ordering::SeqCst),
            auth_failure: self.auth_failure.load(Ordering::SeqCst),
            uptime_seconds: (chrono::Utc::now() - self.started_at).num_seconds() as u64,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct MetricsSnapshot {
    connections_total: u64,
    connections_active: u64,
    messages_received: u64,
    messages_sent: u64,
    pairing_requests: u64,
    auth_success: u64,
    auth_failure: u64,
    uptime_seconds: u64,
}

#[derive(Clone)]
struct GatewayState {
    config: Arc<AppConfig>,
    router: Arc<MessageRouter>,
    webhook: Option<Arc<WebhookChannel>>,
    metrics: Arc<GatewayMetrics>,
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting BorgClaw Gateway...");

    let config_path =
        parse_config_path_from_args(std::env::args_os()).unwrap_or_else(default_config_path);
    let config = Arc::new(load_app_config(&config_path));
    let router = Arc::new(MessageRouter::from_config(&config));
    let websocket_port = websocket_port(&config);
    let webhook_port = webhook_port(&config);
    let webhook = configured_webhook_channel(&config).await.map(Arc::new);
    let metrics = Arc::new(GatewayMetrics::new());
    let state = GatewayState {
        config,
        router,
        webhook,
        metrics,
    };

    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let websocket_app = Router::new()
        .route("/", get(index))
        .route("/health", get(api_status))
        .route("/ws", get(websocket_handler))
        .route("/api/status", get(api_status))
        .route("/api/health", get(api_health))
        .route("/api/ready", get(api_ready))
        .route("/api/metrics", get(api_metrics))
        .route("/api/config", get(api_config))
        .route("/api/chat", get(api_chat_get))
        .layer(cors.clone())
        .with_state(state.clone());
    let webhook_app = Router::new()
        .route("/webhook", post(webhook_handler))
        .route("/webhook/health", get(webhook_health))
        .route("/webhook/trigger/{id}", post(webhook_trigger_handler))
        .layer(DefaultBodyLimit::max(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES))
        .layer(cors)
        .with_state(state.clone());

    match webhook_port {
        Some(port) if port == websocket_port => {
            let app = websocket_app.merge(webhook_app);
            let addr = SocketAddr::from(([0, 0, 0, 0], port));
            info!("Gateway + webhook listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        }
        Some(port) => {
            let ws_addr = SocketAddr::from(([0, 0, 0, 0], websocket_port));
            let webhook_addr = SocketAddr::from(([0, 0, 0, 0], port));
            info!("Gateway listening on http://{}", ws_addr);
            info!("Webhook listening on http://{}", webhook_addr);

            let ws_listener = tokio::net::TcpListener::bind(ws_addr).await.unwrap();
            let webhook_listener = tokio::net::TcpListener::bind(webhook_addr).await.unwrap();

            let ws_server = axum::serve(ws_listener, websocket_app);
            let webhook_server = axum::serve(webhook_listener, webhook_app);
            let (ws_result, webhook_result) = tokio::join!(ws_server, webhook_server);
            ws_result.unwrap();
            webhook_result.unwrap();
        }
        None => {
            let addr = SocketAddr::from(([0, 0, 0, 0], websocket_port));
            info!("Gateway listening on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, websocket_app).await.unwrap();
        }
    }
}

async fn index() -> &'static str {
    "BorgClaw Gateway\nUse /ws for WebSocket connections"
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<GatewayState>,
) -> impl IntoResponse {
    if matches!(
        state.config.channels.get("websocket"),
        Some(channel) if !channel.enabled
    ) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: GatewayState) {
    let mut socket = socket;
    let client_id = uuid::Uuid::new_v4().to_string();
    let requires_pairing = state
        .config
        .channels
        .get("websocket")
        .map(|channel| {
            channel
                .extra
                .get("require_pairing")
                .and_then(|value| value.as_bool())
                .unwrap_or(matches!(
                    channel.dm_policy,
                    borgclaw_core::config::DmPolicy::Pairing
                ))
        })
        .unwrap_or(true);

    state.metrics.increment_connections();
    info!("New WebSocket connection: {} (active: {})", client_id, state.metrics.connections_active.load(Ordering::SeqCst));

    let _ = send_event(
        &mut socket,
        serde_json::json!({
            "type": "welcome",
            "client_id": client_id,
            "auth_required": requires_pairing,
            "message": "Connected to BorgClaw"
        }),
    )
    .await;
    state.metrics.increment_messages_sent();

    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if send_event(&mut socket, serde_json::json!({
                    "type": "heartbeat",
                    "client_id": client_id,
                    "ts": chrono::Utc::now(),
                })).await.is_err() {
                    break;
                }
                state.metrics.increment_messages_sent();
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        state.metrics.increment_messages_received();
                        if let Err(e) = handle_ws_message(&mut socket, &state, &client_id, &text).await {
                            error!("Error handling message: {}", e);
                            let _ = send_event(&mut socket, error_event("internal gateway error")).await;
                            state.metrics.increment_messages_sent();
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        let _ = send_event(&mut socket, error_event(&e.to_string())).await;
                        state.metrics.increment_messages_sent();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    state.metrics.decrement_connections();
    info!("Connection closed: {} (active: {})", client_id, state.metrics.connections_active.load(Ordering::SeqCst));
}

async fn handle_ws_message(
    socket: &mut WebSocket,
    state: &GatewayState,
    client_id: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let request: serde_json::Value = serde_json::from_str(text).map_err(|_| "Invalid JSON")?;

    let msg_type = request
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");

    match msg_type {
        "request_pairing" => {
            state.metrics.increment_pairing_requests();
            let code = state.router.request_pairing_code(client_id).await?;
            send_event(
                socket,
                serde_json::json!({
                    "type": "pairing_code",
                    "client_id": client_id,
                    "pairing_code": code
                }),
            )
            .await?;
            state.metrics.increment_messages_sent();
        }
        "auth" => {
            let pairing_code = request
                .get("pairing_code")
                .and_then(|v| v.as_str())
                .ok_or("Missing pairing_code")?;
            let approved_sender = state.router.approve_pairing_code(pairing_code).await?;
            let authenticated = approved_sender == client_id;
            if authenticated {
                state.metrics.increment_auth_success();
            } else {
                state.metrics.increment_auth_failure();
            }
            send_event(socket, serde_json::json!({
                    "type": if authenticated { "authenticated" } else { "error" },
                    "client_id": client_id,
                    "approved_sender": approved_sender,
                    "message": if authenticated { "Pairing approved" } else { "Pairing code belongs to another sender" }
                }))
            .await?;
            state.metrics.increment_messages_sent();
        }
        "message" => {
            let content = request
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let group_id = request
                .get("group_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let inbound = InboundMessage {
                channel: ChannelType::websocket(),
                sender: Sender::new(client_id).with_name("Web User"),
                content: MessagePayload::text(content),
                group_id,
                timestamp: chrono::Utc::now(),
                raw: request.clone(),
            };
            match state.router.route(inbound).await {
                Ok(outcome) => {
                    send_event(
                        socket,
                        serde_json::json!({
                            "type": "response",
                            "session_id": outcome.session_id.0,
                            "text": outcome.response.text,
                            "tool_calls": outcome.response.tool_calls,
                            "metadata": outcome.response.metadata,
                        }),
                    )
                    .await?;
                }
                Err(borgclaw_core::channel::ChannelError::AuthFailed(message)) => {
                    let event = if message.contains("pairing required") {
                        serde_json::json!({
                            "type": "auth_required",
                            "client_id": client_id,
                            "message": message,
                        })
                    } else if message.contains("pairing pending") {
                        serde_json::json!({
                            "type": "pairing_pending",
                            "client_id": client_id,
                            "message": message,
                        })
                    } else {
                        error_event(&message)
                    };
                    send_event(socket, event).await?;
                }
                Err(err) => {
                    send_event(socket, error_event(&err.to_string())).await?;
                }
            }
        }
        "ping" => {
            send_event(socket, serde_json::json!({ "type": "pong" })).await?;
            state.metrics.increment_messages_sent();
        }
        _ => {
            send_event(socket, error_event("Unknown message type")).await?;
            state.metrics.increment_messages_sent();
        }
    }

    Ok(())
}

async fn send_event(socket: &mut WebSocket, event: serde_json::Value) -> Result<(), axum::Error> {
    socket.send(Message::Text(event.to_string())).await
}

fn default_config_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("borgclaw").join("config.toml");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("borgclaw")
            .join("config.toml");
    }

    PathBuf::from(".").join("borgclaw").join("config.toml")
}

fn parse_config_path_from_args<I>(args: I) -> Option<PathBuf>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let _ = args.next();

    while let Some(arg) = args.next() {
        if arg == "--config" || arg == "-c" {
            return args.next().map(PathBuf::from);
        }
    }

    None
}

fn load_app_config(path: &PathBuf) -> AppConfig {
    if !path.exists() {
        warn!(
            "Gateway config not found at {}; using defaults",
            path.display()
        );
        return AppConfig::default();
    }

    match load_config(path) {
        Ok(config) => config,
        Err(err) => {
            warn!(
                "Failed to load gateway config from {}: {}; using defaults",
                path.display(),
                err
            );
            AppConfig::default()
        }
    }
}

fn websocket_port(config: &AppConfig) -> u16 {
    config
        .channels
        .get("websocket")
        .and_then(|channel| channel.extra.get("port"))
        .and_then(|value| value.as_integer())
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(18789)
}

fn webhook_port(config: &AppConfig) -> Option<u16> {
    config
        .channels
        .get("webhook")
        .filter(|channel| channel.enabled)
        .and_then(|channel| channel.extra.get("port"))
        .and_then(|value| value.as_integer())
        .and_then(|value| u16::try_from(value).ok())
        .or_else(|| {
            config
                .channels
                .get("webhook")
                .filter(|channel| channel.enabled)
                .map(|_| 8080)
        })
}

async fn configured_webhook_channel(config: &AppConfig) -> Option<WebhookChannel> {
    let channel = config.channels.get("webhook")?;
    if !channel.enabled {
        return None;
    }

    let shared_secret = configured_channel_secret(config, channel, "secret").await;
    let mut trigger = WebhookTrigger::new("incoming", "/webhook");
    if let Some(secret) = shared_secret.clone() {
        trigger = trigger.with_secret(secret);
    }
    if let Some(rpm) = channel
        .extra
        .get("rate_limit_per_minute")
        .and_then(|value| value.as_integer())
        .and_then(|value| u32::try_from(value).ok())
    {
        trigger = trigger.with_rate_limit(rpm);
    }

    let webhook = WebhookChannel::new();
    for trigger in configured_named_webhook_triggers(channel, shared_secret.clone()) {
        webhook.register_trigger(trigger).await;
    }

    let mut named_trigger = WebhookTrigger::new("named_trigger", "/webhook/trigger/{id}");
    if let Some(secret) = shared_secret {
        named_trigger = named_trigger.with_secret(secret);
    }
    if let Some(rpm) = channel
        .extra
        .get("rate_limit_per_minute")
        .and_then(|value| value.as_integer())
        .and_then(|value| u32::try_from(value).ok())
    {
        named_trigger = named_trigger.with_rate_limit(rpm);
    }

    webhook.register_trigger(trigger).await;
    webhook.register_trigger(named_trigger).await;
    Some(webhook)
}

async fn configured_channel_secret(
    config: &AppConfig,
    channel: &borgclaw_core::config::ChannelConfig,
    key: &str,
) -> Option<String> {
    let configured = channel.extra.get(key)?.as_str()?;
    if let Some(env_key) = configured
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
    {
        if let Ok(value) = std::env::var(env_key) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }

        return SecurityLayer::with_config(config.security.clone())
            .get_secret(env_key)
            .await;
    }

    Some(configured.to_string())
}

fn configured_named_webhook_triggers(
    channel: &borgclaw_core::config::ChannelConfig,
    inherited_secret: Option<String>,
) -> Vec<WebhookTrigger> {
    let Some(triggers) = channel
        .extra
        .get("triggers")
        .and_then(|value| value.as_table())
    else {
        return Vec::new();
    };

    let inherited_rate_limit = channel
        .extra
        .get("rate_limit_per_minute")
        .and_then(|value| value.as_integer())
        .and_then(|value| u32::try_from(value).ok());

    triggers
        .iter()
        .filter_map(|(name, value)| {
            let table = value.as_table()?;
            let path = table
                .get("path")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| format!("/webhook/trigger/{}", name));
            let method = table
                .get("method")
                .and_then(|value| value.as_str())
                .unwrap_or("POST");

            let mut trigger = WebhookTrigger::new(name, path)
                .with_id(name)
                .with_method(method);

            if let Some(secret) = table
                .get("secret")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| inherited_secret.clone())
            {
                trigger = trigger.with_secret(secret);
            }

            if let Some(rate_limit) = table
                .get("rate_limit_per_minute")
                .and_then(|value| value.as_integer())
                .and_then(|value| u32::try_from(value).ok())
                .or(inherited_rate_limit)
            {
                trigger = trigger.with_rate_limit(rate_limit);
            }

            if let Some(url) = table.get("url").and_then(|value| value.as_str()) {
                trigger = trigger.with_forward_url(url);
            }

            if let Some(body_template) = table.get("body_template").and_then(|value| value.as_str())
            {
                trigger = trigger.with_body_template(body_template);
            }

            if let Some(headers) = table.get("headers").and_then(|value| value.as_table()) {
                for (key, value) in headers {
                    if let Some(value) = value.as_str() {
                        trigger = trigger.with_header(key, value);
                    }
                }
            }

            Some(trigger)
        })
        .collect()
}

async fn webhook_handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    route_webhook_request(&state, "/webhook", headers, body)
        .await
        .into_response()
}

async fn webhook_trigger_handler(
    Path(id): Path<String>,
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let path = format!("/webhook/trigger/{id}");
    route_webhook_request(&state, &path, headers, body)
        .await
        .into_response()
}

async fn webhook_health(State(state): State<GatewayState>) -> impl IntoResponse {
    let enabled = state.webhook.is_some();
    let body = serde_json::json!({
        "status": if enabled { "ok" } else { "disabled" },
        "webhook_enabled": enabled,
    });
    (StatusCode::OK, body.to_string())
}

async fn route_webhook_request(
    state: &GatewayState,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let webhook = match &state.webhook {
        Some(webhook) => webhook,
        None => return (StatusCode::NOT_FOUND, "Webhook channel disabled").into_response(),
    };

    let (tx, mut rx) = mpsc::channel(1);
    let headers = header_map_to_hash_map(&headers);
    match webhook
        .handle_request(path, "POST", headers, body.to_vec(), tx)
        .await
    {
        Ok(response) => {
            if let Some(inbound) = rx.recv().await {
                let route_outcome = match state.router.route(inbound).await {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        warn!("webhook request rejected: {}", err);
                        return json_error_response(StatusCode::BAD_REQUEST, "request rejected");
                    }
                };

                if let Some(trigger_id) = response
                    .body
                    .get("trigger_id")
                    .and_then(|value| value.as_str())
                {
                    match webhook.get_trigger(trigger_id).await {
                        Some(trigger) => {
                            if let Err(err) =
                                forward_configured_webhook(&trigger, &route_outcome.response.text)
                                    .await
                            {
                                warn!("webhook forward failed for trigger {}: {}", trigger_id, err);
                                return json_error_response(
                                    StatusCode::BAD_GATEWAY,
                                    "forward failed",
                                );
                            }
                        }
                        None => {}
                    }
                }

                let _ = route_outcome;
            }
            (
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK),
                response.body.to_string(),
            )
                .into_response()
        }
        Err(WebhookError::NotFound) => {
            json_error_response(StatusCode::NOT_FOUND, "Webhook not found")
        }
        Err(WebhookError::Unauthorized) => {
            json_error_response(StatusCode::UNAUTHORIZED, "Unauthorized")
        }
        Err(WebhookError::RateLimited(retry_after_seconds)) => {
            rate_limited_response(retry_after_seconds)
        }
        Err(WebhookError::ChannelClosed) => {
            error!("webhook channel closed while handling request");
            json_error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

async fn forward_configured_webhook(trigger: &WebhookTrigger, message: &str) -> Result<(), String> {
    let Some(url) = trigger.forward_url.as_deref() else {
        return Ok(());
    };

    let method =
        reqwest::Method::from_bytes(trigger.method.as_bytes()).map_err(|err| err.to_string())?;
    let body = render_webhook_body(trigger.body_template.as_deref(), message);
    let client = reqwest::Client::new();
    let mut request = client.request(method, url);

    for (key, value) in &trigger.headers {
        request = request.header(key, value);
    }

    let response = request
        .body(body)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("http {}", response.status()))
    }
}

fn render_webhook_body(template: Option<&str>, message: &str) -> String {
    template
        .unwrap_or("{{message}}")
        .replace("{{message}}", message)
}

fn header_map_to_hash_map(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(key, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (key.as_str().to_string(), value.to_string()))
        })
        .collect()
}

fn json_error_response(status: StatusCode, message: &str) -> axum::response::Response {
    (status, serde_json::json!({ "error": message }).to_string()).into_response()
}

fn rate_limited_response(retry_after_seconds: u64) -> axum::response::Response {
    let mut response = json_error_response(StatusCode::TOO_MANY_REQUESTS, "Rate limited");
    if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_seconds.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

fn error_event(message: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "error",
        "message": message,
    })
}

async fn api_status(State(state): State<GatewayState>) -> impl IntoResponse {
    let body = serde_json::json!({
        "status": "running",
        "model": state.config.agent.model,
        "provider": state.config.agent.provider,
    });

    (
        StatusCode::OK,
        serde_json::to_string(&body).unwrap_or_default(),
    )
}

async fn api_health(State(state): State<GatewayState>) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    checks.insert("gateway".to_string(), serde_json::json!("ok"));
    
    let workspace_ok = state.config.agent.workspace.exists();
    checks.insert("workspace".to_string(), serde_json::json!(if workspace_ok { "ok" } else { "error" }));
    
    let memory_db_ok = state.config.memory.database_path.parent()
        .map(|p| p.exists())
        .unwrap_or(true);
    checks.insert("memory_db".to_string(), serde_json::json!(if memory_db_ok { "ok" } else { "error" }));
    
    let healthy = workspace_ok && memory_db_ok;
    let body = serde_json::json!({
        "status": if healthy { "healthy" } else { "unhealthy" },
        "checks": checks,
    });
    
    let status = if healthy { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, serde_json::to_string(&body).unwrap_or_default())
}

async fn api_ready(State(state): State<GatewayState>) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    
    let workspace_ready = state.config.agent.workspace.exists();
    checks.insert("workspace".to_string(), serde_json::json!(if workspace_ready { "ready" } else { "not_ready" }));
    
    let skills_path = &state.config.skills.skills_path;
    let skills_ready = skills_path.exists() || std::fs::create_dir_all(skills_path).is_ok();
    checks.insert("skills_path".to_string(), serde_json::json!(if skills_ready { "ready" } else { "not_ready" }));
    
    let ready = workspace_ready && skills_ready;
    let body = serde_json::json!({
        "ready": ready,
        "checks": checks,
    });
    
    let status = if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, serde_json::to_string(&body).unwrap_or_default())
}

async fn api_metrics(State(state): State<GatewayState>) -> impl IntoResponse {
    let metrics = state.metrics.snapshot();
    (
        StatusCode::OK,
        serde_json::to_string(&metrics).unwrap_or_default(),
    )
}

async fn api_config(State(state): State<GatewayState>) -> impl IntoResponse {
    let sanitized = serde_json::json!({
        "agent": {
            "model": state.config.agent.model,
            "provider": state.config.agent.provider,
            "workspace": state.config.agent.workspace,
        },
        "channels": state.config.channels.keys().collect::<Vec<_>>(),
        "memory": {
            "database_path": state.config.memory.database_path,
            "hybrid_search": state.config.memory.hybrid_search,
            "session_max_entries": state.config.memory.session_max_entries,
        },
        "security": {
            "approval_mode": state.config.security.approval_mode,
            "pairing_enabled": state.config.security.pairing.enabled,
            "prompt_injection_defense": state.config.security.prompt_injection_defense,
            "secret_leak_detection": state.config.security.secret_leak_detection,
            "wasm_sandbox": state.config.security.wasm_sandbox,
            "command_blocklist": state.config.security.command_blocklist,
        },
        "skills": {
            "github_configured": !state.config.skills.github.token.is_empty(),
            "google_configured": !state.config.skills.google.client_id.is_empty(),
            "browser_configured": !state.config.skills.browser.node_path.as_os_str().is_empty(),
        },
    });

    (
        StatusCode::OK,
        serde_json::to_string(&sanitized).unwrap_or_default(),
    )
}

async fn api_chat_get() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, "Use POST for chat")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

    #[test]
    fn error_event_is_structured() {
        let event = error_event("bad request");
        assert_eq!(event["type"], "error");
        assert_eq!(event["message"], "bad request");
    }

    #[test]
    fn rate_limited_response_includes_retry_after_header() {
        let response = rate_limited_response(17);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get(axum::http::header::RETRY_AFTER),
            Some(&axum::http::HeaderValue::from_static("17"))
        );
    }

    #[test]
    fn parse_config_path_from_args_supports_short_and_long_flags() {
        assert_eq!(
            parse_config_path_from_args(["borgclaw-gateway", "--config", "/tmp/custom.toml"]),
            Some(PathBuf::from("/tmp/custom.toml"))
        );
        assert_eq!(
            parse_config_path_from_args(["borgclaw-gateway", "-c", "/tmp/custom.toml"]),
            Some(PathBuf::from("/tmp/custom.toml"))
        );
    }

    #[test]
    fn load_app_config_reads_documented_default_path_shape() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gateway_config_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        std::fs::write(
            &path,
            r#"
            [agent]
            provider = "openai"
            model = "gpt-4o"

            [channels.websocket]
            enabled = true
            port = 19002
            require_pairing = false
            "#,
        )
        .unwrap();

        let config = load_app_config(&path);
        assert_eq!(config.agent.provider, "openai");
        assert_eq!(websocket_port(&config), 19002);
        assert_eq!(
            config
                .channels
                .get("websocket")
                .and_then(|channel| channel.extra.get("require_pairing"))
                .and_then(|value| value.as_bool()),
            Some(false)
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn webhook_port_defaults_to_documented_port_when_enabled() {
        let mut config = AppConfig::default();
        config
            .channels
            .entry("webhook".to_string())
            .or_default()
            .enabled = true;

        assert_eq!(webhook_port(&config), Some(8080));
    }

    #[tokio::test]
    async fn api_status_reports_running_state() {
        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config: Arc::new(AppConfig::default()),
            router: Arc::new(MessageRouter::from_config(&AppConfig::default())),
            webhook: None,
            metrics,
        };

        let response = api_status(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn configured_webhook_trigger_contract_is_loaded_from_config() {
        let mut channel = borgclaw_core::config::ChannelConfig::default();
        channel.enabled = true;
        channel.extra.insert(
            "triggers".to_string(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "notify_slack".to_string(),
                toml::Value::Table(toml::map::Map::from_iter([
                    (
                        "url".to_string(),
                        toml::Value::String(
                            "https://hooks.slack.com/services/T00/B00/XXX".to_string(),
                        ),
                    ),
                    (
                        "body_template".to_string(),
                        toml::Value::String("{\"text\":\"{{message}}\"}".to_string()),
                    ),
                ])),
            )])),
        );

        let triggers = configured_named_webhook_triggers(&channel, None);
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].id, "notify_slack");
        assert_eq!(triggers[0].path, "/webhook/trigger/notify_slack");
        assert_eq!(
            triggers[0].forward_url.as_deref(),
            Some("https://hooks.slack.com/services/T00/B00/XXX")
        );
        assert_eq!(
            triggers[0].body_template.as_deref(),
            Some("{\"text\":\"{{message}}\"}")
        );
    }

    #[test]
    fn webhook_body_template_renders_documented_message_placeholder() {
        assert_eq!(
            render_webhook_body(Some("{\"text\":\"{{message}}\"}"), "hello"),
            "{\"text\":\"hello\"}"
        );
    }

    #[tokio::test]
    async fn configured_webhook_channel_resolves_secret_placeholder_from_secure_store() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gateway_webhook_secret_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut config = AppConfig::default();
        config.security.secrets_path = root.join("secrets.enc");
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;
        webhook.extra.insert(
            "secret".to_string(),
            toml::Value::String("${WEBHOOK_SECRET}".to_string()),
        );

        SecurityLayer::with_config(config.security.clone())
            .store_secret("WEBHOOK_SECRET", "hook-secret")
            .await
            .unwrap();

        let webhook = configured_webhook_channel(&config).await.unwrap();
        let triggers = webhook.list_triggers().await;

        assert!(triggers
            .iter()
            .all(|trigger| { trigger.secret.as_deref() == Some("hook-secret") }));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn websocket_protocol_flow_matches_documented_events() {
        let mut config = AppConfig::default();
        let websocket = config.channels.entry("websocket".to_string()).or_default();
        websocket.enabled = true;
        websocket
            .extra
            .insert("require_pairing".to_string(), toml::Value::Boolean(true));

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
        };

        let app = Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let (mut socket, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();

        let welcome = next_event_of_type(&mut socket, "welcome").await;
        assert_eq!(welcome["type"], "welcome");
        assert_eq!(welcome["auth_required"], true);
        let client_id = welcome["client_id"].as_str().unwrap().to_string();

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "message",
                    "content": "hello before auth"
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let auth_required = next_event_of_type(&mut socket, "auth_required").await;
        assert_eq!(auth_required["type"], "auth_required");
        assert_eq!(auth_required["client_id"], client_id);

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "request_pairing"
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let pairing = next_event_of_type(&mut socket, "pairing_code").await;
        assert_eq!(pairing["type"], "pairing_code");
        assert_eq!(pairing["client_id"], client_id);
        let pairing_code = pairing["pairing_code"].as_str().unwrap().to_string();
        assert_eq!(pairing_code.len(), 6);

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "auth",
                    "pairing_code": pairing_code
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let authenticated = next_event_of_type(&mut socket, "authenticated").await;
        assert_eq!(authenticated["type"], "authenticated");
        assert_eq!(authenticated["client_id"], client_id);
        assert_eq!(authenticated["approved_sender"], client_id);

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "ping"
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let pong = next_event_of_type(&mut socket, "pong").await;
        assert_eq!(pong["type"], "pong");

        socket
            .send(WsMessage::Text(
                serde_json::json!({
                    "type": "unknown"
                })
                .to_string(),
            ))
            .await
            .unwrap();
        let error = next_event_of_type(&mut socket, "error").await;
        assert_eq!(error["type"], "error");
        assert_eq!(error["message"], "Unknown message type");

        socket.close(None).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn websocket_upgrade_is_rejected_when_channel_is_disabled() {
        let mut config = AppConfig::default();
        let websocket = config.channels.entry("websocket".to_string()).or_default();
        websocket.enabled = false;

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
        };

        let app = Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let err = connect_async(format!("ws://{addr}/ws")).await.unwrap_err();
        match err {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            }
            other => panic!("unexpected websocket connect error: {other:?}"),
        }

        server.abort();
    }

    #[tokio::test]
    async fn webhook_http_flow_matches_documented_health_secret_and_rate_limit_behavior() {
        let mut config = AppConfig::default();
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;
        webhook.extra.insert(
            "secret".to_string(),
            toml::Value::String("test-secret".to_string()),
        );
        webhook
            .extra
            .insert("rate_limit_per_minute".to_string(), toml::Value::Integer(1));

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
        };

        let app = Router::new()
            .route("/webhook", post(webhook_handler))
            .route("/webhook/health", get(webhook_health))
            .layer(DefaultBodyLimit::max(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();

        let health = client
            .get(format!("http://{addr}/webhook/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let health_body: serde_json::Value = health.json().await.unwrap();
        assert_eq!(health_body["status"], "ok");
        assert_eq!(health_body["webhook_enabled"], true);

        let unauthorized = client
            .post(format!("http://{addr}/webhook"))
            .header("content-type", "application/json")
            .header("x-forwarded-for", "10.0.0.1")
            .body(r#"{"content":"hello"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_body: serde_json::Value = unauthorized.json().await.unwrap();
        assert_eq!(unauthorized_body["error"], "Unauthorized");

        let first = client
            .post(format!("http://{addr}/webhook"))
            .header("content-type", "application/json")
            .header("x-webhook-secret", "test-secret")
            .header("x-forwarded-for", "10.0.0.1")
            .body(r#"{"content":"hello"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::BAD_REQUEST);
        let first_body: serde_json::Value = first.json().await.unwrap();
        assert_eq!(first_body["error"], "request rejected");

        let limited = client
            .post(format!("http://{addr}/webhook"))
            .header("content-type", "application/json")
            .header("x-webhook-secret", "test-secret")
            .header("x-forwarded-for", "10.0.0.1")
            .body(r#"{"content":"hello again"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(limited
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .is_some());
        let limited_body: serde_json::Value = limited.json().await.unwrap();
        assert_eq!(limited_body["error"], "Rate limited");

        let other_requester = client
            .post(format!("http://{addr}/webhook"))
            .header("content-type", "application/json")
            .header("x-webhook-secret", "test-secret")
            .header("x-forwarded-for", "10.0.0.2")
            .body(r#"{"content":"hello from elsewhere"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(other_requester.status(), StatusCode::BAD_REQUEST);
        let other_body: serde_json::Value = other_requester.json().await.unwrap();
        assert_eq!(other_body["error"], "request rejected");

        server.abort();
    }

    #[tokio::test]
    async fn webhook_http_flow_rejects_oversized_bodies() {
        let mut config = AppConfig::default();
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
        };

        let app = Router::new()
            .route("/webhook", post(webhook_handler))
            .route("/webhook/health", get(webhook_health))
            .layer(DefaultBodyLimit::max(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();

        let oversized = "x".repeat(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES + 1);
        let response = client
            .post(format!("http://{addr}/webhook"))
            .header("content-type", "application/json")
            .body(oversized)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

        server.abort();
    }

    async fn next_text_message(
        socket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> serde_json::Value {
        loop {
            match socket.next().await.unwrap().unwrap() {
                WsMessage::Text(text) => return serde_json::from_str(&text).unwrap(),
                WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                other => panic!("unexpected websocket message: {other:?}"),
            }
        }
    }

    async fn next_event_of_type(
        socket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        event_type: &str,
    ) -> serde_json::Value {
        loop {
            let event = next_text_message(socket).await;
            if event["type"] == event_type {
                return event;
            }
        }
    }
}
