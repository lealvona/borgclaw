# BorgClaw Roadmap

## Summary

Implement the remaining documented BorgClaw feature set in a security-first sequence. The goal is to make the runtime match the README and `docs/` contract without breaking the existing config and crate layout.

Status note as of March 11, 2026:
- Phase 1 is largely complete.
- Phase 2 is largely complete.
- Phase 3 is largely complete. Memory, session compaction, scheduler execution, sub-agent persistence, heartbeat persistence, retry/dead-letter behavior, and background-context inheritance are landed; explicit workspace/security policy depth still remains.
- Phase 4 is partially complete, with substantial shared tool/runtime coverage across documented skill families, but broader operational completeness still remains.
- Phase 5 is partially complete, with security enforcement substantially improved but security-pipeline unification and operator UX still behind the documented end state.

## Phase 1: Core Runtime

- Replace the echo-only `SimpleAgent` path with provider-backed chat execution.
- Add a provider abstraction for OpenAI, Anthropic, Google, and Ollama.
- Load system prompt content from `agent.soul_path` when configured.
- Persist session history in the agent and feed that history into provider calls.
- Convert built-in tools from metadata-only definitions into executable runtime actions.
- Centralize approval checks for dangerous tools through `SecurityLayer`.

## Phase 2: Shared Routing

- Introduce a core message router that owns agent invocation, approvals, and session mapping.
- Move CLI, Telegram, webhook, and gateway onto the same router path.
- Enforce pairing and DM policy consistently across transports.
- Upgrade the WebSocket gateway to use authenticated sessions and structured response events.

## Phase 3: Memory and Scheduling

- Finish SQLite memory initialization, metadata round-tripping, and group isolation.
- Implement session compaction before provider calls when transcripts exceed limits.
- Complete solution memory recall and heartbeat scheduler execution.
- Integrate sub-agent background execution with status tracking.

Recent landed work in this phase:
- Heartbeat engine state gating, manual-run state updates, and disable/re-enable consistency.
- Heartbeat background loop startup with duplicate-start protection.
- Heartbeat config is now owned by the shared runtime, with auto-started persistent engine state when enabled.
- Heartbeat task persistence across engine reconstruction.
- Heartbeat retry rescheduling and dead-letter handling for exhausted tasks.
- Sub-agent concurrency limits, cancellation precedence, memory tool policy enforcement, parent-context inheritance, and persisted task state.
- Sub-agent retry rescheduling, retry-backoff visibility, and dead-letter handling for exhausted tasks.
- Scheduler job initialization with stable `next_run` state for cron, interval, and one-shot jobs.
- Scheduler loop startup, concurrency limits, timeout enforcement, due-job execution, and bounded run history.
- Scheduler retry rescheduling and dead-letter handling for exhausted jobs.
- Scheduled jobs can now dispatch built-in tool calls through the shared runtime, not just synthetic message actions.
- Background scheduled and sub-agent tool selection now reject approval-gated tools when interactive approval is unavailable.
- Tool execution now carries conversation context so memory tools respect `group_id`, and scheduled tool jobs inherit originating sender/session metadata.

## Phase 4: Skills and Integrations

- Complete operational paths for GitHub, Google Workspace, browser automation, STT, TTS, image, QR, URL, MCP, and WASM plugins.
- Normalize skill execution results and error handling behind a shared runtime interface.
- Route plugin and MCP capabilities into the same tool execution layer used by the agent.

Recent landed work in this phase:
- WASM plugin manifests now accept the documented TOML permission table shape, default invocations honor `entry_point`, and non-exported function calls are rejected.
- MCP stdio server commands now pass through the shared command policy instead of bypassing the blocklist.

## Phase 5: Hardening and UX

- Make `SecurityLayer` authoritative for approvals, pairing, blocklists, prompt-injection defense, and secret scanning.
- Finish secret storage and vault integration.
- Expand onboarding, `status`, and `doctor` to reflect provider, channel, memory, and integration state.
- Add focused unit and integration coverage for each completed phase.

Recent landed work in this phase:
- Plugin and MCP invocation now use the same approval flow as command execution in supervised mode, instead of bypassing approval semantics entirely.
- Shared file-path tools now honor a typed workspace policy with forbidden paths and additive allowed roots, and scheduled tool execution inherits that policy through the shared runtime path.
- Shared command policy now supports an additive allowlist, and that policy is enforced consistently for foreground command execution, scheduled command execution, and MCP stdio server commands.
- MCP stdio transport setup now inherits injected secret environment and resolves `${VAR}` placeholders through the security layer instead of relying on raw configured env only.
- MCP client coverage now includes a local stdio-stub flow for `initialize`, `tools/list`, and `tools/call`, matching the plan’s stub-server test requirement.
- Plugin manifest file read/write permissions now preserve declared paths and are enforced against the shared workspace policy instead of being treated as mostly informational.
- Plugin coverage now includes a real loaded-WASM runtime path through `plugin_invoke`, plus a permission-denial case that proves workspace policy is enforced before execution.
- Gateway coverage now includes black-box WebSocket and webhook tests for the documented welcome/pairing/auth event flow plus webhook health, secret enforcement, and rate-limiting behavior.
