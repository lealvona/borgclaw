# Security

BorgClaw implements defense-in-depth security with multiple protective layers.

## Security Layers

```
┌─────────────────────────────────────────────────┐
│               Security Layer                    │
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌──────────────┐  ┌──────────────┐           │
│  │ WASM Sandbox │  │   Secrets    │           │
│  │  (wasmtime)  │  │  (Encrypted) │           │
│  └──────────────┘  └──────────────┘           │
│                                                 │
│  ┌──────────────┐  ┌──────────────┐           │
│  │   Pairing    │  │  Injection   │           │
│  │    Codes     │  │   Defense    │           │
│  └──────────────┘  └──────────────┘           │
│                                                 │
│  ┌──────────────┐  ┌──────────────┐           │
│  │  Command     │  │    Vault     │           │
│  │  Blocklist   │  │  Integration │           │
│  └──────────────┘  └──────────────┘           │
│                                                 │
└─────────────────────────────────────────────────┘
```

## WASM Sandbox

### Overview

BorgClaw uses WebAssembly (WASM) as its primary sandbox mechanism for executing untrusted tools. WASM provides better isolation and lower overhead than traditional container approaches:

- **Memory isolation**: Each plugin runs in its own isolated memory space
- **Capability-based security**: Explicit permissions for filesystem, network, and system access
- **Resource limits**: Configurable memory and CPU constraints
- **Fast startup**: No container image overhead
- **Cross-platform**: Works consistently across operating systems

Untrusted tools run via wasmtime runtime:

- Isolated memory space
- No direct filesystem access
- No network access (unless permitted)
- Resource limits (memory, CPU)

### Configuration

```toml
[security]
wasm_sandbox = true
wasm_max_instances = 10  # Maximum concurrent WASM instances
```

The `max_instances` parameter controls resource usage and prevents resource exhaustion:
- Each running plugin consumes one instance slot
- New plugin executions wait if limit is reached
- Default is 10 concurrent instances
- Increase for high-throughput scenarios, decrease for resource-constrained environments

### Plugin Permissions

```toml
[permissions]
file_read = []                          # No file read
file_write = ["/tmp"]                   # Write to /tmp only
network = ["api.example.com:443"]       # Specific hosts only
memory = true                           # Extended memory
shell = false                           # No shell access
```

### Implementation

```rust
pub struct WasmSandbox {
    modules: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    engine: Engine,
    max_instances: usize,
}

// Register module
sandbox.register_module("plugin", wasm_bytes).await?;

// Execute with isolation
let result = sandbox.execute("plugin", "function", input).await?;
```

## Secrets Management

### Encrypted Storage

Secrets stored encrypted with ChaCha20-Poly1305:

```rust
let secrets = SecretsManager::new(master_key);

// Store
secrets.store("api_key", "sk-xxx").await?;

// Retrieve
let key = secrets.get("api_key").await?;
```

### Configuration

```toml
[security]
secrets_encryption = true
secrets_path = ".local/data/secrets.enc"
```

## Pairing Codes

### Overview

6-digit codes for channel authentication:

- Prevents unauthorized access
- Time-limited (5 minutes)
- One-time use

### Flow

```
1. User requests pairing from bot
2. Bot generates 6-digit code (e.g., "123456")
3. User enters code in client
4. Client sends code to WebSocket
5. Server validates and grants access
```

### Configuration

```toml
[security.pairing]
enabled = true
code_length = 6
expiry_seconds = 300
```

### Implementation

```rust
let pairing = PairingManager::new();

// Generate code
let code = pairing.generate_code("user123").await?;
// code = "123456"

// Validate
let valid = pairing.validate_code("user123", "123456").await?;
```

## Prompt Injection Defense

### Detection Patterns

```rust
const INJECTION_PATTERNS: &[&str] = &[
    r"(?i)ignore (all )?(previous|above) instructions",
    r"(?i)forget (all )?(previous|above)",
    r"(?i)disregard (all )?(previous|above)",
    r"(?i)you are now",
    r"(?i)jailbreak",
    r"(?i)DAN",
    r"(?i)system:\s*",
];
```

### Sanitization

```rust
let defender = InjectionDefender::new();

// Check input
if defender.detect(&user_input) {
    return Err(SecurityError::InjectionDetected);
}

// Sanitize
let safe_input = defender.sanitize(&user_input);
```

### Configuration

```toml
[security]
prompt_injection_defense = true
injection_action = "block"  # "block", "sanitize", "warn"
```

## Command Blocklist

### Blocked Commands

Dangerous system commands are blocked:

```rust
const BLOCKED_COMMANDS: &[&str] = &[
    r"^rm\s+-rf\s+/",
    r"^rm\s+-rf\s+~",
    r"^mkfs",
    r"^dd\s+if=",
    r"^:\(\)\{.*\|.*&\};:",  // Fork bomb
    r"^chmod\s+777",
    r"^chown\s+.*:.*\s+/",
    r"^shutdown",
    r"^reboot",
    r"^halt",
    r"^init\s+[06]",
];
```

### Implementation

```rust
let blocklist = CommandBlocklist::new();

// Check command
if blocklist.is_blocked("rm -rf /") {
    return Err(SecurityError::BlockedCommand);
}
```

### Configuration

```toml
[security]
command_blocklist = true
# Additional patterns
extra_blocked = [
    "^custom_dangerous_command"
]
```

## SSRF Protection

### Overview

Server-Side Request Forgery (SSRF) protection prevents malicious URLs from accessing internal resources:

- Blocks requests to localhost/loopback addresses
- Blocks private IP ranges (10.x.x.x, 172.16-31.x.x, 192.168.x.x)
- Blocks link-local addresses (169.254.x.x, fe80::/10)
- Blocks IPv6 unique local addresses (fc00::/7)

### Blocked IP Ranges

```
IPv4:
- 127.0.0.0/8      (loopback)
- 10.0.0.0/8       (private)
- 172.16.0.0/12    (private)
- 192.168.0.0/16   (private)
- 169.254.0.0/16   (link-local)
- 224.0.0.0/4      (multicast)
- 0.0.0.0          (unspecified)

IPv6:
- ::1              (loopback)
- fc00::/7         (unique local)
- fe80::/10        (link-local)
- ff00::/8         (multicast)
- ::               (unspecified)

Hostnames:
- localhost
- 127.*.*.*
- 10.*.*.*
- 192.168.*.*
- 172.16-31.*.*
- 169.254.*.*
- fc*, fd*         (IPv6 unique local)
- fe8*              (IPv6 link-local)
```

### Usage

```rust
use borgclaw_core::security::{SecurityLayer, SsrfGuard};

// Use via SecurityLayer
let security = SecurityLayer::new();

// Validate URL before making request
match security.validate_url("https://example.com/api") {
    Ok(()) => println!("URL is safe"),
    Err(e) => println!("Blocked: {}", e),
}

// Direct usage with custom configuration
let guard = SsrfGuard::new()
    .with_localhost(true)      // Allow localhost (for testing)
    .with_private_ips(false);   // Block private IPs
```

### Examples of Blocked URLs

```rust
// These URLs will be blocked:
let blocked_urls = vec![
    "http://localhost/admin",
    "http://127.0.0.1/secrets",
    "http://192.168.1.1/router-config",
    "http://10.0.0.1/internal-api",
    "http://172.16.0.1/metadata",
    "http://169.254.169.254/latest/meta-data",  // AWS metadata
];

for url in blocked_urls {
    assert!(security.validate_url(url).is_err());
}
```

### Configuration

```toml
[security]
ssrf_protection = true  # Enabled by default

# Allow specific hosts (overrides default blocks)
[security.ssrf_allowlist]
patterns = [
    "^trusted-internal\\.example\\.com$"
]

# Block additional hosts
[security.ssrf_blocklist]
patterns = [
    "^malicious\\.example\\.com$"
]
```

### Implementation

The `SsrfGuard` provides URL validation:

```rust
pub struct SsrfGuard {
    allow_localhost: bool,
    allow_private_ips: bool,
    allowed_hosts: Vec<Regex>,
    blocked_hosts: Vec<Regex>,
}

impl SsrfGuard {
    pub fn validate_url(&self, url: &str) -> Result<(), SsrfError>;
    pub fn allow_host(&mut self, pattern: &str) -> Result<(), regex::Error>;
    pub fn block_host(&mut self, pattern: &str) -> Result<(), regex::Error>;
}
```

## Vault Integration

### Bitwarden (Primary)

```bash
# Install
npm install -g @bitwarden/cli

# Login
bw login

# Unlock (sets BW_SESSION)
export BW_SESSION=$(bw unlock --raw)
```

```rust
let bw = BitwardenClient::new(BitwardenConfig {
    cli_path: "bw".into(),
    session_env: "BW_SESSION".into(),
});

// Get secret
let secret = bw.get_secret("api-key-name").await?;

// List items
let items = bw.list_items(Some("folder")).await?;

// Create
let id = bw.create_item("new-secret", "value", Some("folder")).await?;
```

### 1Password (Secondary)

```bash
# Install from https://1password.com/downloads/command-line/

# Sign in
op signin
```

```rust
let op = OnePasswordClient::new(OnePasswordConfig {
    account: Some("my-account".into()),
    vault: Some("Private".into()),
});

// Get secret
let secret = op.get_secret("api-key-name").await?;
```

### Configuration

```toml
[security.vault]
provider = "bitwarden"  # "bitwarden" or "1password"

[security.vault.bitwarden]
cli_path = "bw"
session_env = "BW_SESSION"

[security.vault.1password]
cli_path = "op"
account = "my-account"
vault = "Private"
```

## Secret Leak Detection

### Patterns

```rust
const SECRET_PATTERNS: &[(&str, &str)] = &[
    ("AWS Access Key", r"AKIA[0-9A-Z]{16}"),
    ("AWS Secret Key", r"[A-Za-z0-9/+=]{40}"),
    ("GitHub Token", r"ghp_[A-Za-z0-9]{36}"),
    ("OpenAI API Key", r"sk-[A-Za-z0-9]{48}"),
    ("Anthropic API Key", r"sk-ant-[A-Za-z0-9\-_]{80,}"),
];
```

### Redaction

```rust
let detector = LeakDetector::new();

// Detect
let leaks = detector.scan(&output);
for leak in leaks {
    warn!("Secret detected: {}", leak.pattern_name);
}

// Redact
let safe_output = detector.redact(&output);
// "My key is sk-ant-xxxx..."
```

### Configuration

```toml
[security]
leak_detection = true
leak_action = "redact"  # "redact", "block", "warn"
```

## Best Practices

### API Keys

1. Store in vault, not config files
2. Use environment variable references: `${ANTHROPIC_API_KEY}`
3. Never log API keys
4. Rotate keys periodically

### Channels

1. Use pairing codes for WebSocket
2. Restrict Telegram DM policy
3. Verify webhook secrets
4. Rate limit all endpoints

### WASM Plugins

1. Only load trusted plugins
2. Use minimal permissions
3. Audit plugin code
4. Run in sandbox

### Logging

1. Never log secrets
2. Enable leak detection
3. Audit security events
4. Monitor for anomalies

## Security Checklist

- [ ] WASM sandbox enabled
- [ ] Prompt injection defense enabled
- [ ] Command blocklist enabled
- [ ] Secret leak detection enabled
- [ ] Pairing codes required for WebSocket
- [ ] API keys stored in vault
- [ ] Webhook secrets configured
- [ ] Rate limiting enabled
