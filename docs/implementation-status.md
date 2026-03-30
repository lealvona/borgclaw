# Implementation Status

This document tracks BorgClaw against the current README and `docs/` contract.

It is additive status only. It does not narrow the documented feature set.

Last reviewed: March 29, 2026, with code verification against the current CLI/runtime surfaces.

Audit note:

- Most documented functionality is implemented and exercised.
- This document no longer claims blanket feature completeness.
- The current audited gaps are listed explicitly below.

## Status Matrix

| Area | Status | Notes |
|---|---|---|
| Provider-backed agent runtime | `complete` | Shared routed provider execution is landed. |
| Shared channel routing | `complete` | CLI, gateway, webhook, and channel policy flow through the shared router. |
| WebSocket gateway auth/events | `complete` | Auth, pairing, error, heartbeat, and control-plane events are implemented. |
| Memory backends + group isolation | `complete` | SQLite, PostgreSQL + pgvector, and in-memory backends share metadata round-trip, isolation, recall, and compaction-facing runtime integration. |
| Solution memory | `complete` | Documented public structs and search helpers are aligned. |
| Heartbeat engine | `complete` | Core engine, persistence, retries, enable/disable flow, operator visibility, and manual CLI trigger execution against persisted state are landed. |
| Scheduler | `complete` | Execution loop, timeout/concurrency policy, retries/dead-letter behavior, persisted job state, bounded run history, restart-recovery coverage, catch-up/recovery semantics, and full operator CRUD (list, show, create, delete, pause, resume) are all landed. |
| Sub-agent coordinator | `complete` | Spawn/status/result flow, concurrency limits, cancellation precedence, memory policy enforcement, parent-context inheritance, persisted task snapshots, retry/dead-letter behavior, workspace policy enforcement, and audit logging are all landed. |
| Security config contract | `complete` | Documented TOML shape parses and core enforcement exists. |
| Secret storage + vault | `complete` | Encrypted secrets, vault clients (Bitwarden, 1Password), CLI commands (list, set, delete, check), and onboarding/auth UX are complete. |
| Skill registry lifecycle | `complete` | Local directory install, local `.tar.gz` install, GitHub-backed listing, archive-backed GitHub repo/registry installs, remote archive install-by-URL, packaging, publishing, inspection, version compatibility, and direct manifest installs with explicit `files:` support, adjacent `SKILL.files.json` sidecar discovery, or manifest-directory discovery are implemented. |
| MCP client | `complete` | Documented transports (Stdio, SSE, WebSocket) and client API are aligned. |
| GitHub skill client | `complete` | Core client surface, shared tool/runtime coverage, local shared-runtime happy-path coverage, and operational completeness are landed. |
| Google skill client | `complete` | Gmail/Drive/Calendar facade, shared runtime coverage, local shared-runtime happy-path coverage, and operational completeness are landed. |
| Browser skill client | `complete` | Playwright/CDP surface, core shared runtime actions, local shared-runtime bridge coverage, and operational completeness are landed. |
| STT/TTS/Image/QR/URL skills | `complete` | Typed config, shared runtime coverage, deeper integration coverage, and operational completeness are landed. |
| Onboarding contract | `complete` | Provider registry, secure-store-backed integration setup, `.env` generation, operator UX, and live-auth flows are complete. |

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

## Temporary Limitations

## Historical Note

Use [IMPLEMENTATION_AUDIT_2026-03-29](IMPLEMENTATION_AUDIT_2026-03-29.md) for the evidence-backed audit details and follow-up steps.
