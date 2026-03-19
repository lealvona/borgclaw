# Implementation Status

This document tracks BorgClaw against the current README and `docs/` contract.

It is additive status only. It does not narrow the documented feature set.

Last reviewed: March 19, 2026, with upstream inspiration follow-up through March 19, 2026

## Status Matrix

| Area | Status | Notes |
|---|---|---|
| Provider-backed agent runtime | `complete` | Shared routed provider execution is landed. |
| Shared channel routing | `complete` | CLI, gateway, webhook, and channel policy flow through the shared router. |
| WebSocket gateway auth/events | `partial` | Auth, pairing, error, and heartbeat events exist; broader control-plane UX is still thin. |
| SQLite memory + group isolation | `complete` | Metadata round-trip, isolation, recall, and compaction are implemented. |
| Solution memory | `complete` | Documented public structs and search helpers are aligned. |
| Heartbeat engine | `partial` | Documented task/handler surface exists. Engine state gating, background loop startup, shared-runtime ownership, enable/disable consistency, manual-run state updates, persisted task snapshots, and retry/dead-letter behavior are landed; richer operator ergonomics remain. |
| Scheduler | `partial` | Execution loop, timeout/concurrency policy, retries/dead-letter behavior, persisted job state, bounded run history, and restart-recovery coverage for persisted due jobs are landed; richer catch-up/recovery semantics still remain. |
| Sub-agent coordinator | `partial` | Spawn/status/result flow exists. Concurrency limits, cancellation precedence, memory policy enforcement, parent-context inheritance, persisted task snapshots, and retry/dead-letter behavior are landed; explicit workspace/security policy depth remains. |
| Security config contract | `complete` | Documented TOML shape parses and core enforcement exists. |
| Secret storage + vault | `partial` | Encrypted secrets and vault clients exist; onboarding/auth UX is still mixed. |
| Skill registry lifecycle | `partial` | Local install, remote `SKILL.md`, and GitHub-backed listing exist; packaging/publishing remain planned. |
| MCP client | `complete` | Documented transports and client API are aligned. |
| GitHub skill client | `partial` | Core client surface, substantial shared tool/runtime coverage, and local shared-runtime happy-path coverage are landed; broader operational completeness remains. |
| Google skill client | `partial` | Gmail/Drive/Calendar facade, shared runtime coverage, and local shared-runtime happy-path coverage are landed; broader operational completeness remains. |
| Browser skill client | `partial` | Playwright/CDP surface, core shared runtime actions, and local shared-runtime bridge coverage are landed; broader operational completeness remains. |
| STT/TTS/Image/QR/URL skills | `partial` | Typed config and shared runtime coverage are landed; broader operational completeness and deeper integration coverage remain. |
| Onboarding contract | `partial` | Provider registry, secure-store-backed integration setup, and `.env` generation exist; operator UX and some live-auth flows still need completion. |

## Current Priorities

1. Harden restart-sensitive background execution: scheduler catch-up, broader restart recovery, explicit health checks before restart, and operator-visible recovery semantics.
2. Unify security-pipeline behavior across built-in tools, MCP, plugins, deferred execution, and the remaining non-webhook ingress paths.
3. Expand end-to-end coverage for gateway, onboarding, MCP, plugins, and remaining skill-family happy paths, especially Google/browser and transport restart flows.
4. Continue operator UX/status/doctor parity for recovery, backup, schedule management, and transport health surfaces.

## Temporary Limitations That Must Stay Explicit

- Skill registry publishing/package workflow is still planned-only.
- Remote skill installs currently persist manifest content, not companion assets.
- Several skill families have meaningful shared tool exposure, but not full operational completeness.
- Background execution now persists scheduler job state and run history, heartbeat task state, and sub-agent task state locally. Scheduler, heartbeat, and sub-agent retry/dead-letter semantics are landed.
- CLI `status`/`doctor` now report persisted scheduler, heartbeat, and sub-agent recovery-state files, including task and dead-letter counts when available.
- Signal polling now has duplicate-start rejection and tracked shutdown for its background receive loop, but broader restart recovery behavior across transports is still incomplete.
- Telegram polling now has duplicate-start rejection and tracked shutdown for its background receive loop, but broader restart recovery behavior across transports is still incomplete.
- GitHub, Google, and browser now all have local shared-runtime happy-path coverage, but broader operational completeness across those families is still incomplete.
- Managed schedule/backup/recovery operator workflows remain incomplete even though core persisted scheduler state is now landed.
- CLI now exposes read-only `schedules list` and `schedules show <job-id>` surfaces backed by persisted scheduler state, but schedule mutation and richer recovery UX remain incomplete.
- CLI now exposes `backup export <path>` for persisted local runtime state, but restore/import workflows remain incomplete.
- CLI `doctor` now summarizes aggregate MCP reachability failures across configured servers, but deeper transport-facing retry/rate-limit diagnostics still remain.
- The shared router now enforces explicit channel disablement, so configured `enabled = false` remote channels are rejected instead of silently routing.
- The gateway now rejects disabled WebSocket upgrades at the transport boundary instead of accepting the connection and failing only later in message routing.
- Webhook `429` responses now include `Retry-After`, but broader transport-facing retry diagnostics and recovery guidance still remain.
