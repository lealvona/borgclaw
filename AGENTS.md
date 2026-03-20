# BorgClaw - Agent Instructions

> Instructions for AI agents working on this Rust-based personal AI agent framework.

## Project Overview
- **Language**: Rust (Edition 2021, MSRV 1.75+)
- **Workspace**: 3 crates - `borgclaw-core`, `borgclaw-cli`, `borgclaw-gateway`
- **Async**: Tokio runtime
- **Storage**: SQLite + FTS5
- **Security**: WASM sandbox, encrypted secrets, command blocklist

## Build Commands

```bash
# Build entire workspace
cargo build

# Build specific crate
cargo build -p borgclaw-core
cargo build -p borgclaw-cli
cargo build -p borgclaw-gateway

# Build release (optimized with LTO)
cargo build --release

# Clean build
cargo clean && cargo build
```

## Test Commands

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p borgclaw-core
cargo test -p borgclaw-gateway

# Run with output visible
cargo test -- --nocapture

# Run specific test (full path required)
cargo test module_name::test_function_name

# Run tests matching pattern
cargo test security
cargo test config_parses
```

## Lint & Format

```bash
# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy lints
cargo clippy

# Clippy with strict warnings (CI standard)
cargo clippy -- -D warnings
```

## Code Style

### Imports
Use `use` statements for traits and types. Group imports:
1. External crates (tokio, serde, thiserror, etc.)
2. Standard library (std::*, core::*)
3. Local modules (`crate::`, `super::`)

### Naming Conventions
- **Modules**: `snake_case` (e.g., `channel/`, `security/`)
- **Types**: `PascalCase` (e.g., `AppConfig`, `SecurityLayer`)
- **Functions/Variables**: `snake_case` (e.g., `load_config`, `check_command`)
- **Traits**: `PascalCase` (e.g., `Agent`, `Channel`, `Memory`)
- **Constants**: `SCREAMING_SNAKE_CASE` or `PascalCase` for const functions

### Error Handling
Use `thiserror` for custom errors with `#[derive(Error, Debug)]`:
```rust
#[derive(Error, Debug)]
pub enum ModuleError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid configuration: {0}")]
    Config(String),
}
pub type Result<T> = std::result::Result<T, ModuleError>;
```
Use `anyhow::Result<()>` for application-level errors that don't need specific variants.

### Async Traits
Use `async_trait::async_trait`:
```rust
#[async_trait]
pub trait MyTrait {
    async fn async_method(&self) -> Result<()>;
}
```

### Configuration Pattern
Serializable structs with `#[serde(default)]` and `Default` impl:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ComponentConfig {
    pub enabled: bool,
    pub path: PathBuf,
}
impl Default for ComponentConfig {
    fn default() -> Self {
        Self { enabled: true, path: PathBuf::from(".borgclaw/component") }
    }
}
```

### Module Organization
```rust
//! Module-level documentation

pub mod submodule;
pub use submodule::{Type, Trait};

mod internal;
```

### Key Dependencies
- **Async**: `tokio` with `full` feature
- **Serialization**: `serde`, `serde_json`, `toml`
- **Errors**: `thiserror`, `anyhow`
- **Logging**: `tracing`, `tracing-subscriber`
- **Web**: `axum`, `tokio-tungstenite`
- **DB**: `sqlx` with sqlite feature

## Testing Guidelines
- Place tests in `#[cfg(test)]` modules within source files
- Use descriptive test names: `fn security_config_parses_documented_contract_shape()`
- Tests verify TOML contract shapes and behavior

## Security Checklist
- WASM sandbox for untrusted code
- Command blocklist for dangerous operations
- No hardcoded credentials (use `${VAR}` placeholder pattern)
- File operations respect workspace policy
- Secrets not logged or exposed in errors

## Git Workflow

### CRITICAL RULES - STRICTLY ENFORCED

> **⚠️ IMPORTANT: NEVER push changes directly to main or master**
> 
> **ALL changes MUST go through feature branches and pull requests**
> 
> **NO exceptions. NO direct pushes. NO matter how small the fix.**

### Branch Naming
- Feature branches: `TICKET-<number>-<short-description>` (e.g., `TICKET-123-fix-backup-import`)
- Use kebab-case for branch names
- Bug fix branches: `TICKET-<number>-bugfix-<description>`
- Hotfix branches: `hotfix-<description>`

### Commit Messages
```
[AREA] Brief description (50 chars or less)

- Detailed change explanation
- Another change detail
- Reference to issue/ticket
```

### Working with Feature Branches (REQUIRED WORKFLOW)

```bash
# 1. ALWAYS start from latest main
git checkout main
git pull origin main

# 2. ALWAYS create a feature branch
git checkout -b TICKET-<number>-<description>

# 3. Make changes and commit frequently
git add .
git commit -m "[AREA] Your commit message"

# 4. Push branch to remote
git push -u origin TICKET-<number>-<description>

# 5. Create PR via GitHub CLI
gh pr create --title "[TICKET-<number>] Your PR title" --body "$(cat <<'EOF'
## Summary
- Bullet point of changes
- Another bullet

## Testing
- [ ] Tests pass
- [ ] Clippy passes
- [ ] Format checked
EOF
)"
```

### PR and Merge Rules (STRICTLY ENFORCED)

| Rule | Enforcement |
|------|-------------|
| **NEVER** push to main/master directly | 🔴 BLOCKED - Will not work |
| **NEVER** force push to main/master | 🔴 BLOCKED |
| **NEVER** skip CI or hooks | 🔴 BLOCKED |
| **NEVER** amend pushed commits | 🔴 BLOCKED |
| **ALWAYS** use feature branches | ✅ REQUIRED |
| **ALWAYS** create PRs for all changes | ✅ REQUIRED |
| **ALWAYS** get review before merge | ✅ REQUIRED |
| Use "Squash and merge" for feature branches | ✅ RECOMMENDED |
| Use "Merge commit" for release branches | ✅ RECOMMENDED |

### Quick Fixes Workflow

For even small one-line fixes:
```bash
git checkout -b TICKET-<number>-tiny-fix
# make the fix
git add . && git commit -m "[AREA] Fix typo in doc"
git push origin TICKET-<number>-tiny-fix
gh pr create --title "[TICKET-<number>] Fix typo" --body "Quick fix"
```

### Before Submitting
```bash
cargo test
cargo fmt --check
cargo clippy -- -D warnings
git status
```
