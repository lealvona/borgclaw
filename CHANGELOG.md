# Changelog

## 1.0.0 - 2026-03-19

First MVP release.

### Highlights

- Provider-backed shared agent runtime across CLI, gateway, webhook, and channel routing.
- Structured WebSocket gateway auth, pairing, heartbeat, and error events.
- SQLite memory with metadata round-trip, group isolation, solution memory, and session compaction.
- Shared scheduler, heartbeat, and sub-agent execution with persistence, retries, dead-letter state, and restart recovery coverage.
- Typed skill configuration and shared runtime execution for GitHub, Google, browser, STT, TTS, image, QR, URL, MCP, and WASM plugins.
- Security hardening across approvals, prompt-injection defense, secret storage, vault integration, command policy, workspace policy, and transport ingress handling.
- Operator-facing onboarding, `status`, and `doctor` coverage for providers, channels, memory, scheduler recovery, skills, vaults, and MCP servers.

### Included Late MVP Hardening

- Aggregate MCP doctor failure summaries.
- Explicit disabled-channel enforcement in the shared router.
- Disabled WebSocket upgrades rejected at the gateway boundary.
- Webhook `429` responses now include `Retry-After`.

### Release Policy

- `1.0.0` is the current release line.
- New work after `1.0.0` should continue on feature branches.
- Do not cut another release until the coordinated `1.1.0` release.
