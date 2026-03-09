# BorgClaw Roadmap

## Summary

Implement the remaining documented BorgClaw feature set in a security-first sequence. The goal is to make the runtime match the README and `docs/` contract without breaking the existing config and crate layout.

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

## Phase 4: Skills and Integrations

- Complete operational paths for GitHub, Google Workspace, browser automation, STT, TTS, image, QR, URL, MCP, and WASM plugins.
- Normalize skill execution results and error handling behind a shared runtime interface.
- Route plugin and MCP capabilities into the same tool execution layer used by the agent.

## Phase 5: Hardening and UX

- Make `SecurityLayer` authoritative for approvals, pairing, blocklists, prompt-injection defense, and secret scanning.
- Finish secret storage and vault integration.
- Expand onboarding, `status`, and `doctor` to reflect provider, channel, memory, and integration state.
- Add focused unit and integration coverage for each completed phase.
