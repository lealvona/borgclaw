# Channels

BorgClaw supports multiple messaging channels for communication.

## Supported Channels

| Channel | Status | Transport |
|---------|--------|-----------|
| CLI | ✅ Stable | stdin/stdout |
| Telegram | ✅ Stable | Bot API (teloxide) |
| Signal | ✅ Stable | signal-cli JSON |
| Webhook | ✅ Stable | HTTP POST |
| WebSocket | ✅ Stable | ws:// |

## CLI Channel

The default REPL interface:

```bash
cargo run --bin borgclaw -- repl
```

Commands:
- `help` - Show available commands
- `exit` - Quit REPL
- `clear` - Clear screen

## Telegram Channel

### Setup

1. Create bot via [@BotFather](https://t.me/botfather)
2. Get bot token
3. Configure:

```toml
[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"
proxy_url = "${TELEGRAM_PROXY_URL}"  # optional: http/https/socks5/socks5h
```

4. Set environment variable:
```bash
export TELEGRAM_BOT_TOKEN="123456:ABC-DEF..."
```

### Features

- Message handling with context
- Group chat support with isolation
- Pairing code authentication
- Command parsing (/help, /status, etc.)

### DM Policy

```toml
[channels.telegram]
dm_policy = "pairing"  # "open", "pairing", "blocked"
```

- `open` - Accept all DMs
- `pairing` - Require pairing code
- `blocked` - Ignore DMs

## Signal Channel

### Prerequisites

Install signal-cli:

```bash
# Linux/macOS
brew install signal-cli  # macOS
# Or download from https://github.com/AsamK/signal-cli

# Windows
scoop install signal-cli
```

### Setup

1. Register phone number:
```bash
signal-cli -u +1234567890 register
```

2. Verify:
```bash
signal-cli -u +1234567890 verify 123456
```

3. Configure:

```toml
[channels.signal]
enabled = true
phone_number = "+1234567890"
```

### Features

- JSON-based message polling
- Group support
- Graceful degradation if signal-cli unavailable

## Webhook Channel

### Setup

```toml
[channels.webhook]
enabled = true
port = 8080
secret = "${WEBHOOK_SECRET}"
proxy_url = "${WEBHOOK_PROXY_URL}"   # optional, used for outbound trigger forwarding
rate_limit_per_minute = 60
```

### Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/webhook` | POST | Receive messages |
| `/webhook/health` | GET | Health check |
| `/webhook/trigger/{id}` | POST | Named trigger |

### Message Format

```json
POST /webhook
Content-Type: application/json
X-Webhook-Secret: your-secret

{
  "content": "Hello, BorgClaw!",
  "sender": "user123",
  "group_id": "optional-group"
}
```

### Rate Limiting

Default: 60 requests/minute per IP

Rate-limited webhook responses return HTTP `429` with a `Retry-After` header.

Request bodies larger than 1 MiB are rejected before routing.

### Triggers

Named triggers for automation:

```bash
# Trigger specific action
curl -X POST http://localhost:8080/webhook/trigger/backup \
  -H "X-Webhook-Secret: secret"
```

## WebSocket Gateway

### Start Gateway

```bash
cargo run --bin borgclaw-gateway
```

### Connect

```javascript
const ws = new WebSocket('ws://localhost:3000/ws');

ws.onopen = () => {
  ws.send(JSON.stringify({
    type: 'auth',
    pairing_code: '123456'
  }));
};

ws.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  console.log(msg);
};

// Send message
ws.send(JSON.stringify({
  type: 'message',
  content: 'Hello!'
}));
```

### Message Types

| Type | Direction | Description |
|------|-----------|-------------|
| `request_pairing` | Client→Server | Request a pairing code |
| `auth` | Client→Server | Authenticate with pairing code |
| `message` | Client→Server | Send message |
| `welcome` | Server→Client | Initial connection state |
| `auth_required` | Server→Client | Authentication required before chat |
| `pairing_code` | Server→Client | Generated pairing code |
| `authenticated` | Server→Client | Authentication success |
| `response` | Server→Client | Agent response |
| `error` | Server→Client | Error message |
| `heartbeat` | Server→Client | Connection keepalive |
| `pong` | Server→Client | Heartbeat acknowledgement |

## Channel Configuration

### Per-Channel Settings

```toml
[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"
proxy_url = "${TELEGRAM_PROXY_URL}"
allow_from = ["user123", "user456"]  # Whitelist
dm_policy = "pairing"

[channels.signal]
enabled = false
phone_number = "+1234567890"

[channels.webhook]
enabled = true
port = 8080
secret = "${WEBHOOK_SECRET}"
proxy_url = "${WEBHOOK_PROXY_URL}"
rate_limit_per_minute = 60

[channels.websocket]
enabled = true
port = 3000
require_pairing = true
```

### Per-Channel Proxies

Channels with outbound HTTP traffic can use an explicit `proxy_url`:

- `channels.telegram.proxy_url` applies to Telegram Bot API traffic
- `channels.webhook.proxy_url` applies to outbound webhook trigger forwarding

Supported schemes:

- `http`
- `https`
- `socks5`
- `socks5h`

Proxy values may be set directly or via environment placeholders such as `${TELEGRAM_PROXY_URL}`.

## Message Flow

```
Channel → MessageRouter → Agent → Response → Channel
   │           │            │          │         │
   │           │            │          │         └─ Send reply
   │           │            │          └─ Generate response
   │           │            └─ Process with tools/memory
   │           └─ Route to agent
   └─ Receive message
```

## Adding Custom Channels

Implement the current `Channel` trait:

```rust
use async_trait::async_trait;
use borgclaw_core::channel::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    OutboundMessage,
};
use tokio::sync::mpsc;

struct MyChannel;

#[async_trait]
impl Channel for MyChannel {
    fn channel_type(&self) -> ChannelType {
        ChannelType::new("my-channel")
    }

    async fn init(&mut self, _config: &ChannelConfig) -> Result<(), ChannelError> {
        Ok(())
    }

    async fn start_receiving(
        &self,
        sender: mpsc::Sender<InboundMessage>,
    ) -> Result<(), ChannelError> {
        let _ = sender;
        // Listen for messages and forward them to the router.
        Ok(())
    }

    async fn send(&self, msg: OutboundMessage) -> Result<(), ChannelError> {
        let _ = msg;
        Ok(())
    }

    async fn status(&self) -> ChannelStatus {
        ChannelStatus::connected()
    }

    async fn shutdown(&self) -> Result<(), ChannelError> {
        Ok(())
    }
}
```
