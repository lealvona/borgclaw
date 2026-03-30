# Security

BorgClaw implements defense-in-depth security with multiple protective layers.

The current sandbox contract has two layers:
- **WASM sandbox** for plugins and other untrusted extension code
- **Optional Docker sandbox** for `execute_command` when operators want containerized shell execution

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

## Docker Command Sandbox

### Overview

Docker is an optional shell-execution sandbox. It does not replace the WASM plugin sandbox.

- **Scope**: `execute_command`
- **Model**: one ephemeral container per command
- **Defaults**: read-only root filesystem, `tmpfs` `/tmp`, explicit network mode, explicit workspace mount mode
- **Policy owner**: `SecurityLayer`
- **PTY behavior**: foreground PTY execution stays on the host path in v1; background command execution is non-PTY and persists process state

### Configuration

```toml
[security.docker]
enabled = false
image = "borgclaw-sandbox:base"
network = "none"          # "none" or "bridge"
workspace_mount = "ro"    # "ro", "rw", or "off"
read_only_rootfs = true
tmpfs = true
memory_limit_mb = 512
cpu_limit = "1.0"
timeout_seconds = 120
allowed_tools = ["execute_command"]
allowed_roots = []
extra_env_allowlist = ["PATH", "HOME"]
```

### Runtime Behavior

The Docker path stays inside the existing security pipeline:

1. command blocklist / allowlist
2. approval gates
3. workspace policy
4. Docker invocation construction
5. execution
6. leak redaction
7. audit logging

Docker execution inherits the same workspace policy used by file tools, scheduled work, heartbeat tasks, and sub-agents. Extra bind mounts must still be allowed by the workspace policy.

## Command Runtime

`execute_command` now supports:

- foreground host execution
- foreground PTY execution
- background non-PTY execution with persisted `processes.json` state
- shared approval, blocklist, workspace-policy, Docker-routing, and audit behavior across all command paths

Tool arguments:

```json
{
  "command": "cargo test -p borgclaw-core",
  "timeout": 120,
  "pty": false,
  "background": true,
  "yield_ms": 250
}
```

Operator surfaces:

- `borgclaw processes list`
- `borgclaw processes show <id>`
- `borgclaw processes cancel <id>`

### Installation

Build the default sandbox image with:

```bash
./scripts/install-docker-sandbox.sh      # Linux/macOS
.\scripts\install-docker-sandbox.ps1     # Windows
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

# Allow specific hosts via regex patterns (overrides default blocks)
ssrf_allowlist = [
    "^trusted-internal\\.example\\.com$",
    "^monitoring\\.internal\\.corp$"
]

# Block additional hosts via regex patterns
ssrf_blocklist = [
    "^malicious\\.example\\.com$",
    "^.*\\.evil\\.com$"
]
```

Set `ssrf_protection = false` to disable all SSRF checks (not recommended for production).

### Protected Tools

SSRF validation is enforced on all tools that make HTTP requests or navigate to URLs:

| Tool | Protection |
|------|-----------|
| `browser_navigate` | SSRF + blocks `file://`, `data://`, `javascript:` schemes |
| `fetch_url` | SSRF validation before HTTP request |
| `url_shorten` | SSRF validation on user-provided URL |
| `url_expand` | SSRF validation on shortened URL |

The `UrlShortener` skill also validates URLs independently as defense in depth.

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

### Validation Order

1. Check custom blocklist (reject if matched)
2. Check custom allowlist (accept if matched, overrides default blocks)
3. Check localhost/loopback (reject unless `allow_localhost` is true)
4. Check private IP ranges (reject unless `allow_private_ips` is true)
5. Accept all other URLs

## Execution Approval Gates

### Overview

Destructive or security-sensitive operations require explicit user approval before execution.
This prevents agents from performing irreversible actions without human confirmation.

### Approval Modes

```toml
[security]
# Options: "read_only", "supervised", "autonomous"
approval_mode = "supervised"
```

| Mode | Behavior |
|------|----------|
| `read_only` | All tool executions require approval |
| `supervised` | Only destructive tools require approval |
| `autonomous` | No approval required (default) |

### Tools Requiring Approval

In `supervised` mode, the following tools require approval:

| Tool | Risk |
|------|------|
| `execute_command` | Arbitrary command execution |
| `write_file` | File system modification |
| `delete` | File deletion |
| `plugin_invoke` | WASM plugin execution |
| `mcp_call_tool` | External MCP tool execution |
| `google_share_file` | Data exposure via sharing |
| `google_remove_permission` | Access revocation |
| `google_delete_file` | Permanent data loss |
| `google_delete_email` | Email deletion |
| `google_trash_email` | Email trashing |
| `google_delete_event` | Calendar event deletion |
| `github_delete_file` | Repository file deletion |
| `github_delete_branch` | Branch deletion |
| `github_merge_pr` | Pull request merge |

### Approval Workflow

```
1. Agent requests tool execution
2. SecurityLayer checks needs_approval(tool_name, mode)
3. If approval needed and no token provided:
   - Generate approval token (UUID)
   - Return approval request to user
4. User approves via /approve command with token
5. Agent retries with approval_token in arguments
6. SecurityLayer validates and consumes token
7. Tool executes
```

### Example

```rust
// Agent calls google_share_file without approval token
// Response: "approval required; rerun with /approve {"tool":"google_share_file","token":"abc-123"}"

// User approves:
// /approve {"tool":"google_share_file","token":"abc-123"}

// Agent retries with token - execution proceeds
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
