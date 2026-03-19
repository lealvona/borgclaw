# Changelog

## 1.3.0 - 2026-03-19

### Features

**Gateway Control-Plane**
- Added /api/metrics endpoint with connection stats, message counts, auth metrics
- Added /api/config endpoint returning sanitized configuration
- Gateway metrics tracking: connections, messages sent/received, pairing requests, auth success/failure

**Heartbeat Operator Commands**
- heartbeat list - Show all persisted heartbeat tasks
- heartbeat show <id> - Display task details
- heartbeat enable/disable <id> - Toggle task state
- heartbeat trigger <id> - Manual task trigger (placeholder)

**Sub-Agent Operator Commands**
- subagent list - Show all persisted sub-agent tasks
- subagent show <id> - Display task details
- subagent cancel <id> - Cancel running tasks

### Release Policy

- 1.3.0 is the current release line
- New work after 1.3.0 should continue on feature branches

## 1.2.0 - 2026-03-19

### Features

- **Schedule Management** - Full CRUD operations for scheduled tasks:
  - `schedules create` - Create jobs with cron, interval, or oneshot triggers
  - `schedules delete <id>` - Remove scheduled tasks
  - `schedules pause <id>` - Disable scheduled tasks
  - `schedules resume <id>` - Re-enable paused tasks
- **Backup Import/Restore** - Complete backup/recovery workflow:
  - `backup import <path>` - Restore runtime state from snapshot
  - `backup verify <path>` - Validate snapshots without importing
  - `--force` flag for non-interactive import
- **Security Visibility** - Enhanced `doctor` output with workspace policy status

### Release Policy

- `1.2.0` is the current release line
- New work after `1.2.0` should continue on feature branches

## 1.1.0 - 2026-03-19

### Features

- **Self-Test Command** (`borgclaw self-test`) - Exit 0 on pass, 1 on failure; surfaces dead-lettered scheduler/heartbeat/sub-agent state as failures
- **Backup Export** (`borgclaw backup export <path>`) - Snapshot persisted scheduler, heartbeat, and sub-agent state for operator recovery
- **Scheduler Status** (`borgclaw schedules list`) - List persisted scheduled tasks with recovery state
- **Scheduler Details** (`borgclaw schedules show <job-id>`) - Inspect individual persisted scheduled task metadata and history

### Release Policy

- `1.1.0` is the current release line
- New work after `1.1.0` should continue on feature branches

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
