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
