# BorgClaw Roadmap

## Status

As of April 1, 2026, the original contract-repair roadmap is complete and a second pass of partial-implementation gaps has been closed. The baseline now includes:

- Shared runtime, memory/scheduler/sub-agent persistence, skill lifecycle, provider profiles, Docker command sandbox, gateway/control-plane baseline (March 2026)
- Compaction lifecycle notifications surfaced in `AgentResponse.metadata`
- WebSocket connection limit at the gateway upgrade path (HTTP 429)
- Per-channel `proxy_url` validated and applied for Telegram; documented as N/A for Signal
- User-level install and uninstall scripts (`scripts/install.sh`, `scripts/uninstall.sh`)
- All 7 LLM providers (OpenAI, Anthropic, Google, Kimi, MiniMax, Z.ai, Ollama) fully integrated

The roadmap is now focused on multi-user correctness, session delivery completeness, and operator ergonomics.

Historical execution records:

- `docs/IMPLEMENTATION_AUDIT_2026-03-29.md`
- `docs/IMPLEMENTATION_PLAN_2026-03-30.md`
- `docs/IMPLEMENTATION_PLAN_2026-03-30_REMAINING_BACKLOG.md`
- `docs/FINAL_SHIPPABILITY_AUDIT_2026-03-30.md`

## Current Priorities

### Priority 1: OAuth And Session Delivery

- Extend the new session-scoped outbound queue model beyond the current live WebSocket path so callback ownership is transport-agnostic.
- Add black-box gateway coverage for the OAuth callback path, including persisted pending state and channel completion behavior.
- Replace browser-popup-only completion ownership with a transport-agnostic model for browser chat and future remote clients.
- Harden the persisted OAuth state/completion stores with clearer pruning, observability, and operator inspection.

Why this matters:
- The main correctness gaps in the landed Google integration are closed, but callback ownership is still uneven across browser-style clients.
- Upstream projects are moving toward stronger hosted OAuth and session ownership patterns, and BorgClaw should not stop at popup-window confirmation.

### Priority 2: Multi-Tenant And Gateway Hardening

- Session state machine (`idle`, `running`, `waiting`, `error`) is now explicit in the gateway WebSocket handler. ✅
- WebSocket connection limit (HTTP 429) is now enforced at the gateway upgrade path. ✅
- Remaining: per-IP HTTP rate limiting for all gateway endpoints (not just WebSocket).
- Remaining: per-request authentication beyond pairing (bearer token or API key validation on individual requests).
- Remaining: session-scoped approval ownership so approvals, callbacks, and background completions route through one authoritative session model.
- Shell-mediated gateway operations: no shell interpolation was found; operations are type-safe. ✅

Why this matters:
- The gateway now has an explicit session state machine and a connection limit, but per-request auth and per-IP rate limiting are still absent for the HTTP API surface.

### Priority 3: Context And Execution Discipline

- Hard caps on history forwarded into sessions are enforced via `max_messages` and `session_max_entries`. ✅
- Compaction lifecycle notifications are now surfaced in `AgentResponse.metadata` and logged at INFO. ✅
- Remaining: operator-facing endpoint or event stream for compaction events (current notification is metadata-only).
- Remaining: regression coverage for tool-call recovery, context overflow, and long-running session behavior.
- PTY support and background execution are landed with host-only restriction documented. ✅

Why this matters:
- Compaction is now observable. The remaining gap is surfacing it to operators through the gateway event stream.

### Priority 4: Operator Workflow And Release Ergonomics

- Install and uninstall helpers are now first-class scripts (`scripts/install.sh`, `scripts/uninstall.sh`). ✅
- Provider credential shapes are standardized via `ProviderProfile` (`api_key`, `env_key`, `model`). ✅
- Remaining: password-gated secret-store unlock as a first-class operator flow (currently file-key-backed only).
- Remaining: clear unlock lifecycle documented across bootstrap, onboarding, CLI, and automation-safe non-interactive paths.
- Remaining: explicit nightly/release automation documentation.

Why this matters:
- Install/uninstall ergonomics are closed. The main remaining gap is the secret-store unlock flow.

## Explicit Non-Goals

These remain useful upstream references, but they are not BorgClaw roadmap items:

- AWS Bedrock provider support
- Composio integration
- Slack approval UI/buttons

## Guidance

- Use `docs/implementation-status.md` for the evidence-backed status of what is complete versus partial.
- Use `docs/inspirations.md` for upstream patterns worth copying next.
- Treat new roadmap items as extension and quality work, not justification to quietly narrow the existing product contract.
