# Onboarding Guide

BorgClaw onboarding is designed to be **repeatable**, **interactive**, and **safe**.

## Run

```bash
# Interactive wizard
cargo run --bin borgclaw -- init

# Or via scripts
./scripts/onboarding.sh    # Linux/macOS
.\scripts\onboarding.ps1   # Windows
```

## First Run

On first run, onboarding will:

1. **Detect state** - Check if config exists
2. **Prompt for provider** - OpenAI, Anthropic, Google, Ollama
3. **Enter API key** - Masked input, stored securely
4. **Select model** - List fetched from provider API
5. **Configure channels** - Enable Telegram, Signal, Webhook
6. **Set security options** - WASM sandbox, injection defense
7. **Generate .env** - Environment variables for secrets

## Subsequent Runs

When config exists, onboarding offers:

```
Your BorgClaw is already configured!

Options:
  [r] Reconfigure  - Run setup again
  [s] Status       - Show current config
  [q] Quit         - Exit without changes
```

## Component Registrar

Add, update, or delete specific components:

```bash
# Add a channel
cargo run --bin borgclaw -- init --component channel --chapter telegram --action add

# Update sandbox config
cargo run --bin borgclaw -- init --component sandbox --chapter wasm --action update

# Delete a channel
cargo run --bin borgclaw -- init --component channel --chapter signal --action delete
```

Available components:
- `channel` - telegram, signal, webhook, websocket
- `sandbox` - wasm, docker
- `memory` - sqlite, vector
- `provider` - openai, anthropic, google, ollama

## Provider Registry

On first run, a provider registry is created at `providers.toml`:

```toml
[openai]
name = "OpenAI"
api_base = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
default_model = "gpt-4o"

[anthropic]
name = "Anthropic"
api_base = "https://api.anthropic.com/v1"
env_key = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"

[google]
name = "Google AI"
api_base = "https://generativelanguage.googleapis.com/v1"
env_key = "GOOGLE_API_KEY"
default_model = "gemini-1.5-pro"

[ollama]
name = "Ollama (Local)"
api_base = "http://localhost:11434/api"
env_key = ""
default_model = "llama3"
```

## Environment Variables

Onboarding generates `.env` with:

```bash
BORGCLAW_PROVIDER=anthropic
BORGCLAW_MODEL=claude-sonnet-4-20250514
ANTHROPIC_API_KEY=sk-ant-...
```

## Config Location

| Platform | Path |
|----------|------|
| Linux | `~/.config/borgclaw/config.toml` |
| macOS | `~/.config/borgclaw/config.toml` |
| Windows | `%APPDATA%\borgclaw\config.toml` |

## Example Config

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

## Refresh Models

Force model list refresh from provider APIs:

```bash
cargo run --bin borgclaw -- init --refresh-models
```

## List Providers

View configured providers:

```bash
cargo run --bin borgclaw -- init --list-providers
```
