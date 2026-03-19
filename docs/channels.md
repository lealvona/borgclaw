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
const ws = new WebSocket('ws://localhost:18789/ws');

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
| `auth` | Client→Server | Authenticate with pairing code |
| `message` | Client→Server | Send message |
| `response` | Server→Client | Agent response |
| `error` | Server→Client | Error message |
| `heartbeat` | Server→Client | Connection keepalive |

## Channel Configuration

### Per-Channel Settings

```toml
[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"
allow_from = ["user123", "user456"]  # Whitelist
dm_policy = "pairing"

[channels.signal]
enabled = false
phone_number = "+1234567890"

[channels.webhook]
enabled = true
port = 8080
secret = "${WEBHOOK_SECRET}"
rate_limit_per_minute = 60

[channels.websocket]
enabled = true
port = 18789
require_pairing = true
```

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

Implement the `Channel` trait:

```rust
use borgclaw_core::channel::{Channel, ChannelSender, InboundMessage, OutboundMessage};

struct MyChannel;

#[async_trait]
impl Channel for MyChannel {
    async fn start(&mut self, sender: ChannelSender) -> Result<(), ChannelError> {
        // Listen for messages, send via sender
    }
    
    async fn send(&self, msg: OutboundMessage) -> Result<(), ChannelError> {
        // Send message to channel
    }
    
    async fn stop(&mut self) -> Result<(), ChannelError> {
        // Cleanup
    }
}
```
