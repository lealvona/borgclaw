# BorgClaw Roadmap

## Status

As of March 31, 2026, the original contract-repair roadmap is effectively complete. BorgClaw now has the documented shared runtime, memory/scheduler/sub-agent persistence, skill lifecycle, provider profiles, Docker command sandbox, and gateway/control-plane baseline.

The roadmap is no longer about "make the README true." It is now about improving implementation quality, multi-user correctness, and operator ergonomics on top of the landed baseline.

Historical execution records remain in:

- `docs/IMPLEMENTATION_AUDIT_2026-03-29.md`
- `docs/IMPLEMENTATION_PLAN_2026-03-30.md`
- `docs/IMPLEMENTATION_PLAN_2026-03-30_REMAINING_BACKLOG.md`
- `docs/FINAL_SHIPPABILITY_AUDIT_2026-03-30.md`

## Current Priorities

### Priority 1: OAuth And Session Delivery

- Finish live OAuth completion routing for WebSocket, web-chat, and CLI sessions instead of relying on the browser success page alone.
- Replace the gateway's connection-metadata-only model with session-scoped outbound actors or callback queues.
- Scope Google OAuth token persistence per user or per session instead of a shared token file.
- Add black-box gateway coverage for the OAuth callback path, including persisted pending state and channel completion behavior.

Why this matters:
- This is the main remaining correctness gap in an otherwise landed Google integration.
- Upstream projects are moving toward stronger hosted OAuth and session ownership patterns, and BorgClaw should not stop at "browser page says success."

### Priority 2: Multi-Tenant And Gateway Hardening

- Tighten gateway authentication and rate limiting for remote/operator-facing surfaces.
- Make session state explicit and inspectable (`idle`, `running`, `waiting`, `error`) instead of inferring everything from scattered metrics.
- Strengthen channel/session ownership so approvals, callbacks, and background completions all route back through one authoritative session model.
- Continue reducing shell-mediated gateway/control-plane operations in favor of typed internal paths.

Why this matters:
- The current gateway is good enough for single-operator local use, but thinner than the stronger control-plane and multi-tenant models visible upstream.

### Priority 3: Context And Execution Discipline

- Enforce stricter bounds on history forwarded into isolated command/container execution contexts.
- Keep compaction and memory continuity transparent to operators with clearer lifecycle notifications and failure-state summaries.
- Extend regression coverage around tool-call recovery, context overflow, and long-running session behavior.
- Keep PTY/background execution behavior aligned with the same policy and audit guarantees already used elsewhere.

Why this matters:
- The platform already supports a lot of runtime surface area; the next gains come from making that surface more predictable under pressure.

### Priority 4: Operator Workflow And Release Ergonomics

- Add first-class install/uninstall helpers for local binary and launcher integration.
- Decide whether password-gated secret-store unlock is a real product goal; if yes, promote it from an abandoned plan into an explicit execution track.
- Standardize provider/integration credential shapes where possible (`api_key`, `oauth_token`, profile id) to reduce per-provider drift.
- Add explicit nightly/release automation documentation and implementation when the operator workflow is stable enough to freeze.

Why this matters:
- Setup and release ergonomics are now the main non-runtime polish gap.

## Explicit Non-Goals

These remain useful upstream references, but they are not BorgClaw roadmap items:

- AWS Bedrock provider support
- Composio integration
- Slack approval UI/buttons

## Guidance

- Use `docs/implementation-status.md` for the evidence-backed status of what is complete versus partial.
- Use `docs/inspirations.md` for upstream patterns worth copying next.
- Treat new roadmap items as extension and quality work, not justification to quietly narrow the existing product contract.
