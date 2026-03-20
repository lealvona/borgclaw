# Inspirations And Implementation Notes

This guide expands the short origin list in the README into an engineering reference.

Last reviewed against upstream repositories: March 20, 2026

Status note:
- Several gaps originally called out here are now partially or fully closed in BorgClaw.
- Use [Implementation Status](implementation-status.md) as the current source of truth for what remains open.
- Keep this document focused on upstream implementation ideas, not on narrowing BorgClaw's contract.

Use it for two things:

1. Understand which upstream project is the best model for a given BorgClaw subsystem.
2. Cross-check BorgClaw roadmap items, stubs, and rough edges against upstream implementations that already solved similar problems well.

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
| OpenClaw | Operational product surface, wizard UX, gateway/control plane, skills lifecycle | Onboarding, gateway UX, managed skills, transport consistency |
| ZeroClaw | Trait-driven subsystem boundaries, auth profiles, runtime/security policy | Core Rust architecture, config shape, provider/channel/tool abstractions |
| NanoClaw | Container-first isolation and always-on orchestration | Safe execution defaults for remote transports and scheduled jobs |
| IronClaw | Security pipeline and unified tool registry model | SecurityLayer hardening, MCP/WASM/tool execution unification |
| PicoClaw | Minimal but explicit runtime contracts, workspace boundary consistency, heartbeat/subagent story | Session/workspace layout, heartbeat semantics, subagent inheritance rules |
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
- OpenClaw documents a managed skills platform with bundled, managed, and workspace skills plus install gating and UI. BorgClaw now supports local `SKILL.md` installs and installed-skill listing, but it still does not have a broader managed registry lifecycle.  
  Source: [OpenClaw README skills platform notes](https://github.com/openclaw/openclaw#readme)
- OpenClaw's security model explicitly distinguishes main-session host execution from non-main session sandbox execution, with allowlists and denylisted capabilities. That is the right level of specificity for channel and group safety.  
  Source: [OpenClaw README security model](https://github.com/openclaw/openclaw#readme)

What BorgClaw should copy:

- Treat onboarding as the primary path, not a thin wrapper around config writing.
- Make gateway behavior a first-class control plane, not just a message tunnel.
- Split sandbox modes by execution context instead of relying on one global switch.
- Turn skills into a lifecycle with discovery, install policy, approval, and status.
- Add restart-safe scheduler catch-up and gateway restart guards so recovered processes do not flood or stall background execution.
- Treat backup/restore and destructive-flow verification as part of the operator contract, not as an afterthought.
- Keep workspace/plugin bootstrap behavior explicit at compaction, scheduler, and subagent boundaries.
- Tighten transport-specific SSRF/media policy enforcement and deployment-time secret handling, not just runtime prompt/tool defenses.

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
- Expand security config from “feature flags” to policy objects.
- Add a provider auth/profile registry instead of only relying on process env.
- Treat identity format as an interface, not a one-off prompt file.
- Preserve richer provider transcript artifacts such as reasoning content when tools are involved, instead of flattening every turn to plain text.
- Keep release and bootstrap paths reproducible and supply-chain-conscious.
- Expand self-test/healthcheck and gateway-session persistence depth so runtime and operator UX stay aligned.

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
- Rate-limit retry semantics (429 detection, Retry-After respect, exponential backoff) now implemented in BorgClaw provider layer.
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
- Separate “parallel across agents” from “ordered within one session/task owner”.
- Add retries, terminal failure state, and dead-letter semantics to background execution.
- Keep workspaces isolated per spawned agent/task.
- Treat long-lived polling/channel loops as restart-sensitive state machines with explicit shutdown, health verification, and restart guards.
- Treat persisted schedule management and operator-visible scheduler state as part of the product surface, not only internal runtime machinery.

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
