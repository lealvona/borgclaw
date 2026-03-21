# BorgClaw - Agent Instructions

> Instructions for AI agents working on this Rust-based personal AI agent framework.

## Project Overview
- **Language**: Rust (Edition 2021, MSRV 1.75+)
- **Workspace**: 3 crates - `borgclaw-core`, `borgclaw-cli`, `borgclaw-gateway`
- **Async**: Tokio runtime (`full` feature)
- **Storage**: SQLite + FTS5 (via `sqlx`)
- **Security**: WASM sandbox (`wasmtime`), encrypted secrets, command blocklist

## Critical Directives

**READ `/home/lvona/.config/opencode/AGENTS.md` FIRST** - contains universal rules including NEVER push to main/master and all skill references.

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
cargo test -- --nocapture            # show print statements
cargo test module_name::test_fn      # single test (full path)
cargo test security                  # pattern match filter
cargo test -p borgclaw-core security::tests::blocklist_rejects_dangerous  # specific test
cargo test --test-threads=1          # deterministic parallel execution
cargo test -- --include-ignored      # run ignored tests
```

## Lint & Format
```bash
cargo fmt --check                    # check formatting
cargo fmt                            # format code
cargo clippy                         # run clippy
cargo clippy -- -D warnings          # CI strict mode
```

## Code Style

### Imports
Group order: (1) local/external crates, (2) std library, (3) logging:
```rust
use borgclaw_core::{Agent, Tool};
use chrono::{Duration, Utc};
use std::path::PathBuf;
use tracing::{error, info};
```

### Naming Conventions
| Element | Convention | Example |
|---------|------------|---------|
| Modules | `snake_case` | `channel/`, `security/` |
| Types/Traits | `PascalCase` | `AppConfig`, `SecurityLayer` |
| Functions/Variables | `snake_case` | `load_config`, `check_command` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_RETRY_COUNT` |
| Newtypes | `PascalCase` | `SessionId(pub String)` |

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
Convert across boundaries with `.map_err(|e| ModuleError::Variant(e.to_string()))`. Use full `Result<T, ConcreteError>` types inline — no `pub type Result<T>` aliases.

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
    fn default() -> Self { Self { enabled: true, database_path: PathBuf::from(".borgclaw/db"), credentials: None, extra: HashMap::new() } }
}
```

### Builder & Fluent API
Accept `impl Into<String>` and return `Self`:
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
`lib.rs` selectively re-exports. `mod.rs` re-exports from submodules.

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
- Tests in `#[cfg(test)] mod tests` at bottom of source files
- Descriptive names: `fn config_parses_documented_contract_shape()`
- TOML config tests: `toml::from_str(r#"[section]..."#)`
- Async tests: `#[tokio::test]`; sync needing async create own `Runtime`
- Integration tests: bind to `127.0.0.1:0` with temp listeners
- Temp dirs: UUID prefixes, cleanup with `std::fs::remove_dir_all`
- Slow tests: `#[ignore]`, run with `cargo test -- --include-ignored`

## Security Checklist
- WASM sandbox for untrusted code execution
- Command blocklist for dangerous operations
- No hardcoded credentials — use `${VAR}` placeholder pattern
- Secret leak detection via regex on tool outputs
- Secrets never logged or exposed in error messages
- Audit logging after every tool execution

## Git Workflow
See `/home/lvona/.config/opencode/AGENTS.md` for universal git rules.

### Before Submitting
```bash
cargo test && cargo fmt --check && cargo clippy -- -D warnings && git status
```

### Safety Verification
```bash
git remote -v | grep "upstream.*NO_PUSH"  # Verify upstream push protection
```

## Additional References
- `.codex/instructions/` — per-ticket instruction files with YAML frontmatter
- `.codex/skills/` — progressive-loading agent skill definitions
