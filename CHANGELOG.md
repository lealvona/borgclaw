# Changelog

## 1.6.0 - 2026-03-19

### Features

**Memory System Tools**
- **Memory Management**: Full CRUD operations for memory entries
  - memory_delete - Delete memory entries by ID
  - memory_keys - List all memory keys
  - memory_groups - List all memory groups
  - memory_clear_group - Clear all memories in a group
- **Solution Memory**: Store and find problem-solution patterns
  - solution_store - Save problem-solution pairs with tags
  - solution_find - Search solutions by query

**Audio Processing**
- **STT URL Support**: Transcribe audio directly from URLs
- **TTS Streaming**: Stream large TTS outputs to files

### Release Policy

- `1.6.0` is the current release line
- New work after `1.6.0` should continue on feature branches

## 1.5.0 - 2026-03-19

### Features

**Skills Operational Completeness**
- **Google Workspace**: Added delete_email, trash_email, update_event, delete_event operations
- **Browser**: Added go_back, go_forward, reload navigation tools
- **Memory Management**: Exposed missing memory operations as agent tools
  - memory_delete - Delete a memory entry by id
  - memory_keys - List all memory keys
  - memory_groups - List all memory groups
  - memory_clear_group - Clear all memories in a group

**GitHub Tools Expansion**
- Added update_file, delete_file, and close_issue operations

**Transport Restart Recovery**
- Telegram channel with last_update_id tracking for message catch-up
- Signal channel with last_timestamp tracking for deduplication

### Release Policy

- `1.5.0` is the current release line
- New work after `1.5.0` should continue on feature branches

## 1.4.0 - 2026-03-19

### Features

**Runtime Status Command**
- `runtime` command showing comprehensive background task status
- Displays scheduler, heartbeat, and sub-agent tasks in one view
- Shows provider configuration and credential status

**Doctor Improvements**
- MCP server count visible in doctor output

### Release Policy

- `1.4.0` is the current release line
- New work after `1.4.0` should continue on feature branches

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
