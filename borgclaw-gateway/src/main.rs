//! BorgClaw Gateway - WebSocket gateway for remote connections

use axum::{
    body::Bytes,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
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
    AppConfig,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{net::SocketAddr, path::PathBuf};
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

#[derive(Clone)]
struct GatewayState {
    config: Arc<AppConfig>,
    router: Arc<MessageRouter>,
    webhook: Option<Arc<WebhookChannel>>,
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
    let state = GatewayState {
        config,
        router,
        webhook,
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
        .route("/api/chat", get(api_chat_get))
        .layer(cors.clone())
        .with_state(state.clone());
    let webhook_app = Router::new()
        .route("/webhook", post(webhook_handler))
        .route("/webhook/health", get(webhook_health))
        .route("/webhook/trigger/{id}", post(webhook_trigger_handler))
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

    info!("New WebSocket connection: {}", client_id);

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
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(e) = handle_ws_message(&mut socket, &state, &client_id, &text).await {
                            error!("Error handling message: {}", e);
                            let _ = send_event(&mut socket, error_event("internal gateway error")).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        let _ = send_event(&mut socket, error_event(&e.to_string())).await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!("Connection closed: {}", client_id);
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
        }
        "auth" => {
            let pairing_code = request
                .get("pairing_code")
                .and_then(|v| v.as_str())
                .ok_or("Missing pairing_code")?;
            let approved_sender = state.router.approve_pairing_code(pairing_code).await?;
            let authenticated = approved_sender == client_id;
            send_event(socket, serde_json::json!({
                    "type": if authenticated { "authenticated" } else { "error" },
                    "client_id": client_id,
                    "approved_sender": approved_sender,
                    "message": if authenticated { "Pairing approved" } else { "Pairing code belongs to another sender" }
                }))
            .await?;
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
        }
        _ => {
            send_event(socket, error_event("Unknown message type")).await?;
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

    let mut trigger = WebhookTrigger::new("incoming", "/webhook");
    if let Some(secret) = channel.extra.get("secret").and_then(|value| value.as_str()) {
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
    let mut named_trigger = WebhookTrigger::new("named_trigger", "/webhook/trigger/{id}");
    if let Some(secret) = channel.extra.get("secret").and_then(|value| value.as_str()) {
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
                if let Err(err) = state.router.route(inbound).await {
                    return (
                        StatusCode::BAD_REQUEST,
                        serde_json::json!({ "error": err.to_string() }).to_string(),
                    )
                        .into_response();
                }
            }
            (
                StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK),
                response.body.to_string(),
            )
                .into_response()
        }
        Err(WebhookError::NotFound) => (StatusCode::NOT_FOUND, "Webhook not found").into_response(),
        Err(WebhookError::Unauthorized) => {
            (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
        }
        Err(WebhookError::RateLimited) => {
            (StatusCode::TOO_MANY_REQUESTS, "Rate limited").into_response()
        }
        Err(WebhookError::ChannelClosed) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Webhook channel closed").into_response()
        }
    }
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

async fn api_chat_get() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, "Use POST for chat")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_event_is_structured() {
        let event = error_event("bad request");
        assert_eq!(event["type"], "error");
        assert_eq!(event["message"], "bad request");
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
        let state = GatewayState {
            config: Arc::new(AppConfig::default()),
            router: Arc::new(MessageRouter::from_config(&AppConfig::default())),
            webhook: None,
        };

        let response = api_status(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
