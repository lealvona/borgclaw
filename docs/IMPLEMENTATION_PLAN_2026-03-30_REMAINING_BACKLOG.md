# Implementation Plan: Remaining Follow-Up Work

This document is the execution source of truth for the remaining inspiration-driven BorgClaw backlog after the March 2026 contract-repair work.

It assumes the merged baseline through the Docker sandbox tranche and turns the remaining intended work into a sequential PR train that can be landed cleanly on `main`.

Status note:
- PR 1 (`Backlog Contract Repair`) is landed.
- PR 2 (`Provider Profile Registry`) is landed.
- PR 3 (`MiniMax Multi-Turn Stability Fix`) is landed.
- PR 4 (`Identity Formats + Transcript Artifacts`) is the active tranche.

## Scope Notes

Explicitly declined and not planned:
- AWS Bedrock provider support
- Composio integration
- Slack approval UI / Slack approval buttons

These items remain useful upstream references, but they are not part of BorgClaw's intended backlog and must not continue to appear as open required work.

## PR Train

| PR | Title | Status | Main Outcome |
|---|---|---|---|
| 1 | Backlog Contract Repair | complete | Align docs, status tracking, and follow-up ownership with the intended backlog |
| 2 | Provider Profile Registry | complete | Add encrypted named provider profiles and runtime profile selection |
| 3 | MiniMax Multi-Turn Stability Fix | complete | Keep MiniMax chat history valid after the first turn and add regression coverage for the chat turn pipeline |
| 4 | Identity Formats + Transcript Artifacts | in progress | Support markdown and structured identities, plus richer internal provider artifacts |
| 5 | Memory Query Extensions + External Adapter | pending | Add `since`/`until`, richer memory APIs, and an optional OpenMemory-style adapter |
| 6 | Workspace-Layered Memory Privacy | pending | Add sensitivity-aware memory access policy across execution contexts |
| 7 | PTY + Background Command Runtime | pending | Extend `execute_command` with PTY/background execution and persisted process state |
| 8 | Docker Sandbox Hardening | pending | Add split sandbox modes and stricter defaults for remote/background execution |
| 9 | Skills Lifecycle Completion | pending | Add skill gating, richer discovery/status, and explicit source tiers |
| 10 | Gateway + Onboarding Control Plane | pending | Expose all new runtime contracts in onboarding, status, doctor, and gateway surfaces |
| 11 | Final Audit + Smell Cleanup | pending | Resolve cross-cutting cleanup, fill test gaps, and close the intended backlog |

## PR 1: Backlog Contract Repair

Required changes:
- Update `docs/inspirations.md` so declined items are tracked separately from intended work.
- Update `docs/implementation-status.md` so temporary limitations only list intended backlog.
- Update `ROADMAP.md` with a follow-up note pointing to this plan as the remaining execution source of truth.
- Keep `docs/inspirations.md` preserving upstream patterns, but stop treating declined features as BorgClaw requirements.

Acceptance:
- The repo no longer presents Bedrock, Composio, or Slack approval UI as open BorgClaw requirements.
- The remaining intended backlog is explicitly mapped to this PR train.

## PR 2: Provider Profile Registry

Required changes:
- Add encrypted named provider profiles with a stable profile id and selected-profile config reference.
- Keep provider registry definitions separate from user credential profiles.
- Make onboarding, CLI, and gateway resolve provider auth through profiles first, then legacy compatibility paths.
- Add CLI flows for list/show/add/select/remove and refresh status.

Acceptance:
- Existing configs keep working.
- New installs can be fully configured through named profiles without plaintext provider secrets in `config.toml`.

## PR 3: MiniMax Multi-Turn Stability Fix

Required changes:
- Inspect the MiniMax chat turn pipeline end to end, including stored assistant payloads and replayed history.
- Ensure BorgClaw preserves the provider-specific reasoning payload semantics required for valid second and later turns.
- Add regression coverage proving that a second user turn does not degrade into provider-side `400 Bad Request` responses because of malformed replayed history.
- Improve provider-side error visibility so future contract mismatches are diagnosable from logs and surfaced failures.

Acceptance:
- BorgClaw can complete more than one MiniMax chat turn without the observed `400 Bad Request` failure.
- The regression is covered in automated tests.

## PR 4: Identity Formats And Transcript Artifacts

Required changes:
- Replace the one-off `soul_path` loading logic with an identity loader abstraction.
- Support the current markdown identity path and one structured identity format.
- Preserve richer provider/session artifacts internally without changing current plain-text channel behavior.
- Make compaction and summaries consume the structured transcript model safely.

Acceptance:
- Existing `soul_path` users remain compatible.
- Structured identities and richer transcript records round-trip correctly.

## PR 5: Memory Query Extensions And External Adapter

Required changes:
- Add `since` and `until` filters to memory recall.
- Implement time filtering across SQLite, PostgreSQL, and in-memory backends.
- Add richer memory APIs for history and procedural memory.
- Add an optional external OpenMemory-style adapter that complements, not replaces, local memory backends.

Acceptance:
- All built-in backends support the same extended recall contract.
- External memory can be enabled without breaking local memory behavior.

## PR 6: Workspace-Layered Memory Privacy

Required changes:
- Add sensitivity/privacy metadata to memory entries.
- Add policy checks for main agent, sub-agent, scheduler, and heartbeat access.
- Fail closed on unauthorized retrieval while preserving backward compatibility for legacy entries.

Acceptance:
- Privacy levels are enforced consistently across execution contexts.
- Legacy entries without sensitivity metadata remain readable under the default policy.

## PR 7: PTY And Background Command Runtime

Required changes:
- Extend `execute_command` with `pty`, `background`, `yield_ms`, and persisted process state.
- Add operator surfaces for list/show/cancel of background processes.
- Keep approval and sandbox behavior aligned across foreground and background execution.

Acceptance:
- Foreground PTY and non-PTY background execution are both covered.
- Background command execution persists state and integrates with current status/doctor flows.

## PR 8: Docker Sandbox Hardening

Required changes:
- Split Docker sandbox policy by execution context.
- Introduce stricter defaults for higher-risk remote/background command execution.
- Add clearer diagnostics for effective sandbox mode and image availability.

Acceptance:
- Existing Docker configs remain compatible.
- Remote/background command execution defaults to stricter isolation than trusted local sessions.

## PR 9: Skills Lifecycle Completion

Required changes:
- Make bundled, managed/local, and workspace skill tiers explicit.
- Add requirement gates for binaries, env vars, and config prerequisites.
- Add richer `skills list/search/info/status` behavior in the CLI and gateway.

Acceptance:
- Skill load failures explain why the skill is unavailable.
- Source precedence and gate evaluation are operator-visible.

## PR 10: Gateway And Onboarding Control Plane

Required changes:
- Surface provider profiles, identity formats, memory extensions, process runtime, privacy status, and skill gates in onboarding and gateway config/status.
- Add read-only capability/status surfaces before any future mutable remote-control expansion.
- Align README and the docs with the landed contracts.

Acceptance:
- Gateway and onboarding expose the full feature surface without introducing a second implementation path.

## PR 11: Final Audit And Cleanup

Required changes:
- Run the full strict verification suite.
- Fix architectural duplication or smell found during the feature train.
- Add any missing regression coverage.
- Mark implemented features complete only after code and tests verify them.

Acceptance:
- The intended backlog is empty.
- Remaining non-goals are documented as declined rather than left ambiguous.

## Verification Standard For Every PR

- `cargo test --workspace`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Focused backward-compatibility tests for any config or public contract changes
