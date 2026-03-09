//! BorgClaw Gateway - WebSocket gateway for remote connections

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use borgclaw_core::{
    agent::{Agent, AgentContext, AgentResponse, SenderInfo, SessionId, builtin_tools},
    channel::{ChannelType, InboundMessage, MessagePayload, Sender},
    AppState, SimpleAgent,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct GatewayState {
    app_state: Arc<AppState>,
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
    
    // Initialize app state
    let config = borgclaw_core::config::AppConfig::default();
    let app_state = Arc::new(AppState::new(config));
    
    // Initialize agent
    let config = app_state.config.read().await.clone();
    let mut agent = SimpleAgent::new(
        config.agent.clone(),
        Some(config.memory.clone()),
        Some(config.security.clone()),
    );
    for tool in borgclaw_core::agent::builtin_tools() {
        agent.register_tool(tool);
    }
    *app_state.agent.write().await = Some(Box::new(agent));
    
    let state = GatewayState { app_state };
    
    // CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    
    // Build router
    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(websocket_handler))
        .route("/api/status", get(api_status))
        .route("/api/chat", get(api_chat_get))
        .layer(cors)
        .with_state(state);
    
    let addr = SocketAddr::from(([0, 0, 0, 0], 18789));
    info!("Gateway listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
    let (mut sender, mut receiver) = socket.split();
    
    // Create session ID for this connection
    let session_id = SessionId::new();
    
    info!("New WebSocket connection: {}", session_id.0);
    
    // Send welcome message
    let _ = sender.send(Message::Text(
        serde_json::json!({
            "type": "welcome",
            "session_id": session_id.0,
            "message": "Connected to BorgClaw"
        })
        .to_string(),
    )).await;
    
    // Handle incoming messages
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                // Process message
                if let Err(e) = handle_ws_message(&mut sender, &state, &session_id, &text).await {
                    error!("Error handling message: {}", e);
                }
            }
            Ok(Message::Close(_)) => {
                info!("WebSocket closed");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    info!("Connection closed: {}", session_id.0);
}

async fn handle_ws_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &GatewayState,
    session_id: &SessionId,
    text: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Parse incoming message
    let request: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| "Invalid JSON")?;
    
    let msg_type = request.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");
    
    match msg_type {
        "message" => {
            let content = request.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            // Create agent context
            let ctx = AgentContext {
                session_id: session_id.clone(),
                message: content.to_string(),
                sender: SenderInfo {
                    id: "websocket".to_string(),
                    name: Some("Web User".to_string()),
                    channel: "websocket".to_string(),
                },
                metadata: std::collections::HashMap::new(),
            };
            
            // Process with agent
            if let Some(ref mut agent) = *state.app_state.agent.write().await {
                let response = agent.process(&ctx).await;
                
                // Send response
                sender.send(Message::Text(
                    serde_json::json!({
                        "type": "response",
                        "text": response.text,
                    })
                    .to_string(),
                )).await?;
            }
        }
        "ping" => {
            sender.send(Message::Text(
                serde_json::json!({ "type": "pong" }).to_string()
            )).await?;
        }
        _ => {
            sender.send(Message::Text(
                serde_json::json!({
                    "type": "error",
                    "message": "Unknown message type"
                })
                .to_string()
            )).await?;
        }
    }
    
    Ok(())
}

async fn api_status(State(state): State<GatewayState>) -> impl IntoResponse {
    let config = state.app_state.config.read().await;
    
    let body = serde_json::json!({
        "status": "running",
        "model": config.agent.model,
        "provider": config.agent.provider,
    });
    
    (StatusCode::OK, serde_json::to_string(&body).unwrap_or_default())
}

async fn api_chat_get() -> impl IntoResponse {
    (StatusCode::METHOD_NOT_ALLOWED, "Use POST for chat")
}
