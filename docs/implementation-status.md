# Implementation Status

This document tracks BorgClaw against the current README and `docs/` contract.

It is additive status only. It does not narrow the documented feature set.

Last reviewed: March 10, 2026

## Status Matrix

| Area | Status | Notes |
|---|---|---|
| Provider-backed agent runtime | `complete` | Shared routed provider execution is landed. |
| Shared channel routing | `complete` | CLI, gateway, webhook, and channel policy flow through the shared router. |
| WebSocket gateway auth/events | `partial` | Auth, pairing, error, and heartbeat events exist; broader control-plane UX is still thin. |
| SQLite memory + group isolation | `complete` | Metadata round-trip, isolation, recall, and compaction are implemented. |
| Solution memory | `complete` | Documented public structs and search helpers are aligned. |
| Heartbeat engine | `partial` | Documented task/handler surface exists. Engine state gating, background loop startup, enable/disable consistency, manual-run state updates, and persisted task snapshots are landed; richer operator ergonomics and retry/dead-letter behavior remain. |
| Sub-agent coordinator | `partial` | Spawn/status/result flow exists. Concurrency limits, cancellation precedence, memory policy enforcement, parent-context inheritance, and persisted task snapshots are landed; retry/dead-letter behavior and deeper execution policy inheritance remain. |
| Security config contract | `complete` | Documented TOML shape parses and core enforcement exists. |
| Secret storage + vault | `partial` | Encrypted secrets and vault clients exist; onboarding/auth UX is still mixed. |
| Skill registry lifecycle | `partial` | Local install, remote `SKILL.md`, and GitHub-backed listing exist; packaging/publishing remain planned. |
| MCP client | `complete` | Documented transports and client API are aligned. |
| GitHub skill client | `partial` | Core client surface exists; shared tool/runtime integration is still incomplete. |
| Google skill client | `partial` | Gmail/Drive/Calendar facade exists; shared tool/runtime integration is still incomplete. |
| Browser skill client | `partial` | Playwright/CDP surface exists; shared tool/runtime integration is still incomplete. |
| STT/TTS/Image/QR/URL skills | `partial` | Client APIs exist; typed config/runtime integration is still being completed. |
| Onboarding contract | `partial` | Provider registry and `.env` generation exist; secure-storage-first onboarding and full live model refresh remain incomplete. |

## Current Priorities

1. Finish typed skill config and runtime integration for every documented `skills.*` section.
2. Make onboarding/auth the authoritative setup path for providers and integrations.
3. Harden scheduler, heartbeat, and subagent execution with inherited security/workspace policy.
4. Expand end-to-end coverage for gateway, onboarding, MCP, and skills.

## Temporary Limitations That Must Stay Explicit

- Skill registry publishing/package workflow is still planned-only.
- Remote skill installs currently persist manifest content, not companion assets.
- Several skill families have library clients and config support before full agent-tool exposure.
- Background execution now persists scheduler run history, heartbeat task state, and sub-agent task state locally. Scheduler retry/dead-letter semantics are landed; heartbeat and sub-agent retry/dead-letter semantics are still not implemented.
