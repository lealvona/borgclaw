# Skills

BorgClaw skills are modular capabilities that extend agent functionality.

## Available Skills

| Skill | Description | Backend |
|-------|-------------|---------|
| GitHub | Repository operations | REST API |
| Google | Gmail, Drive, Calendar | OAuth2 + REST |
| Browser | Web automation | Playwright / CDP |
| STT | Speech-to-text | OpenAI / whisper.cpp |
| TTS | Text-to-speech | ElevenLabs |
| Image | Image generation | DALL-E / Stable Diffusion |
| QR | QR code generation | qrcode crate |
| URL | URL shortening | is.gd / tinyurl / YOURLS |

## GitHub Integration

### Safety Rules

```rust
pub struct GitHubSafety {
    pub repo_access: RepoAccess,
    pub require_confirmation: bool,
}

pub enum RepoAccess {
    OwnedOnly,      // Only user-owned repos
    Whitelist(Vec<String>),
    All,
}
```

### Configuration

```toml
[skills.github]
token = "${GITHUB_TOKEN}"
user_agent = "BorgClaw/1.0"
base_url = "https://api.github.com"

[skills.github.safety]
repo_access = "owned_only"
require_confirmation = true
```

### API

```rust
let github = GitHubClient::new(config, safety);

// List repos
let repos = github.list_repos().await?;

// Create PR
let pr = github.create_pr(owner, repo, CreatePrRequest {
    title: "Fix bug".to_string(),
    head: "fix/bug".to_string(),
    base: "main".to_string(),
    body: Some("Description...".to_string()),
}).await?;

// Destructive operations require confirmation
let confirmation = github.prepare_delete_branch(owner, repo, "old-branch").await?;
// User must confirm within 60 seconds
let result = github.confirm_destructive_op(&confirmation.token).await?;
```

### Double-Confirm

Destructive operations (delete, force push) require explicit confirmation:

1. Call preparation method → returns confirmation token
2. User confirms (via UI or separate call)
3. Call confirm method with token within 60 seconds

## Google Workspace

### OAuth2 Setup

1. Create project at [Google Cloud Console](https://console.cloud.google.com)
2. Enable Gmail, Drive, Calendar APIs
3. Create OAuth2 credentials
4. Configure:

```toml
[skills.google]
client_id = "${GOOGLE_CLIENT_ID}"
client_secret = "${GOOGLE_CLIENT_SECRET}"
redirect_uri = "http://localhost:8080/callback"
token_path = ".local/data/google_token.json"
```

### Gmail

```rust
let google = GoogleClient::new(config);

// List messages
let messages = google.list_messages(Some("is:unread"), 10).await?;

// Send email
google.send_email(
    "recipient@example.com",
    "Subject",
    "Body content"
).await?;
```

### Drive

```rust
// Upload file
let file = google.upload_file(
    "document.txt",
    b"content".to_vec(),
    "text/plain",
    Some("folder_id")
).await?;

// Search files
let files = google.search_files("name contains 'report'").await?;
```

### Calendar

```rust
// List events
let events = google.list_events(
    "primary",
    Utc::now(),
    Utc::now() + chrono::Duration::days(7)
).await?;

// Create event
google.create_event(CalendarEvent {
    summary: "Meeting".to_string(),
    start: Utc::now() + chrono::Duration::hours(1),
    end: Utc::now() + chrono::Duration::hours(2),
    ..Default::default()
}).await?;
```

## Browser Automation

### Playwright Bridge

```bash
# Install
./scripts/install-playwright.sh
```

### Usage

```rust
let browser = PlaywrightClient::new(PlaywrightConfig {
    browser: BrowserType::Chromium,
    headless: true,
    bridge_path: ".local/tools/playwright/playwright-bridge.js".into(),
});

// Navigate
browser.navigate("https://example.com").await?;

// Screenshot
let png = browser.screenshot(false).await?;

// Click
browser.click("#submit-button").await?;

// Fill form
browser.fill("#username", "user@example.com").await?;

// Extract text
let text = browser.get_text("body").await?;
```

### CDP Fallback

When Playwright unavailable, falls back to Chrome DevTools Protocol:

```rust
let cdp = CdpClient::new("http://localhost:9222");
```

## Speech-to-Text

### Backends

| Backend | Quality | Speed | Cost |
|---------|---------|-------|------|
| OpenAI | High | Fast | $$ |
| Open WebUI | High | Medium | $ |
| whisper.cpp | High | Slow | Free |

### Configuration

```toml
[skills.stt]
backend = "openai"  # "openai", "openwebui", "whispercpp"

[skills.stt.openai]
api_key = "${OPENAI_API_KEY}"
model = "whisper-1"

[skills.stt.openwebui]
base_url = "http://localhost:3000"
api_key = "${OPENWEBUI_API_KEY}"

[skills.stt.whispercpp]
binary_path = ".local/tools/whisper.cpp/build/bin/whisper-cli"
model_path = ".local/tools/whisper.cpp/models/ggml-base.en.bin"
```

### Usage

```rust
let stt = SttClient::new(backend, config);

// Transcribe
let text = stt.transcribe(&audio_bytes, AudioFormat::Wav).await?;
```

## Text-to-Speech

### Configuration

```toml
[skills.tts]
provider = "elevenlabs"
api_key = "${ELEVENLABS_API_KEY}"
voice_id = "21m00Tcm4TlvDq8ikWAM"
model_id = "eleven_monolingual_v1"
```

### Usage

```rust
let tts = ElevenLabsClient::new(config);

// Synthesize
let audio = tts.speak("Hello, world!").await?;

// Stream
let stream = tts.speak_stream("Long text to stream...").await?;
while let Some(chunk) = stream.next().await {
    // Play chunk
}
```

## Image Generation

### Configuration

```toml
[skills.image]
provider = "dalle"  # "dalle" or "stable_diffusion"

[skills.image.dalle]
api_key = "${OPENAI_API_KEY}"
model = "dall-e-3"
size = "1024x1024"

[skills.image.stable_diffusion]
base_url = "http://localhost:7860"
```

### Usage

```rust
let image = ImageClient::new(provider, config);

// Generate
let result = image.generate("A sunset over mountains").await?;

// Get image data
let png_bytes = result.image;
let revised_prompt = result.revised_prompt;
```

## QR Codes

### Usage

```rust
// Generate QR
let png = QrCodeSkill::encode("https://example.com", QrFormat::Png)?;
let svg = QrCodeSkill::encode("https://example.com", QrFormat::Svg)?;
let terminal = QrCodeSkill::encode("https://example.com", QrFormat::Terminal)?;

// Encode URL
let qr = QrCodeSkill::encode_url("https://example.com/path?query=1", QrFormat::Png)?;
```

## URL Shortening

### Providers

| Provider | API | Self-hosted |
|----------|-----|-------------|
| is.gd | Free | No |
| tinyurl | Free | No |
| YOURLS | Yes | Yes |

### Configuration

```toml
[skills.url_shortener]
provider = "isgd"  # "isgd", "tinyurl", "yourls"

[skills.url_shortener.yourls]
base_url = "https://your-domain.com/yourls-api.php"
username = "admin"
password = "${YOURLS_PASSWORD}"
```

### Usage

```rust
let shortener = UrlShortener::new(provider, config);

// Shorten
let short = shortener.shorten("https://very-long-url.com/path?query=123").await?;

// Expand
let original = shortener.expand(&short).await?;
```

## Plugin SDK (WASM)

### Plugin Manifest

```toml
# plugin.toml
name = "my-plugin"
version = "1.0.0"
description = "My custom plugin"
author = "Developer"
entry_point = "main"

[permissions]
file_read = []
file_write = ["/tmp"]
network = ["api.example.com"]
```

### Plugin Registry

```rust
let registry = PluginRegistry::new();

// Load from directory
registry.load_from_dir(&plugin_dir).await?;

// Invoke
let result = registry.invoke(
    "my-plugin",
    "process",
    r#"{"input": "data"}"#
).await?;
```

### Security

WASM plugins run in sandboxed wasmtime environment with:
- Memory limits
- No filesystem access (unless permitted)
- No network access (unless permitted)
- No shell access (unless permitted)

## Skill Packaging and Publishing

BorgClaw provides a complete skill lifecycle management system including packaging and publishing capabilities.

### Packaging Skills

Package a local skill directory into a distributable `.tar.gz` archive:

```bash
# Package a skill from current directory
borgclaw skills package ./my-skill

# Package with custom output path
borgclaw skills package ./my-skill --output ./my-skill-1.0.0.tar.gz
```

The package will include:
- `SKILL.md` - The skill manifest and instructions
- All files in the skill directory
- `borgclaw-package.json` - Metadata including name, version, and packaging timestamp

**Requirements:**
- Directory must contain a valid `SKILL.md` file
- `SKILL.md` must have a `name` field in the frontmatter

**Package Structure:**
```
my-skill-1.0.0.tar.gz
├── borgclaw-package.json    # Metadata (auto-generated)
├── SKILL.md                 # Skill manifest
├── src/                     # Source files (if any)
│   ├── main.rs
│   └── lib.rs
└── README.md               # Documentation
```

### Publishing Skills

Publish a packaged skill to a skill registry:

```bash
# Publish to default registry
borgclaw skills publish ./my-skill-1.0.0.tar.gz

# Publish to specific registry
borgclaw skills publish ./my-skill-1.0.0.tar.gz --registry https://borgclaw.io/registry

# Publish without confirmation prompt
borgclaw skills publish ./my-skill-1.0.0.tar.gz --force
```

**Publishing Process:**
1. Validates the package format (.tar.gz extension)
2. Extracts and validates package metadata
3. Confirms with user (unless `--force` is used)
4. Uploads to registry via multipart form POST
5. Returns package ID and public URL

**Registry Configuration:**

Set the default registry in your config:

```toml
[skills]
registry_url = "https://github.com/openclaw/clawhub"
```

### Skill Registry

Skills can be discovered and installed from registries:

```bash
# List available skills
borgclaw skills list

# Search for skills
borgclaw skills list weather

# Install from registry
borgclaw skills install openclaw/weather

# Install from GitHub
borgclaw skills install owner/repo

# Install from URL
borgclaw skills install https://example.com/skills/weather/SKILL.md

# Install from local package
borgclaw skills install ./my-skill-1.0.0.tar.gz
```

**Registry Format:**

Registries are GitHub repositories containing skill directories, each with a `SKILL.md` file:

```
clawhub/
├── openclaw/
│   ├── weather/
│   │   └── SKILL.md
│   └── calendar/
│       └── SKILL.md
└── community/
    └── todo/
        └── SKILL.md
```

### SKILL.md Format

The skill manifest defines the skill's capabilities:

```markdown
---
name: weather
version: 1.0.0
description: Get weather information for any location
author: BorgClaw Team
---

## Commands

- `/weather <location>` - Get current weather
- `/forecast <location> [days]` - Get weather forecast

## Environment

- `OPENWEATHER_API_KEY` - Required API key

## Examples

### Get current weather

Input: `/weather London`

Output: `Current weather in London: 15°C, partly cloudy`

### Get 5-day forecast

Input: `/forecast London 5`

Output: `London 5-day forecast: ...`
```

### Complete Workflow Example

```bash
# 1. Create a skill directory
mkdir my-skill && cd my-skill

# 2. Create SKILL.md
cat > SKILL.md << 'EOF'
---
name: my-skill
version: 1.0.0
description: A custom skill
description: Developer Name
---

## Commands

- `/my-command` - Do something useful

## Instructions

Use this skill to perform custom operations.
EOF

# 3. Add any additional files
mkdir src
echo '// Skill implementation' > src/main.rs

# 4. Package the skill
borgclaw skills package . --output my-skill-1.0.0.tar.gz

# 5. Publish to registry
borgclaw skills publish my-skill-1.0.0.tar.gz

# 6. Others can now install it
borgclaw skills install my-skill
```
