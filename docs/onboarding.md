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
3. **Enter API key** - Masked input, stored securely in the encrypted secret store
4. **Create a provider profile** - The selected provider, model, and secret become a named provider profile
5. **Select model** - List fetched from provider API
6. **Configure channels** - Enable Telegram, Signal, Webhook
7. **Set security options** - WASM sandbox, optional Docker command sandbox, injection defense
8. **Generate .env** - Environment variables for secrets

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
- `memory` - sqlite, postgres, vector (`vector` is the legacy component chapter for the in-memory mode)
- `provider` - openai, anthropic, google, ollama

## Provider Registry

On first run, a provider registry is created at `providers.toml`:

```toml
[openai]
name = "OpenAI"
api_base = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
default_model = "gpt-4o"
rate_limit_rpm = 60  # Requests per minute (uses sensible defaults if not set)

[anthropic]
name = "Anthropic"
api_base = "https://api.anthropic.com/v1"
env_key = "ANTHROPIC_API_KEY"
default_model = "claude-sonnet-4-20250514"
rate_limit_rpm = 50

[google]
name = "Google AI"
api_base = "https://generativelanguage.googleapis.com/v1"
env_key = "GOOGLE_API_KEY"
default_model = "gemini-1.5-pro"
rate_limit_rpm = 15  # Conservative for free tier

[ollama]
name = "Ollama (Local)"
api_base = "http://localhost:11434/api"
env_key = ""
default_model = "llama3"
rate_limit_rpm = 120  # Local provider allows higher limits
```

## Rate Limiting

BorgClaw enforces per-provider rate limits to prevent 429 "Too Many Requests" errors. Each provider has sensible defaults:

| Provider | Default RPM | Notes |
|----------|-------------|-------|
| OpenAI | 60 | |
| Anthropic | 50 | |
| Google AI | 15 | Conservative for free tier |
| Kimi | 30 | |
| MiniMax | 30 | |
| Z.ai | 30 | |
| Ollama | 120 | Local provider |

**Retry semantics**: When a provider returns 429 Too Many Requests, BorgClaw automatically waits and retries with exponential backoff (up to 3 retries). The `Retry-After` header is respected when provided.

Override in your `config.toml`:

```toml
[agent]
provider = "google"
model = "gemini-2.0-flash"
rate_limit_rpm = 30  # Override provider default
```

## Environment Variables

Onboarding generates `.env` with:

```bash
BORGCLAW_PROVIDER=anthropic
BORGCLAW_MODEL=claude-sonnet-4-20250514
ANTHROPIC_API_KEY=sk-ant-...
```

Provider credentials do not need to live in `config.toml`. BorgClaw stores provider profiles in the encrypted secret store and keeps only the selected profile id in config.

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
provider_profile = "anthropic-default"
identity_format = "markdown"
workspace = ".borgclaw/workspace"
heartbeat_interval = 30
rate_limit_rpm = 50  # Optional override (uses provider defaults if unset)

[security]
wasm_sandbox = true
prompt_injection_defense = true

[security.docker]
enabled = false
image = "borgclaw-sandbox:base"
network = "none"
workspace_mount = "ro"
timeout_seconds = 120

[memory]
backend = "sqlite"
hybrid_search = true
database_path = ".borgclaw/memory.db"
session_max_entries = 100

[heartbeat]
enabled = true
check_interval_seconds = 60

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

## Provider Profiles

Named provider profiles separate provider definitions from user credentials. The selected profile is stored in `agent.provider_profile`, and the runtime resolves credentials from that profile before falling back to legacy env-based lookup.

Representative config:

```toml
[agent]
provider = "openai"
provider_profile = "openai-work"
model = "gpt-4o"
```

CLI management:

```bash
borgclaw providers list
borgclaw providers show openai-work
borgclaw providers add openai-work openai --model gpt-4o
borgclaw providers select openai-work
borgclaw providers delete openai-work
```

Provider profiles are stored in the encrypted secret store, not as plaintext API keys in `config.toml`.

## Identity Documents

BorgClaw supports two identity-document formats for `agent.soul_path`:

- `markdown` for the existing raw prompt-file contract
- `aieos` for structured JSON identity documents

Representative AIEOS config:

```toml
[agent]
soul_path = "IDENTITY.json"
identity_format = "aieos"
```

Representative AIEOS file:

```json
{
  "name": "BorgClaw Unit",
  "role": "Precision operator",
  "summary": "You coordinate complex work.",
  "instructions": ["Stay concise", "Use tools only when needed"],
  "guardrails": ["Do not leak secrets"]
}
```

If `identity_format = "auto"`, BorgClaw treats `.json` identity files as AIEOS and other files as markdown.

The onboarding summary now surfaces:

- selected `provider_profile`
- `identity_format` and `soul_path`
- memory backend plus external-adapter/privacy state
- process state path inside the workspace
- managed and workspace skill paths

## Memory Backends

Onboarding can configure three runtime memory modes:

- `SQLite + FTS5 (default)` for local persistent storage
- `PostgreSQL + pgvector` for server-backed persistence using `memory.connection_string`
- `In-memory only` for non-persistent local or test runs

Representative PostgreSQL config:

```toml
[memory]
backend = "postgres"
connection_string = "postgres://user:pass@localhost/borgclaw"
embedding_endpoint = "http://127.0.0.1:11434/api/embeddings"
hybrid_search = true
session_max_entries = 100
```

If you leave the embedding endpoint blank during onboarding, PostgreSQL runs in text-only mode and `hybrid_search` is disabled for that backend.

Onboarding also exposes the optional external memory adapter and the memory privacy policy:

- `memory.external.enabled`
- `memory.external.endpoint`
- `memory.external.mirror_writes`
- `memory.privacy.enabled`
- `memory.privacy.default_sensitivity`
- `memory.privacy.subagent_scope`
- `memory.privacy.scheduler_scope`
- `memory.privacy.heartbeat_scope`

Helper scripts:
- `./scripts/install-pgvector.sh` or `.\scripts\install-pgvector.ps1` provisions a local pgvector-ready PostgreSQL runtime
- `./scripts/install-ollama.sh` or `.\scripts\install-ollama.ps1` installs Ollama and pulls a recommended embeddings model
- `./scripts/install-docker-sandbox.sh` or `.\scripts\install-docker-sandbox.ps1` builds the default Docker command-sandbox image
