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
    channel::{ChannelType, InboundMessage, MessagePayload, MessageRouter, Sender},
    AppConfig,
};
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Clone)]
struct GatewayState {
    config: Arc<AppConfig>,
    router: Arc<MessageRouter>,
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
    let config = Arc::new(AppConfig::default());
    let router = Arc::new(MessageRouter::from_config(&config));
    let state = GatewayState { config, router };
    
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
    let mut socket = socket;
    let client_id = uuid::Uuid::new_v4().to_string();
    let requires_pairing = state
        .config
        .channels
        .get("websocket")
        .map(|channel| matches!(channel.dm_policy, borgclaw_core::config::DmPolicy::Pairing))
        .unwrap_or(true);

    info!("New WebSocket connection: {}", client_id);
    
    let _ = send_event(&mut socket, serde_json::json!({
            "type": "welcome",
            "client_id": client_id,
            "auth_required": requires_pairing,
            "message": "Connected to BorgClaw"
        }))
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
    let request: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| "Invalid JSON")?;
    
    let msg_type = request.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("message");
    
    match msg_type {
        "request_pairing" => {
            let code = state.router.request_pairing_code(client_id).await?;
            send_event(socket, serde_json::json!({
                    "type": "pairing_code",
                    "client_id": client_id,
                    "pairing_code": code
                }))
            .await?;
        }
        "auth" => {
            let pairing_code = request.get("pairing_code")
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
            let content = request.get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let group_id = request.get("group_id")
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
                    send_event(socket, serde_json::json!({
                        "type": "response",
                        "session_id": outcome.session_id.0,
                        "text": outcome.response.text,
                        "tool_calls": outcome.response.tool_calls,
                        "metadata": outcome.response.metadata,
                    }))
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

async fn send_event(
    socket: &mut WebSocket,
    event: serde_json::Value,
) -> Result<(), axum::Error> {
    socket.send(Message::Text(event.to_string())).await
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
    
    (StatusCode::OK, serde_json::to_string(&body).unwrap_or_default())
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
}
