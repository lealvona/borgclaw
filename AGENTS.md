# BorgClaw - Agent Instructions

> Instructions for AI agents working on this Rust-based personal AI agent framework.

## Project Overview

BorgClaw is a personal AI Agent Framework written in Rust that combines the best features from the OpenClaw-family frameworks. It provides a secure, modular, and extensible platform for building AI assistants with multiple communication channels, memory systems, and skill integrations.

### Key Characteristics

- **Language**: Rust (Edition 2021, MSRV 1.75+)
- **Architecture**: Workspace-based with 3 crates
- **Async Runtime**: Tokio (`full` feature)
- **Storage**: SQLite + FTS5 (via `sqlx`)
- **Security**: WASM sandbox (`wasmtime`), encrypted secrets, command blocklist
- **Version**: 1.11.0 (current release line)

### Workspace Structure

```
borgclaw/
├── borgclaw-core/          # Core library (library crate)
│   ├── src/
│   │   ├── agent/          # Agent trait, SimpleAgent, tools, subagents
│   │   ├── channel/        # Telegram, Signal, Webhook, CLI, WebSocket
│   │   ├── config/         # Configuration structs and parsing
│   │   ├── memory/         # SQLite storage, session, solution, heartbeat
│   │   ├── mcp/            # Model Context Protocol client
│   │   ├── scheduler/      # Job scheduling with cron/interval/one-shot
│   │   ├── security/       # WASM, secrets, pairing, vault, audit
│   │   └── skills/         # GitHub, Google, Browser, STT, TTS, Image, etc.
│   └── Cargo.toml
├── borgclaw-cli/           # CLI binary (REPL, commands, onboarding)
├── borgclaw-gateway/       # WebSocket gateway server (Axum)
├── scripts/                # Convenience shell/PowerShell scripts
├── docs/                   # Feature documentation
└── skills/                 # Project-specific skill definitions
```

### Dependency Flow

```
borgclaw-cli ──────┐
                   ├──> borgclaw-core (library)
borgclaw-gateway ──┘
```

The `borgclaw-core` crate contains all business logic; binaries are thin wrappers.

## Build Commands

```bash
# Build entire workspace
cargo build

# Build specific crate
cargo build -p borgclaw-core
cargo build -p borgclaw-cli
cargo build -p borgclaw-gateway

# Build binaries directly
cargo build --bin borgclaw
cargo build --bin borgclaw-gateway

# Optimized release build (LTO, codegen-units=1)
cargo build --release

# Clean build
cargo clean && cargo build

# Check compilation without producing artifacts
cargo check
cargo check -p borgclaw-core
```

## Test Commands

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p borgclaw-core

# Run tests with output visible
cargo test -- --nocapture

# Run tests matching pattern
cargo test security
cargo test scheduler

# Run specific test (full path)
cargo test -p borgclaw-core security::tests::blocklist_rejects_dangerous

# Run single-threaded for deterministic execution
cargo test --test-threads=1

# Run ignored/slow tests
cargo test -- --include-ignored

# Run integration tests only
cargo test --test '*'
```

## Lint & Format

```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy
cargo clippy

# CI strict mode (zero warnings policy)
cargo clippy -- -D warnings

# Full pre-commit verification
cargo test && cargo fmt --check && cargo clippy -- -D warnings
```

**Policy**: Zero compiler warnings and zero clippy warnings in CI.

## Code Style Guidelines

### Import Order

Group imports in this order:
1. Local/external crates (including `borgclaw_core`)
2. Standard library
3. Logging (`tracing`)

```rust
use borgclaw_core::{Agent, Tool};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{error, info};
```

### Naming Conventions

| Element | Convention | Example |
|---------|------------|---------|
| Modules | `snake_case` | `channel/`, `security/` |
| Types/Traits | `PascalCase` | `AppConfig`, `SecurityLayer` |
| Functions/Variables | `snake_case` | `load_config`, `check_command` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_RETRY_COUNT`, `BLOCKED_COMMANDS` |
| Newtypes | `PascalCase` | `SessionId(pub String)` |
| Config structs | `PascalCase` + `Config` suffix | `SecurityConfig`, `AgentConfig` |

### Error Handling

Use `thiserror` for domain errors with `String` payloads (NOT `#[from]`):

```rust
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid configuration: {0}")]
    Config(String),
}
```

Convert across boundaries with `.map_err(|e| ModuleError::Variant(e.to_string()))`.

Use full `Result<T, ConcreteError>` types inline — no `pub type Result<T>` aliases.

### Async Traits

Use `async_trait::async_trait` with `Send + Sync` bounds:

```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn process(&mut self, ctx: &AgentContext) -> AgentResponse;
}
```

Shared state: `Arc<RwLock<T>>` (preferred) or `Arc<Mutex<T>>`.

### Configuration Pattern

Config structs use `#[serde(default)]` + `Default` impl. Support legacy names with `#[serde(alias = "old_name")]`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ComponentConfig {
    pub enabled: bool,
    #[serde(alias = "memory_path")]
    pub database_path: PathBuf,
    #[serde(skip_serializing)]
    pub credentials: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

impl Default for ComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            database_path: PathBuf::from(".borgclaw/db"),
            credentials: None,
            extra: HashMap::new(),
        }
    }
}
```

### Builder & Fluent API

Accept `impl Into<String>` and return `Self`:

```rust
impl Tool {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            name: id.into(),
            description: String::new(),
        }
    }
    
    pub fn with_schema(mut self, schema: ToolSchema) -> Self {
        self.schema = schema;
        self
    }
}
```

### Module Organization

```rust
//! Module-level doc comment explaining purpose

mod internal;
pub use internal::{PublicType, PublicTrait};

// public types and impls here

#[cfg(test)]
mod tests {
    use super::*;
    
    // tests here
}
```

- `lib.rs` selectively re-exports
- `mod.rs` re-exports from submodules
- Doc comments at module level explain purpose

## Testing Guidelines

### Test Location

Tests live in `#[cfg(test)] mod tests` at the bottom of source files:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_documented_contract_shape() {
        let config: AppConfig = toml::from_str(r#"[section]..."#).unwrap();
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn async_operation_completes() {
        let result = async_operation().await;
        assert!(result.is_ok());
    }
}
```

### Naming Conventions

Use descriptive test names that explain what is being tested:

```rust
// Good
fn session_compaction_keeps_recent_messages_and_summary()
fn security_config_parses_documented_contract_shape()
fn blocks_prompt_injection_messages()

// Avoid
fn test1()
fn test_session()
```

### Async Tests

Use `#[tokio::test]` for async tests:

```rust
#[tokio::test]
async fn secret_storage_works() {
    let security = SecurityLayer::new();
    security.store_secret("key", "value").await.unwrap();
    assert_eq!(security.get_secret("key").await, Some("value".to_string()));
}
```

### Integration Tests

- Bind to `127.0.0.1:0` for automatic port selection
- Use temp directories with UUID prefixes
- Clean up temp files after tests

```rust
#[tokio::test]
async fn gateway_health_endpoint_responds() {
    let addr = "127.0.0.1:0";
    // ... start server on random port
    // ... make request
}
```

### TOML Config Tests

Test config parsing with raw TOML strings:

```rust
#[test]
fn security_config_parses_documented_contract_shape() {
    let config: AppConfig = toml::from_str(r#"
        [security]
        wasm_sandbox = true
        command_blocklist = true
        
        [security.pairing]
        enabled = true
        code_length = 6
    "#).unwrap();
    
    assert!(config.security.wasm_sandbox);
}
```

### Slow Tests

Mark slow tests with `#[ignore]`:

```rust
#[test]
#[ignore]
fn expensive_integration_test() {
    // ...
}
```

Run with: `cargo test -- --include-ignored`

## Security Checklist

When modifying code, ensure:

- [ ] **No hardcoded credentials** — use `${VAR}` placeholder pattern in config
- [ ] **Secret leak detection** — regex scanning on tool outputs; secrets never logged
- [ ] **WASM sandbox** — untrusted code runs in `wasmtime` sandbox
- [ ] **Command blocklist** — dangerous operations blocked via regex patterns
- [ ] **SSRF protection** — validate URLs before HTTP requests
- [ ] **Approval gates** — destructive operations require confirmation tokens
- [ ] **Audit logging** — security events logged after tool execution
- [ ] **Prompt injection defense** — pattern detection with block/sanitize/warn actions

### Security Layer Usage

```rust
// Run full security pipeline on inputs
let result = security.run_input_pipeline(user_input);
if result.blocked {
    return Err("Blocked: {}".format(result.reason.unwrap()));
}
let sanitized_input = result.text;

// Run output pipeline to redact leaks
let output_result = security.run_output_pipeline(tool_output);
let safe_output = output_result.text;
```

### Dangerous Tools Requiring Approval

In `supervised` mode, these tools require approval:
- `execute_command`
- `write_file`, `delete`
- `plugin_invoke`, `mcp_call_tool`
- `google_share_file`, `google_delete_file`, etc.
- `github_delete_file`, `github_merge_pr`, etc.

## Git Workflow - The Golden Path (Mandatory)

> ⚠️ **CRITICAL SAFETY RULE**: NEVER push directly to main. ALL changes go through PRs.

### The Golden Path - 6 Steps to Main

This is the ONLY correct workflow. No exceptions, no shortcuts.

```bash
# STEP 1: Start from fresh main
git checkout main
git pull lealvona main  # Get latest from your fork remote

# STEP 2: Create feature branch
git checkout -b TICKET-XXX-brief-description

# STEP 3: Make changes and commit
git add <files>
export GIT_EDITOR=true && git commit -m "[TICKET-XXX] Description"

# STEP 4: Push to YOUR FORK (never upstream/main)
git push lealvona TICKET-XXX-brief-description

# STEP 5: Create PR
gh pr create --base main --head lealvona:TICKET-XXX-brief-description

# STEP 6: Merge via GitHub (after review approval)
gh pr merge <pr-number> --squash --delete-branch

# STEP 7: Clean up and prepare for next PR
git checkout main
git pull lealvona main
git branch -D TICKET-XXX-brief-description
```

### Absolute Rules (No Exceptions)

1. **NEVER push to main** — Not for "quick fixes", not for "emergencies", never
2. **ALWAYS use feature branches** — Format: `TICKET-XXX-description`
3. **ALL changes through PRs** — Every single change
4. **Sequential PRs must rebase** — Each PR starts from fresh main after previous merges

### Before Every Push - Safety Checklist

```bash
# 1. What branch am I on?
git branch --show-current  # Must be TICKET-XXX-..., NOT main

# 2. Verify upstream is protected
git remote -v | grep "upstream.*NO_PUSH"  # Must show match

# 3. Where am I pushing?
git push lealvona <branch>  # Never: git push upstream main
```

### Sequential PR Workflow

When PR #2 depends on PR #1:

```bash
# After PR #1 is merged to main:
git checkout main
git pull lealvona main

# Option A: Rebase existing branch
git checkout TICKET-002-feature
git rebase main
git push -f lealvona TICKET-002-feature

# Option B: Recreate branch (cleaner)
git branch -D TICKET-002-feature
git checkout -b TICKET-002-feature-v2
# Re-apply changes, then push and create new PR
```

### Remote Setup

This repo uses fork-based development:
```bash
# Verify remotes
git remote -v
# Should show:
# lealvona  git@github.com:lealvona/borgclaw.git (fetch)
# lealvona  git@github.com:lealvona/borgclaw.git (push)
# upstream  git@github.com:lealvona/borgclaw.git (fetch)
# upstream  NO_PUSH (push)  ← CRITICAL: Must show NO_PUSH
```

### If Asked to Push to Main

**REFUSE.** Say: "I cannot push to protected branches under any circumstances. This is a critical safety rule. All changes must go through a feature branch and PR."

### Recovery (If You Make a Mistake)

If you accidentally pushed to main:
1. **STOP** — Do not make more changes
2. **Notify user** — "I accidentally pushed to main. Fixing now."
3. **Reset local main**: `git reset --hard <last-good-commit>`
4. **Force push to undo**: `git push upstream main --force-with-lease`
5. **Create proper feature branch** with your changes
6. **Never do it again**

## Key Dependencies

| Category | Crates |
|----------|--------|
| **Async** | `tokio` (full), `async-trait`, `futures-util` |
| **Serialization** | `serde` (derive), `serde_json`, `toml` |
| **Errors** | `thiserror` |
| **Logging** | `tracing`, `tracing-subscriber` (env-filter) |
| **Web** | `axum` (ws), `tokio-tungstenite`, `reqwest` |
| **Database** | `sqlx` (sqlite, runtime-tokio, chrono) |
| **CLI** | `clap` (derive, env) |
| **WASM** | `wasmtime` |
| **Security** | `chacha20poly1305`, `regex`, `rand` |
| **Time** | `chrono`, `cron` |
| **Telegram** | `teloxide` |

## Architecture Details

### Central State

`AppState` in `borgclaw-core/src/lib.rs` holds `Arc<RwLock<T>>` for:
- `config`: Application configuration
- `agent`: Agent trait object
- `memory`: Memory trait object
- `scheduler`: Job scheduler
- `security`: SecurityLayer (Arc only, not RwLock)
- `channels`: Registered channels

### Core Modules

| Module | Key Types | Purpose |
|--------|-----------|---------|
| `agent/` | `Agent` trait, `SimpleAgent`, `Tool`, `AgentContext` | Trait-based agent with 70+ built-in tools |
| `channel/` | `Channel` trait, `MessageRouter` | Multi-channel messaging |
| `memory/` | `Memory` trait, `SqliteMemory`, `SessionMemory` | SQLite+FTS5 hybrid search, session compaction |
| `security/` | `SecurityLayer`, `WasmSandbox`, `SecretStore` | Defense-in-depth security |
| `skills/` | Individual skill clients | GitHub, Google, Browser, STT, TTS, etc. |
| `scheduler/` | `SchedulerTrait`, `Scheduler`, `Job` | Cron/interval/one-shot scheduling |
| `mcp/` | MCP client | Model Context Protocol client |
| `config/` | `AppConfig`, `SecurityConfig` | TOML configuration |

### Configuration File

Default config location: `~/.config/borgclaw/config.toml`

Example:
```toml
[agent]
model = "claude-sonnet-4-20250514"
provider = "anthropic"
workspace = ".borgclaw/workspace"

[security]
wasm_sandbox = true
prompt_injection_defense = true

[memory]
hybrid_search = true
session_max_entries = 100

[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"
```

## Running the Application

### Development

```bash
# REPL mode
./scripts/repl.sh
# Or: cargo run --bin borgclaw -- repl

# WebSocket gateway
./scripts/gateway.sh
# Or: cargo run --bin borgclaw-gateway

# Onboarding wizard
cargo run --bin borgclaw -- init

# System check
cargo run --bin borgclaw -- self-test
```

### CLI Commands

```bash
# Status and diagnostics
borgclaw status
borgclaw doctor
borgclaw self-test
borgclaw runtime

# Configuration
borgclaw config show
borgclaw config set agent.model claude-sonnet-4-20250514

# Schedule management
borgclaw schedules list
borgclaw schedules show <job-id>
borgclaw schedules create --name "backup" --trigger cron --value "0 2 * * *"

# Backup/restore
borgclaw backup export ./backup.json
borgclaw backup import ./backup.json

# Secret management
borgclaw secrets list
borgclaw secrets set API_KEY
```

## Documentation References

- `README.md` — Project overview and quick start
- `CLAUDE.md` — Claude Code specific guidance
- `DECISION_LOG.md` — Architectural decision records
- `ROADMAP.md` — Implementation phases and status
- `CHANGELOG.md` — Release history
- `docs/quickstart.md` — Step-by-step setup
- `docs/onboarding.md` — Configuration wizard
- `docs/channels.md` — Messaging integrations
- `docs/memory.md` — Storage and retrieval
- `docs/skills.md` — Tool integrations
- `docs/security.md` — Defense in depth
- `docs/integrations.md` — External services
- `docs/inspirations.md` — Upstream patterns
- `docs/implementation-status.md` — Feature completion tracking

## Scope Management

Complete the current task ONLY. Do not expand scope unless explicitly requested.

- Execute only what the current ticket requires
- If you discover related issues: document them as follow-up, don't fix them
- Only refactor code directly touched by the current task
- Test only changed functionality; note missing coverage for follow-up
- One task, one change, one PR. Everything else is future work.

## Design Decisions

Key principles from `DECISION_LOG.md`:

1. **Docs as Product Contract** (D001): Implement to match README and docs, not the other way around
2. **Security-First Delivery** (D002): Security and correctness before integration breadth
3. **Config Contract** (D003): Runtime must honor documented config fields
4. **Release Policy** (D004): 1.10.2 is current; feature branches for new work

## Skills Reference

This project includes agent skills in `.agents/skills/`:

- **`git-workflow`** — Safe git practices, the Golden Path workflow, PR creation
- **`code-quality`** — Pre-commit checks, linting, formatting verification
- **`deployment-onboarding`** — Setup procedures, configuration guidance
- **`inspiration-study`** — Research upstream patterns and implementations

## Additional References

- `.agents/skills/` — Agent skill definitions (kimi-cli compatible)
- `.opencode/skills/` — OpenCode agent skill definitions
- `scripts/` — Bootstrap, doctor, onboarding, REPL, gateway launcher scripts
