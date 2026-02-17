# BorgClaw 🦞

> Personal AI Agent Framework combining the best of OpenClaw-family frameworks.

BorgClaw is a secure, modular personal AI assistant built in Rust. It combines the best features from OpenClaw, ZeroClaw, NanoClaw, IronClaw, PicoClaw, and other frameworks in the ecosystem.

## Features

### Core
- **Trait-based architecture** - Swappable implementations for all components
- **Hybrid memory** - Vector + keyword search (SQLite FTS5)
- **Skills system** - SKILL.md standard parser
- **Multi-channel** - Telegram, CLI, WebSocket, Signal (planned)

### Security (Defense in Depth)
- **WASM Sandbox** - Isolated tool execution
- **Command blocklist** - Blocks dangerous system commands
- **Pairing codes** - 6-digit authentication
- **Prompt injection defense** - Pattern detection + sanitization
- **Secret leak detection** - API key protection
- **Encrypted secrets** - ChaCha20-Poly1305

### Automation
- **Cron scheduling** - Time-based jobs
- **Heartbeat system** - Proactive background tasks
- **Background sub-agents** - Parallel task execution
- **Webhook triggers** - External automation

## Architecture

```
┌─────────────────────────────────────────────────┐
│                   BorgClaw                     │
├─────────────────────────────────────────────────┤
│  Gateway (Router)  →  Agent Core  →  Tools    │
│         ↓                  ↓            ↓     │
│   Channels           Memory        WASM Sandbox │
│   (Telegram,        (Hybrid)       Security    │
│    CLI, Web)                                       │
└─────────────────────────────────────────────────┘
```

## Quick Start

For a full step-by-step setup, see `docs/quickstart.md`.

### Prerequisites
- Rust 1.75+
- SQLite (for memory)

### Build

```bash
# Clone
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw

# Build
cargo build --release

# Run
cargo run --release --bin borgclaw repl
```

### Convenience Scripts

- Bash:
  - `./scripts/bootstrap.sh`
  - `./scripts/repl.sh`
  - `./scripts/gateway.sh`
  - `./scripts/doctor.sh`
- PowerShell:
  - `./scripts/bootstrap.ps1`
  - `./scripts/repl.ps1`
  - `./scripts/gateway.ps1`
  - `./scripts/doctor.ps1`

### Configuration

Create `~/.config/borgclaw/config.toml`:

```toml
[agent]
model = "claude-sonnet-4-20250514"
provider = "anthropic"
workspace = ".borgclaw/workspace"
heartbeat_interval = 30

[security]
wasm_sandbox = true
prompt_injection_defense = true

[memory]
hybrid_search = true
```

## Usage

### CLI REPL
```bash
borgclaw repl
```

### Send a message
```bash
borgclaw send "Hello, help me write a function"
```

### WebSocket Gateway
```bash
# Start gateway on port 18789
cargo run --bin borgclaw-gateway
```

Connect via WebSocket: `ws://localhost:18789/ws`

## Modules

| Module | Description |
|--------|-------------|
| `borgclaw-core` | Core library with traits and implementations |
| `borgclaw-cli` | CLI with REPL |
| `borgclaw-gateway` | WebSocket gateway |

## Reference Implementations

BorgClaw synthesizes features from:
- **OpenClaw** - Full-featured, skills system
- **ZeroClaw** - Rust trait-based, AIEOS identity
- **NanoClaw** - Container isolation
- **IronClaw** - WASM sandbox, security
- **PicoClaw** - Ultra-lightweight
- **TinyClaw** - Multi-agent teams
- **NanoBot** - Python, message bus
- **Agent Zero** - Self-expanding tools

## License

MIT
