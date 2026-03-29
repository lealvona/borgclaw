# Implementation Audit - 2026-03-29

This document records a code-and-doc audit performed after the TICKET-086 and TICKET-087 landing sequence.

Superseded in part by later March 29, 2026 follow-up work:
- manual heartbeat trigger has since been implemented
- archive-backed skill install has since landed for local packages, remote archive URLs, and GitHub-backed sources
- direct `SKILL.md` installs now support explicit companion `files:` entries, adjacent `SKILL.files.json` sidecar discovery, and manifest-directory discovery where listings are available

It has two goals:

1. Identify functionality that is still incomplete, placeholder-only, or only partially implemented.
2. Reconcile product docs so they describe the actual runtime instead of stale intermediate states.

## Audit Scope

Reviewed sources:

- `README.md`
- `ROADMAP.md`
- `CHANGELOG.md`
- `docs/implementation-status.md`
- `docs/integrations.md`
- `docs/REMAINING_TASKS.md`
- `docs/skills.md`
- `borgclaw-cli/src/main.rs`
- selected runtime surfaces in `borgclaw-core`

Searches used:

- explicit incompleteness markers such as `TODO`, `FIXME`, `placeholder`, `not implemented`, `todo!`, and `unimplemented!`
- command-surface verification in the CLI
- targeted feature-claim verification against code paths and tests

## Verified Implemented Work

The following claims were re-checked against code and are implemented:

- skill packaging, publishing, and archive inspection in `borgclaw-cli`
- backup export, import, and verify workflows
- schedule CRUD flows (`list`, `show`, `create`, `delete`, `pause`, `resume`)
- sub-agent operator flows (`list`, `show`, `cancel`)
- provider registrations for OpenAI, Anthropic, Google, Ollama, Kimi, MiniMax, and Z.ai
- tool-module split across `memory`, `file`, `shell`, `web`, `plugin`, `mcp`, `schedule`, `github`, `google`, `browser`, and `media`

The audit did not find live `todo!()` or `unimplemented!()` macros in the main Rust crates.

## Historical Gaps Found During The Audit

The first two findings below were resolved later on March 29, 2026. They remain here as an audit trail of what was verified at the time, not as current product gaps.

### 1. Heartbeat manual trigger is still placeholder-only

Evidence:

- `borgclaw-cli/src/main.rs` handles `HeartbeatAction::Trigger` by printing informational text only.
- The command does not update persisted state, invoke a runtime engine, or enqueue an immediate run.

Current behavior:

- Prints "Manual trigger requires a running agent with heartbeat engine active."
- Prints "Task '<id>' trigger request logged for next scheduled run."

Required fix steps:

1. Define the intended trigger semantics for offline and live-runtime cases.
2. Add a real trigger path that either:
   - dispatches directly through an active heartbeat engine, or
   - writes durable trigger intent that the engine consumes.
3. Return success/failure based on actual execution or accepted trigger intent.
4. Add focused CLI and runtime tests for trigger behavior.
5. Keep docs and release notes explicit about whether trigger is immediate, queued, or best-effort.

### 2. Remote skill archive install by URL is not implemented

Evidence:

- `install_skill()` supports:
  - local directories
  - `owner/repo`
  - direct `SKILL.md` URLs
- It does not support downloading `.tar.gz` skill archives by URL.

Required fix steps:

1. Extend source detection to distinguish archive URLs from direct manifest URLs.
2. Download archive bytes safely to a temp location.
3. Validate archive contents before extraction.
4. Extract into the destination skill directory atomically.
5. Add tests for success, malformed archives, duplicate installs, and missing `SKILL.md`.

## Current Remaining Gaps

### 1. Repo-wide quality debt remained outside the original audited feature work

Status update:

- This was resolved in a later follow-up pass.
- `cargo clippy --workspace --all-targets -- -D warnings` now passes.

Required fix steps:

1. Keep the workspace on the passing `clippy -D warnings` baseline.
2. Treat new clippy regressions as release blockers rather than deferred cleanup.

## Documentation Problems Found

### Contradictory status sources

- `docs/implementation-status.md` previously claimed all documented features were fully implemented.
- `ROADMAP.md` still described multiple phases as only partially complete.
- `docs/integrations.md` still said packaging/publishing were not implemented, even though the CLI implements them.
- `CHANGELOG.md` described heartbeat manual trigger as a placeholder, which is accurate, but other status docs contradicted it.

### Historical task docs presented as current state

- `docs/REMAINING_TASKS.md` still described the tool split as incomplete even though that work has been merged.

## Documentation Repair Plan

1. Use `docs/implementation-status.md` as the current audited source of truth, but make it honest about remaining gaps.
2. Rewrite `ROADMAP.md` so it tracks only current remaining work, not stale in-flight tranche notes.
3. Update `docs/integrations.md` to:
   - use the correct `borgclaw skills ...` command spelling
   - describe packaging/publishing as implemented
   - distinguish archive-backed installs from direct manifest installs with explicit `files:` entries, `SKILL.files.json` sidecars, or manifest-directory discovery
4. Add superseded/historical framing to `docs/REMAINING_TASKS.md`.
5. Update `README.md` where the top-level product description should reflect real skill lifecycle limitations.
6. Keep `CHANGELOG.md` accurate for historical releases when a listed feature is only partial or placeholder.

## Current Recommended Priorities

1. Keep docs explicit about the distinction between archive-backed installs and direct manifest installs with declared companion files.
2. Keep the workspace on the passing lint baseline and treat regressions as blocking.
3. Continue treating docs as contract and re-audit feature claims whenever a major tranche lands.
