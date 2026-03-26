# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

BorgClaw is a Rust personal AI agent framework. Workspace with 3 crates:

- **borgclaw-core** — Library: agent runtime, channels, memory, security, skills, scheduler, MCP client, config
- **borgclaw-cli** — Binary: REPL, CLI commands, onboarding wizard
- **borgclaw-gateway** — Binary: Axum WebSocket gateway with health/webhook endpoints

Edition 2021, MSRV 1.75+, Tokio async runtime.

## Build / Test / Lint

```bash
cargo build                                    # workspace
cargo build -p borgclaw-core                   # single crate
cargo build --release                          # optimized (LTO, codegen-units=1)

cargo test                                     # all tests
cargo test -p borgclaw-core                    # single crate
cargo test -p borgclaw-core scheduler::tests::catch_up_policy_default_is_skip  # specific test
cargo test security                            # pattern match
cargo test -- --nocapture                      # show stdout
cargo test -- --include-ignored                # run slow/ignored tests

cargo fmt --check && cargo clippy -- -D warnings   # CI gate (zero warnings policy)
```

Pre-commit verification: `cargo test && cargo fmt --check && cargo clippy -- -D warnings`

## Architecture

**Dependency flow**: `borgclaw-cli` and `borgclaw-gateway` both depend on `borgclaw-core`.

**Central state**: `AppState` in `borgclaw-core/src/lib.rs` holds `Arc<RwLock<T>>` for config, agent, memory, scheduler, security, and channels. This is the composition root shared across async boundaries.

**Core modules** (all in `borgclaw-core/src/`):

| Module | Key types | Purpose |
|--------|-----------|---------|
| `agent/` | `Agent` trait, `SimpleAgent`, `Tool`, `AgentContext` | Trait-based agent with tool execution and sessions. `tools.rs` is the largest file (~273KB) containing 70+ built-in tools. |
| `channel/` | `Channel` trait, `MessageRouter` | Multi-channel messaging: Telegram (teloxide), Signal (JSON polling), Webhook, CLI, WebSocket |
| `memory/` | `Memory` trait, `SqliteMemory`, `SessionMemory`, `HeartbeatEngine` | SQLite+FTS5 hybrid search, per-group sessions with auto-compaction, heartbeat scheduled tasks |
| `security/` | `SecurityLayer`, `WasmSandbox`, `SecretStore`, `PairingManager`, `AuditLogger` | Defense-in-depth: WASM sandbox (wasmtime), command blocklist, prompt injection defense, encrypted secrets (ChaCha20-Poly1305), vault integration (Bitwarden/1Password), audit logging |
| `skills/` | Individual integrations | GitHub, Google Workspace, Browser (Playwright/CDP), STT, TTS, Image gen, QR, URL shortener, WASM plugins, MCP |
| `scheduler/` | `SchedulerTrait`, `Scheduler`, `Job`, `CatchUpPolicy` | Cron/interval/one-shot scheduling with disk persistence, retry/dead-letter, catch-up recovery |
| `mcp/` | MCP client | Model Context Protocol with Stdio, SSE, WebSocket transports |
| `config/` | `AppConfig`, `SecurityConfig`, `ChannelConfig` | TOML config with `#[serde(default)]` + `Default` impls, legacy aliases |

## Code Conventions

**Error handling**: `thiserror` with `String` payloads (not `#[from]`). Convert across boundaries with `.map_err(|e| MyError::Variant(e.to_string()))`. Use full `Result<T, ConcreteError>` inline, no type aliases.

**Async patterns**: `#[async_trait]` with `Send + Sync` bounds. Shared state via `Arc<RwLock<T>>`.

**Config structs**: `#[serde(default)]` + `Default` impl. Legacy field names via `#[serde(alias = "old_name")]`.

**Constructors**: `pub fn new(id: impl Into<String>) -> Self` with chainable `with_*` methods.

**Module layout**: Doc comment at top, `mod`/`pub use` declarations, public API, then `#[cfg(test)] mod tests` at bottom. `lib.rs` selectively re-exports.

**Import order**: (1) local/external crates, (2) std, (3) tracing.

**Testing**: Descriptive names (`fn config_parses_documented_contract_shape()`). Async tests use `#[tokio::test]`. Integration tests bind `127.0.0.1:0`. Temp files use UUID prefixes. Slow tests marked `#[ignore]`.

## Scope Management

Complete the current task ONLY. Do not expand scope unless explicitly requested.

- Execute only what the current ticket requires
- If you discover related issues: document them as follow-up, don't fix them
- Only refactor code directly touched by the current task
- Test only changed functionality; note missing coverage for follow-up
- One task, one change, one PR. Everything else is future work.

## Git Workflow

**Remote setup**: This repo uses fork-based development. The personal fork remote is `lealvona`; upstream push is disabled (`NO_PUSH`). Verify with `git remote -v`.

**Branch naming**: `TICKET-<number>-<description>` (e.g., `TICKET-060-scheduler-recovery`)

**Commit format**: `[TICKET-###] Brief description` — imperative mood, under 72 chars

**PR process**: Always create PRs as **draft** first (`gh pr create --draft`). Mark ready only after CI passes and self-review is complete.

**Protected branches**: Never push directly to `main`, `master`, `dev`, `prod`, or any upstream remote. All changes go through feature branches and PRs.

**Before any push**: Require explicit user approval. Run pre-commit verification first.

## Key Design Decisions

- Docs are treated as product contract — implement to match docs, not the other way around (see `DECISION_LOG.md` D001)
- Security-first delivery sequence — security and correctness before integration breadth (D002)
- Config file: `~/.config/borgclaw/config.toml`; credentials use `${VAR}` env placeholders
- Release profile uses LTO + single codegen-unit for binary size

## Security Checklist

- No hardcoded credentials — use `${VAR}` placeholder pattern in config
- Secret leak detection via regex on tool outputs; secrets never logged or in error messages
- WASM sandbox for untrusted code execution
- Command blocklist for dangerous operations
- Audit logging after every tool execution

## Upstream Inspirations

The project tracks patterns from upstream codebases documented in `docs/inspirations.md`. Key rule: never remove "What BorgClaw should copy" items until actually implemented and verified in both codebase and `docs/implementation-status.md`.

## References

- `AGENTS.md` — Full agent working instructions (coding standards, git workflow, security checklist)
- `DECISION_LOG.md` — Architectural decision records
- `docs/` — Feature documentation (channels, memory, skills, security, integrations)
- `docs/inspirations.md` — Upstream patterns to adopt (preserve unimplemented items)
- `docs/implementation-status.md` — Feature completion tracking
- `scripts/` — Bootstrap, doctor, onboarding, REPL, gateway launcher scripts
- `.opencode/skills/` — Project-specific skill directives (deployment-onboarding, inspiration-study, code-quality, git-workflow)
