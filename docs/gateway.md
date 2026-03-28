# Gateway Web Interface

The BorgClaw Gateway provides a real-time web dashboard and HTTP API for monitoring, configuration, and interaction with your AI agent.

## Overview

The gateway (`borgclaw-gateway`) runs an HTTP server with:

- **🎮 Web Dashboard** — Visual interface at `http://localhost:3000`
- **💬 Live Chat** — Send messages to your agent directly from the browser
- **⚙️ Configuration Editor** — Point-and-click configuration management
- **📊 Metrics & Monitoring** — Real-time connection and message statistics
- **🔌 WebSocket Endpoint** — Real-time bidirectional communication
- **🛠️ HTTP API** — REST endpoints for integration

## Quick Start

```bash
# Start the gateway
cargo run --bin borgclaw-gateway

# Or use the script
./scripts/gateway.sh

# Open the dashboard
open http://localhost:3000        # macOS
xdg-open http://localhost:3000    # Linux
start http://localhost:3000       # Windows
```

## Web Dashboard

### Live Chat

The chat interface allows you to send messages to your agent directly from the browser:

1. Type your message in the input field
2. Press Enter or click Send
3. View the agent's response in the chat history

The chat uses the same message routing as other channels, so all your agent's capabilities are available.

### Real-time Metrics

The dashboard displays live metrics:

| Metric | Description |
|--------|-------------|
| **Active Connections** | Current WebSocket clients connected |
| **Messages Received** | Total messages processed |
| **Uptime** | Gateway runtime duration |

Metrics update every 5 seconds automatically.

### Configuration Editor

The visual configuration editor lets you modify settings without editing `config.toml` directly.

**Opening the Editor:**
- Click "⚙️ Edit Config" in the sidebar
- Or press `Ctrl + ,` (Cmd + , on macOS)

**Keyboard Shortcuts:**
| Shortcut | Action |
|----------|--------|
| `Ctrl + ,` | Open configuration editor |
| `Esc` | Close configuration editor |

#### Configuration Tabs

**Agent Tab**
- **Provider** — Select LLM provider (OpenAI, Anthropic, Kimi, MiniMax, etc.)
- **Model** — Enter model name (e.g., `gpt-4o`, `claude-sonnet-4-20250514`, `MiniMax-M2.7`)
- **System Prompt** — Optional system instructions for the agent

**Channels Tab**
- **WebSocket Enabled** — Toggle WebSocket channel on/off
- **WebSocket Port** — Port for WebSocket connections (default: 3000)
- **Require Pairing** — Enable 6-digit pairing code authentication
- **Webhook Enabled** — Toggle Webhook channel on/off
- **Webhook Port** — Port for webhook server (default: 8080)

**Security Tab**
- **Approval Mode** — Control execution behavior:
  - *ReadOnly* — No tool execution, view-only
  - *Supervised* — Destructive operations require approval
  - *Autonomous* — Full automation
- **Prompt Injection Defense** — Detect and block injection attempts
- **Secret Leak Detection** — Scan outputs for exposed secrets
- **WASM Sandbox** — Isolate plugin execution
- **Command Blocklist** — Comma-separated list of blocked commands (e.g., `rm, del, format`)

**Memory Tab**
- **Hybrid Search Enabled** — Use SQLite + FTS5 for semantic search
- **Session Max Entries** — Maximum messages per session before compaction

**Skills Tab**
- **Auto-load Skills** — Automatically load skills on startup
- **Skill Status** — View configuration status of GitHub, Google, and Browser skills

#### Saving Changes

1. Modify settings in any tab
2. Click "💾 Save Changes"
3. View the status message:
   - Success: Lists all changes made
   - Note: Some changes require gateway restart to take effect

Changes are saved to your `config.toml` file immediately.

## API Endpoints

### Configuration

**GET /api/config**
Returns current configuration as JSON:

```bash
curl http://localhost:3000/api/config
```

Response includes all configuration sections: agent, channels, memory, security, skills, mcp.

**POST /api/config**
Update configuration programmatically:

```bash
curl -X POST http://localhost:3000/api/config \
  -H "Content-Type: application/json" \
  -d '{
    "agent": {
      "model": "gpt-4o",
      "provider": "openai"
    },
    "security": {
      "approval_mode": "supervised"
    }
  }'
```

Response:
```json
{
  "success": true,
  "message": "Configuration updated successfully...",
  "changes": ["agent.model = gpt-4o", "security.approval_mode = supervised"],
  "requires_restart": true
}
```

### Status & Health

**GET /api/status**
System status and version information.

**GET /api/health**
Health check endpoint (returns 200 if healthy).

**GET /api/ready**
Readiness probe for orchestration.

**GET /api/doctor**
Run comprehensive health checks:
- Workspace directory
- Memory database
- Security layer
- Skills path
- Scheduler state
- Heartbeat state

### Metrics

**GET /api/metrics**
Real-time metrics as JSON:

```json
{
  "connections_total": 42,
  "connections_active": 3,
  "messages_received": 156,
  "messages_sent": 149,
  "pairing_requests": 5,
  "auth_success": 3,
  "auth_failure": 2,
  "uptime_seconds": 3600
}
```

### Connections

**GET /api/connections**
List active WebSocket connections:

```json
{
  "connections": [
    {
      "client_id": "uuid-here",
      "connected_at": "2026-03-26T12:00:00Z",
      "authenticated": true,
      "messages_received": 10,
      "messages_sent": 8
    }
  ],
  "count": 1
}
```

### Tools

**GET /api/tools**
List available agent tools with their descriptions and schemas.

### Chat

**POST /api/chat**
Send a message to the agent via HTTP:

```bash
curl -X POST http://localhost:3000/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello, agent!",
    "sender_id": "web-user",
    "group_id": "web-chat"
  }'
```

### WebSocket

**WS /ws**
Real-time WebSocket endpoint for bidirectional communication.

Connection flow:
1. Connect to `ws://localhost:3000/ws`
2. Receive `welcome` event with `client_id` and `auth_required`
3. If auth is required, send `{"type": "request_pairing"}`
4. Receive `pairing_code` event with 6-digit code
5. Authenticate with `{"type": "auth", "pairing_code": "123456"}`
6. Send messages with `{"type": "message", "content": "hello"}`
7. Receive agent responses as `message` events

Message types:
- `welcome` — Sent on connection
- `pairing_code` — 6-digit authentication code
- `auth_required` — Prompt to authenticate
- `authenticated` — Successful authentication
- `message` — Chat message
- `heartbeat` / `pong` — Keepalive
- `error` — Error notification

### Webhooks

**POST /webhook**
Receive webhook events (requires `X-Webhook-Secret` header if configured).

**GET /webhook/health**
Webhook channel health check.

**POST /webhook/trigger/{id}**
Trigger named webhook endpoints.

## Configuration

### Port Configuration

Gateway ports are configured in `config.toml`:

```toml
[channels.websocket]
enabled = true
port = 3000

[channels.websocket.extra]
require_pairing = true

[channels.webhook]
enabled = true
port = 8080
secret = "${WEBHOOK_SECRET}"
```

### CORS

The gateway enables CORS for all origins by default, allowing browser-based clients to connect from any domain.

## Security

### Authentication

WebSocket connections support optional pairing-based authentication:

1. Enable in config: `channels.websocket.extra.require_pairing = true`
2. Connect via WebSocket
3. Request pairing code via `{"type": "request_pairing"}`
4. Display the 6-digit code to the user
5. User enters code to authenticate

### Rate Limiting

Webhook endpoints support rate limiting per IP:

```toml
[channels.webhook.extra]
rate_limit_per_minute = 60
```

### Secret Headers

Webhooks can require secret headers for authentication:

```toml
[channels.webhook]
secret = "${WEBHOOK_SECRET}"
```

Requests must include: `X-Webhook-Secret: your-secret`

## Troubleshooting

### Dashboard Not Loading

1. Verify gateway is running: `curl http://localhost:3000/api/health`
2. Check port is not in use: `lsof -i :3000`
3. Check firewall settings

### Configuration Not Saving

1. Verify config file path is writable
2. Check file permissions on `config.toml`
3. Review gateway logs for serialization errors

### WebSocket Connection Fails

1. Check WebSocket is enabled in config
2. Verify client supports WebSocket protocol
3. Check browser console for CORS errors
4. Review `api/connections` to see active connections

### Metrics Not Updating

1. Check JavaScript console for errors
2. Verify `/api/metrics` endpoint returns data
3. Refresh the page to re-establish polling

## See Also

- [Channels](channels.md) — Detailed channel configuration
- [Security](security.md) — Security settings and best practices
- [Configuration](onboarding.md) — Full configuration reference
