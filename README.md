# BorgClaw

> Personal AI Agent Framework — Secure, Modular, Extensible

BorgClaw is a Rust-based personal AI assistant combining the best features from the OpenClaw-family frameworks. It provides trait-based modularity, defense-in-depth security, and comprehensive integrations.

## Features

### Channels
- **Telegram** - Full bot support via teloxide
- **Signal** - signal-cli JSON polling with graceful degradation
- **Webhook** - HTTP triggers with rate limiting and secret verification
- **CLI** - Interactive REPL
- **WebSocket** - Real-time gateway for web clients

### Memory Systems
- **Hybrid search** - SQLite + FTS5 full-text search
- **Per-group isolation** - Separate memory contexts per conversation
- **Session auto-compaction** - Configurable context window management
- **Solution patterns** - Store and recall reusable solutions
- **Heartbeat engine** - Scheduled background tasks with cron expressions
- **Sub-agents** - Parallel background task execution

### Skills & Integrations
- **GitHub** - Repos, PRs, issues, releases with safety rules
- **Google Workspace** - Gmail, Calendar, Drive (upload, download, folders, sharing, batch operations) via OAuth2
- **MCP Protocol** - Model Context Protocol client (Stdio, SSE, WebSocket)
- **Browser Automation** - Playwright bridge + CDP fallback
- **Speech-to-Text** - OpenAI, Open WebUI, whisper.cpp
- **Text-to-Speech** - ElevenLabs streaming synthesis
- **Image Generation** - DALL-E 3, Stable Diffusion
- **QR Codes** - Generation (PNG/SVG/Terminal)
- **URL Shortening** - is.gd, tinyurl, YOURLS
- **Plugin SDK** - WASM sandboxed tools

### Security (Defense in Depth)
- **WASM Sandbox** - Isolated tool execution via wasmtime
- **SSRF Protection** - Blocks localhost, private IPs, internal addresses
- **Command blocklist** - Regex-based dangerous command blocking
- **Pairing codes** - 6-digit channel authentication
- **Prompt injection defense** - Pattern detection + sanitization
- **Secret leak detection** - API key redaction
- **Encrypted secrets** - ChaCha20-Poly1305
- **Vault integration** - Bitwarden (primary), 1Password (secondary)
- **Approval gates** - Destructive operations require confirmation

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        BorgClaw                             │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │Telegram  │  │ Signal   │  │ Webhook  │  │   CLI    │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │             │             │             │          │
│       └─────────────┴──────┬──────┴─────────────┘          │
│                            ▼                                │
│                   ┌────────────────┐                       │
│                   │  Message Router │                       │
│                   └────────┬───────┘                       │
│                            ▼                                │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    Agent Core                        │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌────────┐ │   │
│  │  │ Memory  │  │ Session │  │Solution │  │Heartbeat│ │   │
│  │  └─────────┘  └─────────┘  └─────────┘  └────────┘ │   │
│  └────────────────────────┬────────────────────────────┘   │
│                           ▼                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    Skills Layer                      │   │
│  │  GitHub │ Google │ Browser │ STT/TTS │ Image │ QR   │   │
│  └────────────────────────┬────────────────────────────┘   │
│                           ▼                                 │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                  Security Layer                      │   │
│  │  WASM Sandbox │ Secrets │ Pairing │ Injection Def   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites
- Rust 1.75+ (via [rustup](https://rustup.rs))
- Git
- SQLite (bundled with Rust crate)

### Installation

```bash
# Clone
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw

# Bootstrap (checks dependencies, builds, creates .local/)
./scripts/bootstrap.sh    # Linux/macOS
.\scripts\bootstrap.ps1   # Windows
```

### Onboarding

```bash
# Interactive setup
./scripts/onboarding.sh    # Linux/macOS
.\scripts\onboarding.ps1   # Windows

# Or directly
cargo run --bin borgclaw -- init
```

### Running

```bash
# REPL mode
./scripts/repl.sh
# Or: cargo run --bin borgclaw -- repl

# WebSocket gateway
./scripts/gateway.sh
# Or: cargo run --bin borgclaw-gateway
```

### System Check

```bash
./scripts/doctor.sh    # Verify all components
cargo run --bin borgclaw -- self-test   # Exit 0 on pass, 1 on failure
cargo run --bin borgclaw -- runtime     # Show comprehensive runtime status
```

### Managing Scheduled Tasks

```bash
cargo run --bin borgclaw -- schedules list              # List all scheduled tasks
cargo run --bin borgclaw -- schedules show <job-id>     # Show task details
cargo run --bin borgclaw -- schedules create --name "backup" --trigger "cron:0 2 * * *" --action "message:backup"
cargo run --bin borgclaw -- schedules pause <job-id>    # Pause a task
cargo run --bin borgclaw -- schedules resume <job-id>   # Resume a paused task
```

### Managing Heartbeat Tasks

```bash
cargo run --bin borgclaw -- heartbeat list              # List all heartbeat tasks
cargo run --bin borgclaw -- heartbeat show <id>         # Show task details
cargo run --bin borgclaw -- heartbeat enable <id>       # Enable a task
cargo run --bin borgclaw -- heartbeat disable <id>      # Disable a task
```

### Managing Secrets

```bash
cargo run --bin borgclaw -- secrets list                # List stored secrets
cargo run --bin borgclaw -- secrets set MY_API_KEY      # Store a secret
cargo run --bin borgclaw -- secrets check MY_API_KEY    # Verify secret exists
cargo run --bin borgclaw -- secrets delete MY_API_KEY   # Remove a secret
```

### Backup and Recovery

```bash
cargo run --bin borgclaw -- backup export ./backup.json              # Export runtime state
cargo run --bin borgclaw -- backup import ./backup.json --force      # Import state
cargo run --bin borgclaw -- backup verify ./backup.json              # Verify backup
```

## Configuration

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
session_max_entries = 100

[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"

[channels.signal]
enabled = false

[channels.webhook]
enabled = true
port = 8080
secret = "${WEBHOOK_SECRET}"
```

## Optional Components

### Playwright (Browser Automation)

```bash
./scripts/install-playwright.sh    # Linux/macOS
.\scripts\install-playwright.ps1   # Windows
```

### Whisper.cpp (Local STT)

```bash
./scripts/install-whisper.sh    # Linux/macOS
.\scripts\install-whisper.ps1   # Windows
```

### Bitwarden CLI (Vault)

```bash
# Linux/macOS
bw install

# Authenticate
bw login
export BW_SESSION=$(bw unlock --raw)
```

### 1Password CLI (Secondary Vault)

```bash
# Install from https://1password.com/downloads/command-line/
op signin
```

## Project Structure

```
borgclaw/
├── borgclaw-core/         # Core library
│   ├── src/
│   │   ├── agent/         # Agent, tools, subagents
│   │   ├── channel/       # Telegram, Signal, Webhook, CLI
│   │   ├── memory/        # Storage, session, solution, heartbeat
│   │   ├── security/      # WASM, secrets, pairing, vault
│   │   ├── skills/        # GitHub, Google, Browser, STT, TTS, etc.
│   │   └── mcp/           # MCP protocol client
│   └── Cargo.toml
├── borgclaw-cli/          # CLI binary
├── borgclaw-gateway/      # WebSocket gateway
├── scripts/               # Convenience scripts
│   ├── bootstrap.sh/ps1
│   ├── doctor.sh/ps1
│   ├── onboarding.sh/ps1
│   ├── install-playwright.sh/ps1
│   └── install-whisper.sh/ps1
├── docs/                  # Documentation
├── .local/                # Local data (gitignored)
│   ├── tools/             # Installed tools
│   ├── data/              # Runtime data
│   └── cache/             # Cache files
└── config.toml            # Example config
```

## Documentation

- [Changelog](CHANGELOG.md) - Release history and release-line policy
- [Quick Start Guide](docs/quickstart.md) - Step-by-step setup
- [Onboarding](docs/onboarding.md) - Configuration wizard
- [Channels](docs/channels.md) - Messaging integrations
- [Memory](docs/memory.md) - Storage and retrieval
- [Skills](docs/skills.md) - Tool integrations
- [Security](docs/security.md) - Defense in depth
- [Integrations](docs/integrations.md) - External services
- [Inspirations And Implementation Notes](docs/inspirations.md) - Upstream examples mapped to BorgClaw gaps
- [Implementation Status](docs/implementation-status.md) - Current contract-completion tracking

## Origin

BorgClaw synthesizes features from the OpenClaw family:
- **OpenClaw** - Full-featured, skills system
- **ZeroClaw** - Rust trait-based, AIEOS identity
- **NanoClaw** - Container isolation
- **IronClaw** - WASM sandbox, security
- **PicoClaw** - Ultra-lightweight
- **TinyClaw** - Multi-agent teams

See [Inspirations And Implementation Notes](docs/inspirations.md) for upstream implementation examples and how they map to BorgClaw's current roadmap and rough edges.

## License

MIT
