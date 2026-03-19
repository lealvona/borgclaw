# Implementation Status

This document tracks BorgClaw against the current README and `docs/` contract.

It is additive status only. It does not narrow the documented feature set.

Last reviewed: March 11, 2026

## Status Matrix

| Area | Status | Notes |
|---|---|---|
| Provider-backed agent runtime | `complete` | Shared routed provider execution is landed. |
| Shared channel routing | `complete` | CLI, gateway, webhook, and channel policy flow through the shared router. |
| WebSocket gateway auth/events | `partial` | Auth, pairing, error, and heartbeat events exist; broader control-plane UX is still thin. |
| SQLite memory + group isolation | `complete` | Metadata round-trip, isolation, recall, and compaction are implemented. |
| Solution memory | `complete` | Documented public structs and search helpers are aligned. |
| Heartbeat engine | `partial` | Documented task/handler surface exists. Engine state gating, background loop startup, shared-runtime ownership, enable/disable consistency, manual-run state updates, persisted task snapshots, and retry/dead-letter behavior are landed; richer operator ergonomics remain. |
| Sub-agent coordinator | `partial` | Spawn/status/result flow exists. Concurrency limits, cancellation precedence, memory policy enforcement, parent-context inheritance, persisted task snapshots, and retry/dead-letter behavior are landed; explicit workspace/security policy depth remains. |
| Security config contract | `complete` | Documented TOML shape parses and core enforcement exists. |
| Secret storage + vault | `partial` | Encrypted secrets and vault clients exist; onboarding/auth UX is still mixed. |
| Skill registry lifecycle | `partial` | Local install, remote `SKILL.md`, and GitHub-backed listing exist; packaging/publishing remain planned. |
| MCP client | `complete` | Documented transports and client API are aligned. |
| GitHub skill client | `partial` | Core client surface and substantial shared tool/runtime coverage are landed; broader operational completeness remains. |
| Google skill client | `partial` | Gmail/Drive/Calendar facade and shared runtime coverage are landed; broader operational completeness remains. |
| Browser skill client | `partial` | Playwright/CDP surface and core shared runtime actions are landed; broader operational completeness remains. |
| STT/TTS/Image/QR/URL skills | `partial` | Typed config and shared runtime coverage are landed; broader operational completeness and deeper integration coverage remain. |
| Onboarding contract | `partial` | Provider registry, secure-store-backed integration setup, and `.env` generation exist; operator UX and some live-auth flows still need completion. |

## Current Priorities

1. Harden restart-sensitive background execution: scheduler catch-up, polling-loop restart guards, and explicit health checks before restart.
2. Unify security-pipeline behavior across built-in tools, MCP, plugins, deferred execution, and the remaining non-webhook ingress paths.
3. Expand end-to-end coverage for gateway, onboarding, MCP, plugins, and remaining skill-family happy paths.
4. Continue operator UX/status/doctor parity for recovery, backup, and transport health surfaces.

## Temporary Limitations That Must Stay Explicit

- Skill registry publishing/package workflow is still planned-only.
- Remote skill installs currently persist manifest content, not companion assets.
- Several skill families have meaningful shared tool exposure, but not full operational completeness.
- Background execution now persists scheduler run history, heartbeat task state, and sub-agent task state locally. Scheduler, heartbeat, and sub-agent retry/dead-letter semantics are landed.
- CLI `status`/`doctor` now report persisted heartbeat and sub-agent recovery-state files, including task and dead-letter counts when available.
- Signal polling now has duplicate-start rejection and tracked shutdown for its background receive loop, but broader restart recovery behavior across transports is still incomplete.
- Telegram polling now has duplicate-start rejection and tracked shutdown for its background receive loop, but broader restart recovery behavior across transports is still incomplete.
