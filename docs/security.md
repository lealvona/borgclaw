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

Untrusted tools run in WebAssembly sandbox via wasmtime:

- Isolated memory space
- No direct filesystem access
- No network access (unless permitted)
- Resource limits (memory, CPU)

### Configuration

```toml
[security]
wasm_sandbox = true
wasm_max_instances = 10
```

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
