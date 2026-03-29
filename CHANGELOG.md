# Changelog

> **Version Renumbering Notice (2026-03-28):**  
> BorgClaw has been re-versioned from the 1.x series to 0.x to accurately reflect its pre-1.0 status.  
> Historical releases 1.0.0 through 1.14.0 now correspond to 0.1.0 through 0.14.0.  
> The codebase remains unchanged; only version numbers were updated.

## Unreleased

### Features

- `borgclaw heartbeat trigger <id>` now executes persisted heartbeat tasks immediately through the heartbeat engine instead of acting as a placeholder-only CLI surface.
- `borgclaw skills install` now supports local packaged `.tar.gz` skill archives, remote `.tar.gz` archive URLs, archive-backed GitHub repo installs, archive-backed GitHub registry installs, and direct GitHub raw `SKILL.md` URLs with companion file extraction.
- Telegram polling now uses explicit update polling with persisted offsets, improving restart-state correctness beyond simple duplicate-start/shutdown handling.

## 0.14.0 - 2026-03-26 (was 1.14.0)

### Features

**Gateway Configuration UI (PR #233)**
- **Visual Configuration Editor** — Full web-based configuration management
  - Tabbed interface: Agent, Channels, Security, Memory, Skills
  - Real-time configuration updates with save/restore
  - Keyboard shortcuts: `Ctrl+,` to open, `Esc` to close
  - Edit configuration without touching config.toml directly

- **Enhanced API Endpoints**
  - `GET /api/config` — Returns full configuration with channels, security, memory, skills
  - `POST /api/config` — Update configuration programmatically
  - Configuration changes saved to disk automatically
  - Returns list of changes made and restart requirements

- **Dashboard Improvements**
  - Configuration menu in sidebar with "Edit Config" and "View Config (JSON)" options
  - Real-time skill status indicators (GitHub, Google, Browser)
  - Modal-based configuration editor with responsive design
  - Status feedback for save operations

- **Configuration Management via Web UI**
  - **Agent**: Provider selection (OpenAI, Anthropic, MiniMax, Kimi, etc.), model name, system prompt
  - **Channels**: WebSocket/Webhook toggle, port configuration, pairing settings
  - **Security**: Approval mode (ReadOnly/Supervised/Autonomous), defense toggles, command blocklist
  - **Memory**: Hybrid search toggle, session max entries
  - **Skills**: Auto-load toggle, skill configuration status

## 1.13.0 - 2026-03-26

### Polish

**README Redesign (TICKET-069)**
- New hypercube metaphor: "Six Faces. One Mind. Infinite Adaptation."
- Collective theme: assimilation of tools into unified consciousness
- Visual ASCII cube architecture diagram
- Six faces organization: Channels, Memory, Skills, Security, Providers, Runtime
- Cleaner feature tables with emoji indicators
- Polished layout with centered badges and footer
- "We are the sum of many technologies" positioning

## 1.12.0 - 2026-03-26

### Features

**New LLM Provider Support (PR #202)**
- **Kimi (Moonshot)** - Full OpenAI-compatible API support
  - Chat completions via `api.moonshot.cn/v1`
  - Live model fetching from API
  - 30 RPM rate limiting
  - Environment variable: `KIMI_API_KEY`
  
- **MiniMax** - Full OpenAI-compatible API support
  - Chat completions via `api.minimax.chat/v1`
  - Live model fetching from API
  - 30 RPM rate limiting
  - Environment variable: `MINIMAX_API_KEY`
  
- **Z.ai** - Full OpenAI-compatible API support
  - Chat completions via `api.z.ai/v1`
  - Live model fetching from API
  - 30 RPM rate limiting
  - Environment variable: `Z_API_KEY`

All three providers are now first-class citizens alongside OpenAI, Anthropic, Google, and Ollama.

## 1.11.0 - 2026-03-26

### Features

**Unified Security Pipeline (PR #198)**
- `SecurityLayer::run_input_pipeline()` - Unified entry point for injection check + leak redaction
- `SecurityLayer::run_output_pipeline()` - Unified output processing for secret leak detection
- `PipelineResult` struct captures blocked status, reason, sanitized text, and leak count
- Consistent security handling across foreground tools, sub-agents, heartbeat, and MCP paths
- HeartbeatEngine now supports optional audit logger with `with_audit_logger()` builder

**Skill Registry Polish (PR #197)**
- `borgclaw skills inspect <package.tar.gz>` - List archive contents and parse manifest
- `min_version` field in SkillManifest with semver compatibility checking (`is_compatible()`)
- Package command validates frontmatter and prints manifest info
- Publish pre-validates archive contains valid SKILL.md
- Pre-publish validation via `validate_archive_skill_md()`

**Secret Storage UX (PR #196)**
- `borgclaw secrets list` - List all stored secret keys
- `borgclaw secrets set <key>` - Store a secret in the encrypted store
- `borgclaw secrets delete <key>` - Remove a secret from the store
- `borgclaw secrets check <key>` - Verify a secret exists
- Encryption key backup notice during onboarding when secrets_encryption is enabled
- Exported `secrets_key_path` helper for deriving encryption key file location

**Sub-Agent Security (PR #195)**
- SubAgentCoordinator workspace policy enforcement
- Sub-agent inputs checked for prompt injection
- Sub-agent outputs scanned for secret leaks and redacted
- Workspace policy blocks dangerous tools (execute_command, delete, plugin_invoke) when workspace_only is enabled
- Audit logging for task start, completion, and failure

**Scheduler Recovery (PR #194)**
- Scheduler catch-up/recovery for missed jobs with configurable policies
- Dead-letter management for exhausted scheduler jobs
- `borgclaw schedules pause <id>` - Pause scheduled jobs
- `borgclaw schedules resume <id>` - Resume paused jobs
- Recovery state visibility via CLI and self-test

**Heartbeat Ergonomics (PR #193)**
- `borgclaw heartbeat list` - Show all persisted heartbeat tasks
- `borgclaw heartbeat show <id>` - Display task details
- `borgclaw heartbeat enable <id>` - Enable disabled tasks
- `borgclaw heartbeat disable <id>` - Disable tasks
- `borgclaw heartbeat trigger <id>` - CLI trigger surface added, but full runtime dispatch remains follow-up work

**Gateway Control Plane (PR #192)**
- `/api/metrics` endpoint with connection stats, message counts, auth metrics
- `/api/config` endpoint returning sanitized configuration
- Gateway metrics tracking: connections, messages sent/received, pairing requests, auth success/failure
- Control-plane UX for monitoring gateway health

**Security Documentation (PR #191)**
- Updated security docs with SSRF configuration
- Approval gates documentation

**Approval Gate Tests (PR #190)**
- Comprehensive tests for approval gate enforcement
- Fixed `needs_approval` logic for consistency

**SSRF Protection Tests (PR #189)**
- Comprehensive test coverage for SSRF protection
- Tests for allowlist/blocklist pattern matching

**SSRF Integration (PR #188)**
- SSRF protection integrated across all HTTP tools
- Blocks localhost, private IPs, internal addresses by default
- Configurable allowlist/blocklist patterns

**Google Drive Approval Gates (PR #187)**
- Fixed approval gate enforcement for Google Drive tools
- Delete and share operations now require approval

### Release Policy

- `1.11.0` is the current release line
- New work after `1.11.0` should continue on feature branches

---

## 1.10.2 - 2026-03-19

### Fixes

**Compiler Warnings Cleanup**
- Fixed all 21 compiler warnings (zero warnings remaining)
- Added workspace resolver = "2" for edition 2021
- Removed unused traits: `ChannelFactory`
- Removed unused structs: `WebhookChannelBuilder`, `WasmTool`
- Removed unused functions: `default_skill`
- Removed unused fields from internal response structs
- Fixed Signal API structs with proper serde rename attributes

**Completed Missing Functionality**
- `SseTransport`: Now uses stored client instead of creating new one per connection
- `WasmSandbox`: Implemented max_instances limit using Semaphore
- `OpListItem`: item_type now properly maps to VaultItemType
- `PendingPairing`: Removed redundant code field (was already HashMap key)

### Release Policy

- `1.11.0` is the **current release**
- Available at: https://github.com/lealvona/borgclaw/releases/tag/v1.11.0
- New work after `1.11.0` should continue on feature branches

## 1.10.2 - 2026-03-19

### Fixes

**Compiler Warnings Cleanup**
- Fixed all 21 compiler warnings (zero warnings remaining)
- Added workspace resolver = "2" for edition 2021
- Removed unused traits: `ChannelFactory`
- Removed unused structs: `WebhookChannelBuilder`, `WasmTool`
- Removed unused functions: `default_skill`
- Removed unused fields from internal response structs
- Fixed Signal API structs with proper serde rename attributes

**Completed Missing Functionality**
- `SseTransport`: Now uses stored client instead of creating new one per connection
- `WasmSandbox`: Implemented max_instances limit using Semaphore
- `OpListItem`: item_type now properly maps to VaultItemType
- `PendingPairing`: Removed redundant code field (was already HashMap key)

### Release Policy

- `1.10.2` was the previous release line
- Superseded by `1.11.0`

## 1.10.1 - 2026-03-19

### Fixes

**Deprecation Warnings**
- Fixed teloxide `Message::from()` deprecation warning → use `.from` field
- Fixed unused variable warnings in WASM security and STT modules

### Release Policy

- `1.10.1` is the current release line
- New work after `1.10.1` should continue on feature branches

## 1.10.0 - 2026-03-19

### Features

**Audit Logging Integration**
- Audit logger fully integrated with ToolRuntime
- All tool executions automatically logged with actor attribution
- Command execution audit logging (blocked, allowed, success, failure)
- Security events captured in real-time

### Release Policy

- `1.10.0` is the current release line
- New work after `1.10.0` should continue on feature branches

## 1.9.0 - 2026-03-19

### Features

**Security Audit Logging**
- Comprehensive audit logging system for security monitoring
- AuditLogger with buffered async JSONL output
- Event types: tool execution, approvals, commands, MCP, secrets, pairing
- Configurable log path (.local/logs/audit.jsonl)
- Helper methods for common security events

### Release Policy

- `1.9.0` is the current release line
- New work after `1.9.0` should continue on feature branches

## 1.8.0 - 2026-03-19

### Features

**Image Analysis Tools**
- **image_analyze** - Analyze images from URLs using GPT-4o-mini vision AI
- **image_analyze_file** - Analyze local image files from workspace
- Supports descriptive analysis, OCR, object identification
- Requires OPENAI_API_KEY for vision model access

### Release Policy

- `1.8.0` is the current release line
- New work after `1.8.0` should continue on feature branches

## 1.7.0 - 2026-03-19

### Features

**Gateway Health Endpoints**
- **GET /api/health** - Health status with dependency checks
  - Returns workspace and memory database status
  - HTTP 200 when healthy, 503 when unhealthy
- **GET /api/ready** - Readiness probe for load balancers
  - Returns workspace and skills path readiness
  - HTTP 200 when ready, 503 when not ready
- Kubernetes-compatible health check format

### Release Policy

- `1.7.0` is the current release line
- New work after `1.7.0` should continue on feature branches

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
- heartbeat trigger <id> - CLI placeholder for manual task trigger; full runtime dispatch remains follow-up work

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
