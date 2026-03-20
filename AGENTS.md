# BorgClaw - Agent Instructions

> Instructions for AI agents working on this Rust-based personal AI agent framework.

## Project Overview
- **Language**: Rust (Edition 2021, MSRV 1.75+)
- **Workspace**: 3 crates - `borgclaw-core`, `borgclaw-cli`, `borgclaw-gateway`
- **Async**: Tokio runtime (`full` feature)
- **Storage**: SQLite + FTS5 (via `sqlx`)
- **Security**: WASM sandbox (`wasmtime`), encrypted secrets, command blocklist

## Skills

Read the relevant skill file before working in these areas:

| Skill | Purpose |
|-------|---------|
| `skills/git-workflow.md` | Git safety rules, NO_PUSH directive for unowned repos |
| `skills/inspiration-study.md` | Studying upstream codebases, updating plans/roadmaps |
| `skills/deployment-onboarding.md` | Deploying BorgClaw, guiding new users through setup |

## Build Commands

```bash
cargo build                          # entire workspace
cargo build -p borgclaw-core         # specific crate
cargo build --release                # optimized (LTO, codegen-units=1)
cargo clean && cargo build           # clean build
```

## Test Commands

```bash
cargo test                           # all tests
cargo test -p borgclaw-core          # single crate
cargo test -- --nocapture            # show output
cargo test module_name::test_fn      # single test (full path)
cargo test security                  # pattern match
cargo test config_parses             # pattern match
```

## Lint & Format

```bash
cargo fmt --check                    # check formatting
cargo fmt                            # format code
cargo clippy                         # run clippy
cargo clippy -- -D warnings          # CI standard (strict)
```

## Code Style

### Imports
Group imports: (1) local crate refs / external crates, (2) standard library, (3) logging:
```rust
use borgclaw_core::{Agent, Tool};
use chrono::{Duration, Utc};
use std::path::PathBuf;
use tracing::{error, info};
```

### Naming Conventions
- **Modules**: `snake_case` (e.g., `channel/`, `security/`)
- **Types/Traits**: `PascalCase` (e.g., `AppConfig`, `SecurityLayer`, `Agent`)
- **Functions/Variables**: `snake_case` (e.g., `load_config`, `check_command`)
- **Constants**: `SCREAMING_SNAKE_CASE` for values, `PascalCase` for const fns
- **Newtypes**: `PascalCase` wrapping the inner type (e.g., `SessionId(pub String)`)

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
Use full `Result<T, ConcreteError>` types inline — do NOT define `pub type Result<T>` aliases.

### Async Traits
Use `async_trait::async_trait` with `Send + Sync` bounds:
```rust
#[async_trait]
pub trait Agent: Send + Sync {
    async fn process(&mut self, ctx: &AgentContext) -> AgentResponse;
}
```
Shared state uses `Arc<RwLock<T>>` (prefer) or `Arc<Mutex<T>>`.

### Configuration Pattern
All config structs use `#[serde(default)]` + `Default` impl. Support legacy field names with `#[serde(alias = "old_name")]`:
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
    fn default() -> Self { Self { enabled: true, database_path: PathBuf::from(".borgclaw/db"), credentials: None, extra: HashMap::new() } }
}
```

### Builder & Fluent API
Constructors and builders accept `impl Into<String>` and return `Self`:
```rust
pub fn new(id: impl Into<String>) -> Self { ... }
pub fn with_schema(mut self, schema: ToolSchema) -> Self { ... }
```

### Module Organization
```rust
//! Module-level doc comment

mod internal;
pub use internal::{PublicType, PublicTrait};

// public types and impls here

#[cfg(test)]
mod tests { ... }
```
`lib.rs` selectively re-exports key types. `mod.rs` files re-export from submodules.

### Key Dependencies
- **Async**: `tokio` (full), `async-trait`, `futures-util`
- **Serialization**: `serde` (derive), `serde_json`, `toml`
- **Errors**: `thiserror`
- **Logging**: `tracing`, `tracing-subscriber` (env-filter)
- **Web**: `axum` (ws), `tokio-tungstenite`, `reqwest`
- **DB**: `sqlx` (sqlite, runtime-tokio, chrono)
- **CLI**: `clap` (derive, env)
- **WASM**: `wasmtime`

## Testing Guidelines
- Tests live in `#[cfg(test)] mod tests` at the bottom of source files
- Use descriptive sentence-like names: `fn config_parses_documented_contract_shape()`
- Config parsing tests embed TOML strings: `toml::from_str(r#"[section]..."#)`
- Async tests use `#[tokio::test]`; sync tests needing async create their own `Runtime`
- Integration tests in gateway bind to `127.0.0.1:0` with temp listeners
- Temp dirs use UUID prefixes and clean up: `std::fs::remove_dir_all`

## Security Checklist
- WASM sandbox for untrusted code execution
- Command blocklist for dangerous operations
- No hardcoded credentials — use `${VAR}` placeholder pattern
- Secret leak detection via regex on tool outputs
- Secrets never logged or exposed in error messages
- Audit logging after every tool execution

## Git Workflow

> **NEVER push directly to main/master. ALL changes go through feature branches and PRs.**

### Branch Naming
`TICKET-<number>-<short-description>` (e.g., `TICKET-123-fix-backup-import`)

### Commit Messages
```
[AREA] Brief description (50 chars or less)

- Detailed change explanation
- Reference to issue/ticket
```

### Before Submitting
```bash
cargo test && cargo fmt --check && cargo clippy -- -D warnings && git status
git remote -v  # verify correct remote (see skills/git-workflow.md)
```

## Additional References
- `.codex/instructions/` — per-ticket instruction files with YAML frontmatter
- `.codex/skills/` — progressive-loading agent skill definitions
