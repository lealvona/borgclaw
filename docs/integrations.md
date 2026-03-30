# Integrations

BorgClaw integrates with external services via skills and the MCP protocol.

## MCP Protocol

### Overview

Model Context Protocol (MCP) is a standardized way for AI models to interact with tools and resources.

### Transports

| Transport | Use Case |
|-----------|----------|
| Stdio | Local tools, CLI |
| SSE | HTTP streaming |
| WebSocket | Real-time, bidirectional |

### Configuration

```toml
[mcp]
servers = ["filesystem", "github", "postgres"]

[mcp.servers.filesystem]
transport = "stdio"
command = "mcp-filesystem"
args = ["--root", "/home/user"]

[mcp.servers.github]
transport = "stdio"
command = "mcp-github"
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

[mcp.servers.postgres]
transport = "stdio"
command = "mcp-postgres"
args = ["postgres://user:pass@localhost/db"]
```

### Client API

```rust
let client = McpClient::connect(transport).await?;

// List tools
let tools = client.list_tools().await?;

// Call tool
let result = client.call_tool("read_file", json!({
    "path": "/home/user/document.txt"
})).await?;

// List resources
let resources = client.list_resources().await?;

// Read resource
let content = client.read_resource("file:///home/user/doc.txt").await?;
```

## GitHub

See [Skills - GitHub](skills.md#github-integration)

## Google Workspace

See [Skills - Google](skills.md#google-workspace)

## Browser Automation

See [Skills - Browser](skills.md#browser-automation)

## Vault Integration

See [Security - Vault](security.md#vault-integration)

## Speech Services

See [Skills - STT](skills.md#speech-to-text) and [Skills - TTS](skills.md#text-to-speech)

## Image Generation

See [Skills - Image](skills.md#image-generation)

## ClawHub Skill Registry

### Overview

ClawHub is the official skill registry at `https://github.com/openclaw/clawhub`

### Skill Manifest

```markdown
---
name: example-skill
version: 1.0.0
description: Example skill for BorgClaw
author: developer
---

## Instructions

Process input data.
```

### Installing Skills

Current runtime support:
- Local skill directory installs with `SKILL.md`
- Local packaged `.tar.gz` installs
- GitHub `owner/repo` installs via archive-backed extraction from `main`
- Direct GitHub raw `SKILL.md` URL installs with archive-backed companion file extraction
- Direct remote `.tar.gz` archive URL installs
- Direct remote `SKILL.md` URL installs
- Registry-backed skill listing for GitHub-hosted registries such as ClawHub

Current limitations within that support:
- Registry listing currently supports GitHub-hosted registries only
- Remote URL installs must point directly to `SKILL.md`
- Arbitrary non-GitHub direct `SKILL.md` URLs can fetch companion files when the manifest declares relative `files:` entries, provides an adjacent `SKILL.files.json`, or the source exposes a browsable directory listing

```bash
# From local path
borgclaw skills install ./my-skill

# From local package
borgclaw skills install ./my-skill-1.0.0.tar.gz

# From ClawHub-style GitHub repo path
borgclaw skills install openclaw/weather

# From direct SKILL.md URL
borgclaw skills install https://example.com/skills/weather/SKILL.md

# From remote package URL
borgclaw skills install https://example.com/skills/weather-1.0.0.tar.gz
```

### Publishing Skills

Packaging and publishing are implemented in the current CLI.

Current limitation:

- Arbitrary non-GitHub direct `SKILL.md` URLs fetch companion assets from manifest `files:`, an adjacent `SKILL.files.json`, or manifest-directory discovery when listings are available; archive-backed installs remain available for GitHub-backed sources and `.tar.gz` URLs.

```bash
# Package
borgclaw skills package ./my-skill

# Publish to ClawHub
borgclaw skills publish ./my-skill.tar.gz
```

## Custom Integrations

### Creating a Skill

Skill installs and packaging use `SKILL.md` as the manifest source of truth. A minimal local skill directory looks like:

```text
my-skill/
├── SKILL.md
├── README.md
└── assets/        # Optional companion files
```

Example manifest:

```markdown
---
name: my-skill
version: 1.0.0
description: My custom skill
author: me
---

## Instructions

Execute the skill against the user's request.
```

### Creating a Channel

Implement the current `Channel` trait:

```rust
use async_trait::async_trait;
use borgclaw_core::channel::{
    Channel, ChannelConfig, ChannelError, ChannelStatus, ChannelType, InboundMessage,
    OutboundMessage,
};
use tokio::sync::mpsc;

pub struct MyChannel;

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

### Creating a Tool

BorgClaw currently exposes tool definitions through built-in runtime registration and plugin-integrated execution paths rather than a public third-party `Tool` trait.

For runtime integrations:
- use MCP servers when the capability already exists out-of-process
- use WASM plugins when the capability fits the plugin sandbox
- extend the core runtime only when you are intentionally adding a first-party built-in tool

## Webhook Integrations

### Slack

```bash
# Create Slack app, get webhook URL
curl -X POST https://hooks.slack.com/services/T00/B00/XXX \
  -H "Content-Type: application/json" \
  -d '{"text": "BorgClaw notification"}'
```

### Discord

```bash
# Create webhook in Discord channel settings
curl -X POST https://discord.com/api/webhooks/XXX/YYY \
  -H "Content-Type: application/json" \
  -d '{"content": "BorgClaw notification"}'
```

### Custom Webhooks

Configure BorgClaw webhook triggers:

```toml
[channels.webhook.triggers.notify_slack]
url = "https://hooks.slack.com/services/T00/B00/XXX"
method = "POST"
headers = { "Content-Type" = "application/json" }
body_template = '{"text": "{{message}}"}'
```

## API Integrations

### OpenAI

```toml
[providers.openai]
api_key = "${OPENAI_API_KEY}"
base_url = "https://api.openai.com/v1"
```

### Anthropic

```toml
[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"
base_url = "https://api.anthropic.com/v1"
```

### Google AI

```toml
[providers.google]
api_key = "${GOOGLE_API_KEY}"
base_url = "https://generativelanguage.googleapis.com/v1"
```

### Ollama (Local)

```toml
[providers.ollama]
base_url = "http://localhost:11434/api"
```

## Database Integrations

### SQLite (Default)

```toml
[memory]
backend = "sqlite"
database_path = ".local/data/memory.db"
hybrid_search = true
```

### PostgreSQL

```toml
[memory]
backend = "postgres"
connection_string = "postgres://user:pass@localhost/borgclaw"
hybrid_search = true
```

### In-Memory

```toml
[memory]
backend = "memory"
hybrid_search = false
```

Notes:
- `sqlite` remains the default when `backend` is omitted.
- `postgres` requires `memory.connection_string`.
- `memory` is non-persistent and intended for ephemeral local runs or tests.

## Monitoring

### Health Checks

```bash
# Webhook health
curl http://localhost:8080/webhook/health

# Gateway health
curl http://localhost:3000/api/health
```

### Metrics

```toml
[monitoring]
enabled = true
metrics_port = 9090
```

### Logging

```toml
[logging]
level = "info"  # "debug", "info", "warn", "error"
format = "json"  # "json", "pretty"
file = ".local/logs/borgclaw.log"
```

## Skill Registry and Publishing

### Overview

BorgClaw provides a skill packaging and publishing system that allows you to:
- Package skills into distributable archives
- Publish skills to public or private registries
- Install skills from local directories, local `.tar.gz` archives, GitHub `owner/repo`, direct GitHub raw `SKILL.md` URLs, remote `.tar.gz` URLs, and direct `SKILL.md` URLs

### Packaging

Skills are packaged as `.tar.gz` archives containing:
- `SKILL.md` - The skill manifest (required)
- `borgclaw-package.json` - Auto-generated metadata
- Any additional files (source code, documentation, etc.)

```bash
# Package a skill directory
borgclaw skills package ./my-skill

# Package with specific output
borgclaw skills package ./my-skill --output my-skill-1.0.0.tar.gz
```

### Publishing

Skills can be published to a registry for others to install:

```bash
# Publish to default registry
borgclaw skills publish ./my-skill-1.0.0.tar.gz

# Publish to specific registry
borgclaw skills publish ./my-skill-1.0.0.tar.gz --registry https://registry.example.com

# Force publish without confirmation
borgclaw skills publish ./my-skill-1.0.0.tar.gz --force
```

### Registry Configuration

Configure your default registry in `config.toml`:

```toml
[skills]
registry_url = "https://github.com/openclaw/clawhub"
```

### Registry API

When implementing a custom registry, it must support:

**Upload Endpoint:**
```
POST /api/v1/skills/upload
Content-Type: multipart/form-data

Fields:
- name: Skill name
- package: The .tar.gz file

Response:
{
  "id": "skill-id",
  "url": "https://registry.example.com/skills/skill-name"
}
```

**List Endpoint:**
```
GET /api/v1/skills

Response:
[
  {
    "id": "skill-id",
    "name": "Skill Name",
    "version": "1.0.0",
    "description": "Skill description"
  }
]
```

### GitHub Registry

For GitHub-based registries (like `clawhub`), the registry is a repository with the structure:

```
clawhub/
├── skill-name/
│   └── SKILL.md
└── another-skill/
    └── SKILL.md
```

Skills are accessed via raw GitHub URLs:
```
https://raw.githubusercontent.com/owner/repo/main/skill-name/SKILL.md
```

### Skill Installation Sources

Skills can be installed from multiple sources:

```bash
# From registry (configured in config.toml)
borgclaw skills install skill-name

# From GitHub (owner/repo format)
borgclaw skills install openclaw/weather

# From direct URL
borgclaw skills install https://example.com/skills/weather/SKILL.md

# From local directory (development)
borgclaw skills install ./my-skill
```

Current installation limitation:
- arbitrary non-GitHub direct `SKILL.md` URLs fetch companion assets from manifest `files:`, adjacent `SKILL.files.json`, or manifest-directory discovery when listings are available

### Registry Implementation Example

Here's a minimal registry server implementation in Rust:

```rust
use axum::{
    extract::Multipart,
    response::Json,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
struct UploadResponse {
    id: String,
    url: String,
}

async fn upload_skill(mut multipart: Multipart) -> Json<UploadResponse> {
    let mut skill_name = String::new();
    let mut package_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        if name == "name" {
            skill_name = field.text().await.unwrap();
        } else if name == "package" {
            package_data = Some(field.bytes().await.unwrap().to_vec());
        }
    }

    // Save package to storage
    if let Some(data) = package_data {
        let path = PathBuf::from(format!("./skills/{}.tar.gz", skill_name));
        tokio::fs::write(&path, data).await.unwrap();

        Json(UploadResponse {
            id: skill_name.clone(),
            url: format!("https://registry.example.com/skills/{}", skill_name),
        })
    } else {
        panic!("No package data");
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api/v1/skills/upload", post(upload_skill));

    axum::serve(tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap(), app)
        .await
        .unwrap();
}
```
