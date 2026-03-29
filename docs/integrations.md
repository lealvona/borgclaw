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

```yaml
name: example-skill
version: 1.0.0
description: Example skill for BorgClaw
author: developer
tags: [utility, example]

entry_points:
  - name: process
    description: Process input data
    input_schema:
      type: object
      properties:
        input:
          type: string
          description: Input to process
      required: [input]

permissions:
  file_read: []
  network: ["api.example.com"]
```

### Installing Skills

Current runtime support:
- Local skill directory installs with `SKILL.md`
- GitHub `owner/repo` installs that fetch the repository-root `SKILL.md` from `main`
- Direct remote `SKILL.md` URL installs
- Registry-backed skill listing for GitHub-hosted registries such as ClawHub

Current limitations within that support:
- Remote installs currently persist the downloaded `SKILL.md` manifest only; companion assets and packaged archives are not fetched yet
- Registry listing currently supports GitHub-hosted registries only
- Remote URL installs must point directly to `SKILL.md`

Planned but not yet implemented:
- Remote archive installs by URL

```bash
# From local path
borgclaw skills install ./my-skill

# From ClawHub-style GitHub repo path
borgclaw skills install openclaw/weather

# From direct SKILL.md URL
borgclaw skills install https://example.com/skills/weather/SKILL.md
```

### Publishing Skills

Packaging and publishing are implemented in the current CLI.

Current limitation:

- Remote archive installs by URL are still pending; remote installs currently use local directories, GitHub `owner/repo`, or direct `SKILL.md` URLs.

```bash
# Package
borgclaw skills package ./my-skill

# Publish to ClawHub
borgclaw skills publish ./my-skill.tar.gz
```

## Custom Integrations

### Creating a Skill

1. Create skill directory:
```
my-skill/
├── skill.yaml      # Manifest
├── main.wasm       # Compiled WASM
└── README.md       # Documentation
```

2. Write manifest:
```yaml
name: my-skill
version: 1.0.0
description: My custom skill
author: me

entry_points:
  - name: execute
    description: Execute the skill
    input_schema:
      type: object
      properties:
        query:
          type: string

permissions:
  network: ["api.myservice.com"]
```

3. Implement in Rust:
```rust
use borgclaw_core::skills::*;

#[derive(Deserialize)]
struct Input {
    query: String,
}

#[skill_entrypoint]
fn execute(input: Input) -> Result<String, SkillError> {
    // Implementation
    Ok(format!("Processed: {}", input.query))
}
```

4. Compile to WASM:
```bash
cargo build --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/my_skill.wasm main.wasm
```

### Creating a Channel

Implement the `Channel` trait:

```rust
use borgclaw_core::channel::*;

pub struct MyChannel {
    config: MyConfig,
    sender: Option<ChannelSender>,
}

#[async_trait]
impl Channel for MyChannel {
    async fn start(&mut self, sender: ChannelSender) -> Result<(), ChannelError> {
        self.sender = Some(sender);
        // Start listening for messages
        Ok(())
    }
    
    async fn send(&self, msg: OutboundMessage) -> Result<(), ChannelError> {
        // Send message to channel
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), ChannelError> {
        self.sender = None;
        Ok(())
    }
}
```

### Creating a Tool

Implement the `Tool` trait:

```rust
use borgclaw_core::agent::*;

pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }
    
    fn description(&self) -> &str {
        "Does something useful"
    }
    
    fn input_schema(&self) -> ToolSchema {
        ToolSchema {
            properties: vec![
                ("input".to_string(), PropertyDef {
                    property_type: "string".to_string(),
                    description: Some("Input to process".to_string()),
                    required: true,
                }),
            ],
        }
    }
    
    async fn execute(&self, input: serde_json::Value) -> Result<ToolResult, ToolError> {
        let input: String = serde_json::from_value(input)?;
        Ok(ToolResult::success(format!("Result: {}", input)))
    }
}
```

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
database_path = ".local/data/memory.db"
```

### PostgreSQL

```toml
[memory]
backend = "postgres"
connection_string = "postgres://user:pass@localhost/borgclaw"
```

## Monitoring

### Health Checks

```bash
# Webhook health
curl http://localhost:8080/webhook/health

# Gateway health
curl http://localhost:18789/health
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
- Install skills from local directories, GitHub `owner/repo`, direct `SKILL.md` URLs, and packaged archives already present on disk

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

# From local package
borgclaw skills install ./my-skill-1.0.0.tar.gz

# From local directory (development)
borgclaw skills install ./my-skill
```

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
