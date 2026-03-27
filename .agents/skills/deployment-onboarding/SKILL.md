---
name: deployment-onboarding
description: Guide users through setup and deployment. Use when a user needs help getting started or deploying the project.
---

# Deployment and Onboarding Skill

> Directive for deploying the project and guiding new users through setup.

## Quick Start for New Users

### Prerequisites

| Requirement | Version | Install |
|-------------|---------|---------|
| Rust | 1.75+ | [rustup.rs](https://rustup.rs) |
| Cargo | Latest | Comes with Rust |
| Git | Any | [git-scm.com](https://git-scm.com) |

### Optional Tools

| Tool | Purpose | Install |
|------|---------|---------|
| Node.js | Browser automation (Playwright) | [nodejs.org](https://nodejs.org) |
| Docker | Sandboxed execution | [docker.com](https://docker.com) |

### Step-by-Step Deployment

#### 1. Clone Repository

```bash
git clone https://github.com/<org>/<project>.git
cd <project>
```

#### 2. Run Bootstrap

```bash
# Linux/macOS
./scripts/bootstrap.sh

# Windows
.\scripts\bootstrap.ps1
```

Bootstrap will:
- Check Rust/Cargo installation
- Build workspace in release mode
- Create `.local/` directory structure
- Install optional tools

#### 3. Get Required API Keys

**Choose a Provider:**

| Provider | Key Location | Sign Up |
|----------|--------------|---------|
| Anthropic | console.anthropic.com | Required for Claude |
| OpenAI | platform.openai.com | For GPT models |
| Google | aistudio.google.com | For Gemini |

**Channel Setup (Optional):**

| Channel | Key Type | How to Get |
|---------|----------|------------|
| Telegram | Bot Token | Message @BotFather on Telegram |
| Signal | CLI + number | signal-cli documentation |

#### 4. Run Onboarding

```bash
# Interactive setup
./scripts/onboarding.sh

# Quick mode (skip optional integrations)
./scripts/onboarding.sh --quick

# See all options
./scripts/onboarding.sh --help
```

Onboarding creates:
- `~/.config/<project>/config.toml` - Main configuration
- `.env` - Environment variables with API keys

### Starting the Project

```bash
# Start REPL (interactive terminal)
./scripts/repl.sh

# Start Gateway (headless, for API access)
./scripts/gateway.sh

# Check system health
./scripts/doctor.sh
```

## Configuration Reference

### config.toml Structure

```toml
[agent]
provider = "anthropic"        # or "openai", "google", "ollama"
model = "claude-sonnet-4-20250514"
workspace = ".project/workspace"

[security]
wasm_sandbox = true           # Enable WASM sandboxing
docker_sandbox = true         # Enable Docker isolation
command_blocklist = true      # Block dangerous commands
secrets_encryption = true     # Encrypt stored secrets

[memory]
database_path = ".project/memory"
hybrid_search = true

[channels.telegram]
enabled = true
bot_token = "${TELEGRAM_BOT_TOKEN}"

[channels.webhook]
enabled = true
port = 8080
```

### Environment Variables

Store secrets in `.env` (never commit this file):

```bash
# Required
ANTHROPIC_API_KEY=sk-ant-...

# Channels
TELEGRAM_BOT_TOKEN=123456:ABC-...
WEBHOOK_SECRET=your-secret-here

# Skills
GITHUB_TOKEN=ghp_...
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
```

## Troubleshooting

### Common Issues

| Problem | Solution |
|---------|----------|
| "API key not found" | Run onboarding or check `.env` exists |
| "Tool not found" | Run `./scripts/install-<tool>.sh` |
| "Permission denied" | `chmod +x scripts/*.sh` |

### Doctor Script

Always check system health first:

```bash
./scripts/doctor.sh
```

### Verbose Debugging

```bash
# Run with debug logging
RUST_LOG=debug ./scripts/repl.sh

# Check config loading
cargo run --bin <project> -- init --help
```

## For Operators

### Backup and Restore

```bash
# Export state
cargo run --bin <project> -- backup export /path/to/backup.json

# Import state
cargo run --bin <project> -- backup import /path/to/backup.json --force

# Verify backup without importing
cargo run --bin <project> -- backup verify /path/to/backup.json
```

### Scheduler Management

```bash
# List scheduled jobs
cargo run --bin <project> -- schedules list

# Create job
cargo run --bin <project> -- schedules create \
  --name "daily-report" \
  --trigger "cron:0 9 * * *"

# Delete job
cargo run --bin <project> -- schedules delete <job-id>
```

### Heartbeat Tasks

```bash
# List heartbeat tasks
cargo run --bin <project> -- heartbeat list

# Enable/Disable task
cargo run --bin <project> -- heartbeat enable <task-id>
cargo run --bin <project> -- heartbeat disable <task-id>
```
