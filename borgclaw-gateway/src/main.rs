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
        ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender, WebhookChannel,
        WebhookError, WebhookTrigger,
    },
    config::load_config,
    security::SecurityLayer,
    AppConfig,
};
use futures_util::StreamExt;
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
    authenticated: bool,
    messages_received: u64,
    messages_sent: u64,
}

#[derive(Clone)]
struct GatewayState {
    config: Arc<AppConfig>,
    config_path: Arc<PathBuf>,
    router: Arc<MessageRouter>,
    webhook: Option<Arc<WebhookChannel>>,
    metrics: Arc<GatewayMetrics>,
    connections: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
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
        config_path: Arc::new(config_path),
        router,
        webhook,
        metrics,
        connections: Arc::new(RwLock::new(HashMap::new())),
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

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>BorgClaw Gateway</title>
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }
        :root {
            --bg-primary: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --border-color: #30363d;
            --text-primary: #e6edf3;
            --text-secondary: #8b949e;
            --accent-cyan: #00d4ff;
            --accent-purple: #7b2cbf;
            --accent-green: #3fb950;
            --accent-blue: #58a6ff;
            --accent-orange: #f0883e;
            --danger: #f85149;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif;
            background: var(--bg-primary);
            min-height: 100vh;
            color: var(--text-primary);
            line-height: 1.5;
        }
        
        /* Navigation */
        nav {
            background: var(--bg-secondary);
            border-bottom: 1px solid var(--border-color);
            padding: 0 24px;
            position: sticky;
            top: 0;
            z-index: 100;
        }
        .nav-content {
            max-width: 1400px;
            margin: 0 auto;
            display: flex;
            align-items: center;
            justify-content: space-between;
            height: 64px;
        }
        .logo {
            display: flex;
            align-items: center;
            gap: 12px;
            font-size: 1.25rem;
            font-weight: 600;
        }
        .logo-icon {
            font-size: 1.5rem;
        }
        .nav-links {
            display: flex;
            gap: 24px;
        }
        .nav-links a {
            color: var(--text-secondary);
            text-decoration: none;
            font-size: 0.875rem;
            font-weight: 500;
            transition: color 0.2s;
        }
        .nav-links a:hover {
            color: var(--text-primary);
        }
        
        /* Main Layout */
        main {
            max-width: 1400px;
            margin: 0 auto;
            padding: 24px;
            display: grid;
            grid-template-columns: 280px 1fr;
            gap: 24px;
        }
        
        /* Sidebar */
        aside {
            display: flex;
            flex-direction: column;
            gap: 16px;
        }
        .panel {
            background: var(--bg-secondary);
            border: 1px solid var(--border-color);
            border-radius: 12px;
            padding: 16px;
        }
        .panel h3 {
            font-size: 0.75rem;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            color: var(--text-secondary);
            margin-bottom: 12px;
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
        footer a:hover {
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
                    <a href="/api/status" class="menu-item" target="_blank">
                        <span class="menu-icon">📊</span>
                        Status
                    </a>
                    <a href="/api/metrics" class="menu-item" target="_blank">
                        <span class="menu-icon">📈</span>
                        Metrics
                    </a>
                    <a href="/api/tools" class="menu-item" target="_blank">
                        <span class="menu-icon">🛠️</span>
                        Tools
                    </a>
                    <a href="/api/connections" class="menu-item" target="_blank">
                        <span class="menu-icon">🔌</span>
                        Connections
                    </a>
                    <a href="/api/doctor" class="menu-item" target="_blank">
                        <span class="menu-icon">🔍</span>
                        Health Check
                    </a>
                </nav>
            </div>

            <div class="panel">
                <h3>Configuration</h3>
                <nav class="menu-list">
                    <a href="#" class="menu-item" onclick="showConfigEditor(); return false;">
                        <span class="menu-icon">⚙️</span>
                        Edit Config
                    </a>
                    <a href="/api/config" class="menu-item" target="_blank">
                        <span class="menu-icon">📋</span>
                        View Config (JSON)
                    </a>
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
                            <a href="/api/chat" class="endpoint-item" target="_blank">
                                <span class="method post">POST</span>
                                <code>/api/chat</code>
                            </a>
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
                            <a href="/webhook/health" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/webhook/health</code>
                            </a>
                        </div>
                    </div>

                    <div class="endpoint-card">
                        <h4>📊 Observability</h4>
                        <div class="endpoint-list">
                            <a href="/api/health" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/health</code>
                            </a>
                            <a href="/api/ready" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/ready</code>
                            </a>
                            <a href="/api/metrics" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/metrics</code>
                            </a>
                        </div>
                    </div>

                    <div class="endpoint-card">
                        <h4>🔧 Management</h4>
                        <div class="endpoint-list">
                            <a href="/api/status" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/status</code>
                            </a>
                            <a href="/api/config" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/config</code>
                            </a>
                            <a href="/api/connections" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/connections</code>
                            </a>
                            <a href="/api/tools" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/tools</code>
                            </a>
                            <a href="/api/doctor" class="endpoint-item" target="_blank">
                                <span class="method get">GET</span>
                                <code>/api/doctor</code>
                            </a>
                        </div>
                    </div>
                </div>
            </div>

            <footer>
                <p id="footer-version">BorgClaw — Personal AI Agent Framework</p>
                <p style="margin-top: 8px;">
                    <a href="https://github.com/lealvona/borgclaw">GitHub</a> • 
                    <a href="/api/status">Status</a> • 
                    <a href="/api/health">Health</a> •
                    <a href="/api/version">Version</a>
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
                    </div>
                </div>
                
                <!-- Security Tab -->
                <div id="tab-security" class="config-tab-content">
                    <div class="config-form">
                        <div class="form-group">
                            <label>Approval Mode</label>
                            <select id="cfg-approval-mode" class="form-control">
                                <option value="manual">Manual (ask for approval)</option>
                                <option value="auto">Auto (execute automatically)</option>
                                <option value="confirm">Confirm (show before execute)</option>
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
                    </div>
                </div>
                
                <!-- Memory Tab -->
                <div id="tab-memory" class="config-tab-content">
                    <div class="config-form">
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
                        <div class="status-box">
                            <h4>Skill Status</h4>
                            <div id="skill-status-list"></div>
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
    </style>

    <script>
        // State
        let messageHistory = [];
        let isProcessing = false;
        
        // Initialize
        document.addEventListener('DOMContentLoaded', () => {
            const input = document.getElementById('chat-input');
            input.addEventListener('keypress', (e) => {
                if (e.key === 'Enter' && !isProcessing) {
                    sendMessage();
                }
            });
            
            // Load initial metrics
            updateMetrics();
            // Poll metrics every 5 seconds
            setInterval(updateMetrics, 5000);
        });
        
        // Update metrics
        async function updateMetrics() {
            try {
                const res = await fetch('/api/metrics');
                if (res.ok) {
                    const data = await res.json();
                    document.getElementById('conn-active').textContent = 
                        data.connections_active || 0;
                    document.getElementById('msg-received').textContent = 
                        (data.messages_received || 0).toLocaleString();
                }
            } catch (err) {
                // Silently fail - metrics are optional
            }
        }
        
        // Send message
        async function sendMessage() {
            const input = document.getElementById('chat-input');
            const sendBtn = document.getElementById('chat-send');
            const messages = document.getElementById('chat-messages');
            const content = input.value.trim();
            
            if (!content || isProcessing) return;
            
            // Add user message
            addMessage('user', content);
            input.value = '';
            
            // Show loading
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
                
                // Remove loading
                document.getElementById(loadingId)?.remove();
                
                const data = await res.json();
                if (data.error) {
                    addMessage('system', 'Error: ' + data.error);
                } else {
                    addMessage('assistant', data.text);
                    // Update metrics after successful message
                    updateMetrics();
                }
            } catch (err) {
                document.getElementById(loadingId)?.remove();
                addMessage('system', 'Connection error: ' + err.message);
            } finally {
                isProcessing = false;
                sendBtn.disabled = false;
                input.focus();
            }
        }
        
        // Add message to chat
        function addMessage(role, text) {
            const messages = document.getElementById('chat-messages');
            const msg = document.createElement('div');
            msg.className = 'message ' + role;
            msg.textContent = text;
            messages.appendChild(msg);
            messages.scrollTop = messages.scrollHeight;
            return msg;
        }
        
        // Add loading indicator
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
            }
            
            // Security tab
            if (config.security) {
                document.getElementById('cfg-approval-mode').value = config.security.approval_mode || 'manual';
                document.getElementById('cfg-prompt-injection').checked = config.security.prompt_injection_defense !== false;
                document.getElementById('cfg-secret-leak').checked = config.security.secret_leak_detection !== false;
                document.getElementById('cfg-wasm-sandbox').checked = config.security.wasm_sandbox !== false;
                if (config.security.command_blocklist) {
                    document.getElementById('cfg-blocklist').value = config.security.command_blocklist.join(', ');
                }
            }
            
            // Memory tab
            if (config.memory) {
                document.getElementById('cfg-hybrid-search').checked = config.memory.hybrid_search === true;
                document.getElementById('cfg-session-max').value = config.memory.session_max_entries || 1000;
                document.getElementById('cfg-db-path').value = config.memory.database_path || '.borgclaw/memory.db';
            }
            
            // Skills tab
            if (config.skills) {
                document.getElementById('cfg-auto-load').checked = config.skills.auto_load !== false;
                document.getElementById('cfg-skills-path').value = config.skills.skills_path || '.borgclaw/skills';
                
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
                `;
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
                updates.security.command_blocklist = blocklistVal.split(',').map(s => s.trim()).filter(s => s);
            }
            
            // Collect memory settings
            updates.memory.hybrid_search = document.getElementById('cfg-hybrid-search').checked;
            updates.memory.session_max_entries = parseInt(document.getElementById('cfg-session-max').value) || 1000;
            
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
        
        // Close modal on backdrop click
        document.getElementById('config-modal').addEventListener('click', function(e) {
            if (e.target === this) hideConfigEditor();
        });
        
        // Keyboard shortcut for config (Ctrl/Cmd + ,)
        document.addEventListener('keydown', function(e) {
            if ((e.ctrlKey || e.metaKey) && e.key === ',') {
                e.preventDefault();
                showConfigEditor();
            }
            if (e.key === 'Escape') {
                hideConfigEditor();
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
    {
        let mut conns = state.connections.write().await;
        conns.insert(
            client_id.clone(),
            ConnectionInfo {
                client_id: client_id.clone(),
                connected_at: chrono::Utc::now(),
                authenticated: !requires_pairing,
                messages_received: 0,
                messages_sent: 0,
            },
        );
    }
    info!(
        "New WebSocket connection: {} (active: {})",
        client_id,
        state.metrics.connections_active.load(Ordering::SeqCst)
    );

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

    let memory_db_ok = state
        .config
        .memory
        .database_path
        .parent()
        .map(|p| p.exists())
        .unwrap_or(true);
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

async fn api_config(State(state): State<GatewayState>) -> impl IntoResponse {
    let sanitized = serde_json::json!({
        "agent": {
            "model": state.config.agent.model,
            "provider": state.config.agent.provider,
            "workspace": state.config.agent.workspace,
        },
        "channels": state.config.channels,
        "memory": {
            "database_path": state.config.memory.database_path,
            "hybrid_search": state.config.memory.hybrid_search,
            "session_max_entries": state.config.memory.session_max_entries,
        },
        "security": {
            "approval_mode": state.config.security.approval_mode,
            "pairing_enabled": state.config.security.pairing.enabled,
            "pairing": state.config.security.pairing,
            "prompt_injection_defense": state.config.security.prompt_injection_defense,
            "secret_leak_detection": state.config.security.secret_leak_detection,
            "wasm_sandbox": state.config.security.wasm_sandbox,
            "command_blocklist": state.config.security.command_blocklist,
        },
        "skills": {
            "github_configured": !state.config.skills.github.token.is_empty(),
            "google_configured": !state.config.skills.google.client_id.is_empty(),
            "browser_configured": !state.config.skills.browser.node_path.as_os_str().is_empty(),
            "skills_path": state.config.skills.skills_path,
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
    hybrid_search: Option<bool>,
    session_max_entries: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
struct SecurityConfigUpdate {
    approval_mode: Option<String>,
    pairing_enabled: Option<bool>,
    prompt_injection_defense: Option<bool>,
    secret_leak_detection: Option<bool>,
    wasm_sandbox: Option<bool>,
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
        if let Some(hybrid_search) = memory.hybrid_search {
            updated_config.memory.hybrid_search = hybrid_search;
            changes_made.push(format!("memory.hybrid_search = {}", hybrid_search));
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
                "autonomous" | "Autonomous" | _ => borgclaw_core::config::ApprovalMode::Autonomous,
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
    let connections: Vec<&ConnectionInfo> = conns.values().collect();

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

async fn api_doctor(State(state): State<GatewayState>) -> impl IntoResponse {
    let mut checks = Vec::new();

    // Workspace check
    if state.config.agent.workspace.exists() {
        checks.push(serde_json::json!({"name": "workspace", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "workspace", "status": "error", "message": "Workspace directory does not exist"}));
    }

    // Memory database check
    if let Some(parent) = state.config.memory.database_path.parent() {
        if parent.exists() || parent == std::path::Path::new("") {
            checks.push(serde_json::json!({"name": "memory_db", "status": "ok"}));
        } else {
            checks.push(serde_json::json!({"name": "memory_db", "status": "error", "message": "Memory database parent directory missing"}));
        }
    }

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

    // Skills path check
    if state.config.skills.skills_path.exists() {
        checks.push(serde_json::json!({"name": "skills_path", "status": "ok"}));
    } else {
        checks.push(serde_json::json!({"name": "skills_path", "status": "warning", "message": "Skills path does not exist"}));
    }

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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(AppConfig::default()),
            router: Arc::new(MessageRouter::from_config(&AppConfig::default())),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: None,
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
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
            config_path: Arc::new(PathBuf::from(".")),
            config: Arc::new(config.clone()),
            router: Arc::new(MessageRouter::from_config(&config)),
            webhook: configured_webhook_channel(&config).await.map(Arc::new),
            metrics,
            connections: Arc::new(RwLock::new(HashMap::new())),
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
