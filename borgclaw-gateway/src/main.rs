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
    agent::builtin_tools,
    channel::{
        Channel, ChannelType, InboundMessage, MessagePayload, MessageRouter, OutboundMessage,
        Sender, TelegramChannel, WebhookChannel, WebhookError, WebhookTrigger,
    },
    config::load_config,
    security::{load_process_records, process_state_path, CommandProcessStatus, SecurityLayer},
    skills::{GoogleClient, OAuthPendingStore, SkillsRegistry},
    AppConfig,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{net::SocketAddr, path::PathBuf};
use tokio::sync::{mpsc, RwLock};
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

/// Tracked information about an active WebSocket connection
#[derive(Debug, Clone, serde::Serialize)]
struct ConnectionInfo {
    client_id: String,
    connected_at: chrono::DateTime<chrono::Utc>,
    session_id: Option<String>,
    authenticated: bool,
    messages_received: u64,
    messages_sent: u64,
}

#[derive(Clone)]
struct ConnectionHandle {
    info: ConnectionInfo,
    outbound: mpsc::UnboundedSender<serde_json::Value>,
}

#[derive(Clone)]
struct GatewayState {
    config: Arc<AppConfig>,
    config_path: Arc<PathBuf>,
    router: Arc<MessageRouter>,
    webhook: Option<Arc<WebhookChannel>>,
    metrics: Arc<GatewayMetrics>,
    connections: Arc<RwLock<HashMap<String, ConnectionHandle>>>,
    oauth_pending: OAuthPendingStore,
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
    let oauth_pending = OAuthPendingStore::from_google_config(&config.skills.google);
    let state = GatewayState {
        config,
        config_path: Arc::new(config_path),
        router,
        webhook,
        metrics,
        connections: Arc::new(RwLock::new(HashMap::new())),
        oauth_pending,
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
        .route("/oauth/callback", get(oauth_callback_handler))
        .route("/api/status", get(api_status))
        .route("/api/version", get(api_version))
        .route("/api/health", get(api_health))
        .route("/api/ready", get(api_ready))
        .route("/api/metrics", get(api_metrics))
        .route("/api/config", get(api_config).post(api_config_post))
        .route("/api/chat", get(api_chat_get).post(api_chat_post))
        .route("/api/tools", get(api_tools))
        .route("/api/connections", get(api_connections))
        .route("/api/schedules", get(api_schedules))
        .route("/api/heartbeat/tasks", get(api_heartbeat_tasks))
        .route("/api/subagents", get(api_subagents))
        .route("/api/mcp/servers", get(api_mcp_servers))
        .route("/api/doctor", get(api_doctor))
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

async fn index() -> impl IntoResponse {
    axum::response::Html(INDEX_HTML)
}

/// OAuth callback handler for Google authentication
async fn oauth_callback_handler(
    State(gateway_state): State<GatewayState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let code = params.get("code").cloned();
    let oauth_state_str = params.get("state").cloned();
    let error = params.get("error").cloned();

    // Handle OAuth errors
    if let Some(err) = error {
        return (
            StatusCode::BAD_REQUEST,
            axum::response::Html(format!(
                r##"<!DOCTYPE html>
<html>
<head><title>Authentication Failed</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #f85149;">Authentication Failed</h1>
    <p>Error: {}</p>
    <p>You can close this window and return to BorgClaw.</p>
</body>
</html>"##,
                err
            )),
        );
    }

    // Validate parameters
    let (code, oauth_state_str) = match (code, oauth_state_str) {
        (Some(c), Some(s)) => (c, s),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                axum::response::Html(
                    r##"<!DOCTYPE html>
<html>
<head><title>Invalid Request</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #f85149;">Invalid Request</h1>
    <p>Missing authorization code or state parameter.</p>
    <p>You can close this window and return to BorgClaw.</p>
</body>
</html>"##
                        .to_string(),
                ),
            );
        }
    };

    // Look up the pending OAuth request
    let oauth_state = match gateway_state.oauth_pending.get(&oauth_state_str).await {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                axum::response::Html(
                    r##"<!DOCTYPE html>
<html>
<head><title>Invalid or Expired Request</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #f85149;">Invalid or Expired Request</h1>
    <p>This authentication request has expired or is invalid.</p>
    <p>Please try authenticating again from BorgClaw.</p>
</body>
</html>"##
                        .to_string(),
                ),
            );
        }
    };

    // Remove the pending request
    gateway_state.oauth_pending.remove(&oauth_state_str).await;

    // Create a Google client to exchange the code
    let google_config = &gateway_state.config.skills.google;
    if google_config.client_id.is_empty() || google_config.client_secret.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::response::Html(
                r##"<!DOCTYPE html>
<html>
<head><title>Configuration Error</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #f85149;">Configuration Error</h1>
    <p>Google OAuth is not properly configured.</p>
    <p>Please check your BorgClaw configuration.</p>
</body>
</html>"##
                    .to_string(),
            ),
        );
    }

    let google_client = GoogleClient::new(google_config.clone());

    // Exchange the authorization code for tokens
    match google_client.auth().exchange_code(&code).await {
        Ok(_token) => {
            info!(
                "OAuth successful for user: {}, session: {}",
                oauth_state.user_id, oauth_state.session_id
            );

            if let Err(err) = notify_oauth_completion(&gateway_state, &oauth_state).await {
                warn!(
                    "OAuth completed but channel notification failed for session {}: {}",
                    oauth_state.session_id, err
                );
            }

            (
                StatusCode::OK,
                axum::response::Html(format!(
                    r##"<!DOCTYPE html>
<html>
<head><title>Authentication Successful</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #3fb950;">Authentication Successful!</h1>
    <p>You have successfully authenticated with Google.</p>
    <p>You can now use Gmail, Google Drive, and Calendar features in BorgClaw.</p>
    <p>You can close this window and return to BorgClaw.</p>
    <script>
        if (window.opener) {{
            window.opener.postMessage({{ type: 'oauth_complete', success: true, state: '{}' }}, '*');
        }}
    </script>
</body>
</html>"##,
                    oauth_state_str
                )),
            )
        }
        Err(err) => {
            error!("OAuth token exchange failed: {}", err);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::response::Html(format!(
                    r##"<!DOCTYPE html>
<html>
<head><title>Authentication Failed</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1 style="color: #f85149;">Authentication Failed</h1>
    <p>Failed to complete authentication: {}</p>
    <p>Please try again.</p>
</body>
</html>"##,
                    err
                )),
            )
        }
    }
}

async fn notify_oauth_completion(
    gateway_state: &GatewayState,
    oauth_state: &borgclaw_core::skills::OAuthState,
) -> Result<(), String> {
    let message = "Google authentication completed successfully. You can return to BorgClaw and continue using Google tools.";

    match oauth_state.channel.as_str() {
        "telegram" => send_telegram_oauth_notification(gateway_state, oauth_state, message).await,
        "websocket" | "web" => {
            send_session_oauth_notification(gateway_state, oauth_state, message).await
        }
        "cli" => Ok(()),
        other => {
            info!(
                "OAuth callback has no direct notifier for channel '{}'; browser success page remains the fallback",
                other
            );
            Ok(())
        }
    }
}

async fn send_session_oauth_notification(
    gateway_state: &GatewayState,
    oauth_state: &borgclaw_core::skills::OAuthState,
    message: &str,
) -> Result<(), String> {
    let event = serde_json::json!({
        "type": "oauth_complete",
        "session_id": oauth_state.session_id,
        "channel": oauth_state.channel,
        "provider": "google",
        "success": true,
        "message": message,
    });

    if queue_oauth_event_for_session(gateway_state, &oauth_state.session_id, event).await? {
        return Ok(());
    }

    info!(
        "OAuth completed for session {} but no live gateway session was connected; browser success page remains the fallback",
        oauth_state.session_id
    );
    Ok(())
}

async fn queue_oauth_event_for_session(
    gateway_state: &GatewayState,
    session_id: &str,
    event: serde_json::Value,
) -> Result<bool, String> {
    let mut delivered = false;
    let mut stale_clients = Vec::new();
    let mut connections = gateway_state.connections.write().await;

    for (client_id, handle) in connections.iter_mut() {
        if handle.info.session_id.as_deref() != Some(session_id) {
            continue;
        }

        if handle.outbound.send(event.clone()).is_ok() {
            handle.info.messages_sent += 1;
            gateway_state.metrics.increment_messages_sent();
            delivered = true;
        } else {
            stale_clients.push(client_id.clone());
        }
    }

    for client_id in stale_clients {
        connections.remove(&client_id);
    }

    Ok(delivered)
}

async fn send_telegram_oauth_notification(
    gateway_state: &GatewayState,
    oauth_state: &borgclaw_core::skills::OAuthState,
    message: &str,
) -> Result<(), String> {
    let channel_config = gateway_state
        .config
        .channels
        .get("telegram")
        .cloned()
        .ok_or_else(|| "telegram channel is not configured".to_string())?;

    if !channel_config.enabled {
        return Err("telegram channel is disabled".to_string());
    }

    let runtime_config = borgclaw_core::channel::ChannelConfig {
        channel_type: ChannelType::telegram(),
        enabled: channel_config.enabled,
        credentials: channel_config.credentials.clone(),
        proxy_url: channel_config.proxy_url.clone(),
        allow_from: channel_config.allow_from.clone(),
        dm_policy: channel_config.dm_policy,
        extra: channel_config.extra.clone(),
    };

    let target = oauth_state
        .group_id
        .clone()
        .filter(|group_id| !group_id.is_empty())
        .unwrap_or_else(|| oauth_state.user_id.clone());

    let mut telegram = TelegramChannel::new();
    telegram
        .init(&runtime_config)
        .await
        .map_err(|err| err.to_string())?;
    telegram
        .send(OutboundMessage::new(
            target,
            ChannelType::telegram(),
            MessagePayload::text(message),
        ))
        .await
        .map_err(|err| err.to_string())
}

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>BorgClaw Gateway</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        :root {
            --bg-primary: #0a0e14;
            --bg-secondary: #0d1117;
            --bg-tertiary: #161b22;
            --bg-glass: rgba(13, 17, 23, 0.85);
            --border-color: #30363d;
            --border-glow: rgba(0, 212, 170, 0.3);
            --text-primary: #e6edf3;
            --text-secondary: #8b949e;
            --text-muted: #484f58;
            --accent-cyan: #00d4aa;
            --accent-cyan-glow: rgba(0, 212, 170, 0.4);
            --accent-purple: #a371f7;
            --accent-green: #3fb950;
            --accent-blue: #58a6ff;
            --accent-orange: #f0883e;
            --accent-red: #f85149;
            --danger: #f85149;
            --font-mono: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', Consolas, 'Courier New', monospace;
            --shadow-sm: 0 1px 2px rgba(0,0,0,0.3);
            --shadow-md: 0 4px 12px rgba(0,0,0,0.4);
            --shadow-glow: 0 0 20px var(--accent-cyan-glow);
            --radius-sm: 6px;
            --radius-md: 8px;
            --radius-lg: 12px;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif;
            background: 
                radial-gradient(ellipse at top, rgba(0, 212, 170, 0.03) 0%, transparent 50%),
                radial-gradient(ellipse at bottom right, rgba(123, 44, 191, 0.03) 0%, transparent 50%),
                var(--bg-primary);
            min-height: 100vh;
            color: var(--text-primary);
            line-height: 1.5;
        }
        
        /* Navigation - Compact Elite Style */
        nav {
            background: var(--bg-glass);
            backdrop-filter: blur(12px);
            border-bottom: 1px solid var(--border-glow);
            padding: 0 20px;
            position: sticky;
            top: 0;
            z-index: 100;
            box-shadow: var(--shadow-sm);
        }
        .nav-content {
            max-width: 1400px;
            margin: 0 auto;
            display: flex;
            align-items: center;
            justify-content: space-between;
            height: 52px;
        }
        .logo {
            display: flex;
            align-items: center;
            gap: 10px;
            font-size: 1.1rem;
            font-weight: 700;
            letter-spacing: -0.02em;
            text-transform: uppercase;
        }
        .logo-icon {
            font-size: 1.3rem;
            filter: drop-shadow(0 0 8px var(--accent-cyan-glow));
            animation: pulse-glow 3s ease-in-out infinite;
        }
        @keyframes pulse-glow {
            0%, 100% { filter: drop-shadow(0 0 5px var(--accent-cyan-glow)); }
            50% { filter: drop-shadow(0 0 15px var(--accent-cyan-glow)); }
        }
        .nav-links {
            display: flex;
            gap: 20px;
            align-items: center;
        }
        .nav-links a {
            color: var(--text-secondary);
            text-decoration: none;
            font-size: 0.8rem;
            font-weight: 500;
            text-transform: uppercase;
            letter-spacing: 0.05em;
            padding: 6px 12px;
            border-radius: var(--radius-sm);
            transition: all 0.2s ease;
            position: relative;
        }
        .nav-links a:hover {
            color: var(--accent-cyan);
            background: rgba(0, 212, 170, 0.1);
        }
        .nav-links a::after {
            content: '';
            position: absolute;
            bottom: -2px;
            left: 50%;
            width: 0;
            height: 2px;
            background: var(--accent-cyan);
            transition: all 0.2s ease;
            transform: translateX(-50%);
        }
        .nav-links a:hover::after {
            width: 60%;
        }
        
        /* Main Layout - Compact Grid */
        main {
            max-width: 1600px;
            margin: 0 auto;
            padding: 16px 20px;
            display: grid;
            grid-template-columns: 260px 1fr;
            gap: 16px;
        }
        
        /* Sidebar - Glass Panels */
        aside {
            display: flex;
            flex-direction: column;
            gap: 12px;
        }
        .panel {
            background: var(--bg-glass);
            backdrop-filter: blur(10px);
            border: 1px solid var(--border-color);
            border-radius: var(--radius-md);
            padding: 12px;
            transition: all 0.2s ease;
            box-shadow: var(--shadow-sm);
        }
        .panel:hover {
            border-color: var(--border-glow);
            box-shadow: 0 0 15px rgba(0, 212, 170, 0.1);
        }
        .panel h3 {
            font-size: 0.65rem;
            font-weight: 700;
            text-transform: uppercase;
            letter-spacing: 0.08em;
            color: var(--text-secondary);
            margin-bottom: 10px;
            font-family: var(--font-mono);
            display: flex;
            align-items: center;
            gap: 6px;
        }
        .panel h3::before {
            content: '›';
            color: var(--accent-cyan);
            font-size: 0.9rem;
        }
        
        /* Status Indicator */
        .status-container {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 12px;
            background: rgba(63, 185, 80, 0.1);
            border: 1px solid rgba(63, 185, 80, 0.3);
            border-radius: 8px;
        }
        .status-dot {
            width: 8px;
            height: 8px;
            background: var(--accent-green);
            border-radius: 50%;
            animation: pulse 2s infinite;
        }
        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }
        .status-text {
            font-size: 0.875rem;
            font-weight: 500;
            color: var(--accent-green);
        }
        
        /* Metrics Grid */
        .metrics-grid {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 8px;
        }
        .metric {
            background: var(--bg-tertiary);
            border-radius: 8px;
            padding: 12px;
            text-align: center;
        }
        .metric-value {
            font-size: 1.5rem;
            font-weight: 700;
            color: var(--accent-cyan);
        }
        .metric-label {
            font-size: 0.6875rem;
            color: var(--text-secondary);
            text-transform: uppercase;
            margin-top: 4px;
        }
        
        /* Navigation Menu */
        .menu-list {
            list-style: none;
        }
        .menu-item {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 8px 12px;
            border-radius: 6px;
            cursor: pointer;
            transition: background 0.2s;
            font-size: 0.875rem;
            color: var(--text-secondary);
            text-decoration: none;
            margin-bottom: 2px;
        }
        button.menu-item {
            width: 100%;
            border: none;
            background: transparent;
            text-align: left;
            font: inherit;
        }
        .menu-item:hover {
            background: var(--bg-tertiary);
            color: var(--text-primary);
        }
        .menu-item.active {
            background: rgba(88, 166, 255, 0.1);
            color: var(--accent-blue);
        }
        .menu-icon {
            width: 20px;
            text-align: center;
        }
        
        /* Content Area */
        .content {
            display: flex;
            flex-direction: column;
            gap: 24px;
        }
        
        /* Chat Interface */
        .chat-container {
            background: var(--bg-secondary);
            border: 1px solid var(--border-color);
            border-radius: 12px;
            display: flex;
            flex-direction: column;
            height: 600px;
        }
        .chat-header {
            padding: 16px 20px;
            border-bottom: 1px solid var(--border-color);
            display: flex;
            align-items: center;
            justify-content: space-between;
        }
        .chat-title {
            font-size: 0.875rem;
            font-weight: 600;
        }
        .chat-messages {
            flex: 1;
            overflow-y: auto;
            padding: 20px;
            display: flex;
            flex-direction: column;
            gap: 16px;
        }
        .message {
            max-width: 80%;
            padding: 12px 16px;
            border-radius: 12px;
            font-size: 0.875rem;
            line-height: 1.6;
        }
        .message.user {
            align-self: flex-end;
            background: var(--accent-blue);
            color: white;
            border-bottom-right-radius: 4px;
        }
        .message.assistant {
            align-self: flex-start;
            background: var(--bg-tertiary);
            border: 1px solid var(--border-color);
            border-bottom-left-radius: 4px;
        }
        .message.system {
            align-self: center;
            background: transparent;
            color: var(--text-secondary);
            font-size: 0.75rem;
            font-style: italic;
        }
        .message.loading {
            display: flex;
            align-items: center;
            gap: 8px;
            color: var(--text-secondary);
        }
        .message-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            gap: 10px;
            margin-bottom: 10px;
            font-size: 0.7rem;
            text-transform: uppercase;
            letter-spacing: 0.08em;
        }
        .message-role-badge {
            display: inline-flex;
            align-items: center;
            gap: 6px;
            padding: 4px 8px;
            border-radius: 999px;
            font-weight: 700;
        }
        .message.user .message-role-badge {
            background: rgba(255,255,255,0.18);
        }
        .message.assistant .message-role-badge {
            background: rgba(0, 212, 170, 0.12);
            color: var(--accent-cyan);
        }
        .message.system .message-role-badge {
            background: rgba(240, 136, 62, 0.14);
            color: var(--accent-orange);
        }
        .message-time {
            color: var(--text-secondary);
        }
        .message-body {
            display: flex;
            flex-direction: column;
            gap: 12px;
        }
        .rich-text, .markdown-body, .json-pretty {
            white-space: pre-wrap;
            word-break: break-word;
        }
        .markdown-body h1, .markdown-body h2, .markdown-body h3 {
            margin: 0 0 8px;
            color: var(--text-primary);
        }
        .markdown-body p, .markdown-body ul, .markdown-body ol, .markdown-body blockquote {
            margin: 0 0 10px;
        }
        .markdown-body code,
        .json-pretty code {
            font-family: var(--font-mono);
            background: rgba(0,0,0,0.25);
            border-radius: 6px;
            padding: 2px 6px;
        }
        .markdown-body pre,
        .json-pretty {
            font-family: var(--font-mono);
            background: rgba(0,0,0,0.28);
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 10px;
            padding: 14px;
            overflow-x: auto;
        }
        .markdown-body blockquote {
            border-left: 3px solid var(--accent-cyan);
            padding-left: 12px;
            color: var(--text-secondary);
        }
        .message-rail {
            display: grid;
            gap: 10px;
        }
        .message-panel {
            background: rgba(255,255,255,0.03);
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 10px;
            padding: 12px;
        }
        .message-panel-title {
            font-size: 0.72rem;
            text-transform: uppercase;
            letter-spacing: 0.08em;
            color: var(--text-secondary);
            margin-bottom: 8px;
        }
        .tool-call-list,
        .attachment-grid,
        .meta-pill-list,
        .inspector-summary {
            display: grid;
            gap: 10px;
        }
        .tool-call-card,
        .attachment-card,
        .inspector-card {
            background: linear-gradient(145deg, rgba(88, 166, 255, 0.08), rgba(0, 212, 170, 0.05));
            border: 1px solid rgba(88, 166, 255, 0.18);
            border-radius: 12px;
            padding: 12px;
        }
        .tool-call-name,
        .attachment-title,
        .inspector-card-title {
            font-weight: 700;
            margin-bottom: 6px;
        }
        .attachment-preview {
            width: 100%;
            border-radius: 10px;
            border: 1px solid rgba(255,255,255,0.08);
            background: rgba(0,0,0,0.2);
        }
        .meta-pill-list {
            grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        }
        .meta-pill {
            background: rgba(255,255,255,0.04);
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 999px;
            padding: 8px 10px;
            font-size: 0.76rem;
        }
        .meta-pill strong {
            color: var(--accent-cyan);
        }
        .html-preview-frame {
            width: 100%;
            min-height: 180px;
            border: 1px solid rgba(255,255,255,0.08);
            border-radius: 10px;
            background: #fff;
        }
        .loading-dots {
            display: flex;
            gap: 4px;
        }
        .loading-dots span {
            width: 6px;
            height: 6px;
            background: var(--text-secondary);
            border-radius: 50%;
            animation: bounce 1.4s infinite ease-in-out both;
        }
        .loading-dots span:nth-child(1) { animation-delay: -0.32s; }
        .loading-dots span:nth-child(2) { animation-delay: -0.16s; }
        @keyframes bounce {
            0%, 80%, 100% { transform: scale(0); }
            40% { transform: scale(1); }
        }
        .chat-input-container {
            padding: 16px 20px;
            border-top: 1px solid var(--border-color);
            display: flex;
            gap: 12px;
        }
        .chat-input {
            flex: 1;
            background: var(--bg-tertiary);
            border: 1px solid var(--border-color);
            border-radius: 8px;
            padding: 12px 16px;
            color: var(--text-primary);
            font-size: 0.875rem;
            outline: none;
            transition: border-color 0.2s;
        }
        .chat-input:focus {
            border-color: var(--accent-blue);
        }
        .chat-send {
            background: var(--accent-blue);
            color: white;
            border: none;
            border-radius: 8px;
            padding: 12px 20px;
            font-size: 0.875rem;
            font-weight: 500;
            cursor: pointer;
            transition: opacity 0.2s;
        }
        .chat-send:hover {
            opacity: 0.9;
        }
        .chat-send:disabled {
            opacity: 0.5;
            cursor: not-allowed;
        }
        
        /* API Endpoints */
        .endpoints-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 16px;
        }
        .endpoint-card {
            background: var(--bg-secondary);
            border: 1px solid var(--border-color);
            border-radius: 12px;
            padding: 16px;
        }
        .endpoint-card h4 {
            font-size: 0.875rem;
            font-weight: 600;
            margin-bottom: 12px;
            display: flex;
            align-items: center;
            gap: 8px;
        }
        .endpoint-list {
            display: flex;
            flex-direction: column;
            gap: 8px;
        }
        .endpoint-item {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 8px 12px;
            background: var(--bg-tertiary);
            border-radius: 6px;
            font-family: 'SF Mono', Monaco, monospace;
            font-size: 0.8125rem;
            text-decoration: none;
            color: var(--text-secondary);
            transition: all 0.2s;
        }
        button.endpoint-item {
            width: 100%;
            border: none;
            text-align: left;
            cursor: pointer;
        }
        .endpoint-item:hover {
            background: var(--border-color);
            color: var(--text-primary);
        }
        .method {
            font-size: 0.625rem;
            font-weight: 700;
            text-transform: uppercase;
            padding: 2px 6px;
            border-radius: 4px;
        }
        .method.get { background: rgba(63, 185, 80, 0.2); color: var(--accent-green); }
        .method.post { background: rgba(88, 166, 255, 0.2); color: var(--accent-blue); }
        .method.ws { background: rgba(240, 136, 62, 0.2); color: var(--accent-orange); }
        
        /* Footer */
        footer {
            text-align: center;
            padding: 24px;
            color: var(--text-secondary);
            font-size: 0.75rem;
            border-top: 1px solid var(--border-color);
            margin-top: auto;
        }
        footer a {
            color: var(--accent-cyan);
            text-decoration: none;
        }
        .footer-link {
            background: transparent;
            border: none;
            color: var(--accent-cyan);
            cursor: pointer;
            font: inherit;
            padding: 0;
        }
        footer a:hover {
            text-decoration: underline;
        }
        .footer-link:hover {
            text-decoration: underline;
        }
        
        /* Mobile */
        @media (max-width: 968px) {
            main {
                grid-template-columns: 1fr;
            }
            aside {
                order: 2;
            }
            .content {
                order: 1;
            }
            .chat-container {
                height: 500px;
            }
        }
        
        /* ============================================
           RESPONSIVE DESIGN - MOBILE OPTIMIZED
           ============================================ */
        
        /* Tablet (768px - 1024px) */
        @media (max-width: 1024px) {
            main {
                grid-template-columns: 220px 1fr;
                gap: 14px;
                padding: 14px 16px;
            }
            .nav-content {
                height: 48px;
            }
            .logo {
                font-size: 1rem;
            }
        }
        
        /* Mobile (up to 767px) */
        @media (max-width: 767px) {
            :root {
                --radius-md: 6px;
                --radius-lg: 8px;
            }
            
            /* Nav becomes compact with hamburger */
            nav {
                padding: 0 12px;
            }
            .nav-content {
                height: 44px;
            }
            .logo {
                font-size: 0.95rem;
                gap: 8px;
            }
            .logo-icon {
                font-size: 1.1rem;
            }
            .nav-links {
                gap: 8px;
            }
            .nav-links a {
                font-size: 0.7rem;
                padding: 4px 8px;
                letter-spacing: 0.03em;
            }
            
            /* Main becomes single column */
            main {
                grid-template-columns: 1fr;
                gap: 12px;
                padding: 12px;
            }
            
            /* Sidebar becomes horizontal scrollable cards */
            aside {
                flex-direction: row;
                gap: 10px;
                overflow-x: auto;
                padding-bottom: 4px;
                scrollbar-width: none;
                -ms-overflow-style: none;
            }
            aside::-webkit-scrollbar {
                display: none;
            }
            .panel {
                min-width: 160px;
                flex-shrink: 0;
                padding: 10px;
            }
            .panel h3 {
                font-size: 0.6rem;
                margin-bottom: 8px;
            }
            
            /* Status compact */
            .status-container {
                padding: 8px;
                gap: 8px;
            }
            .status-text {
                font-size: 0.75rem;
            }
            
            /* Metrics smaller */
            .metrics-grid {
                gap: 6px;
            }
            .metric {
                padding: 8px;
            }
            .metric-value {
                font-size: 1.1rem;
            }
            .metric-label {
                font-size: 0.6rem;
            }
            
            /* Menu items compact */
            .menu-item {
                padding: 6px 10px;
                font-size: 0.8rem;
            }
            
            /* Chat full height */
            .chat-container {
                height: calc(100vh - 200px);
                min-height: 400px;
            }
            .chat-header {
                padding: 12px 14px;
            }
            .chat-header h2 {
                font-size: 0.9rem;
            }
            .chat-messages {
                padding: 12px;
            }
            .message {
                max-width: 90%;
                padding: 10px 12px;
                font-size: 0.9rem;
            }
            .chat-input-container {
                padding: 10px 12px;
            }
            .chat-input {
                padding: 10px 12px;
                font-size: 16px; /* Prevents zoom on iOS */
            }
            
            /* Footer compact */
            footer {
                padding: 12px;
                font-size: 0.75rem;
            }
        }
        
        /* Small Mobile (up to 480px) */
        @media (max-width: 480px) {
            .nav-links a span {
                display: none;
            }
            .nav-links a {
                padding: 4px 6px;
            }
            .panel {
                min-width: 140px;
            }
            .metric-value {
                font-size: 1rem;
            }
        }
        
        /* Touch optimizations */
        @media (hover: none) and (pointer: coarse) {
            .btn, .menu-item, .nav-links a {
                min-height: 44px;
                min-width: 44px;
            }
            .chat-input {
                font-size: 16px; /* Prevent zoom on focus */
            }
        }
        
        /* Dark mode optimization */
        @media (prefers-color-scheme: dark) {
            :root {
                --bg-primary: #0a0e14;
                --bg-secondary: #0d1117;
            }
        }
        
        /* Reduced motion preference */
        @media (prefers-reduced-motion: reduce) {
            *, *::before, *::after {
                animation-duration: 0.01ms !important;
                animation-iteration-count: 1 !important;
                transition-duration: 0.01ms !important;
            }
            .logo-icon {
                animation: none;
            }
            .status-dot {
                animation: none;
            }
        }
        
        /* Landscape mobile optimization */
        @media (max-height: 500px) and (orientation: landscape) {
            .chat-container {
                height: calc(100vh - 120px);
            }
            nav {
                position: relative;
            }
        }
    </style>
</head>
<body>
    <nav>
        <div class="nav-content">
            <div class="logo">
                <span class="logo-icon">🦞</span>
                <span>BorgClaw Gateway</span>
            </div>
            <div class="nav-links">
                <a href="#chat">Chat</a>
                <a href="#api">API</a>
                <a href="https://github.com/lealvona/borgclaw" target="_blank">GitHub</a>
            </div>
        </div>
    </nav>

    <main>
        <aside>
            <div class="panel">
                <div class="status-container">
                    <span class="status-dot"></span>
                    <span class="status-text">Gateway Online</span>
                </div>
            </div>

            <div class="panel">
                <h3>Real-time Metrics</h3>
                <div class="metrics-grid">
                    <div class="metric">
                        <div class="metric-value" id="conn-active">0</div>
                        <div class="metric-label">Active</div>
                    </div>
                    <div class="metric">
                        <div class="metric-value" id="msg-received">0</div>
                        <div class="metric-label">Messages</div>
                    </div>
                </div>
            </div>

            <div class="panel">
                <h3>Navigation</h3>
                <nav class="menu-list">
                    <a href="#chat" class="menu-item active">
                        <span class="menu-icon">💬</span>
                        Chat
                    </a>
                    <button type="button" class="menu-item" onclick="openInspector('Status', '/api/status', 'Gateway runtime state and version')">
                        <span class="menu-icon">📊</span>
                        Status
                    </button>
                    <button type="button" class="menu-item" onclick="openInspector('Metrics', '/api/metrics', 'Live gateway counters and throughput')">
                        <span class="menu-icon">📈</span>
                        Metrics
                    </button>
                    <button type="button" class="menu-item" onclick="openInspector('Tools', '/api/tools', 'Built-in tool catalog exposed by the gateway')">
                        <span class="menu-icon">🛠️</span>
                        Tools
                    </button>
                    <button type="button" class="menu-item" onclick="openInspector('Connections', '/api/connections', 'Active live sessions and transport state')">
                        <span class="menu-icon">🔌</span>
                        Connections
                    </button>
                    <button type="button" class="menu-item" onclick="openInspector('Doctor', '/api/doctor', 'Health checks across runtime, memory, skills, and workspace')">
                        <span class="menu-icon">🔍</span>
                        Health Check
                    </button>
                </nav>
            </div>

            <div class="panel">
                <h3>Configuration</h3>
                <nav class="menu-list">
                    <button type="button" class="menu-item" onclick="showConfigEditor()">
                        <span class="menu-icon">⚙️</span>
                        Edit Config
                    </button>
                    <button type="button" class="menu-item" onclick="openInspector('Configuration Snapshot', '/api/config', 'Current effective configuration surface exposed by the gateway')">
                        <span class="menu-icon">📋</span>
                        Review Config
                    </button>
                </nav>
            </div>
        </aside>

        <div class="content">
            <div class="chat-container" id="chat">
                <div class="chat-header">
                    <span class="chat-title">💬 Agent Chat</span>
                    <span style="font-size: 0.75rem; color: var(--text-secondary);">WebSocket + HTTP API</span>
                </div>
                <div class="chat-messages" id="chat-messages">
                    <div class="message system">
                        Welcome to BorgClaw Gateway. Send a message to start chatting with your agent.
                    </div>
                </div>
                <div class="chat-input-container">
                    <input 
                        type="text" 
                        class="chat-input" 
                        id="chat-input" 
                        placeholder="Type a message..."
                        autocomplete="off"
                    >
                    <button class="chat-send" id="chat-send" onclick="sendMessage()">
                        Send
                    </button>
                </div>
            </div>

            <div id="api">
                <div class="endpoints-grid">
                    <div class="endpoint-card">
                        <h4>💬 Chat & Messages</h4>
                        <div class="endpoint-list">
                            <button type="button" class="endpoint-item" onclick="openApiGuide('Chat API', 'POST /api/chat', 'Send a prompt to the router-backed agent and inspect the structured outbound payload returned to the dashboard.')">
                                <span class="method post">POST</span>
                                <code>/api/chat</code>
                            </button>
                            <div class="endpoint-item">
                                <span class="method ws">WS</span>
                                <code>/ws</code>
                            </div>
                        </div>
                    </div>

                    <div class="endpoint-card">
                        <h4>📡 Webhooks</h4>
                        <div class="endpoint-list">
                            <div class="endpoint-item">
                                <span class="method post">POST</span>
                                <code>/webhook</code>
                            </div>
                            <button type="button" class="endpoint-item" onclick="openInspector('Webhook Health', '/webhook/health', 'Readiness and shared-secret status for the webhook surface')">
                                <span class="method get">GET</span>
                                <code>/webhook/health</code>
                            </button>
                        </div>
                    </div>

                    <div class="endpoint-card">
                        <h4>📊 Observability</h4>
                        <div class="endpoint-list">
                            <button type="button" class="endpoint-item" onclick="openInspector('Health', '/api/health', 'Gateway health summary with HTTP-ready status')">
                                <span class="method get">GET</span>
                                <code>/api/health</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Readiness', '/api/ready', 'Startup/readiness gate for automated deployment checks')">
                                <span class="method get">GET</span>
                                <code>/api/ready</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Metrics', '/api/metrics', 'Connection, message, auth, and uptime counters')">
                                <span class="method get">GET</span>
                                <code>/api/metrics</code>
                            </button>
                        </div>
                    </div>

                    <div class="endpoint-card">
                        <h4>🔧 Management</h4>
                        <div class="endpoint-list">
                            <button type="button" class="endpoint-item" onclick="openInspector('Status', '/api/status', 'Gateway process status and release metadata')">
                                <span class="method get">GET</span>
                                <code>/api/status</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Configuration Snapshot', '/api/config', 'Current config as consumed by the gateway')">
                                <span class="method get">GET</span>
                                <code>/api/config</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Connections', '/api/connections', 'Connected clients, auth state, and routed session IDs')">
                                <span class="method get">GET</span>
                                <code>/api/connections</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Tools', '/api/tools', 'Introspect the runtime tool registry exposed to the agent')">
                                <span class="method get">GET</span>
                                <code>/api/tools</code>
                            </button>
                            <button type="button" class="endpoint-item" onclick="openInspector('Doctor', '/api/doctor', 'Full diagnostic output with per-check status')">
                                <span class="method get">GET</span>
                                <code>/api/doctor</code>
                            </button>
                        </div>
                    </div>
                </div>
            </div>

            <footer>
                <p id="footer-version">BorgClaw — Personal AI Agent Framework</p>
                <p style="margin-top: 8px;">
                    <a href="https://github.com/lealvona/borgclaw">GitHub</a> • 
                    <button type="button" class="footer-link" onclick="openInspector('Status', '/api/status', 'Gateway runtime state and version')">Status</button> • 
                    <button type="button" class="footer-link" onclick="openInspector('Health', '/api/health', 'Gateway health summary')">Health</button> •
                    <button type="button" class="footer-link" onclick="openInspector('Version', '/api/version', 'Release metadata for the running build')">Version</button>
                </p>
            </footer>
        </div>
    </main>

    <!-- Configuration Editor Modal -->
    <div id="config-modal" class="modal" style="display: none;">
        <div class="modal-content">
            <div class="modal-header">
                <h2>⚙️ Configuration Editor</h2>
                <button class="modal-close" onclick="hideConfigEditor()">&times;</button>
            </div>
            <div class="modal-body">
                <div id="config-tabs" class="config-tabs">
                    <button class="tab-btn active" onclick="showConfigTab('agent')">Agent</button>
                    <button class="tab-btn" onclick="showConfigTab('channels')">Channels</button>
                    <button class="tab-btn" onclick="showConfigTab('security')">Security</button>
                    <button class="tab-btn" onclick="showConfigTab('memory')">Memory</button>
                    <button class="tab-btn" onclick="showConfigTab('skills')">Skills</button>
                    <button class="tab-btn" onclick="showConfigTab('scheduler')">Scheduler</button>
                    <button class="tab-btn" onclick="showConfigTab('heartbeat')">Heartbeat</button>
                    <button class="tab-btn" onclick="showConfigTab('subagents')">Sub-agents</button>
                    <button class="tab-btn" onclick="showConfigTab('mcp')">MCP</button>
                </div>
                
                <!-- Agent Tab -->
                <div id="tab-agent" class="config-tab-content active">
                    <div class="config-form">
                        <div class="form-group">
                            <label>Provider</label>
                            <select id="cfg-provider" class="form-control">
                                <option value="openai">OpenAI</option>
                                <option value="minimax">MiniMax</option>
                                <option value="kimi">Kimi</option>
                                <option value="anthropic">Anthropic</option>
                            </select>
                            <small>LLM provider for agent responses</small>
                        </div>
                        <div class="form-group">
                            <label>Model</label>
                            <input type="text" id="cfg-model" class="form-control" placeholder="gpt-4o">
                            <small>Model name (e.g., gpt-4o, MiniMax-M2.7, kimi-k2.5)</small>
                        </div>
                        <div class="form-group">
                            <label>Provider Profile</label>
                            <input type="text" id="cfg-provider-profile" class="form-control" readonly>
                            <small>Managed through the CLI provider-profile commands.</small>
                        </div>
                        <div class="form-group">
                            <label>Identity Format</label>
                            <input type="text" id="cfg-identity-format" class="form-control" readonly>
                        </div>
                        <div class="form-group">
                            <label>Soul Path</label>
                            <input type="text" id="cfg-soul-path" class="form-control" readonly>
                        </div>
                        <div class="form-group">
                            <label>System Prompt</label>
                            <textarea id="cfg-system-prompt" class="form-control" rows="4" placeholder="Optional system prompt..."></textarea>
                        </div>
                    </div>
                </div>
                
                <!-- Channels Tab -->
                <div id="tab-channels" class="config-tab-content">
                    <div class="config-form">
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-ws-enabled">
                                WebSocket Channel Enabled
                            </label>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-ws-pairing">
                                Require Pairing for WebSocket
                            </label>
                        </div>
                        <div class="form-group">
                            <label>WebSocket Port</label>
                            <input type="number" id="cfg-ws-port" class="form-control" placeholder="3000">
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-webhook-enabled">
                                Webhook Channel Enabled
                            </label>
                        </div>
                        <div class="form-group">
                            <label>Webhook Port</label>
                            <input type="number" id="cfg-webhook-port" class="form-control" placeholder="8080">
                        </div>
                        <div class="form-group">
                            <label>Webhook Proxy</label>
                            <div id="cfg-webhook-proxy-status" class="text-muted">(not configured)</div>
                        </div>
                    </div>
                </div>
                
                <!-- Security Tab -->
                <div id="tab-security" class="config-tab-content">
                    <div class="config-form">
                        <div class="form-group">
                            <label>Approval Mode</label>
                            <select id="cfg-approval-mode" class="form-control">
                                <option value="readonly">ReadOnly</option>
                                <option value="supervised">Supervised</option>
                                <option value="autonomous">Autonomous</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-prompt-injection">
                                Prompt Injection Defense
                            </label>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-secret-leak">
                                Secret Leak Detection
                            </label>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-wasm-sandbox">
                                WASM Sandbox
                            </label>
                        </div>
                        <div class="form-group">
                            <label>Command Blocklist (comma-separated)</label>
                            <input type="text" id="cfg-blocklist" class="form-control" placeholder="rm, del, format">
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-docker-sandbox">
                                Docker Sandbox For Commands
                            </label>
                        </div>
                        <div class="form-group">
                            <label>Docker Image</label>
                            <input type="text" id="cfg-docker-image" class="form-control" placeholder="borgclaw-sandbox:base">
                        </div>
                        <div class="form-group">
                            <label>Docker Network</label>
                            <select id="cfg-docker-network" class="form-control">
                                <option value="none">none</option>
                                <option value="bridge">bridge</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>Workspace Mount</label>
                            <select id="cfg-docker-workspace-mount" class="form-control">
                                <option value="ro">ro</option>
                                <option value="rw">rw</option>
                                <option value="off">off</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>Docker Timeout (seconds)</label>
                            <input type="number" id="cfg-docker-timeout" class="form-control" placeholder="120">
                        </div>
                        <div class="status-box">
                            <h4>Process Runtime</h4>
                            <div id="process-runtime-status"></div>
                        </div>
                    </div>
                </div>
                
                <!-- Memory Tab -->
                <div id="tab-memory" class="config-tab-content">
                    <div class="config-form">
                        <div class="form-group">
                            <label>Memory Backend</label>
                            <select id="cfg-memory-backend" class="form-control">
                                <option value="sqlite">SQLite</option>
                                <option value="postgres">PostgreSQL</option>
                                <option value="memory">In-memory</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-hybrid-search">
                                Hybrid Search Enabled
                            </label>
                        </div>
                        <div class="form-group">
                            <label>Session Max Entries</label>
                            <input type="number" id="cfg-session-max" class="form-control" placeholder="1000">
                        </div>
                        <div class="form-group">
                            <label>Database Path</label>
                            <input type="text" id="cfg-db-path" class="form-control" placeholder=".borgclaw/memory.db" readonly>
                            <small>Read-only: Edit config.toml directly to change</small>
                        </div>
                        <div class="form-group">
                            <label>Embedding Endpoint</label>
                            <input type="text" id="cfg-embedding-endpoint" class="form-control" placeholder="http://127.0.0.1:11434/api/embeddings">
                            <small>Required for semantic pgvector search and runtime hybrid ranking.</small>
                        </div>
                        <div class="status-box">
                            <h4>External Memory</h4>
                            <div id="memory-external-status"></div>
                        </div>
                        <div class="status-box">
                            <h4>Privacy Policy</h4>
                            <div id="memory-privacy-status"></div>
                        </div>
                    </div>
                </div>
                
                <!-- Skills Tab -->
                <div id="tab-skills" class="config-tab-content">
                    <div class="config-form">
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="cfg-auto-load">
                                Auto-load Skills
                            </label>
                        </div>
                        <div class="form-group">
                            <label>Skills Path</label>
                            <input type="text" id="cfg-skills-path" class="form-control" readonly>
                        </div>
                        <div class="form-group">
                            <label>Workspace Skills Path</label>
                            <input type="text" id="cfg-workspace-skills-path" class="form-control" readonly>
                        </div>
                        <div class="form-group">
                            <label>Registry URL</label>
                            <input type="text" id="cfg-skills-registry-url" class="form-control" readonly>
                        </div>
                        <div class="status-box">
                            <h4>Skill Status</h4>
                            <div id="skill-status-list"></div>
                        </div>
                        <div class="status-box">
                            <h4>Discovered Skills</h4>
                            <div id="skill-discovery-list"></div>
                        </div>
                    </div>
                </div>
                
                <div id="config-status" class="config-status"></div>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" onclick="hideConfigEditor()">Cancel</button>
                <button class="btn btn-primary" onclick="saveConfiguration()">💾 Save Changes</button>
            </div>
        </div>
    </div>

    <div id="inspector-modal" class="modal" style="display: none;">
        <div class="modal-content inspector-modal-content">
            <div class="modal-header">
                <div>
                    <h2 id="inspector-title">Inspector</h2>
                    <div id="inspector-subtitle" class="text-muted"></div>
                </div>
                <button class="modal-close" onclick="hideInspector()">&times;</button>
            </div>
            <div class="modal-body">
                <div id="inspector-summary" class="inspector-summary"></div>
                <pre id="inspector-json" class="json-pretty"></pre>
            </div>
            <div class="modal-footer">
                <button class="btn btn-secondary" onclick="hideInspector()">Close</button>
            </div>
        </div>
    </div>

    <style>
        .modal {
            position: fixed;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            background: rgba(0,0,0,0.8);
            display: flex;
            align-items: center;
            justify-content: center;
            z-index: 1000;
            backdrop-filter: blur(5px);
        }
        .modal-content {
            background: linear-gradient(145deg, #1a1a2e 0%, #16213e 100%);
            border: 1px solid var(--neon-cyan);
            border-radius: 12px;
            width: 90%;
            max-width: 700px;
            max-height: 85vh;
            display: flex;
            flex-direction: column;
            box-shadow: 0 0 30px rgba(0,212,255,0.2);
        }
        .inspector-modal-content {
            max-width: 900px;
        }
        .text-muted {
            color: var(--text-secondary);
            font-size: 0.82rem;
            margin-top: 4px;
        }
        .modal-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 20px 24px;
            border-bottom: 1px solid rgba(0,212,255,0.2);
        }
        .modal-header h2 {
            margin: 0;
            color: var(--neon-cyan);
        }
        .modal-close {
            background: none;
            border: none;
            color: #888;
            font-size: 28px;
            cursor: pointer;
            transition: color 0.2s;
        }
        .modal-close:hover {
            color: var(--neon-cyan);
        }
        .modal-body {
            padding: 24px;
            overflow-y: auto;
            flex: 1;
        }
        .modal-footer {
            display: flex;
            justify-content: flex-end;
            gap: 12px;
            padding: 20px 24px;
            border-top: 1px solid rgba(0,212,255,0.2);
        }
        .config-tabs {
            display: flex;
            gap: 8px;
            margin-bottom: 24px;
            border-bottom: 1px solid rgba(0,212,255,0.2);
            padding-bottom: 12px;
        }
        .tab-btn {
            background: rgba(0,212,255,0.1);
            border: 1px solid transparent;
            color: #aaa;
            padding: 8px 16px;
            border-radius: 6px;
            cursor: pointer;
            transition: all 0.2s;
        }
        .tab-btn:hover {
            background: rgba(0,212,255,0.2);
            color: #fff;
        }
        .tab-btn.active {
            background: rgba(0,212,255,0.3);
            border-color: var(--neon-cyan);
            color: var(--neon-cyan);
        }
        .config-tab-content {
            display: none;
        }
        .config-tab-content.active {
            display: block;
        }
        .config-form {
            display: flex;
            flex-direction: column;
            gap: 16px;
        }
        .form-group {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }
        .form-group label {
            color: var(--neon-cyan);
            font-size: 0.9rem;
            font-weight: 500;
        }
        .form-control {
            background: rgba(0,0,0,0.3);
            border: 1px solid rgba(0,212,255,0.3);
            border-radius: 6px;
            padding: 10px 12px;
            color: #fff;
            font-size: 0.95rem;
            transition: border-color 0.2s;
        }
        .form-control:focus {
            outline: none;
            border-color: var(--neon-cyan);
            box-shadow: 0 0 10px rgba(0,212,255,0.2);
        }
        .form-control[readonly] {
            opacity: 0.6;
            cursor: not-allowed;
        }
        .form-group small {
            color: #888;
            font-size: 0.8rem;
        }
        .checkbox-label {
            display: flex;
            align-items: center;
            gap: 10px;
            cursor: pointer;
        }
        .checkbox-label input[type="checkbox"] {
            width: 18px;
            height: 18px;
            accent-color: var(--neon-cyan);
        }
        .btn {
            padding: 10px 20px;
            border-radius: 6px;
            border: none;
            cursor: pointer;
            font-size: 0.95rem;
            transition: all 0.2s;
        }
        .btn-primary {
            background: linear-gradient(135deg, var(--neon-cyan), var(--neon-purple));
            color: #fff;
        }
        .btn-primary:hover {
            opacity: 0.9;
            transform: translateY(-1px);
        }
        .btn-secondary {
            background: rgba(255,255,255,0.1);
            color: #aaa;
            border: 1px solid rgba(255,255,255,0.2);
        }
        .btn-secondary:hover {
            background: rgba(255,255,255,0.15);
            color: #fff;
        }
        .config-status {
            margin-top: 16px;
            padding: 12px;
            border-radius: 6px;
            display: none;
        }
        .config-status.success {
            display: block;
            background: rgba(0,255,136,0.1);
            border: 1px solid var(--neon-green);
            color: var(--neon-green);
        }
        .config-status.error {
            display: block;
            background: rgba(255,0,0,0.1);
            border: 1px solid #ff4444;
            color: #ff6666;
        }
        .status-box {
            background: rgba(0,0,0,0.2);
            border: 1px solid rgba(0,212,255,0.2);
            border-radius: 8px;
            padding: 16px;
            margin-top: 16px;
        }
        .status-box h4 {
            margin: 0 0 12px 0;
            color: var(--neon-cyan);
            font-size: 0.95rem;
        }
        .skill-status-item {
            display: flex;
            align-items: center;
            gap: 8px;
            padding: 6px 0;
            border-bottom: 1px solid rgba(255,255,255,0.1);
        }
        .skill-status-item:last-child {
            border-bottom: none;
        }
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
        }
        .status-dot.ok { background: var(--neon-green); }
        .status-dot.warn { background: #ffaa00; }
        .status-dot.off { background: #666; }
        
        /* ============================================
           MODAL RESPONSIVE STYLES
           ============================================ */
        
        @media (max-width: 767px) {
            .modal-content {
                width: 95%;
                max-width: none;
                max-height: 90vh;
                margin: 10px;
            }
            .modal-header {
                padding: 14px 16px;
            }
            .modal-header h2 {
                font-size: 1rem;
            }
            .modal-body {
                padding: 16px;
            }
            .modal-footer {
                padding: 14px 16px;
                flex-direction: column;
                gap: 8px;
            }
            .modal-footer .btn {
                width: 100%;
            }
            
            /* Tabs become scrollable */
            .config-tabs {
                flex-wrap: nowrap;
                overflow-x: auto;
                gap: 6px;
                padding-bottom: 8px;
                scrollbar-width: none;
                -ms-overflow-style: none;
            }
            .config-tabs::-webkit-scrollbar {
                display: none;
            }
            .tab-btn {
                flex-shrink: 0;
                padding: 6px 12px;
                font-size: 0.8rem;
                white-space: nowrap;
            }
            
            /* Form elements larger for touch */
            .form-control {
                padding: 12px 14px;
                font-size: 16px; /* Prevents zoom on iOS */
            }
            .form-group label {
                font-size: 0.85rem;
            }
            .btn {
                padding: 12px 20px;
                min-height: 44px;
            }
            
            /* Status box compact */
            .status-box {
                padding: 12px;
            }
            .status-box h4 {
                font-size: 0.85rem;
            }
        }
        
        @media (max-width: 480px) {
            .modal-content {
                width: 100%;
                height: 100%;
                max-height: 100vh;
                border-radius: 0;
                margin: 0;
            }
            .tab-btn {
                padding: 5px 10px;
                font-size: 0.75rem;
            }
        }
    </style>

    <script>
        // State
        let messageHistory = [];
        let isProcessing = false;
        let currentChatSessionId = null;
        
        // Initialize
        document.addEventListener('DOMContentLoaded', () => {
            const input = document.getElementById('chat-input');
            input.addEventListener('keypress', (e) => {
                if (e.key === 'Enter' && !isProcessing) {
                    sendMessage();
                }
            });
            window.addEventListener('message', handleOAuthWindowMessage);
            
            updateMetrics();
            setInterval(updateMetrics, 5000);
        });
        
        async function updateMetrics() {
            try {
                const res = await fetch('/api/metrics');
                if (res.ok) {
                    const data = await res.json();
                    document.getElementById('conn-active').textContent = data.connections_active || 0;
                    document.getElementById('msg-received').textContent = (data.messages_received || 0).toLocaleString();
                }
            } catch (err) {
                // Metrics are optional in the UI
            }
        }
        
        async function sendMessage() {
            const input = document.getElementById('chat-input');
            const sendBtn = document.getElementById('chat-send');
            const content = input.value.trim();
            
            if (!content || isProcessing) return;
            
            addMessage('user', {
                text: content,
                content_type: 'markdown',
                content: { kind: 'markdown', text: content }
            });
            input.value = '';
            
            isProcessing = true;
            sendBtn.disabled = true;
            const loadingId = addLoading();
            
            try {
                const res = await fetch('/api/chat', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        content,
                        sender_id: 'web-ui',
                        group_id: 'web-chat'
                    })
                });
                document.getElementById(loadingId)?.remove();
                
                const data = await res.json();
                if (data.error) {
                    addMessage('system', { text: 'Error: ' + data.error });
                } else {
                    currentChatSessionId = data.session_id || currentChatSessionId;
                    addMessage('assistant', data);
                    updateMetrics();
                }
            } catch (err) {
                document.getElementById(loadingId)?.remove();
                addMessage('system', { text: 'Connection error: ' + err.message });
            } finally {
                isProcessing = false;
                sendBtn.disabled = false;
                input.focus();
            }
        }
        
        function addMessage(role, payload) {
            const messages = document.getElementById('chat-messages');
            const msg = document.createElement('article');
            msg.className = 'message ' + role;
            msg.style.opacity = '0';
            msg.style.transform = 'translateY(8px)';
            
            const normalized = normalizePayload(payload);
            const time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            msg.innerHTML = `
                <div class="message-header">
                    <span class="message-role-badge">${roleLabel(role, normalized)}</span>
                    <span class="message-time">${time}</span>
                </div>
                <div class="message-body"></div>
            `;
            
            const body = msg.querySelector('.message-body');
            body.appendChild(renderPrimaryContent(normalized));
            
            if (Array.isArray(normalized.tool_calls) && normalized.tool_calls.length > 0) {
                body.appendChild(renderToolCalls(normalized.tool_calls));
            }
            if (normalized.metadata && Object.keys(normalized.metadata).length > 0) {
                body.appendChild(renderMetadata(normalized.metadata));
            }
            
            messages.appendChild(msg);
            requestAnimationFrame(() => {
                msg.style.transition = 'opacity 220ms ease, transform 220ms ease';
                msg.style.opacity = '1';
                msg.style.transform = 'translateY(0)';
            });
            messages.scrollTop = messages.scrollHeight;
            messageHistory.push({ role, payload: normalized });
            return msg;
        }
        
        function addLoading() {
            const messages = document.getElementById('chat-messages');
            const id = 'loading-' + Date.now();
            const msg = document.createElement('div');
            msg.id = id;
            msg.className = 'message loading';
            msg.innerHTML = `
                Thinking
                <div class="loading-dots">
                    <span></span>
                    <span></span>
                    <span></span>
                </div>
            `;
            messages.appendChild(msg);
            messages.scrollTop = messages.scrollHeight;
            return id;
        }
        
        function normalizePayload(payload) {
            if (payload == null) {
                return { text: '' };
            }
            if (typeof payload === 'string') {
                return {
                    text: payload,
                    content_type: 'text',
                    content: { kind: 'text', text: payload },
                    tool_calls: [],
                    metadata: {}
                };
            }
            const normalized = { ...payload };
            if (!normalized.content) {
                const kind = normalized.content_type || 'text';
                if (kind === 'html') {
                    normalized.content = { kind: 'html', html: normalized.text || '' };
                } else {
                    normalized.content = { kind, text: normalized.text || '' };
                }
            }
            normalized.tool_calls = normalized.tool_calls || [];
            normalized.metadata = normalized.metadata || {};
            return normalized;
        }
        
        function roleLabel(role, payload) {
            if (role === 'assistant' && payload.content_type) {
                return `Assistant · ${payload.content_type}`;
            }
            if (role === 'system' && payload.metadata?.provider) {
                return `System · ${payload.metadata.provider}`;
            }
            return role.charAt(0).toUpperCase() + role.slice(1);
        }
        
        function renderPrimaryContent(payload) {
            const panel = document.createElement('div');
            panel.className = 'message-panel';
            const content = payload.content || { kind: payload.content_type || 'text', text: payload.text || '' };
            
            if (content.kind === 'markdown') {
                panel.innerHTML = `<div class="markdown-body">${renderMarkdown(content.text || payload.text || '')}</div>`;
                return panel;
            }
            if (content.kind === 'html') {
                const escapedHtml = escapeHtml(content.html || payload.text || '');
                panel.innerHTML = `
                    <div class="message-panel-title">HTML Response</div>
                    <iframe class="html-preview-frame" sandbox="" srcdoc="${escapedHtml.replace(/"/g, '&quot;')}"></iframe>
                    <pre class="json-pretty">${escapedHtml}</pre>
                `;
                return panel;
            }
            if (content.kind === 'media') {
                return renderAttachmentCard(content.url, content.mime_type);
            }
            if (content.kind === 'file') {
                panel.innerHTML = `
                    <div class="message-panel-title">Attachment</div>
                    <div class="attachment-card">
                        <div class="attachment-title">${escapeHtml(content.name || 'File')}</div>
                        <div class="text-muted">${escapeHtml(content.path || '')}</div>
                    </div>
                `;
                return panel;
            }
            panel.innerHTML = `<div class="rich-text">${renderPlainText(content.text || payload.text || '')}</div>`;
            return panel;
        }
        
        function renderAttachmentCard(url, mimeType) {
            const panel = document.createElement('div');
            panel.className = 'message-panel';
            const safeUrl = escapeHtml(url || '');
            if ((mimeType || '').startsWith('image/')) {
                panel.innerHTML = `
                    <div class="message-panel-title">Image</div>
                    <div class="attachment-card">
                        <img class="attachment-preview" src="${safeUrl}" alt="attachment preview">
                    </div>
                `;
            } else if ((mimeType || '').startsWith('video/')) {
                panel.innerHTML = `
                    <div class="message-panel-title">Video</div>
                    <div class="attachment-card">
                        <video class="attachment-preview" controls src="${safeUrl}"></video>
                    </div>
                `;
            } else {
                panel.innerHTML = `
                    <div class="message-panel-title">Media</div>
                    <div class="attachment-card">
                        <div class="attachment-title">${escapeHtml(mimeType || 'media')}</div>
                        <a href="${safeUrl}" target="_blank" rel="noopener noreferrer">${safeUrl}</a>
                    </div>
                `;
            }
            return panel;
        }
        
        function renderToolCalls(toolCalls) {
            const wrapper = document.createElement('div');
            wrapper.className = 'message-panel';
            wrapper.innerHTML = '<div class="message-panel-title">Tool Calls</div>';
            const list = document.createElement('div');
            list.className = 'tool-call-list';
            toolCalls.forEach(call => {
                const card = document.createElement('div');
                card.className = 'tool-call-card';
                card.innerHTML = `
                    <div class="tool-call-name">${escapeHtml(call.name || 'tool')}</div>
                    <pre class="json-pretty">${escapeHtml(JSON.stringify(call.arguments || {}, null, 2))}</pre>
                `;
                list.appendChild(card);
            });
            wrapper.appendChild(list);
            return wrapper;
        }
        
        function renderMetadata(metadata) {
            const wrapper = document.createElement('div');
            wrapper.className = 'message-panel';
            wrapper.innerHTML = '<div class="message-panel-title">Metadata</div>';
            const list = document.createElement('div');
            list.className = 'meta-pill-list';
            Object.entries(metadata).forEach(([key, value]) => {
                const pill = document.createElement('div');
                pill.className = 'meta-pill';
                pill.innerHTML = `<strong>${escapeHtml(key)}:</strong> ${escapeHtml(formatScalar(value))}`;
                list.appendChild(pill);
            });
            wrapper.appendChild(list);
            return wrapper;
        }
        
        function renderPlainText(text) {
            return escapeHtml(text).replace(/(https?:\/\/[^\s]+)/g, '<a href="$1" target="_blank" rel="noopener noreferrer">$1</a>').replace(/\n/g, '<br>');
        }
        
        function renderMarkdown(text) {
            const escaped = escapeHtml(text || '');
            const fenced = escaped.replace(/```([\s\S]*?)```/g, (_, code) => `<pre><code>${code.trim()}</code></pre>`);
            const headings = fenced
                .replace(/^### (.*)$/gm, '<h3>$1</h3>')
                .replace(/^## (.*)$/gm, '<h2>$1</h2>')
                .replace(/^# (.*)$/gm, '<h1>$1</h1>');
            const emphasis = headings
                .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
                .replace(/`([^`]+)`/g, '<code>$1</code>')
                .replace(/\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
            return emphasis
                .split(/\n\n+/)
                .map(block => {
                    if (block.startsWith('<h') || block.startsWith('<pre>')) return block;
                    if (/^[-*] /m.test(block)) {
                        const items = block.split('\n').map(line => line.replace(/^[-*] /, '').trim()).filter(Boolean);
                        return `<ul>${items.map(item => `<li>${item}</li>`).join('')}</ul>`;
                    }
                    if (block.startsWith('&gt;')) {
                        return `<blockquote>${block.replace(/^&gt;\s?/gm, '').replace(/\n/g, '<br>')}</blockquote>`;
                    }
                    return `<p>${block.replace(/\n/g, '<br>')}</p>`;
                })
                .join('');
        }
        
        function formatScalar(value) {
            if (value == null) return 'null';
            if (typeof value === 'object') return JSON.stringify(value);
            return String(value);
        }
        
        function escapeHtml(value) {
            return String(value)
                .replace(/&/g, '&amp;')
                .replace(/</g, '&lt;')
                .replace(/>/g, '&gt;')
                .replace(/"/g, '&quot;')
                .replace(/'/g, '&#39;');
        }
        
        function handleOAuthWindowMessage(event) {
            if (!event.data || event.data.type !== 'oauth_complete' || !event.data.success) {
                return;
            }
            addMessage('system', {
                text: 'Google authentication completed in the browser. The session is ready for Gmail, Drive, and Calendar tools.',
                metadata: {
                    provider: 'google',
                    state: event.data.state || '(unknown)',
                    session_id: currentChatSessionId || '(web-chat)'
                }
            });
        }
        
        // Configuration Editor Functions
        let currentConfig = null;
        
        async function showConfigEditor() {
            document.getElementById('config-modal').style.display = 'flex';
            await loadConfiguration();
        }
        
        function hideConfigEditor() {
            document.getElementById('config-modal').style.display = 'none';
            hideConfigStatus();
        }
        
        function showConfigTab(tabName) {
            // Hide all tabs
            document.querySelectorAll('.config-tab-content').forEach(el => el.classList.remove('active'));
            document.querySelectorAll('.tab-btn').forEach(el => el.classList.remove('active'));
            
            // Show selected tab
            document.getElementById('tab-' + tabName).classList.add('active');
            event.target.classList.add('active');
        }
        
        async function loadConfiguration() {
            try {
                const res = await fetch('/api/config');
                if (res.ok) {
                    currentConfig = await res.json();
                    populateConfigForm(currentConfig);
                }
            } catch (err) {
                showConfigStatus('Failed to load configuration', 'error');
            }
        }
        
        function populateConfigForm(config) {
            // Agent tab
            if (config.agent) {
                document.getElementById('cfg-provider').value = config.agent.provider || 'openai';
                document.getElementById('cfg-model').value = config.agent.model || '';
                document.getElementById('cfg-provider-profile').value = config.agent.provider_profile || '(none)';
                document.getElementById('cfg-identity-format').value = config.agent.identity_format || 'auto';
                document.getElementById('cfg-soul-path').value = config.agent.soul_path || '(not configured)';
            }
            
            // Channels tab
            if (config.channels) {
                const ws = config.channels.websocket || {};
                document.getElementById('cfg-ws-enabled').checked = ws.enabled !== false;
                document.getElementById('cfg-ws-pairing').checked = 
                    ws.extra?.require_pairing === true || ws.dm_policy === 'Pairing';
                document.getElementById('cfg-ws-port').value = ws.port || 3000;
                
                const webhook = config.channels.webhook || {};
                document.getElementById('cfg-webhook-enabled').checked = webhook.enabled === true;
                document.getElementById('cfg-webhook-port').value = webhook.port || 8080;
                document.getElementById('cfg-webhook-proxy-status').textContent =
                    webhook.proxy_configured ? (webhook.proxy_display || '(configured)') : '(not configured)';
            }
            
            // Security tab
            if (config.security) {
                document.getElementById('cfg-approval-mode').value = config.security.approval_mode || 'autonomous';
                document.getElementById('cfg-prompt-injection').checked = config.security.prompt_injection_defense !== false;
                document.getElementById('cfg-secret-leak').checked = config.security.secret_leak_detection !== false;
                document.getElementById('cfg-wasm-sandbox').checked = config.security.wasm_sandbox !== false;
                document.getElementById('cfg-blocklist').value = (config.security.extra_blocked || []).join(', ');
                document.getElementById('cfg-docker-sandbox').checked = config.security.docker?.enabled === true;
                document.getElementById('cfg-docker-image').value = config.security.docker?.image || 'borgclaw-sandbox:base';
                document.getElementById('cfg-docker-network').value = config.security.docker?.network || 'none';
                document.getElementById('cfg-docker-workspace-mount').value = config.security.docker?.workspace_mount || 'ro';
                document.getElementById('cfg-docker-timeout').value = config.security.docker?.timeout_seconds || 120;
            }
            if (config.runtime?.processes) {
                document.getElementById('process-runtime-status').innerHTML = `
                    <div class="skill-status-item">
                        <span class="status-dot ${config.runtime.processes.running > 0 ? 'ok' : 'off'}"></span>
                        <span>State: ${config.runtime.processes.state_path}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ok"></span>
                        <span>Processes total=${config.runtime.processes.total}, running=${config.runtime.processes.running}, finished=${config.runtime.processes.finished}</span>
                    </div>
                `;
            }
            
            // Memory tab
            if (config.memory) {
                document.getElementById('cfg-memory-backend').value = config.memory.backend || 'sqlite';
                document.getElementById('cfg-hybrid-search').checked = config.memory.hybrid_search === true;
                document.getElementById('cfg-session-max').value = config.memory.session_max_entries || 1000;
                document.getElementById('cfg-db-path').value = config.memory.database_path || '.borgclaw/memory.db';
                document.getElementById('cfg-embedding-endpoint').value = config.memory.embedding_endpoint || '';
                document.getElementById('memory-external-status').innerHTML = `
                    <div class="skill-status-item">
                        <span class="status-dot ${config.memory.external?.enabled ? 'ok' : 'off'}"></span>
                        <span>External adapter ${config.memory.external?.enabled ? '(enabled)' : '(disabled)'}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ${config.memory.external?.endpoint ? 'ok' : 'off'}"></span>
                        <span>Endpoint ${config.memory.external?.endpoint || '(not configured)'}</span>
                    </div>
                `;
                document.getElementById('memory-privacy-status').innerHTML = `
                    <div class="skill-status-item">
                        <span class="status-dot ${config.memory.privacy?.enabled ? 'ok' : 'off'}"></span>
                        <span>Privacy ${config.memory.privacy?.enabled ? '(enabled)' : '(disabled)'}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ok"></span>
                        <span>Default=${config.memory.privacy?.default_sensitivity || 'workspace'}, subagent=${config.memory.privacy?.subagent_scope || 'workspace'}, scheduler=${config.memory.privacy?.scheduler_scope || 'workspace'}, heartbeat=${config.memory.privacy?.heartbeat_scope || 'workspace'}</span>
                    </div>
                `;
            }
            
            // Skills tab
            if (config.skills) {
                document.getElementById('cfg-auto-load').checked = config.skills.auto_load !== false;
                document.getElementById('cfg-skills-path').value = config.skills.skills_path || '.borgclaw/skills';
                document.getElementById('cfg-workspace-skills-path').value = config.skills.workspace_path || '.borgclaw/workspace/skills';
                document.getElementById('cfg-skills-registry-url').value = config.skills.registry_url || '(none)';
                
                // Populate skill status
                const skillList = document.getElementById('skill-status-list');
                skillList.innerHTML = `
                    <div class="skill-status-item">
                        <span class="status-dot ${config.skills.github_configured ? 'ok' : 'off'}"></span>
                        <span>GitHub ${config.skills.github_configured ? '(configured)' : '(not configured)'}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ${config.skills.google_configured ? 'ok' : 'off'}"></span>
                        <span>Google ${config.skills.google_configured ? '(configured)' : '(not configured)'}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ${config.skills.browser_configured ? 'ok' : 'off'}"></span>
                        <span>Browser ${config.skills.browser_configured ? '(configured)' : '(not configured)'}</span>
                    </div>
                    <div class="skill-status-item">
                        <span class="status-dot ok"></span>
                        <span>Ready effective=${config.skills.summary?.ready_effective || 0}, gated=${config.skills.summary?.gated || 0}, shadowed=${config.skills.summary?.shadowed || 0}</span>
                    </div>
                `;
                const discoveryList = document.getElementById('skill-discovery-list');
                const discovered = config.skills.discovered || [];
                if (discovered.length === 0) {
                    discoveryList.innerHTML = '<div class="skill-status-item"><span class="status-dot off"></span><span>No discovered SKILL.md skills</span></div>';
                } else {
                    discoveryList.innerHTML = discovered.map(skill => `
                        <div class="skill-status-item">
                            <span class="status-dot ${(skill.available && skill.effective) ? 'ok' : 'off'}"></span>
                            <span>${skill.id} [${skill.source}] ${skill.effective ? '(effective)' : '(shadowed)'} ${skill.available ? '' : '- ' + (skill.reasons || []).join(', ')}</span>
                        </div>
                    `).join('');
                }
            }
        }
        
        async function saveConfiguration() {
            const updates = {
                agent: {},
                channels: {},
                security: {},
                memory: {},
                skills: {}
            };
            
            // Collect agent settings
            const provider = document.getElementById('cfg-provider').value;
            const model = document.getElementById('cfg-model').value.trim();
            if (provider) updates.agent.provider = provider;
            if (model) updates.agent.model = model;
            
            // Collect channel settings
            updates.channels.websocket = {
                enabled: document.getElementById('cfg-ws-enabled').checked,
                port: parseInt(document.getElementById('cfg-ws-port').value) || 3000,
                extra: {
                    require_pairing: document.getElementById('cfg-ws-pairing').checked
                }
            };
            updates.channels.webhook = {
                enabled: document.getElementById('cfg-webhook-enabled').checked,
                port: parseInt(document.getElementById('cfg-webhook-port').value) || 8080
            };
            
            // Collect security settings
            updates.security.approval_mode = document.getElementById('cfg-approval-mode').value;
            updates.security.pairing_enabled = document.getElementById('cfg-ws-pairing').checked;
            updates.security.prompt_injection_defense = document.getElementById('cfg-prompt-injection').checked;
            updates.security.secret_leak_detection = document.getElementById('cfg-secret-leak').checked;
            updates.security.wasm_sandbox = document.getElementById('cfg-wasm-sandbox').checked;
            
            const blocklistVal = document.getElementById('cfg-blocklist').value.trim();
            if (blocklistVal) {
                updates.security.extra_blocked = blocklistVal.split(',').map(s => s.trim()).filter(s => s);
            }
            updates.security.docker = {
                enabled: document.getElementById('cfg-docker-sandbox').checked,
                image: document.getElementById('cfg-docker-image').value.trim(),
                network: document.getElementById('cfg-docker-network').value,
                workspace_mount: document.getElementById('cfg-docker-workspace-mount').value,
                timeout_seconds: parseInt(document.getElementById('cfg-docker-timeout').value) || 120
            };
            
            // Collect memory settings
            updates.memory.backend = document.getElementById('cfg-memory-backend').value;
            updates.memory.hybrid_search = document.getElementById('cfg-hybrid-search').checked;
            updates.memory.session_max_entries = parseInt(document.getElementById('cfg-session-max').value) || 1000;
            updates.memory.embedding_endpoint = document.getElementById('cfg-embedding-endpoint').value.trim();
            
            // Collect skills settings
            updates.skills.auto_load = document.getElementById('cfg-auto-load').checked;
            
            try {
                const res = await fetch('/api/config', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(updates)
                });
                
                const result = await res.json();
                if (result.success) {
                    showConfigStatus(result.message + ' Changes: ' + result.changes.join(', '), 'success');
                    if (result.requires_restart) {
                        setTimeout(() => {
                            showConfigStatus('Configuration saved! A restart is required for some changes to take effect.', 'success');
                        }, 2000);
                    }
                } else {
                    showConfigStatus('Error: ' + (result.error || 'Unknown error'), 'error');
                }
            } catch (err) {
                showConfigStatus('Failed to save: ' + err.message, 'error');
            }
        }
        
        function showConfigStatus(message, type) {
            const statusEl = document.getElementById('config-status');
            statusEl.textContent = message;
            statusEl.className = 'config-status ' + type;
        }
        
        function hideConfigStatus() {
            document.getElementById('config-status').className = 'config-status';
        }

        async function openInspector(title, url, subtitle) {
            const modal = document.getElementById('inspector-modal');
            document.getElementById('inspector-title').textContent = title;
            document.getElementById('inspector-subtitle').textContent = subtitle || url;
            document.getElementById('inspector-summary').innerHTML = '<div class="inspector-card">Loading…</div>';
            document.getElementById('inspector-json').textContent = '';
            modal.style.display = 'flex';

            try {
                const res = await fetch(url);
                const data = await res.json();
                renderInspectorData(data, res.status, url);
            } catch (err) {
                document.getElementById('inspector-summary').innerHTML = `
                    <div class="inspector-card">
                        <div class="inspector-card-title">Request Failed</div>
                        <div>${escapeHtml(err.message)}</div>
                    </div>
                `;
            }
        }

        function openApiGuide(title, endpoint, description) {
            const modal = document.getElementById('inspector-modal');
            document.getElementById('inspector-title').textContent = title;
            document.getElementById('inspector-subtitle').textContent = endpoint;
            document.getElementById('inspector-summary').innerHTML = `
                <div class="inspector-card">
                    <div class="inspector-card-title">${escapeHtml(endpoint)}</div>
                    <div>${escapeHtml(description)}</div>
                </div>
                <div class="inspector-card">
                    <div class="inspector-card-title">Dashboard Note</div>
                    <div>This endpoint is intentionally surfaced in the UI instead of opening raw JSON in a separate tab.</div>
                </div>
            `;
            document.getElementById('inspector-json').textContent = '';
            modal.style.display = 'flex';
        }

        function renderInspectorData(data, status, url) {
            const summary = document.getElementById('inspector-summary');
            const cards = [
                `<div class="inspector-card"><div class="inspector-card-title">HTTP Status</div><div>${status}</div></div>`,
                `<div class="inspector-card"><div class="inspector-card-title">Endpoint</div><div>${escapeHtml(url)}</div></div>`
            ];

            if (data && typeof data === 'object') {
                Object.entries(data).slice(0, 6).forEach(([key, value]) => {
                    cards.push(`
                        <div class="inspector-card">
                            <div class="inspector-card-title">${escapeHtml(key)}</div>
                            <div>${escapeHtml(formatScalar(value))}</div>
                        </div>
                    `);
                });
            }

            summary.innerHTML = cards.join('');
            document.getElementById('inspector-json').textContent = JSON.stringify(data, null, 2);
        }

        function hideInspector() {
            document.getElementById('inspector-modal').style.display = 'none';
        }
        
        // Close modal on backdrop click
        document.getElementById('config-modal').addEventListener('click', function(e) {
            if (e.target === this) hideConfigEditor();
        });
        document.getElementById('inspector-modal').addEventListener('click', function(e) {
            if (e.target === this) hideInspector();
        });
        
        // Keyboard shortcut for config (Ctrl/Cmd + ,)
        document.addEventListener('keydown', function(e) {
            if ((e.ctrlKey || e.metaKey) && e.key === ',') {
                e.preventDefault();
                showConfigEditor();
            }
            if (e.key === 'Escape') {
                hideConfigEditor();
                hideInspector();
            }
        });
        
        // Fetch and display live runtime version
        fetch('/api/version')
            .then(r => r.json())
            .then(data => {
                const versionEl = document.getElementById('footer-version');
                if (versionEl && data.version) {
                    versionEl.textContent = 'BorgClaw v' + data.version + ' — Personal AI Agent Framework';
                }
            })
            .catch(() => {
                // Fallback: leave static text if API fails
            });
    </script>
</body>
</html>"##;

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
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let client_id = uuid::Uuid::new_v4().to_string();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<serde_json::Value>();
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
    {
        let mut conns = state.connections.write().await;
        conns.insert(
            client_id.clone(),
            ConnectionHandle {
                info: ConnectionInfo {
                    client_id: client_id.clone(),
                    connected_at: chrono::Utc::now(),
                    session_id: None,
                    authenticated: !requires_pairing,
                    messages_received: 0,
                    messages_sent: 0,
                },
                outbound: outbound_tx.clone(),
            },
        );
    }
    info!(
        "New WebSocket connection: {} (active: {})",
        client_id,
        state.metrics.connections_active.load(Ordering::SeqCst)
    );

    let writer = tokio::spawn(async move {
        while let Some(event) = outbound_rx.recv().await {
            if socket_sender
                .send(Message::Text(event.to_string()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    queue_client_event(
        &state,
        &client_id,
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
                if !queue_client_event(&state, &client_id, serde_json::json!({
                    "type": "heartbeat",
                    "client_id": client_id,
                    "ts": chrono::Utc::now(),
                })).await {
                    break;
                }
            }
            msg = socket_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        state.metrics.increment_messages_received();
                        increment_connection_received(&state, &client_id).await;
                        if let Err(e) = handle_ws_message(&state, &client_id, &text).await {
                            error!("Error handling message: {}", e);
                            queue_client_event(&state, &client_id, error_event("internal gateway error")).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        queue_client_event(&state, &client_id, error_event(&e.to_string())).await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    writer.abort();

    state.metrics.decrement_connections();
    {
        let mut conns = state.connections.write().await;
        conns.remove(&client_id);
    }
    info!(
        "Connection closed: {} (active: {})",
        client_id,
        state.metrics.connections_active.load(Ordering::SeqCst)
    );
}

async fn handle_ws_message(
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
            queue_client_event(
                state,
                client_id,
                serde_json::json!({
                    "type": "pairing_code",
                    "client_id": client_id,
                    "pairing_code": code
                }),
            )
            .await;
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
                update_connection_auth(state, client_id, true).await;
            } else {
                state.metrics.increment_auth_failure();
            }
            queue_client_event(state, client_id, serde_json::json!({
                    "type": if authenticated { "authenticated" } else { "error" },
                    "client_id": client_id,
                    "approved_sender": approved_sender,
                    "message": if authenticated { "Pairing approved" } else { "Pairing code belongs to another sender" }
                }))
            .await;
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
                    update_connection_session(state, client_id, &outcome.session_id.0).await;
                    queue_client_event(
                        state,
                        client_id,
                        serde_json::json!({
                            "type": "response",
                            "session_id": outcome.session_id.0,
                            "text": outcome.response.text,
                            "content_type": message_payload_kind(&outcome.outbound.content),
                            "content": message_payload_json(&outcome.outbound.content),
                            "tool_calls": outcome.response.tool_calls,
                            "metadata": outcome.response.metadata,
                        }),
                    )
                    .await;
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
                    queue_client_event(state, client_id, event).await;
                }
                Err(err) => {
                    queue_client_event(state, client_id, error_event(&err.to_string())).await;
                }
            }
        }
        "ping" => {
            queue_client_event(state, client_id, serde_json::json!({ "type": "pong" })).await;
        }
        _ => {
            queue_client_event(state, client_id, error_event("Unknown message type")).await;
        }
    }

    Ok(())
}

async fn queue_client_event(
    state: &GatewayState,
    client_id: &str,
    event: serde_json::Value,
) -> bool {
    let mut connections = state.connections.write().await;
    let Some(handle) = connections.get_mut(client_id) else {
        return false;
    };

    if handle.outbound.send(event).is_err() {
        connections.remove(client_id);
        return false;
    }

    handle.info.messages_sent += 1;
    state.metrics.increment_messages_sent();
    true
}

async fn update_connection_auth(state: &GatewayState, client_id: &str, authenticated: bool) {
    let mut connections = state.connections.write().await;
    if let Some(handle) = connections.get_mut(client_id) {
        handle.info.authenticated = authenticated;
    }
}

async fn update_connection_session(state: &GatewayState, client_id: &str, session_id: &str) {
    let mut connections = state.connections.write().await;
    if let Some(handle) = connections.get_mut(client_id) {
        handle.info.session_id = Some(session_id.to_string());
    }
}

async fn increment_connection_received(state: &GatewayState, client_id: &str) {
    let mut connections = state.connections.write().await;
    if let Some(handle) = connections.get_mut(client_id) {
        handle.info.messages_received += 1;
    }
}

fn message_payload_kind(payload: &MessagePayload) -> &'static str {
    match payload {
        MessagePayload::Text(_) => "text",
        MessagePayload::Markdown(_) => "markdown",
        MessagePayload::Html(_) => "html",
        MessagePayload::Media { mime_type, .. } if mime_type.starts_with("image/") => "image",
        MessagePayload::Media { mime_type, .. } if mime_type.starts_with("video/") => "video",
        MessagePayload::Media { .. } => "media",
        MessagePayload::File { .. } => "file",
    }
}

fn message_payload_json(payload: &MessagePayload) -> serde_json::Value {
    match payload {
        MessagePayload::Text(text) => serde_json::json!({
            "kind": "text",
            "text": text,
        }),
        MessagePayload::Markdown(text) => serde_json::json!({
            "kind": "markdown",
            "text": text,
        }),
        MessagePayload::Html(html) => serde_json::json!({
            "kind": "html",
            "html": html,
        }),
        MessagePayload::Media { url, mime_type } => serde_json::json!({
            "kind": "media",
            "url": url,
            "mime_type": mime_type,
        }),
        MessagePayload::File { path, name } => serde_json::json!({
            "kind": "file",
            "path": path,
            "name": name,
        }),
    }
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
        .unwrap_or(3000)
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
                    if let Some(trigger) = webhook.get_trigger(trigger_id).await {
                        let proxy_url = state
                            .config
                            .channels
                            .get("webhook")
                            .and_then(|channel| channel.proxy_url.as_deref());
                        if let Err(err) = forward_configured_webhook(
                            &trigger,
                            proxy_url,
                            &route_outcome.response.text,
                        )
                        .await
                        {
                            warn!("webhook forward failed for trigger {}: {}", trigger_id, err);
                            return json_error_response(StatusCode::BAD_GATEWAY, "forward failed");
                        }
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

async fn forward_configured_webhook(
    trigger: &WebhookTrigger,
    proxy_url: Option<&str>,
    message: &str,
) -> Result<(), String> {
    let Some(url) = trigger.forward_url.as_deref() else {
        return Ok(());
    };

    let method =
        reqwest::Method::from_bytes(trigger.method.as_bytes()).map_err(|err| err.to_string())?;
    let body = render_webhook_body(trigger.body_template.as_deref(), message);
    let client = borgclaw_core::config::apply_proxy_to_client_builder(
        reqwest::Client::builder(),
        proxy_url,
    )?
    .build()
    .map_err(|err| err.to_string())?;
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

/// Version endpoint - returns live runtime version information
async fn api_version() -> impl IntoResponse {
    let body = serde_json::json!({
        "name": "BorgClaw",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Personal AI Agent Framework",
        "repository": "https://github.com/lealvona/borgclaw",
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
    checks.insert(
        "workspace".to_string(),
        serde_json::json!(if workspace_ok { "ok" } else { "error" }),
    );

    let memory_db_ok = match state.config.memory.effective_backend() {
        borgclaw_core::config::MemoryBackend::Sqlite => state
            .config
            .memory
            .database_path
            .parent()
            .map(|p| p.exists())
            .unwrap_or(true),
        borgclaw_core::config::MemoryBackend::Postgres => state
            .config
            .memory
            .connection_string
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        borgclaw_core::config::MemoryBackend::Memory => true,
    };
    checks.insert(
        "memory_db".to_string(),
        serde_json::json!(if memory_db_ok { "ok" } else { "error" }),
    );

    let healthy = workspace_ok && memory_db_ok;
    let body = serde_json::json!({
        "status": if healthy { "healthy" } else { "unhealthy" },
        "checks": checks,
    });

    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, serde_json::to_string(&body).unwrap_or_default())
}

async fn api_ready(State(state): State<GatewayState>) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();

    let workspace_ready = state.config.agent.workspace.exists();
    checks.insert(
        "workspace".to_string(),
        serde_json::json!(if workspace_ready {
            "ready"
        } else {
            "not_ready"
        }),
    );

    let skills_path = &state.config.skills.skills_path;
    let skills_ready = skills_path.exists() || std::fs::create_dir_all(skills_path).is_ok();
    checks.insert(
        "skills_path".to_string(),
        serde_json::json!(if skills_ready { "ready" } else { "not_ready" }),
    );

    let ready = workspace_ready && skills_ready;
    let body = serde_json::json!({
        "ready": ready,
        "checks": checks,
    });

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, serde_json::to_string(&body).unwrap_or_default())
}

async fn api_metrics(State(state): State<GatewayState>) -> impl IntoResponse {
    let metrics = state.metrics.snapshot();
    (
        StatusCode::OK,
        serde_json::to_string(&metrics).unwrap_or_default(),
    )
}

#[derive(Debug, serde::Serialize)]
struct GatewaySkillStatus {
    id: String,
    name: String,
    source: String,
    effective: bool,
    available: bool,
    reasons: Vec<String>,
}

async fn gateway_skill_statuses(config: &AppConfig) -> Vec<GatewaySkillStatus> {
    let mut registry = SkillsRegistry::new(config.skills.skills_path.clone());
    let bundled = PathBuf::from("skills");
    if bundled.exists() {
        registry = registry.with_bundled_path(bundled);
    }
    let workspace_skills = config.agent.workspace.join("skills");
    if workspace_skills != config.skills.skills_path {
        registry = registry.with_workspace_path(workspace_skills);
    }
    if registry.load_all().await.is_err() {
        return Vec::new();
    }

    let catalog = registry.catalog().await;
    let mut skills = Vec::with_capacity(catalog.len());
    for entry in catalog {
        let (available, reasons) = gateway_skill_requirements(config, &entry.manifest).await;
        skills.push(GatewaySkillStatus {
            id: entry.id,
            name: entry.manifest.name,
            source: entry.source.as_str().to_string(),
            effective: entry.shadowed_by.is_none(),
            available,
            reasons,
        });
    }
    skills
}

async fn gateway_skill_requirements(
    config: &AppConfig,
    manifest: &borgclaw_core::skills::SkillManifest,
) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();
    if !manifest.is_compatible(env!("CARGO_PKG_VERSION")) {
        reasons.push(format!(
            "requires borgclaw >= {}",
            manifest.min_version.as_deref().unwrap_or("unknown")
        ));
    }
    for binary in &manifest.binaries {
        if !gateway_binary_available(std::path::Path::new(binary)) {
            reasons.push(format!("missing binary '{}'", binary));
        }
    }
    for env_key in manifest.env.keys() {
        if !gateway_env_requirement_available(config, env_key).await {
            reasons.push(format!("missing env/secret '{}'", env_key));
        }
    }
    for config_key in manifest.config.keys() {
        if !gateway_config_key_available(config, config_key) {
            reasons.push(format!("missing config '{}'", config_key));
        }
    }
    (reasons.is_empty(), reasons)
}

fn gateway_binary_available(binary: &std::path::Path) -> bool {
    if binary.components().count() > 1 {
        return binary.exists();
    }

    let Some(binary_name) = binary.to_str() else {
        return false;
    };

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(binary_name);
                candidate.exists()
                    || cfg!(windows) && dir.join(format!("{}.exe", binary_name)).exists()
            })
        })
        .unwrap_or(false)
}

async fn gateway_env_requirement_available(config: &AppConfig, key: &str) -> bool {
    if std::env::var(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }

    SecurityLayer::with_config(config.security.clone())
        .get_secret(key)
        .await
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn gateway_config_key_available(config: &AppConfig, key: &str) -> bool {
    let Ok(value) = serde_json::to_value(config) else {
        return false;
    };
    let mut current = &value;
    for segment in key.split('.') {
        match current {
            serde_json::Value::Object(map) => {
                let Some(next) = map.get(segment) else {
                    return false;
                };
                current = next;
            }
            _ => return false,
        }
    }

    match current {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
        serde_json::Value::Number(_) => true,
    }
}

fn process_runtime_summary(config: &AppConfig) -> serde_json::Value {
    let state_path = process_state_path(&config.agent.workspace);
    let records = load_process_records(&state_path).unwrap_or_default();
    let total = records.len();
    let running = records
        .values()
        .filter(|record| record.status == CommandProcessStatus::Running)
        .count();
    let finished = total.saturating_sub(running);
    serde_json::json!({
        "supported": true,
        "state_path": state_path,
        "total": total,
        "running": running,
        "finished": finished,
    })
}

fn sanitized_channels(config: &AppConfig) -> serde_json::Value {
    let mut channels = serde_json::Map::new();
    for (name, channel) in &config.channels {
        let mut value = serde_json::to_value(channel).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(object) = value.as_object_mut() {
            let (configured, display) = channel_proxy_summary(channel);
            object.insert(
                "proxy_configured".to_string(),
                serde_json::json!(configured),
            );
            if let Some(display) = display {
                object.insert("proxy_display".to_string(), serde_json::json!(display));
            }
        }
        channels.insert(name.clone(), value);
    }
    serde_json::Value::Object(channels)
}

fn channel_proxy_summary(channel: &borgclaw_core::config::ChannelConfig) -> (bool, Option<String>) {
    match channel.proxy_url() {
        Some(proxy_url) => (
            true,
            Some(borgclaw_core::config::proxy_display_value(proxy_url)),
        ),
        None => (false, None),
    }
}

async fn api_config(State(state): State<GatewayState>) -> impl IntoResponse {
    let discovered_skills = gateway_skill_statuses(&state.config).await;
    let ready_skill_count = discovered_skills
        .iter()
        .filter(|skill| skill.available && skill.effective)
        .count();
    let gated_skill_count = discovered_skills
        .iter()
        .filter(|skill| !skill.available)
        .count();
    let shadowed_skill_count = discovered_skills
        .iter()
        .filter(|skill| !skill.effective)
        .count();
    let process_summary = process_runtime_summary(&state.config);
    let sanitized = serde_json::json!({
        "agent": {
            "model": state.config.agent.model,
            "provider": state.config.agent.provider,
            "workspace": state.config.agent.workspace,
            "provider_profile": state.config.agent.provider_profile,
            "identity_format": state.config.agent.identity_format,
            "soul_path": state.config.agent.soul_path,
        },
        "channels": sanitized_channels(&state.config),
        "memory": {
            "backend": state.config.memory.effective_backend(),
            "database_path": state.config.memory.database_path,
            "connection_configured": state.config.memory.connection_string.as_ref().map(|value| !value.trim().is_empty()).unwrap_or(false),
            "embedding_endpoint": state.config.memory.embedding_endpoint,
            "hybrid_search": state.config.memory.hybrid_search,
            "session_max_entries": state.config.memory.session_max_entries,
            "external": {
                "enabled": state.config.memory.external.enabled,
                "endpoint": state.config.memory.external.endpoint,
                "mirror_writes": state.config.memory.external.mirror_writes,
                "timeout_seconds": state.config.memory.external.timeout_seconds,
            },
            "privacy": {
                "enabled": state.config.memory.privacy.enabled,
                "default_sensitivity": state.config.memory.privacy.default_sensitivity,
                "subagent_scope": state.config.memory.privacy.subagent_scope,
                "scheduler_scope": state.config.memory.privacy.scheduler_scope,
                "heartbeat_scope": state.config.memory.privacy.heartbeat_scope,
            },
        },
        "security": {
            "approval_mode": state.config.security.approval_mode,
            "pairing_enabled": state.config.security.pairing.enabled,
            "pairing": state.config.security.pairing,
            "prompt_injection_defense": state.config.security.prompt_injection_defense,
            "secret_leak_detection": state.config.security.secret_leak_detection,
            "wasm_sandbox": state.config.security.wasm_sandbox,
            "command_blocklist": state.config.security.command_blocklist,
            "extra_blocked": state.config.security.extra_blocked,
            "docker": {
                "enabled": state.config.security.docker.enabled,
                "image": state.config.security.docker.image,
                "network": state.config.security.docker.network,
                "workspace_mount": state.config.security.docker.workspace_mount,
                "timeout_seconds": state.config.security.docker.timeout_seconds,
                "allowed_tools": state.config.security.docker.allowed_tools,
                "contexts": state.config.security.docker.contexts,
            },
        },
        "skills": {
            "auto_load": state.config.skills.auto_load,
            "registry_url": state.config.skills.registry_url,
            "github_configured": !state.config.skills.github.token.is_empty(),
            "google_configured": !state.config.skills.google.client_id.is_empty(),
            "browser_configured": !state.config.skills.browser.node_path.as_os_str().is_empty(),
            "skills_path": state.config.skills.skills_path,
            "workspace_path": state.config.agent.workspace.join("skills"),
            "discovered": discovered_skills,
            "summary": {
                "ready_effective": ready_skill_count,
                "gated": gated_skill_count,
                "shadowed": shadowed_skill_count,
            },
        },
        "runtime": {
            "processes": process_summary,
        },
        "mcp": state.config.mcp,
    });

    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        serde_json::to_string_pretty(&sanitized).unwrap_or_default(),
    )
}

#[derive(Debug, serde::Deserialize)]
struct ConfigUpdateRequest {
    #[serde(default)]
    agent: Option<AgentConfigUpdate>,
    #[serde(default)]
    channels: Option<std::collections::HashMap<String, borgclaw_core::config::ChannelConfig>>,
    #[serde(default)]
    memory: Option<MemoryConfigUpdate>,
    #[serde(default)]
    security: Option<SecurityConfigUpdate>,
    #[serde(default)]
    skills: Option<SkillsConfigUpdate>,
    #[serde(default)]
    mcp: Option<borgclaw_core::config::McpConfig>,
}

#[derive(Debug, serde::Deserialize)]
struct AgentConfigUpdate {
    model: Option<String>,
    provider: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct MemoryConfigUpdate {
    backend: Option<String>,
    hybrid_search: Option<bool>,
    connection_string: Option<String>,
    embedding_endpoint: Option<String>,
    session_max_entries: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct SecurityConfigUpdate {
    approval_mode: Option<String>,
    pairing_enabled: Option<bool>,
    prompt_injection_defense: Option<bool>,
    secret_leak_detection: Option<bool>,
    wasm_sandbox: Option<bool>,
    extra_blocked: Option<Vec<String>>,
    docker: Option<DockerConfigUpdate>,
}

#[derive(Debug, serde::Deserialize)]
struct DockerConfigUpdate {
    enabled: Option<bool>,
    image: Option<String>,
    network: Option<String>,
    workspace_mount: Option<String>,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct SkillsConfigUpdate {
    auto_load: Option<bool>,
}

async fn api_config_post(
    State(state): State<GatewayState>,
    axum::Json(body): axum::Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    // Build updated config by modifying the current config
    let mut updated_config = (*state.config).clone();
    let mut changes_made = Vec::new();

    // Update agent config
    if let Some(agent) = body.agent {
        if let Some(model) = agent.model {
            updated_config.agent.model = model.clone();
            changes_made.push(format!("agent.model = {}", model));
        }
        if let Some(provider) = agent.provider {
            updated_config.agent.provider = provider.clone();
            changes_made.push(format!("agent.provider = {}", provider));
        }
    }

    // Update channels
    if let Some(channels) = body.channels {
        for (name, config) in channels {
            updated_config.channels.insert(name.clone(), config);
            changes_made.push(format!("channels.{} updated", name));
        }
    }

    // Update memory config
    if let Some(memory) = body.memory {
        if let Some(backend) = memory.backend {
            updated_config.memory.backend = match backend.as_str() {
                "postgres" | "Postgres" => borgclaw_core::config::MemoryBackend::Postgres,
                "memory" | "Memory" => borgclaw_core::config::MemoryBackend::Memory,
                _ => borgclaw_core::config::MemoryBackend::Sqlite,
            };
            changes_made.push(format!("memory.backend = {}", backend));
        }
        if let Some(hybrid_search) = memory.hybrid_search {
            updated_config.memory.hybrid_search = hybrid_search;
            changes_made.push(format!("memory.hybrid_search = {}", hybrid_search));
        }
        if let Some(connection_string) = memory.connection_string {
            updated_config.memory.connection_string = Some(connection_string);
            changes_made.push("memory.connection_string = [redacted]".to_string());
        }
        if let Some(embedding_endpoint) = memory.embedding_endpoint {
            if embedding_endpoint.trim().is_empty() {
                updated_config.memory.embedding_endpoint = None;
                changes_made.push("memory.embedding_endpoint cleared".to_string());
            } else {
                updated_config.memory.embedding_endpoint = Some(embedding_endpoint.clone());
                changes_made.push(format!(
                    "memory.embedding_endpoint = {}",
                    embedding_endpoint
                ));
            }
        }
        if let Some(session_max_entries) = memory.session_max_entries {
            updated_config.memory.session_max_entries = session_max_entries;
            changes_made.push(format!(
                "memory.session_max_entries = {}",
                session_max_entries
            ));
        }
    }

    // Update security config
    if let Some(security) = body.security {
        if let Some(approval_mode) = security.approval_mode {
            let mode = match approval_mode.as_str() {
                "readonly" | "ReadOnly" => borgclaw_core::config::ApprovalMode::ReadOnly,
                "supervised" | "Supervised" => borgclaw_core::config::ApprovalMode::Supervised,
                "autonomous" | "Autonomous" => borgclaw_core::config::ApprovalMode::Autonomous,
                _ => borgclaw_core::config::ApprovalMode::Autonomous,
            };
            updated_config.security.approval_mode = mode;
            changes_made.push(format!("security.approval_mode = {}", approval_mode));
        }
        if let Some(pairing_enabled) = security.pairing_enabled {
            updated_config.security.pairing.enabled = pairing_enabled;
            changes_made.push(format!("security.pairing.enabled = {}", pairing_enabled));
        }
        if let Some(prompt_injection_defense) = security.prompt_injection_defense {
            updated_config.security.prompt_injection_defense = prompt_injection_defense;
            changes_made.push(format!(
                "security.prompt_injection_defense = {}",
                prompt_injection_defense
            ));
        }
        if let Some(secret_leak_detection) = security.secret_leak_detection {
            updated_config.security.secret_leak_detection = secret_leak_detection;
            changes_made.push(format!(
                "security.secret_leak_detection = {}",
                secret_leak_detection
            ));
        }
        if let Some(wasm_sandbox) = security.wasm_sandbox {
            updated_config.security.wasm_sandbox = wasm_sandbox;
            changes_made.push(format!("security.wasm_sandbox = {}", wasm_sandbox));
        }
        if let Some(extra_blocked) = security.extra_blocked {
            updated_config.security.extra_blocked = extra_blocked.clone();
            changes_made.push(format!(
                "security.extra_blocked = {} entries",
                extra_blocked.len()
            ));
        }
        if let Some(docker) = security.docker {
            if let Some(enabled) = docker.enabled {
                updated_config.security.docker.enabled = enabled;
                changes_made.push(format!("security.docker.enabled = {}", enabled));
            }
            if let Some(image) = docker.image {
                updated_config.security.docker.image = image.clone();
                changes_made.push(format!("security.docker.image = {}", image));
            }
            if let Some(network) = docker.network {
                updated_config.security.docker.network = match network.as_str() {
                    "bridge" | "Bridge" => borgclaw_core::config::DockerNetworkPolicy::Bridge,
                    _ => borgclaw_core::config::DockerNetworkPolicy::None,
                };
                changes_made.push(format!("security.docker.network = {}", network));
            }
            if let Some(workspace_mount) = docker.workspace_mount {
                updated_config.security.docker.workspace_mount = match workspace_mount.as_str() {
                    "rw" | "readwrite" | "ReadWrite" => {
                        borgclaw_core::config::DockerWorkspaceMount::ReadWrite
                    }
                    "off" | "Off" => borgclaw_core::config::DockerWorkspaceMount::Off,
                    _ => borgclaw_core::config::DockerWorkspaceMount::ReadOnly,
                };
                changes_made.push(format!(
                    "security.docker.workspace_mount = {}",
                    workspace_mount
                ));
            }
            if let Some(timeout_seconds) = docker.timeout_seconds {
                updated_config.security.docker.timeout_seconds = timeout_seconds;
                changes_made.push(format!(
                    "security.docker.timeout_seconds = {}",
                    timeout_seconds
                ));
            }
        }
    }

    // Update skills config
    if let Some(skills) = body.skills {
        if let Some(auto_load) = skills.auto_load {
            updated_config.skills.auto_load = auto_load;
            changes_made.push(format!("skills.auto_load = {}", auto_load));
        }
    }

    // Update MCP config
    if let Some(mcp) = body.mcp {
        updated_config.mcp = mcp.clone();
        changes_made.push("mcp config updated".to_string());
    }

    // Save config to file
    match save_config_to_file(&updated_config, &state.config_path) {
        Ok(_) => {
            info!("Configuration updated: {:?}", changes_made);
            axum::Json(serde_json::json!({
                "success": true,
                "message": "Configuration updated successfully. Restart required for some changes to take effect.",
                "changes": changes_made,
                "requires_restart": true,
            }))
        }
        Err(e) => {
            error!("Failed to save configuration: {}", e);
            axum::Json(serde_json::json!({
                "success": false,
                "error": format!("Failed to save configuration: {}", e),
            }))
        }
    }
}

fn save_config_to_file(config: &AppConfig, path: &PathBuf) -> Result<(), String> {
    // Serialize to TOML
    let toml_value = toml::Value::try_from(config).map_err(|e| e.to_string())?;
    let toml_string = toml::to_string_pretty(&toml_value).map_err(|e| e.to_string())?;

    // Write to file
    std::fs::write(path, toml_string).map_err(|e| e.to_string())?;

    Ok(())
}

async fn api_chat_get() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, "Use POST for chat")
}

async fn api_chat_post(
    State(state): State<GatewayState>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let group_id = body
        .get("group_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let sender_id = body
        .get("sender_id")
        .and_then(|v| v.as_str())
        .unwrap_or("http-client");

    if content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "content is required"})),
        );
    }

    let inbound = InboundMessage {
        channel: ChannelType::websocket(),
        sender: Sender::new(sender_id).with_name("HTTP Client"),
        content: MessagePayload::text(content),
        group_id,
        timestamp: chrono::Utc::now(),
        raw: body,
    };

    match state.router.route(inbound).await {
        Ok(outcome) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({
                "session_id": outcome.session_id.0,
                "text": outcome.response.text,
                "content_type": message_payload_kind(&outcome.outbound.content),
                "content": message_payload_json(&outcome.outbound.content),
                "tool_calls": outcome.response.tool_calls,
                "metadata": outcome.response.metadata,
            })),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": err.to_string()})),
        ),
    }
}

async fn api_tools() -> impl IntoResponse {
    let tools = builtin_tools();
    let tool_list: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "requires_approval": t.requires_approval,
                "tags": t.tags,
            })
        })
        .collect();

    axum::Json(serde_json::json!({
        "tools": tool_list,
        "count": tool_list.len(),
    }))
}

async fn api_connections(State(state): State<GatewayState>) -> impl IntoResponse {
    let conns = state.connections.read().await;
    let connections: Vec<ConnectionInfo> =
        conns.values().map(|handle| handle.info.clone()).collect();

    axum::Json(serde_json::json!({
        "connections": connections,
        "count": connections.len(),
    }))
}

async fn api_schedules(State(state): State<GatewayState>) -> impl IntoResponse {
    let path = state.config.agent.workspace.join("scheduler.json");
    if !path.exists() {
        return axum::Json(serde_json::json!({
            "jobs": [],
            "count": 0,
            "message": "No scheduler state found"
        }));
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                let jobs = state.get("jobs").cloned().unwrap_or(serde_json::json!([]));
                let count = jobs.as_array().map(|a| a.len()).unwrap_or(0);
                axum::Json(serde_json::json!({
                    "jobs": jobs,
                    "count": count,
                }))
            } else {
                axum::Json(serde_json::json!({
                    "jobs": [],
                    "count": 0,
                    "error": "Failed to parse scheduler state"
                }))
            }
        }
        Err(e) => axum::Json(serde_json::json!({
            "jobs": [],
            "count": 0,
            "error": e.to_string()
        })),
    }
}

async fn api_heartbeat_tasks(State(state): State<GatewayState>) -> impl IntoResponse {
    let path = state.config.agent.workspace.join("heartbeat.json");
    if !path.exists() {
        return axum::Json(serde_json::json!({
            "tasks": [],
            "count": 0,
            "message": "No heartbeat state found"
        }));
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                let tasks = state.get("tasks").cloned().unwrap_or(serde_json::json!([]));
                let count = tasks.as_array().map(|a| a.len()).unwrap_or(0);
                axum::Json(serde_json::json!({
                    "tasks": tasks,
                    "count": count,
                }))
            } else {
                axum::Json(serde_json::json!({
                    "tasks": [],
                    "count": 0,
                    "error": "Failed to parse heartbeat state"
                }))
            }
        }
        Err(e) => axum::Json(serde_json::json!({
            "tasks": [],
            "count": 0,
            "error": e.to_string()
        })),
    }
}

async fn api_subagents(State(state): State<GatewayState>) -> impl IntoResponse {
    let path = state.config.agent.workspace.join("subagents.json");
    if !path.exists() {
        return axum::Json(serde_json::json!({
            "tasks": [],
            "count": 0,
            "message": "No subagent state found"
        }));
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                let tasks = state.get("tasks").cloned().unwrap_or(serde_json::json!([]));
                let count = tasks.as_array().map(|a| a.len()).unwrap_or(0);
                axum::Json(serde_json::json!({
                    "tasks": tasks,
                    "count": count,
                }))
            } else {
                axum::Json(serde_json::json!({
                    "tasks": [],
                    "count": 0,
                    "error": "Failed to parse subagent state"
                }))
            }
        }
        Err(e) => axum::Json(serde_json::json!({
            "tasks": [],
            "count": 0,
            "error": e.to_string()
        })),
    }
}

async fn api_mcp_servers(State(state): State<GatewayState>) -> impl IntoResponse {
    let servers: Vec<serde_json::Value> = state
        .config
        .mcp
        .servers
        .iter()
        .map(|(name, server)| {
            serde_json::json!({
                "name": name,
                "url": server.url,
                "transport": server.transport,
                "enabled": !server.url.as_ref().map(|u| u.is_empty()).unwrap_or(true),
            })
        })
        .collect();

    axum::Json(serde_json::json!({
        "servers": servers,
        "count": servers.len(),
    }))
}

async fn api_doctor(State(state): State<GatewayState>) -> impl IntoResponse {
    let mut checks = Vec::new();

    // Workspace check
    if state.config.agent.workspace.exists() {
        checks.push(serde_json::json!({"name": "workspace", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "workspace", "status": "error", "message": "Workspace directory does not exist"}));
    }

    // Memory backend check
    match state.config.memory.effective_backend() {
        borgclaw_core::config::MemoryBackend::Sqlite => {
            if let Some(parent) = state.config.memory.database_path.parent() {
                if parent.exists() || parent == std::path::Path::new("") {
                    checks.push(serde_json::json!({"name": "memory_db", "status": "ok"}));
                } else {
                    checks.push(serde_json::json!({"name": "memory_db", "status": "error", "message": "Memory database parent directory missing"}));
                }
            }
        }
        borgclaw_core::config::MemoryBackend::Postgres => {
            if state
                .config
                .memory
                .connection_string
                .as_ref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
            {
                checks.push(serde_json::json!({"name": "memory_db", "status": "ok", "message": "Postgres connection configured"}));
            } else {
                checks.push(serde_json::json!({"name": "memory_db", "status": "error", "message": "Postgres connection_string missing"}));
            }
        }
        borgclaw_core::config::MemoryBackend::Memory => {
            checks.push(serde_json::json!({"name": "memory_db", "status": "ok", "message": "In-memory backend configured"}));
        }
    }
    if state.config.memory.external.enabled {
        let endpoint_configured = state
            .config
            .memory
            .external
            .endpoint
            .as_ref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        checks.push(serde_json::json!({
            "name": "memory_external",
            "status": if endpoint_configured { "ok" } else { "error" },
            "message": if endpoint_configured {
                "External memory adapter configured"
            } else {
                "External memory adapter enabled but endpoint missing"
            }
        }));
    } else {
        checks.push(serde_json::json!({
            "name": "memory_external",
            "status": "info",
            "message": "External memory adapter disabled"
        }));
    }

    if let Some(soul_path) = &state.config.agent.soul_path {
        checks.push(serde_json::json!({
            "name": "identity_document",
            "status": if soul_path.exists() { "ok" } else { "error" },
            "message": if soul_path.exists() {
                format!("Identity document {:?} ({:?})", soul_path, state.config.agent.identity_format)
            } else {
                format!("Identity document {:?} is missing", soul_path)
            }
        }));
    } else {
        checks.push(serde_json::json!({
            "name": "identity_document",
            "status": "info",
            "message": "No soul/identity document configured"
        }));
    }
    checks.push(serde_json::json!({
        "name": "provider_profile",
        "status": "info",
        "message": state.config.agent.provider_profile.clone().unwrap_or_else(|| "No provider profile selected".to_string())
    }));

    // Security check
    let security = SecurityLayer::new();
    let blocklist_works = matches!(
        security.check_command("rm -rf /"),
        borgclaw_core::security::CommandCheck::Blocked(_)
    );
    checks.push(serde_json::json!({
        "name": "command_blocklist",
        "status": if blocklist_works { "ok" } else { "error" }
    }));

    if state.config.security.docker.enabled {
        let docker_available = std::process::Command::new("docker")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        checks.push(serde_json::json!({
            "name": "docker_runtime",
            "status": if docker_available { "ok" } else { "error" },
            "message": if docker_available {
                format!(
                    "Docker sandbox enabled (image={}, network={:?}, mount={:?})",
                    state.config.security.docker.image,
                    state.config.security.docker.network,
                    state.config.security.docker.workspace_mount
                )
            } else {
                "Docker sandbox enabled but docker binary is unavailable".to_string()
            }
        }));
    } else {
        checks.push(serde_json::json!({
            "name": "docker_runtime",
            "status": "info",
            "message": "Docker sandbox disabled"
        }));
    }

    // Skills path check
    if state.config.skills.skills_path.exists() {
        checks.push(serde_json::json!({"name": "skills_path", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "skills_path", "status": "warning", "message": "Skills path does not exist"}));
    }
    let discovered_skills = gateway_skill_statuses(&state.config).await;
    let gated_skills = discovered_skills
        .iter()
        .filter(|skill| !skill.available)
        .count();
    checks.push(serde_json::json!({
        "name": "skills_discovery",
        "status": if gated_skills == 0 { "ok" } else { "warning" },
        "message": format!(
            "{} discovered skills, {} gated, {} effective",
            discovered_skills.len(),
            gated_skills,
            discovered_skills.iter().filter(|skill| skill.effective).count()
        )
    }));

    // Scheduler state check
    let scheduler_path = state.config.agent.workspace.join("scheduler.json");
    if scheduler_path.exists() {
        checks.push(serde_json::json!({"name": "scheduler_state", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "scheduler_state", "status": "info", "message": "No persisted scheduler state"}));
    }

    // Heartbeat state check
    let heartbeat_path = state.config.agent.workspace.join("heartbeat.json");
    if heartbeat_path.exists() {
        checks.push(serde_json::json!({"name": "heartbeat_state", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "heartbeat_state", "status": "info", "message": "No persisted heartbeat state"}));
    }
    let process_path = process_state_path(&state.config.agent.workspace);
    let processes = load_process_records(&process_path).unwrap_or_default();
    checks.push(serde_json::json!({
        "name": "process_runtime",
        "status": "info",
        "message": format!(
            "{} persisted background processes ({} running)",
            processes.len(),
            processes
                .values()
                .filter(|record| record.status == CommandProcessStatus::Running)
                .count()
        )
    }));

    let all_ok = checks
        .iter()
        .all(|c| c.get("status").and_then(|s| s.as_str()) != Some("error"));

    axum::Json(serde_json::json!({
        "status": if all_ok { "healthy" } else { "unhealthy" },
        "checks": checks,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request};
    use axum::response::Response;
    use futures_util::{SinkExt, StreamExt};
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    use hyper_util::service::TowerToHyperService;
    use tokio::io::DuplexStream;
    use tokio_tungstenite::{client_async, tungstenite::Message as WsMessage};
    use tower::util::ServiceExt;

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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(AppConfig::default()),
            router: Arc::new(MessageRouter::from_config(&AppConfig::default())),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let response = api_status(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_config_includes_identity_memory_runtime_and_skill_status() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gateway_config_surface_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("managed").join("release")).unwrap();
        std::fs::create_dir_all(root.join("workspace").join("skills").join("release")).unwrap();
        std::fs::write(
            root.join("managed").join("release").join("SKILL.md"),
            "name: managed-release\ndescription: managed\n## Instructions\nUse managed.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("workspace").join("skills").join("release").join("SKILL.md"),
            "name: workspace-release\ndescription: workspace\nconfig:\n- skills.github.token=GitHub token\n## Instructions\nUse workspace.\n",
        )
        .unwrap();
        std::fs::write(root.join("identity.md"), "# Identity").unwrap();

        let mut config = AppConfig::default();
        config.agent.provider_profile = Some("work".to_string());
        config.agent.identity_format = borgclaw_core::config::IdentityFormat::Markdown;
        config.agent.soul_path = Some(root.join("identity.md"));
        config.agent.workspace = root.join("workspace");
        config.skills.skills_path = root.join("managed");
        config.memory.external.enabled = true;
        config.memory.external.endpoint = Some("http://127.0.0.1:8081".to_string());
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;
        webhook.proxy_url = Some("http://user:secret@127.0.0.1:8080".to_string());

        let state = GatewayState {
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics: Arc::new(GatewayMetrics::new()),
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let response = api_config(State(state)).await.into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(payload["agent"]["provider_profile"], "work");
        assert_eq!(payload["agent"]["identity_format"], "markdown");
        assert_eq!(
            payload["memory"]["external"]["enabled"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(payload["runtime"]["processes"]["supported"], true);
        assert_eq!(payload["skills"]["summary"]["shadowed"], 1);
        assert_eq!(payload["skills"]["summary"]["gated"], 1);
        assert_eq!(payload["channels"]["webhook"]["proxy_configured"], true);
        assert_eq!(
            payload["channels"]["webhook"]["proxy_display"],
            "http://127.0.0.1:8080"
        );
        assert!(payload["channels"]["webhook"].get("proxy_url").is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn api_doctor_reports_identity_external_memory_and_process_runtime() {
        let root = std::env::temp_dir().join(format!(
            "borgclaw_gateway_doctor_surface_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join("workspace")).unwrap();

        let mut config = AppConfig::default();
        config.agent.workspace = root.join("workspace");
        config.agent.soul_path = Some(root.join("missing-identity.md"));
        config.memory.external.enabled = true;
        config.memory.external.endpoint = None;

        let state = GatewayState {
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics: Arc::new(GatewayMetrics::new()),
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let response = api_doctor(State(state)).await.into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let checks = payload["checks"].as_array().unwrap();

        assert!(checks
            .iter()
            .any(|check| check["name"] == "identity_document"));
        assert!(checks
            .iter()
            .any(|check| check["name"] == "memory_external"));
        assert!(checks
            .iter()
            .any(|check| check["name"] == "process_runtime"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn configured_webhook_trigger_contract_is_loaded_from_config() {
        let mut channel = borgclaw_core::config::ChannelConfig {
            enabled: true,
            ..Default::default()
        };
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
    async fn forward_configured_webhook_rejects_invalid_proxy_url() {
        let trigger =
            WebhookTrigger::new("notify", "/webhook").with_forward_url("https://example.com/hook");

        let err = forward_configured_webhook(&trigger, Some("ftp://proxy.invalid"), "hello")
            .await
            .unwrap_err();
        assert!(err.contains("unsupported proxy_url scheme"));
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let app = Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(state);
        let mut socket = connect_test_websocket(app, "/ws").await.unwrap();

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
    }

    #[tokio::test]
    async fn oauth_completion_notifies_matching_websocket_session() {
        let mut config = AppConfig::default();
        let websocket = config.channels.entry("websocket".to_string()).or_default();
        websocket.enabled = true;

        let metrics = Arc::new(GatewayMetrics::new());
        let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
        let state = GatewayState {
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::from([(
                "client-1".to_string(),
                ConnectionHandle {
                    info: ConnectionInfo {
                        client_id: "client-1".to_string(),
                        connected_at: chrono::Utc::now(),
                        session_id: Some("session-1".to_string()),
                        authenticated: true,
                        messages_received: 0,
                        messages_sent: 0,
                    },
                    outbound: outbound_tx,
                },
            )]))),
            oauth_pending: OAuthPendingStore::new(),
        };

        let oauth_state = borgclaw_core::skills::OAuthState {
            session_id: "session-1".to_string(),
            user_id: "client-1".to_string(),
            channel: "websocket".to_string(),
            created_at: chrono::Utc::now(),
            group_id: None,
        };

        notify_oauth_completion(&state, &oauth_state).await.unwrap();

        let event = outbound_rx.recv().await.unwrap();
        assert_eq!(event["type"], "oauth_complete");
        assert_eq!(event["session_id"], "session-1");
        assert_eq!(event["provider"], "google");
        assert_eq!(event["success"], true);
    }

    #[tokio::test]
    async fn websocket_upgrade_is_rejected_when_channel_is_disabled() {
        let mut config = AppConfig::default();
        let websocket = config.channels.entry("websocket".to_string()).or_default();
        websocket.enabled = false;

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let app = Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(state);
        let err = connect_test_websocket(app, "/ws").await.unwrap_err();
        match err {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            }
            other => panic!("unexpected websocket connect error: {other:?}"),
        }
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let app = Router::new()
            .route("/webhook", post(webhook_handler))
            .route("/webhook/health", get(webhook_health))
            .layer(DefaultBodyLimit::max(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES))
            .with_state(state);
        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webhook/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let health_body = response_json(health).await;
        assert_eq!(health_body["status"], "ok");
        assert_eq!(health_body["webhook_enabled"], true);

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-forwarded-for", "10.0.0.1")
                    .body(Body::from(r#"{"content":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let unauthorized_body = response_json(unauthorized).await;
        assert_eq!(unauthorized_body["error"], "Unauthorized");

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-webhook-secret", "test-secret")
                    .header("x-forwarded-for", "10.0.0.1")
                    .body(Body::from(r#"{"content":"hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::BAD_REQUEST);
        let first_body = response_json(first).await;
        assert_eq!(first_body["error"], "request rejected");

        let limited = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-webhook-secret", "test-secret")
                    .header("x-forwarded-for", "10.0.0.1")
                    .body(Body::from(r#"{"content":"hello again"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(limited.headers().get(header::RETRY_AFTER).is_some());
        let limited_body = response_json(limited).await;
        assert_eq!(limited_body["error"], "Rate limited");

        let other_requester = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-webhook-secret", "test-secret")
                    .header("x-forwarded-for", "10.0.0.2")
                    .body(Body::from(r#"{"content":"hello from elsewhere"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(other_requester.status(), StatusCode::BAD_REQUEST);
        let other_body = response_json(other_requester).await;
        assert_eq!(other_body["error"], "request rejected");
    }

    #[tokio::test]
    async fn webhook_http_flow_rejects_oversized_bodies() {
        let mut config = AppConfig::default();
        let webhook = config.channels.entry("webhook".to_string()).or_default();
        webhook.enabled = true;

        let metrics = Arc::new(GatewayMetrics::new());
        let state = GatewayState {
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
            oauth_pending: OAuthPendingStore::new(),
        };

        let app = Router::new()
            .route("/webhook", post(webhook_handler))
            .route("/webhook/health", get(webhook_health))
            .layer(DefaultBodyLimit::max(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES))
            .with_state(state);
        let oversized = "x".repeat(DEFAULT_WEBHOOK_BODY_LIMIT_BYTES + 1);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(oversized))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    async fn response_json(response: Response) -> serde_json::Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    async fn connect_test_websocket(
        app: Router,
        path: &str,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<DuplexStream>,
        tokio_tungstenite::tungstenite::Error,
    > {
        let (client_io, server_io) = tokio::io::duplex(16 * 1024);
        tokio::spawn(async move {
            http1::Builder::new()
                .serve_connection(
                    TokioIo::new(server_io),
                    TowerToHyperService::new(app.into_service()),
                )
                .with_upgrades()
                .await
                .unwrap();
        });

        client_async(
            Request::builder()
                .method("GET")
                .uri(format!("ws://localhost{path}"))
                .header(header::HOST, "localhost")
                .header(header::CONNECTION, "upgrade")
                .header(header::UPGRADE, "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(())
                .unwrap(),
            client_io,
        )
        .await
        .map(|(socket, _)| socket)
    }

    async fn next_text_message<S>(
        socket: &mut tokio_tungstenite::WebSocketStream<S>,
    ) -> serde_json::Value
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        loop {
            match socket.next().await.unwrap().unwrap() {
                WsMessage::Text(text) => return serde_json::from_str(&text).unwrap(),
                WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                other => panic!("unexpected websocket message: {other:?}"),
            }
        }
    }

    async fn next_event_of_type<S>(
        socket: &mut tokio_tungstenite::WebSocketStream<S>,
        event_type: &str,
    ) -> serde_json::Value
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        loop {
            let event = next_text_message(socket).await;
            if event["type"] == event_type {
                return event;
            }
        }
    }
}
