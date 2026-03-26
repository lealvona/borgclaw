<div align="center">

```
    ╔═══════════════════════════════════════════════════════════╗
    ║                                                           ║
    ║     ██████  ██████   ██████   ██████ ██      ██   ██      ║
    ║     ██   ██ ██   ██ ██       ██      ██      ██   ██      ║
    ║     ██████  ██████  ██   ███ ██   ███ ██      ███████      ║
    ║     ██   ██ ██   ██ ██    ██ ██    ██ ██           ██      ║
    ║     ██████  ██   ██  ██████   ██████  ███████      ██      ║
    ║                                                           ║
    ║                                                           ║
    ╚═══════════════════════════════════════════════════════════╝
```

**The Hypercube Agent Stack**

[![Version](https://img.shields.io/badge/version-1.12.0-00d4aa?style=for-the-badge)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange?style=for-the-badge&logo=rust)](https://rustup.rs)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)

</div>

---

## 🧊 What is BorgClaw?

> *A personal AI agent framework that stacks the best of every world into one cohesive unit.*

BorgClaw is a **multi-valent**, **hyperdimensional** AI agent system written in Rust. Like a perfect cube—efficient, organized, every side serving a purpose—BorgClaw stacks complementary technologies into a unified whole. Each face connects to a different dimension of capability, yet all work in lockstep from a single core.

**No gaps. No bloat. Just the right tech, on the right face, working as one.**

---

## 🎯 The Six Faces

<div align="center">

| Face | Dimension | Capability |
|:----:|:----------|:-----------|
| ⬆️ **Top** | Channels | Telegram, Signal, Webhooks, CLI, WebSocket |
| ⬇️ **Bottom** | Memory | Hybrid SQLite+FTS5, contexts, patterns |
| ⬅️ **Left** | Skills | GitHub, Google, Browser, STT/TTS, Images |
| ➡️ **Right** | Security | WASM sandbox, SSRF, vault, injection defense |
| 🔲 **Front** | Providers | OpenAI, Anthropic, Google, Kimi, MiniMax, Z.ai, Ollama |
| 🔳 **Back** | Runtime | Scheduler, heartbeat, sub-agents, recovery |

</div>

---

## 🚀 Quick Start

```bash
# Clone the stack
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw

# Bootstrap (builds, checks deps, creates .local/)
./scripts/bootstrap.sh    # Linux/macOS
.\scripts\bootstrap.ps1   # Windows

# Initialize your configuration
cargo run --bin borgclaw -- init

# Start the REPL
./scripts/repl.sh
```

---

## 🏗️ Architecture

```
                    ╭──────────────────────╮
                   ╱   ⬆️ CHANNELS FACE     ╲
                  ╱  Telegram • Signal • WS  ╲
                 ╱────────────────────────────╲
                ╱                              ╲
      ⬅️ SKILLS ╱                                ╲ ➡️ SECURITY
       FACE    ╱                                  ╲   FACE
               │      ┌─────────────────┐         │
               │      │                 │         │
               │      │   AGENT CORE    │         │
               │      │   🧠 The Cube   │         │
               │      │                 │         │
               │      └─────────────────┘         │
                ╲                                  ╱
                 ╲   🔲 PROVIDERS FACE             ╱
                  ╲  OpenAI • Anthropic • Ollama  ╱
                   ╲─────────────────────────────╱
                    ╲   ⬇️ MEMORY FACE           ╱
                     ╲  SQLite • Sessions •     ╱
                      ╲ Solutions • Heartbeat  ╱
                       ╰──────────────────────╯
```

---

## ✨ Capabilities by Face

### ⬆️ Top Face — Channels

*Where BorgClaw interfaces with the world*

- **Telegram** — Full bot support via teloxide
- **Signal** — signal-cli JSON polling with graceful degradation  
- **Webhook** — HTTP triggers with rate limiting & secret verification
- **CLI** — Interactive REPL for direct control
- **WebSocket** — Real-time gateway for web clients

### ⬇️ Bottom Face — Memory

*What BorgClaw knows and remembers*

- **Hybrid Search** — SQLite + FTS5 full-text search
- **Per-Group Isolation** — Separate contexts per conversation
- **Session Auto-Compaction** — Configurable context window management
- **Solution Patterns** — Store & recall reusable solutions
- **Heartbeat Engine** — Scheduled background tasks with cron
- **Sub-Agents** — Parallel background task execution

### ⬅️ Left Face — Skills

*What BorgClaw can do*

| Skill | Capability |
|-------|------------|
| **GitHub** | Repos, PRs, issues, releases with safety rules |
| **Google Workspace** | Gmail, Calendar, Drive (OAuth2) |
| **MCP Protocol** | Model Context Protocol client (Stdio, SSE, WebSocket) |
| **Browser** | Playwright bridge + CDP fallback |
| **Speech** | STT: OpenAI, Open WebUI, whisper.cpp |
| **Voice** | TTS: ElevenLabs streaming synthesis |
| **Images** | DALL-E 3, Stable Diffusion |
| **QR Codes** | Generation (PNG/SVG/Terminal) |
| **URLs** | Shortening via is.gd, tinyurl, YOURLS |
| **Plugins** | WASM sandboxed tools |

### ➡️ Right Face — Security

*How BorgClaw protects*

- **WASM Sandbox** — Isolated tool execution via wasmtime
- **SSRF Protection** — Blocks localhost, private IPs, internal addresses
- **Command Blocklist** — Regex-based dangerous command blocking
- **Pairing Codes** — 6-digit channel authentication
- **Prompt Injection Defense** — Pattern detection + sanitization
- **Secret Leak Detection** — API key redaction
- **Encrypted Secrets** — ChaCha20-Poly1305
- **Vault Integration** — Bitwarden (primary), 1Password (secondary)
- **Approval Gates** — Destructive operations require confirmation

### 🔲 Front Face — LLM Providers

*How BorgClaw thinks*

| Provider | Status | Key |
|----------|--------|-----|
| OpenAI | ✅ | `OPENAI_API_KEY` |
| Anthropic | ✅ | `ANTHROPIC_API_KEY` |
| Google | ✅ | `GOOGLE_API_KEY` |
| **Kimi** | 🆕 | `KIMI_API_KEY` |
| **MiniMax** | 🆕 | `MINIMAX_API_KEY` |
| **Z.ai** | 🆕 | `Z_API_KEY` |
| Ollama | ✅ | Local |

### 🔳 Back Face — Runtime

*How BorgClaw operates*

- **Scheduler** — Cron-based job execution with recovery
- **Dead-Letter Queue** — Failed job management
- **Catch-Up Policy** — Missed job handling
- **Pause/Resume** — Operator control over scheduled tasks
- **Heartbeat** — Background task persistence
- **Backup/Restore** — Full state export/import

---

## 🎮 Operating the Cube

### System Health
```bash
./scripts/doctor.sh                        # Verify all faces
cargo run --bin borgclaw -- self-test      # Exit 0 on pass
cargo run --bin borgclaw -- runtime        # Show runtime status
```

### Scheduled Tasks
```bash
cargo run --bin borgclaw -- schedules list
cargo run --bin borgclaw -- schedules create --name "backup" --trigger "cron:0 2 * * *" --action "message:backup"
cargo run --bin borgclaw -- schedules pause <job-id>
cargo run --bin borgclaw -- schedules resume <job-id>
```

### Heartbeat Tasks
```bash
cargo run --bin borgclaw -- heartbeat list
cargo run --bin borgclaw -- heartbeat show <id>
cargo run --bin borgclaw -- heartbeat enable <id>
cargo run --bin borgclaw -- heartbeat disable <id>
```

### Secrets Management
```bash
cargo run --bin borgclaw -- secrets list
cargo run --bin borgclaw -- secrets set MY_API_KEY
cargo run --bin borgclaw -- secrets check MY_API_KEY
cargo run --bin borgclaw -- secrets delete MY_API_KEY
```

### Backup & Recovery
```bash
cargo run --bin borgclaw -- backup export ./backup.json
cargo run --bin borgclaw -- backup import ./backup.json --force
cargo run --bin borgclaw -- backup verify ./backup.json
```

---

## ⚙️ Configuration

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

---

## 🔧 Optional Components

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
bw install
bw login
export BW_SESSION=$(bw unlock --raw)
```

### 1Password CLI (Secondary Vault)
```bash
# Install from https://1password.com/downloads/command-line/
op signin
```

---

## 📁 Project Structure

```
borgclaw/
├── borgclaw-core/         # Core library (the center)
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
├── docs/                  # Documentation
├── .local/                # Local data (gitignored)
└── config.toml            # Example config
```

---

## 📚 Documentation

- [Changelog](CHANGELOG.md) — Release history
- [Quick Start](docs/quickstart.md) — Step-by-step setup
- [Onboarding](docs/onboarding.md) — Configuration wizard
- [Channels](docs/channels.md) — Messaging integrations
- [Memory](docs/memory.md) — Storage and retrieval
- [Skills](docs/skills.md) — Tool integrations
- [Security](docs/security.md) — Defense in depth
- [Integrations](docs/integrations.md) — External services
- [Inspirations](docs/inspirations.md) — Upstream examples

---

## 🧬 Origin

BorgClaw stacks the best of the OpenClaw family into one cohesive unit:

| Project | Contribution |
|---------|--------------|
| **OpenClaw** | Full-featured skills system |
| **ZeroClaw** | Rust trait-based architecture, AIEOS identity |
| **NanoClaw** | Container isolation patterns |
| **IronClaw** | WASM sandbox, security-first design |
| **PicoClaw** | Ultra-lightweight philosophy |
| **TinyClaw** | Multi-agent team coordination |

See [Inspirations](docs/inspirations.md) for how upstream examples map to BorgClaw's design.

---

<div align="center">

**🧊 Six Faces. One Stack. Zero Compromise.**

[⭐ Star us on GitHub](https://github.com/lealvona/borgclaw) • [📖 Read the Docs](docs/) • [🐛 Report Issues](../../issues)

MIT License © 2026

</div>
