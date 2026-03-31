# Inspirations And Implementation Notes

This guide expands the short origin list in the README into an engineering reference.

Last reviewed against upstream repositories: March 31, 2026

Status note:
- Several gaps originally called out here are now partially or fully closed in BorgClaw.
- Use [Implementation Status](implementation-status.md) as the current source of truth for what remains open.
- Keep this document focused on upstream implementation ideas, not on narrowing BorgClaw's contract.
- The main current value of this document is extension quality: comparing BorgClaw's landed contracts against the stronger operational patterns upstream projects are shipping now.

Current BorgClaw disposition of major inspiration items:
- Implemented and verified in the runtime: provider-backed shared routing, structured gateway auth/events, scheduler/heartbeat/sub-agent persistence and recovery, typed workspace policy, unified approval flow for foreground/background tool execution, explicit memory backend selection with PostgreSQL + pgvector, tool-level retry, system prompt date/time injection, archive-backed skill installs, and the typed optional Docker command sandbox.
- Implemented and still worth iterating on: the Docker sandbox now covers `execute_command` through typed `security.docker` policy plus local/remote/background context overrides. Separate base and remote images are shipped, though operators can still choose whether to use image overrides in their config. The gateway also now has lightweight session-scoped outbound queues for live WebSocket OAuth completion delivery plus a richer dashboard renderer that treats content payloads, tool calls, and metadata as separate UI surfaces instead of flattening everything to plain text.
- Not yet implemented but explicitly tracked here: persisted browser chat/session history, stronger multi-tenant auth and workspace isolation, gateway auth/rate-limit hardening, transport-agnostic browser callback ownership beyond popup `postMessage`, and more explicit operator session-state surfaces (`idle`, `running`, `waiting`, `error`).
- Explicitly declined for BorgClaw: AWS Bedrock provider support, Composio integration, and Slack approval UI/buttons.

Use it for two things:

1. Understand which upstream project is the best model for a given BorgClaw subsystem.
2. Cross-check BorgClaw roadmap items, stubs, and rough edges against upstream implementations that already solved similar problems well.

## Index

Use this index when you need a fast path to the relevant inspiration area.

### Sandbox Improvements

- **Docker sandbox strategy**: [OpenClaw](#openclaw), [NanoClaw](#nanoclaw), and the [Docker appendix](#appendix-docker-sandbox-implementation-focus)
- **WASM sandbox and unified execution pipeline**: [IronClaw](#ironclaw), [OpenClaw](#openclaw), and [Security ownership and policy depth](#security-ownership-and-policy-depth)
- **Sandbox inheritance across subagent, heartbeat, and scheduled execution**: [NanoClaw](#nanoclaw), [PicoClaw](#picoclaw), and the [Docker appendix](#appendix-docker-sandbox-implementation-focus)
- **Typed sandbox policy/config design**: [ZeroClaw](#zeroclaw) and the [Docker appendix](#appendix-docker-sandbox-implementation-focus)
- **Implemented BorgClaw sandbox baseline**: [docs/security.md](security.md) for the current WASM contract, plus [Implementation Status](implementation-status.md) for what is actually complete

### Other Major Areas

- **Onboarding and control plane**: [OpenClaw](#openclaw)
- **Core Rust subsystem boundaries**: [ZeroClaw](#zeroclaw)
- **Background execution and retries**: [TinyClaw](#tinyclaw) and [PicoClaw](#picoclaw)
- **Skills and tool lifecycle**: [OpenClaw](#openclaw), [IronClaw](#ironclaw), and [Skills and integration lifecycle](#skills-and-integration-lifecycle)

## Upstream Follow-Up: March 25, 2026

Recent upstream movement:

- OpenClaw continues active development on Claude Code integration, GPT-5.4 + Gemini 3.1 Flash-Lite models, and critical scheduling fixes including dynamic date/time injection into system prompts (not just timezone) to prevent stale session errors. GitHub issue #26609 documents real scheduling errors where agents gave incorrect advice due to stale timestamps. SafeClaw project (OpenClaw alternative) gained traction with zero-cost deterministic approach using rule-based parsing instead of LLMs.
  Sources: [OpenClaw issues](https://github.com/openclaw/openclaw/issues/26609), [SafeClaw GitHub](https://github.com/princezuda/safeclaw)
- IronClaw `v0.5.x+` added AWS Bedrock LLM provider via native Converse API, context size logging before LLM prompts, tool-level retry with exponential backoff, sensitive JSON redaction helpers, native Composio tool integration for third-party apps, Feishu WebSocket channel, MiniMax LLM provider, context-LLM tool support, webhook reverse proxy for tunnel support, Slack approval buttons for DMs, and CLI skills list/search/info subcommands. Also improved: WASM bundle filename disambiguation, timezone conversion in time tool, message routing reliability, and extensive E2E test coverage.
  Sources: [IronClaw pulse](https://github.com/nearai/ironclaw/pulse), [IronClaw tests](https://github.com/nearai/ironclaw/tree/main/tests)
- TinyClaw released v0.0.19 with bug fixes for `agent_messages` and `signalDone`.
  Source: [TinyClaw releases](https://github.com/TinyAGI/tinyclaw/releases)

**New "What BorgClaw should copy" items** (added per upstream findings, preserving all existing items):

- OpenClaw: Dynamic current date/time injection into system prompts (not cached) to prevent stale session errors
- OpenClaw: Claude Code-style AI coding agent (searches, edits, tests, commits via CLI)
- IronClaw: AWS Bedrock LLM provider via native Converse API
- IronClaw: Tool-level retry with exponential backoff and jitter (BorgClaw has provider-level retry; tool-level retry per-tool is new)
- IronClaw: Native Composio tool integration for third-party app connectivity
- IronClaw: Context-LLM tool for context-aware operations
- IronClaw: Webhook reverse proxy support for tunnel scenarios
- IronClaw: CLI skills list/search/info subcommands
- IronClaw: Slack approval buttons for tool execution in DMs

## Upstream Follow-Up: March 31, 2026

Recent upstream movement:

- OpenClaw's late-March `main` branch continued tightening gateway and execution correctness with unified channel approval/routing work, safer gateway config-file opening without shell interpolation, diff-viewer proxy hardening, and memory scoping fixes.
  Sources: [OpenClaw commits](https://github.com/openclaw/openclaw/commits/main)
- ZeroClaw's latest release line pushed further on context-overflow recovery, auth rate limiting, per-session actor queues, richer session state tracking, web-chat persistence, and memory continuity controls.
  Source: [ZeroClaw releases](https://github.com/zeroclaw-labs/zeroclaw/releases)
- IronClaw `0.23.0` shipped complete multi-tenant isolation phases 2 through 4, hosted OAuth callbacks with proxy auth token support, Streamable HTTP `202 Accepted` MCP handling, and more defensive tool-call recovery.
  Sources: [IronClaw release 0.23.0](https://github.com/nearai/ironclaw/releases), [IronClaw staging commits](https://github.com/nearai/ironclaw/commits/staging)
- PicoClaw `v0.2.4` and the March 31 nightly added model-native search for OpenAI/Codex, configurable logging, and documented hot reload of workspace config files.
  Source: [PicoClaw releases](https://github.com/sipeed/picoclaw/releases)
- TinyAGI `v0.0.20` added a stronger office control plane and standardized provider credential fields around `api_key` and `oauth_token`.
  Source: [TinyAGI releases](https://github.com/TinyAGI/tinyagi/releases)
- NanoClaw's March 28 commits tightened Apple Container credential-proxy routing and explicitly prevented full message histories from being forwarded into container agents.
  Source: [NanoClaw commits](https://github.com/qwibitai/nanoclaw/commits/main)

**New "What BorgClaw should copy" items** (added per upstream findings, preserving all existing items):

- OpenClaw: Unify channel approval, callback routing, and restored-session auth into a single execution/session ownership path
- OpenClaw: Keep gateway file/config operations out of shell interpolation paths by construction
- ZeroClaw: Per-session actor queues so concurrent work and out-of-band events serialize cleanly back to the right session
- ZeroClaw: Gateway and auth rate limiting as a first-class operator/security control
- ZeroClaw: Explicit session state machine surfaces (`idle`, `running`, `error`, `waiting`) for operator introspection
- ZeroClaw: Persisted web chat/session history instead of page-lifetime-only browser state
- IronClaw: Hosted OAuth callback support with explicit callback auth token / reverse-proxy-aware validation
- IronClaw: Stronger multi-tenant auth/workspace isolation boundaries
- NanoClaw: Hard caps on history forwarded into isolated execution contexts
- TinyAGI: Standardized provider credential schema (`api_key`, `oauth_token`) across integrations
- PicoClaw: Operator-visible logger configuration and documented hot-reload behavior for workspace config

## Upstream Follow-Up: March 21, 2026

Recent upstream movement:

- OpenClaw `v2026.3.20` added Grok 4.20 reasoning/non-reasoning models to xAI catalog, SSRF guard coverage for URL credential bypass vectors, LINE webhook hardening with verified raw body, and PAIRING setup codes bootstrap-token only.  
  Source: [OpenClaw recent commits](https://github.com/openclaw/openclaw/commits/main)
- ZeroClaw `v0.5.4` added mem0 (OpenMemory) backend integration with history(), recall_filtered(), store_procedural(), added Gemini vision support for prompt-guided tool calling, added OpenAI Codex as LLM provider, web search provider routing with alias fallback, Slack reaction support, and structured fallback deliverables for failed/stuck jobs.  
  Source: [ZeroClaw recent commits](https://github.com/zeroclaw-labs/zeroclaw/commits/main)
- PicoClaw `v0.2.4` added pico_client outbound WebSocket channel, Telegram streaming LLM responses via sendMessageDraft, TUI configuration and user management, gateway management page, and chat functionality in home page.  
  Source: [PicoClaw recent commits](https://github.com/sipeed/picoclaw/commits/main)
- TinyClaw `v0.0.16` streamlined CLI with single 'tinyagi' command, redesigned office with SSE events, and auto-migrate from ~/.tinyclaw to ~/.tinyagi.  
  Source: [TinyClaw releases](https://github.com/TinyAGI/tinyclaw/releases)
- NanoClaw improved Docker stop timeout for faster restarts.  
  Source: [NanoClaw recent commits](https://github.com/qwibitai/nanoclaw/commits/main)

**New "What BorgClaw should copy" items** (added per upstream findings, preserving all existing items):

- OpenClaw: SSRF URL credential bypass protection via guard coverage
- ZeroClaw: mem0/OpenMemory-style external memory backend integration pattern
- ZeroClaw: Structured fallback deliverables for failed/stuck jobs

## Upstream Follow-Up: March 22, 2026

Recent upstream movement:

- OpenClaw added Claude bundle commands natively, context compaction start/end user notifications, webchat persist images to disk, Telegram auto-rename DM topics, voice call TTS contract enforcement, and Discord routed through plugin SDK.  
  Source: [OpenClaw recent commits](https://github.com/openclaw/openclaw/commits/main)
- ZeroClaw added time range filter to memory recall (since/until), per-channel proxy_url support, LocalWhisperProvider for self-hosted STT, DeepMyst as OpenAI-compatible provider, Claude Code OAuth support, gateway path_prefix for reverse proxy, and ClaudeCodeTool for two-tier agent delegation.  
  Source: [ZeroClaw recent commits](https://github.com/zeroclaw-labs/zeroclaw/commits/main)
- IronClaw added GitHub Copilot as LLM provider, MCP clients persist in ExtensionManager, web search with thumbnails, workspace layered memory with sensitivity-based privacy redirect, and RUSTSEC-2026-0049 patch.  
  Source: [IronClaw recent commits](https://github.com/nearai/ironclaw/commits/staging/)
- TinyClaw added exec tool with PTY support and background execution, and Feishu channel support.  
  Source: [TinyClaw recent commits](https://github.com/TinyAGI/tinyclaw/commits/main)

**New "What BorgClaw should copy" items** (added per upstream findings, preserving all existing items):

- OpenClaw: Context compaction notifications (notify user when starts/completes)
- ZeroClaw: Time range filter for memory recall (since/until parameters)
- ZeroClaw: Per-channel proxy_url support for HTTP/SOCKS5 proxies is now implemented for BorgClaw channels with outbound HTTP traffic.
- TinyClaw: exec tool with PTY support and background execution
- IronClaw: Workspace layered memory with privacy-based access control

## Upstream Follow-Up: March 20, 2026

Recent upstream movement sharpened a few priorities for BorgClaw:

- OpenClaw `v2026.3.13` pushed further on compaction correctness, Telegram media SSRF policy, session continuity after reset, cross-agent workspace resolution, gateway token leak prevention in Docker build context, and Signal config/schema expansion.  
  Source: [OpenClaw releases](https://github.com/openclaw/openclaw/releases)
- ZeroClaw `v0.5.0` pushed further on runtime model switching, configurable sub-agent timeouts, self-test and healthcheck flows, rollback-capable updates, gateway device registry/pairing APIs, persisted WebSocket sessions, and more concrete plugin-host/Wasm execution plumbing.
  Source: [ZeroClaw releases](https://github.com/zeroclaw-labs/zeroclaw/releases)
- ZeroClaw `v0.5.4` added Avian as OpenAI-compatible provider, improved context window overflow handling, and continued security hardening.  
  Source: [ZeroClaw releases](https://github.com/zeroclaw-labs/zeroclaw/releases)
- NanoClaw's latest work added host-level `/remote-control`, explicit read-only `/capabilities` and `/status` skills, and continued its bias toward deterministic local/bootstrap flows and docker sandbox dispatch.  
  Source: [NanoClaw recent commits](https://github.com/qwibitai/nanoclaw/commits/main/)
- IronClaw's latest work tightened approval waiting, jobs limits, rate-limit retry behavior, Telegram verification/routing, live trigger cache refresh, and cross-channel owner-scope/routing fallback correctness.  
  Source: [IronClaw recent commits](https://github.com/nearai/ironclaw/commits/staging/)
- PicoClaw `v0.2.3` pushed further on cron exec policy gating, web-configurable cron execution settings, subagent status reporting, gateway hot reload and polling-state sync, symlink-aware allowed-root normalization, and startup dependency checks for the gateway/backend pair.  
  Source: [PicoClaw releases](https://github.com/sipeed/picoclaw/releases)
- TinyClaw `v0.0.14` moved fully onto an in-process persisted scheduler with schedule-management surfaces and stronger workspace-source-of-truth conventions for agent identity/system prompts.  
  Source: [TinyClaw releases](https://github.com/TinyAGI/tinyclaw/releases)

**BorgClaw action taken**: Implemented provider rate-limit retry semantics (429 detection, Retry-After header respect, exponential backoff) per IronClaw pattern.

Implication for BorgClaw:

- The main remaining work is no longer broad feature discovery.
- It is execution correctness, restart safety, operator introspection, and policy consistency across already-landed surfaces.

The sources here are the public GitHub repositories cited by BorgClaw's own documentation:

- [OpenClaw](https://github.com/openclaw/openclaw)
- [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw)
- [NanoClaw](https://github.com/qwibitai/nanoclaw)
- [IronClaw](https://github.com/nearai/ironclaw)
- [PicoClaw](https://github.com/sipeed/picoclaw)
- [TinyClaw](https://github.com/TinyAGI/tinyclaw)

## Quick Matrix

| Project | What It Does Well | Best BorgClaw Use |
|---|---|---|
| OpenClaw | Operational product surface, wizard UX, gateway/control plane, skills lifecycle, split sandbox images | Onboarding, gateway UX, managed skills, transport consistency, Docker sandbox structure |
| ZeroClaw | Trait-driven subsystem boundaries, auth profiles, runtime/security policy | Core Rust architecture, config shape, provider/channel/tool abstractions, typed sandbox policy |
| NanoClaw | Container-first isolation and always-on orchestration | Safe execution defaults for remote transports, scheduled jobs, and Docker inheritance rules |
| IronClaw | Security pipeline and unified tool registry model | SecurityLayer hardening, MCP/WASM/tool execution unification, pipeline-owned sandboxing |
| PicoClaw | Minimal but explicit runtime contracts, workspace boundary consistency, heartbeat/subagent story | Session/workspace layout, heartbeat semantics, subagent inheritance rules, sandbox inheritance |
| TinyClaw | Explicit multi-agent queueing, retries, isolated workspaces | Background task execution, task status, dead-letter/retry design |

## OpenClaw

OpenClaw is the best reference when BorgClaw needs to feel like a product rather than a crate collection.

Good examples:

- The README pushes a terminal-first onboarding path via `openclaw onboard`, and explicitly says the wizard is the recommended setup path for gateway, workspace, channels, and skills. This is a good model for BorgClaw's onboarding promises in [docs/onboarding.md](onboarding.md).  
  Source: [OpenClaw README wizard section](https://github.com/openclaw/openclaw#readme)
- OpenClaw exposes a clear gateway/control-plane surface with sessions, presence, config, cron, webhooks, and control UI. BorgClaw has landed shared routing and structured gateway events, but the broader control-plane/operator surface is still thinner.  
  Source: [OpenClaw README gateway/control-plane notes](https://github.com/openclaw/openclaw#readme)
- The repo layout shows deliberate runtime segregation for sandboxing with `Dockerfile.sandbox`, `Dockerfile.sandbox-browser`, and `Dockerfile.sandbox-common`. That is stronger than a single toggle-based sandbox story.  
  Source: [OpenClaw repository root](https://github.com/openclaw/openclaw)
- OpenClaw documents a managed skills platform with bundled, managed, and workspace skills plus install gating and UI. BorgClaw now has explicit bundled/managed/workspace tier precedence, requirement-gated skill readiness, operator-facing `skills list/search/info/status` flows, and matching onboarding/gateway visibility for the richer lifecycle contract.  
  Source: [OpenClaw README skills platform notes](https://github.com/openclaw/openclaw#readme)
- OpenClaw's security model explicitly distinguishes main-session host execution from non-main session sandbox execution, with allowlists and denylisted capabilities. That is the right level of specificity for channel and group safety.  
  Source: [OpenClaw README security model](https://github.com/openclaw/openclaw#readme)

What BorgClaw should copy:

- Treat onboarding as the primary path, not a thin wrapper around config writing.
- Make gateway behavior a first-class control plane, not just a message tunnel.
- Split sandbox modes by execution context instead of relying on one global switch.
- Turn skills into a lifecycle with discovery, install policy, approval, and status.
- Treat backup/restore and destructive-flow verification as part of the operator contract, not as an afterthought.
- Keep workspace/plugin bootstrap behavior explicit at compaction, scheduler, and subagent boundaries.
- Tighten transport-specific SSRF/media policy enforcement and deployment-time secret handling, not just runtime prompt/tool defenses.
- Add SSRF guard coverage for URL credential bypass vectors (ZeroClaw pattern).

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 2: shared routing, pairing consistency, structured gateway events.
- `ROADMAP.md` Phase 5: onboarding, `status`, and `doctor` maturity.
- `borgclaw-cli/src/main.rs`: remote `SKILL.md` installs and GitHub-backed registry listing now exist, but broader managed skill lifecycle remains thinner than OpenClaw.

## ZeroClaw

ZeroClaw is the clearest architectural reference for BorgClaw's Rust-side modularity.

Good examples:

- ZeroClaw states the core rule plainly: every subsystem is a trait, swappable by config with zero code changes. It then names the concrete subsystem boundaries: `Provider`, `Channel`, `Memory`, `Tool`, `Observer`, `RuntimeAdapter`, `SecurityPolicy`, `IdentityConfig`, and `Tunnel`.  
  Source: [ZeroClaw architecture section](https://github.com/zeroclaw-labs/zeroclaw#readme)
- ZeroClaw's runtime and security config is concrete enough to prevent ambiguity: `workspace_only`, `allowed_commands`, `forbidden_paths`, `allowed_roots`, plus runtime-specific Docker limits. BorgClaw already has pieces of this idea, but not the same policy completeness.  
  Source: [ZeroClaw runtime/security config](https://github.com/zeroclaw-labs/zeroclaw#readme)
- ZeroClaw's auth profile system is multi-account and encrypted at rest, with explicit profile ids and CLI operations for login, refresh, switch, and status. This is a better precedent for provider auth than raw env-var-only flows.  
  Source: [ZeroClaw subscription auth profiles](https://github.com/zeroclaw-labs/zeroclaw#readme)
- ZeroClaw supports multiple identity formats, including OpenClaw-style markdown and AIEOS JSON, without changing the rest of the runtime contract. That fits BorgClaw's documented `soul_path` plus its “AIEOS identity” inspiration.  
  Source: [ZeroClaw identity config](https://github.com/zeroclaw-labs/zeroclaw#readme)
- ZeroClaw's memory section explicitly names FTS5, hybrid merge, embedding providers, and safe reindexing. That is a useful reference whenever BorgClaw's memory docs over-promise relative to current runtime behavior.  
  Source: [ZeroClaw architecture and memory notes](https://github.com/zeroclaw-labs/zeroclaw#readme)

What BorgClaw should copy:

- Make subsystem boundaries explicit in both code and docs, not just implied by module names.
- Expand security config from "feature flags" to policy objects.
- Keep provider auth/profile registry multi-account and encrypted at rest; the remaining follow-up is profile refresh/status ergonomics rather than the base registry.
- Treat identity format as an interface, not a one-off prompt file.
- Preserve richer provider transcript artifacts such as reasoning content when tools are involved, instead of flattening every turn to plain text.
- Keep release and bootstrap paths reproducible and supply-chain-conscious.
- Consider mem0/OpenMemory-style external memory backend integration with history(), recall_filtered(), store_procedural() patterns.
- Structured fallback deliverables for failed/stuck jobs are now implemented across persisted scheduler, heartbeat, and sub-agent state, with operator-facing detail output in the CLI.

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 1 and Phase 5: provider abstraction hardening, `SecurityLayer` authority, secrets/vault completion.
- `docs/onboarding.md`: provider registry and model refresh intent.
- `borgclaw-core/src/config/mod.rs`: current config has the right top-level shape, but the policy depth is still thinner than ZeroClaw's.

## NanoClaw

NanoClaw is the best reference when the question is “what should run inside a container by default?”

Good examples:

- NanoClaw is explicit that Docker is the default runtime, with Apple Container as an optional lighter alternative on macOS. That is a much more operational answer than “we support isolation somehow.”  
  Source: [NanoClaw requirements and FAQ](https://github.com/qwibitai/nanoclaw#readme)
- The repo positions containerization as the reason it can safely connect to many messaging systems while still running memory and scheduled jobs. That makes the isolation model part of the product contract, not an implementation detail.  
  Source: [NanoClaw repository summary](https://github.com/qwibitai/nanoclaw)

What BorgClaw should copy:

- Decide which transports and background paths must be isolated by default.
- Document the runtime contract for isolation at the same level as transport setup.
- Make scheduled jobs and remote-channel execution inherit the same sandbox boundary automatically.
- Prefer deterministic local bootstrap/install flows over marketplace assumptions when the managed lifecycle is not yet complete.
- Add read-only capabilities/status surfaces before introducing more mutable remote-control surfaces.

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 2 and Phase 3: remote channels, scheduler execution, and transport parity.
- `docs/security.md`: BorgClaw documents sandboxing, but not yet at the same operational depth as NanoClaw.

## IronClaw

IronClaw is the best reference for defense-in-depth flow, especially around request/response tool execution.

Good examples:

- IronClaw describes itself as a Rust implementation focused on privacy and security, which already aligns closely with BorgClaw's stated position.  
  Source: [IronClaw repository summary](https://github.com/nearai/ironclaw)
- Its architecture diagram shows a security pipeline instead of a single yes/no gate: `WASM -> Allowlist -> Leak Scan -> Credential Injector -> Execute -> Leak Scan -> WASM`. That is much closer to a real secure execution path than a standalone blocklist.  
  Source: [IronClaw architecture diagram](https://github.com/nearai/ironclaw#readme)
- The same diagram places built-in tools, MCP, and WASM behind one tool registry. That is directly relevant to BorgClaw's roadmap item to normalize skill execution behind one runtime interface.  
  Source: [IronClaw tool registry architecture](https://github.com/nearai/ironclaw#readme)
- IronClaw also shows channels, HTTP, WASM channels, and a web gateway inside one system view instead of documenting them as mostly separate features.  
  Source: [IronClaw architecture diagram](https://github.com/nearai/ironclaw#readme)

What BorgClaw should copy:

- Treat leak scanning, allowlists, secret injection, and execution as one pipeline owned by `SecurityLayer`.
- Unify built-in tools, MCP, and WASM plugins behind the same dispatch contract.
- Keep channel and gateway architecture in one diagram and one runtime path.
- Gate webhook-triggerable tools on explicit capability declarations, not mere tool existence.
- Keep approval semantics identical across immediate, deferred, and background tool execution paths.
- Redact internal failure detail at transport boundaries while retaining richer internal diagnostics.
- Tighten deferred approval waiting coverage and channel ownership/routing fallback tests as correctness work, not polish.

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 4 and Phase 5: shared tool execution interface, SecurityLayer authority, vault completion.
- `docs/security.md`: the security story is already close conceptually, but IronClaw is a stronger example of pipeline sequencing.

## PicoClaw

PicoClaw is the best reference for small-system discipline: keep the runtime narrow, but make every boundary explicit.

Good examples:

- PicoClaw documents a workspace layout that separates sessions, memory, state, cron, skills, `AGENTS.md`, `HEARTBEAT.md`, `IDENTITY.md`, and `SOUL.md`. That is a strong precedent for making runtime state inspectable and predictable.  
  Source: [PicoClaw workspace layout](https://github.com/sipeed/picoclaw#readme)
- It routes generic slash commands through one path in `pkg/agent/loop.go` and explicitly says channel adapters no longer consume generic commands locally. That is exactly the kind of rule BorgClaw needs for transport consistency.  
  Source: [PicoClaw command routing notes](https://github.com/sipeed/picoclaw#readme)
- PicoClaw's `restrict_to_workspace` policy is detailed, tool-specific, and inherited across the main agent, subagents, and heartbeat tasks. That is a direct answer to “how do we avoid security bypasses through background paths?”  
  Source: [PicoClaw security sandbox](https://github.com/sipeed/picoclaw#readme)
- It documents heartbeat tasks as a user-visible contract with config, cadence, and a clear handoff to subagents for long work. BorgClaw has landed heartbeat runtime ownership, persistence, retries, and subagent durability, but explicit workspace/security inheritance is still thinner.  
  Source: [PicoClaw heartbeat and subagent communication](https://github.com/sipeed/picoclaw#readme)
- PicoClaw's provider config is model-centric, supports fallbacks, and keeps session/channel/provider settings in one coherent config story.  
  Source: [PicoClaw providers and config](https://github.com/sipeed/picoclaw#readme)

What BorgClaw should copy:

- Write down one command-routing path and enforce it across every transport.
- Make workspace and session state layout part of the public contract.
- Inherit workspace restrictions into heartbeat, subagents, and scheduled jobs automatically.
- Promote heartbeat from “scheduler exists” to “periodic agent contract with explicit user files and status.”
- Resolve MCP relative config paths against the workspace predictably, and prefer aggregated reachability failures over the first opaque transport error.
- Keep tool timeouts policy-driven and surfaced in config, not hidden in per-tool defaults.
- Treat memory durability as an atomic-write and crash-safety concern, not just a serialization concern.
- Normalize allowed-root checks through symlink-aware canonicalization, and fail startup early when gateway/backend prerequisites are not actually satisfied.

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 2 and Phase 3: shared routing, session compaction, heartbeat execution, subagent status.
- `docs/memory.md` and `docs/channels.md`: the product contract is already similar, but PicoClaw is a good implementation reference.

## TinyClaw

TinyClaw is the best reference for making multi-agent orchestration concrete instead of aspirational.

Good examples:

- TinyClaw frames the system as multi-agent, multi-team, multi-channel with isolated workspaces. That is a good product-level description for BorgClaw's sub-agent ambitions.  
  Source: [TinyClaw README](https://github.com/TinyAGI/tinyclaw#readme)
- More importantly, it documents queue semantics instead of stopping at “agents collaborate”: SQLite queue, WAL-backed atomic transactions, parallel agents, sequential ordering per agent, retries, dead-letter handling, and isolated workspaces.  
  Source: [TinyClaw queue notes](https://github.com/TinyAGI/tinyclaw#readme)

What BorgClaw should copy:

- Define the scheduler/subagent queue contract explicitly.
- Separate "parallel across agents" from "ordered within one session/task owner".
- Keep workspaces isolated per spawned agent/task.
- Treat long-lived polling/channel loops as restart-sensitive state machines with explicit shutdown, health verification, and restart guards.

Best matches for current BorgClaw gaps:

- `ROADMAP.md` Phase 3: sub-agent background execution with status tracking.
- `docs/memory.md`: sub-agent coordinator is described, but the runtime semantics are still underspecified compared with TinyClaw.

## BorgClaw Gap Map

These are the strongest current “look there first” pairings.

### Shared routing and transport parity

Current BorgClaw signal:

- `ROADMAP.md` Phase 2 explicitly calls for one core message router and consistent pairing/DM policy across transports.
- `docs/channels.md` already describes one `Channel -> MessageRouter -> Agent -> Response -> Channel` flow.

Best upstream references:

- [PicoClaw generic command routing and channel forwarding](https://github.com/sipeed/picoclaw#readme)
- [OpenClaw channel routing and gateway/control-plane model](https://github.com/openclaw/openclaw#readme)
- [IronClaw unified architecture view](https://github.com/nearai/ironclaw#readme)

### Gateway auth, control plane, and event model

Current BorgClaw signal:

- `ROADMAP.md` Phase 2 calls out authenticated sessions and structured WebSocket response events.
- `docs/channels.md` already documents `auth`, `message`, `response`, `error`, and `heartbeat` event types.

Best upstream references:

- [OpenClaw gateway WS control plane](https://github.com/openclaw/openclaw#readme)
- [ZeroClaw SecurityPolicy and gateway pairing references](https://github.com/zeroclaw-labs/zeroclaw#readme)
- [IronClaw channel and web gateway architecture](https://github.com/nearai/ironclaw#readme)

### Memory isolation, session history, and compaction

Current BorgClaw signal:

- `ROADMAP.md` Phase 1 and Phase 3 call out session history, compaction, metadata round-tripping, and group isolation.
- `docs/memory.md` promises hybrid search, compaction, and per-group isolation.

Best upstream references:

- [ZeroClaw hybrid memory architecture](https://github.com/zeroclaw-labs/zeroclaw#readme)
- [PicoClaw workspace layout for sessions/memory/state](https://github.com/sipeed/picoclaw#readme)
- [OpenClaw session pruning note](https://github.com/openclaw/openclaw#readme)

### Heartbeat and sub-agent execution

Current BorgClaw signal:

- `ROADMAP.md` Phase 3 now has most runtime mechanics landed: engine-state gating, shared-runtime heartbeat startup, sub-agent concurrency/cancellation controls, parent-context inheritance, stable scheduler `next_run` state, scheduler timeout/concurrency policy, scheduler run history, and persisted heartbeat/sub-agent task state.
- `docs/memory.md` documents both features as if they already exist as finished runtime contracts.

Best upstream references:

- [PicoClaw heartbeat and subagent communication model](https://github.com/sipeed/picoclaw#readme)
- [TinyClaw queue, retry, and isolated workspace semantics](https://github.com/TinyAGI/tinyclaw#readme)
- [OpenClaw cron restart catch-up staggering](https://github.com/openclaw/openclaw/releases/tag/v2026.3.8)

### Skills and integration lifecycle

Current BorgClaw signal:

- `ROADMAP.md` Phase 4 calls for operational paths, unified runtime results, and MCP/plugin routing.
- `borgclaw-core/src/agent/tools.rs` now has shared runtime coverage across GitHub, Google, browser, STT, TTS, image, QR, URL, MCP, and plugin paths, but deeper operational completeness and end-to-end coverage still remain.
- `borgclaw-cli/src/main.rs` now supports local installs, remote `SKILL.md`, GitHub `owner/repo`, and GitHub-backed registry listing, but packaging/publishing and richer managed lifecycle remain open.

Best upstream references:

- [OpenClaw managed skills platform and install gating](https://github.com/openclaw/openclaw#readme)
- [IronClaw unified tool registry for built-in, MCP, and WASM tools](https://github.com/nearai/ironclaw#readme)
- [ZeroClaw subsystem trait map for Tool and Memory](https://github.com/zeroclaw-labs/zeroclaw#readme)

### Security ownership and policy depth

Current BorgClaw signal:

- `ROADMAP.md` Phase 5 says `SecurityLayer` should become authoritative for approvals, pairing, blocklists, prompt-injection defense, and secret scanning.
- `docs/security.md` already promises defense in depth and vault integration.

Best upstream references:

- [IronClaw security pipeline and tool registry architecture](https://github.com/nearai/ironclaw#readme)
- [ZeroClaw SecurityPolicy and runtime adapter policy model](https://github.com/zeroclaw-labs/zeroclaw#readme)
- [OpenClaw non-main session sandbox model](https://github.com/openclaw/openclaw#readme)
- [PicoClaw inherited workspace restriction across main agent, subagent, and heartbeat](https://github.com/sipeed/picoclaw#readme)

## Questions These Repos Already Help Answer

### How strict should transport routing be?

Prefer PicoClaw's rule: generic commands should go through one agent path, and channel adapters should only adapt transport-specific details.

### How much policy belongs in config versus code?

Prefer ZeroClaw's answer: put security and runtime policy in config, but make the policy object explicit and typed.

### How should security be staged?

Prefer IronClaw's answer: execution should pass through validation, leak scanning, credential injection, and post-execution scanning. Do not collapse this into one blocklist check.

### How should heartbeat and sub-agents inherit constraints?

Prefer PicoClaw's answer: the same workspace restrictions must apply to main agent, subagent, and heartbeat execution paths. TinyClaw adds the missing queue semantics for retries and failure handling.

### What should “skills” mean operationally?

Prefer OpenClaw's answer: skills need installation state, gating, and UI/CLI lifecycle. “List of tools” is not enough.

## Do Not Cargo-Cult

These upstream projects are inspiration, not templates.

- Do not copy Node-first operational surfaces from OpenClaw or PicoClaw without translating them into BorgClaw's Rust workspace and crate boundaries.
- Do not copy ZeroClaw's config breadth blindly; only add knobs that BorgClaw can actually enforce.
- Do not copy NanoClaw's container-first model everywhere if BorgClaw still needs local-first CLI and REPL ergonomics.
- Do not copy TinyClaw's multi-agent behavior until BorgClaw defines ordering, retries, cancellation, and workspace isolation as hard contracts.

The right move is usually: borrow the contract, not the syntax.

## Appendix: Docker Sandbox Implementation Focus

This appendix narrows the upstream inspiration into an implementation guide for BorgClaw's Docker sandbox, including what is already landed and what remains future hardening work.

Status note:
- BorgClaw now ships both WASM sandboxing for plugins and an optional typed Docker sandbox for `execute_command`.
- The older `docker_sandbox = true` snippet remains correctly culled because the implemented contract is the typed `[security.docker]` block instead.
- This appendix now distinguishes implemented Docker work from follow-on improvements so the repo does not keep describing shipped functionality as missing.

### What the upstreams imply

OpenClaw contribution:
- Split sandboxing by execution context instead of treating "sandbox" as one toggle.
- Use distinct container images for general command execution versus browser automation.
- Distinguish trusted main-session execution from more restricted non-main-session or delegated execution.

NanoClaw contribution:
- Treat container isolation as part of the runtime contract for remote transports and background work, not as an optional afterthought buried in docs.
- Prefer deterministic local bootstrap and operational scripts so the isolation runtime is installable, inspectable, and reproducible.

PicoClaw contribution:
- The same workspace policy must apply to main agent, subagent, heartbeat, and scheduled execution paths.
- Sandbox boundaries should inherit automatically instead of depending on each callsite to remember extra checks.

ZeroClaw contribution:
- The sandbox needs a typed policy object, not just a boolean.
- Runtime policy should define mounts, command allowlists, network policy, and time/resource limits explicitly.

IronClaw contribution:
- Docker isolation should be one stage inside the security pipeline, not a replacement for approval, leak scanning, or workspace policy.
- Tool dispatch semantics should stay unified whether execution happens on host or in a container.

### Recommended BorgClaw Contract

The current BorgClaw Docker contract should continue to look like this:

- Keep `wasm_sandbox` as the plugin sandbox.
- Add a separate Docker execution policy for host-process tools, especially `execute_command` and any future shell-like runtime helpers.
- Do not route all tools through Docker. Pure API clients, in-process memory operations, and normal provider calls should stay out of the container path.
- Make scheduled jobs, heartbeat tasks, and subagent command execution inherit the same Docker policy automatically when they invoke command-like tools.
- Keep local CLI ergonomics by allowing host execution when policy says so, but default remote/background command execution toward container isolation once the feature exists.

### Recommended Config Shape

Do not restore a bare `docker_sandbox = true` flag by itself. A typed config is the safer contract:

```toml
[security.docker]
enabled = false
image = "ghcr.io/lealvona/borgclaw-sandbox:base"
network = "none"            # "none", "bridge"
workspace_mount = "rw"      # "ro", "rw", "none"
tmpfs = true
memory_limit_mb = 512
cpu_limit = 1.0
timeout_seconds = 120
allowed_tools = ["execute_command"]
allowed_roots = [".borgclaw/workspace"]
extra_env_allowlist = ["PATH", "HOME"]
```

Why this shape:
- `enabled` alone is not enough; image and resource policy are part of the contract.
- `allowed_tools` prevents silent expansion of Dockerized execution to unrelated tools.
- `workspace_mount` and `allowed_roots` make filesystem exposure auditable.
- `network` must be explicit because it changes the threat model materially.

### Recommended Execution Scope

Phase 1 scope:
- `execute_command`
- any future shell/PTy execution tool

Phase 2 scope:
- browser bridge helpers if BorgClaw later adopts a containerized browser image
- selected MCP stdio servers only if they are explicitly marked container-safe

Out of scope for the first implementation:
- in-process memory backends
- provider HTTP calls
- regular GitHub/Google API wrappers
- WASM plugins, which already have a separate sandbox contract

### Filesystem and Process Model

Recommended first implementation:
- one ephemeral container per command execution
- bind mount only the configured workspace roots
- mount workspace read-write only when the workspace policy already allows writes
- run with read-only root filesystem plus a tmpfs working area when practical
- no Docker socket passthrough
- no privileged mode
- no host PID/network namespace sharing

Why:
- it fits BorgClaw's existing per-tool approval model
- it avoids state leakage between unrelated tool invocations
- it is easier to reason about in tests than a long-lived worker container

### Security Pipeline Placement

The Docker path should be:

1. workspace/security policy validation
2. approval check
3. secret injection filtering
4. Docker command construction
5. container execution
6. output leak scan and redaction
7. audit logging

This keeps Docker isolation subordinate to `SecurityLayer` rather than creating a parallel trust path.

### Runtime Images

Recommended image split:
- base sandbox image for shell/command tools
- browser sandbox image only if browser automation is later containerized

The repo should eventually version and publish these explicitly, following the OpenClaw pattern of separate sandbox Dockerfiles instead of one catch-all image.

### Minimum Implementation Checklist

- add typed `security.docker` config parsing and validation
- add a Docker execution adapter owned by `SecurityLayer` or the shared tool runtime
- route `execute_command` through host or Docker based on policy
- make subagent, scheduler, and heartbeat command execution reuse the same path
- add `doctor` checks for Docker binary availability and configured image/runtime policy
- add bootstrap/install scripts for required images
- add tests for:
  - policy parsing
  - host vs Docker routing
  - mount/path restriction enforcement
  - approval behavior in supervised mode
  - background execution inheritance

### What BorgClaw should copy next

Implemented from this appendix already:
- PicoClaw: inherited sandbox/workspace restrictions across main agent, subagent, heartbeat, and scheduled command execution through the shared command/runtime path
- ZeroClaw: typed Docker runtime policy object instead of a boolean switch
- IronClaw: Docker execution embedded inside one security pipeline, not as a side system

Still-open Docker follow-up items:
- OpenClaw/NanoClaw: stronger operator-facing runtime split between trusted local sessions and more restricted delegated or remote execution contexts in the gateway/control plane
