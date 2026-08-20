# Implementation Status

This document tracks BorgClaw against the current README and `docs/` contract.

It is additive status only. It does not narrow the documented feature set.

Last reviewed: April 1, 2026, with code verification against the current CLI/runtime surfaces.

Audit note:

- Most documented functionality is implemented and exercised.
- This document no longer claims blanket feature completeness.
- The current audited gaps are listed explicitly below.

## Status Matrix

| Area | Status | Notes |
|---|---|---|
| Provider-backed agent runtime | `complete` | Shared routed provider execution is landed, including encrypted named provider profile selection through `agent.provider_profile`. |
| Identity formats + transcript artifacts | `complete` | Markdown and AIEOS identity documents are supported, and assistant session messages can preserve structured reasoning/provider artifacts without changing visible channel text. |
| Shared channel routing | `complete` | CLI, gateway, webhook, and channel policy flow through the shared router. |
| WebSocket gateway auth/events | `complete` | Auth, pairing, error, heartbeat, and control-plane events are implemented. |
| Memory backends + group isolation | `complete` | SQLite, PostgreSQL + pgvector, and in-memory backends share metadata round-trip, isolation, recall, time-range filtering, history, procedural memory, sensitivity-aware access filtering, and compaction-facing runtime integration. |
| Solution memory | `complete` | Documented public structs and search helpers are aligned. |
| Heartbeat engine | `complete` | Core engine, persistence, retries, enable/disable flow, operator visibility, and manual CLI trigger execution against persisted state are landed. |
| Scheduler | `complete` | Execution loop, timeout/concurrency policy, retries/dead-letter behavior, persisted job state, bounded run history, restart-recovery coverage, catch-up/recovery semantics, and full operator CRUD (list, show, create, delete, pause, resume) are all landed. |
| Sub-agent coordinator | `complete` | Spawn/status/result flow, concurrency limits, cancellation precedence, memory policy enforcement, parent-context inheritance, persisted task snapshots, retry/dead-letter behavior, workspace policy enforcement, and audit logging are all landed. |
| Security config contract | `complete` | Documented TOML shape parses and core enforcement exists. |
| Docker command sandbox | `complete` | Typed `security.docker` config, shared command routing, context-specific hardening for local/remote/background execution, diagnostics, installer helpers, and command-runtime integration are landed. |
| Secret storage + vault | `complete` | Encrypted secrets, vault clients (Bitwarden, 1Password), CLI commands (list, set, delete, check), and onboarding/auth UX are complete. |
| Skill registry lifecycle | `complete` | Local directory install, local `.tar.gz` install, GitHub-backed listing, archive-backed GitHub repo/registry installs, remote archive install-by-URL, packaging, publishing, inspection, version compatibility, and direct manifest installs with explicit `files:` support, adjacent `SKILL.files.json` sidecar discovery, or manifest-directory discovery are implemented. |
| MCP client | `complete` | Documented transports (Stdio, SSE, WebSocket) and client API are aligned. |
| GitHub skill client | `complete` | Core client surface, shared tool/runtime coverage, local shared-runtime happy-path coverage, and operational completeness are landed. |
| Google skill client | `complete` | Gmail/Drive/Calendar facade and shared-runtime coverage are landed. Browser OAuth completion works, Telegram receives a direct callback notification, active WebSocket sessions receive live completion events, CLI-originated flows receive a live in-band completion notice through the persisted completion store, and Google tokens are scoped per caller identity instead of one shared token path. |
| Google OAuth callback routing | `complete` | Pending OAuth state now survives tool-runtime to gateway handoff. Telegram, browser `window.opener.postMessage`, active WebSocket sessions, and CLI-originated flows all receive completion signaling, with OAuth completions persisted by `state` for non-socket flows. |
| Browser skill client | `complete` | Playwright/CDP surface, core shared runtime actions, local shared-runtime bridge coverage, and operational completeness are landed. |
| STT/TTS/Image/QR/URL skills | `complete` | Typed config, shared runtime coverage, deeper integration coverage, and operational completeness are landed. |
| Onboarding contract | `complete` | Provider registry, secure-store-backed integration setup, `.env` generation, operator UX, and live-auth flows are complete. |
| Per-channel proxy URL (Telegram) | `complete` | `proxy_url` in `ChannelConfig` is validated and applied when building the Telegram reqwest client. Signal is explicitly N/A (subprocess transport). |
| WebSocket connection limit | `complete` | Gateway rejects new WebSocket upgrades when `connections_active` exceeds `channels.websocket.extra.max_connections` (default 50) and returns HTTP 429. |
| Compaction lifecycle notification | `complete` | `AgentResponse.metadata` carries `compacted: "true"` and `compaction_pass: "N"` when a session compaction is triggered, and the event is logged at INFO level. |
| Install/uninstall helpers | `complete` | `scripts/install.sh` copies binaries to `~/.local/bin` and updates PATH. `scripts/uninstall.sh` removes binaries, PATH entries, and optionally config/state (`--purge`). |

## Completed Features Summary

### Background Execution & Persistence
- ✅ Scheduler persistence across restarts with recovery of in-flight `Running` jobs
- ✅ Heartbeat task persistence across engine reconstruction
- ✅ Sub-agent task persistence with state snapshots
- ✅ Retry/dead-letter semantics for scheduler, heartbeat, and sub-agent
- ✅ Signal polling with duplicate-start rejection and graceful shutdown
- ✅ Telegram polling with duplicate-start rejection and graceful shutdown
- ✅ Webhook channel with duplicate-start rejection, shutdown tracking, and state persistence
- ✅ Scheduler catch-up/recovery for missed jobs with configurable policies
- ✅ Dead-letter management with visibility via `self-test` and CLI

### Security & Policy
- ✅ Security pipeline unified across all execution paths (foreground tools, sub-agents, heartbeat, MCP)
- ✅ SSRF protection with allowlist/blocklist patterns
- ✅ Workspace policy enforcement for file paths and dangerous tools
- ✅ Command blocklist with additive allowlist support
- ✅ Approval gates for destructive operations
- ✅ Prompt injection defense with pattern detection
- ✅ Secret leak detection and redaction
- ✅ Audit logging for sub-agents and heartbeat
- ✅ Optional Docker command sandbox with typed image/network/mount policy
- ✅ Encrypted secrets (ChaCha20-Poly1305)
- ✅ Vault integration (Bitwarden primary, 1Password secondary)

### CLI & Operator UX
- ✅ Self-test command with dead-letter state detection
- ✅ Backup export/import/verify workflows
- ✅ Full schedule management: `list`, `show`, `create`, `delete`, `pause`, `resume`
- ✅ Heartbeat management: `list`, `show`, `enable`, `disable`, `trigger`
- ✅ Sub-agent management: `list`, `show`, `cancel`
- ✅ Secret management: `list`, `set`, `delete`, `check`
- ✅ Doctor with aggregate MCP failure summaries
- ✅ Runtime status showing persisted background state

### Channels & Gateway
- ✅ Shared router with explicit disabled channel rejection
- ✅ WebSocket gateway with disabled upgrade rejection
- ✅ Webhook rate limiting with `Retry-After` metadata
- ✅ Request size enforcement and redacted error responses
- ✅ Black-box WebSocket and webhook tests

### Skills & Integrations
- ✅ Skill packaging with validation (`borgclaw skills package`)
- ✅ Skill publishing (`borgclaw skills publish`)
- ✅ Skill inspection (`borgclaw skills inspect`)
- ✅ Version compatibility checking (semver)
- ✅ Local packaged skill install via `borgclaw skills install ./skill.tar.gz`
- ✅ Remote archive install-by-URL
- ✅ GitHub-backed remote installs now extract companion files instead of persisting only `SKILL.md`
- ✅ Direct `SKILL.md` installs can fetch companion files via manifest `files:`, adjacent `SKILL.files.json`, or manifest-directory discovery when listings are available
- ✅ Bundled, managed, and workspace skill tiers are resolved explicitly with operator-visible precedence
- ✅ Skill requirement gates now evaluate binaries, env/secrets, and dotted config prerequisites
- ✅ Operator discovery surfaces now include `borgclaw skills list`, `search`, `info`, and `status`
- ✅ GitHub runtime happy-path coverage
- ✅ Google runtime happy-path coverage (Gmail, Drive, Calendar)
- ✅ Browser runtime happy-path coverage (Playwright/CDP)
- ✅ QR and URL shortening runtime coverage
- ✅ MCP doctor aggregation
- ✅ Plugin approval flow and workspace policy enforcement
- ✅ WASM sandbox with permission enforcement

### Provider & Runtime
- ✅ Tool-level retry policies with exponential backoff and jitter
- ✅ Provider-level automatic retry with 429 handling
- ✅ System prompt date/time injection
- ✅ Session compaction with token counting
- ✅ Hybrid search with HTTP embedding provider
- ✅ Explicit memory backend selection with SQLite, PostgreSQL + pgvector, and in-memory modes
- ✅ PostgreSQL hybrid recall with native full-text search, pgvector distance queries, and reciprocal-rank fusion
- ✅ Foreground PTY command execution plus persisted background command runtime with operator list/show/cancel surfaces
- ✅ Compaction lifecycle notification via `AgentResponse.metadata` (`compacted`, `compaction_pass`) and INFO log
- ✅ WebSocket connection limit enforced at gateway upgrade (HTTP 429 when limit exceeded)
- ✅ Per-channel `proxy_url` validated and applied for Telegram; documented as N/A for Signal
- ✅ User-level install (`scripts/install.sh`) and uninstall (`scripts/uninstall.sh`) helpers

## Known Gaps (audited April 1, 2026)

The following items are tracked accurately against the code. None narrow the documented feature set — they are known incomplete areas.

| Gap | Detail |
|---|---|
| Web-chat session history persistence | Sessions are in-memory only; gateway restart loses all active sessions. Tracked in roadmap Priority 1. |
| Gateway HTTP rate limiting | Per-webhook-trigger rate limits exist. A per-connection WebSocket limit is now enforced via `channels.websocket.extra.max_connections` (default 50). Global per-IP HTTP rate limiting is not yet implemented. Tracked in roadmap Priority 2. |
| Gateway authentication depth | Authentication is pairing-based. No bearer token or per-request API key validation on individual HTTP requests. Tracked in roadmap Priority 2. |
| Compaction lifecycle notifications | Compaction events are now surfaced in `AgentResponse.metadata` (`compacted: "true"`, `compaction_pass: "N"`) and logged at INFO level. Operator-facing notification endpoints are not yet implemented. |
| `proxy_url` for Signal channel | Architecturally N/A: Signal uses the `signal-cli` subprocess for all network I/O; there is no reqwest client to configure. `proxy_url` is honoured for Telegram. |
| Unified approval-callback ownership | OAuth callbacks notify the originating channel, but approvals are tool-scoped rather than session-scoped. |
| Password-gated secret-store unlock | Secret store is key-file backed. Interactive password-gated unlock is not yet implemented. Tracked in roadmap Priority 4. |

## Explicitly Declined

Not planned: AWS Bedrock provider support, Composio integration, Slack approval UI/buttons.

## Residual Release Risk

`sqlx-postgres v0.7.4` emits a Rust future-incompatibility warning for a future toolchain bump.

Final verification record: [FINAL_SHIPPABILITY_AUDIT_2026-03-30](FINAL_SHIPPABILITY_AUDIT_2026-03-30.md).

## Historical Note

Use [IMPLEMENTATION_AUDIT_2026-03-29](IMPLEMENTATION_AUDIT_2026-03-29.md) for the evidence-backed audit details and follow-up steps.
