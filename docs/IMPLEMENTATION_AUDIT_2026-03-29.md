# Implementation Audit - 2026-03-29

This document records a code-and-doc audit performed after the TICKET-086 and TICKET-087 landing sequence.

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

## Confirmed Gaps

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

### 3. Remote manifest installs still fetch only `SKILL.md`

Evidence:

- `install_skill_manifest()` creates a destination directory and writes only `SKILL.md`.
- Companion assets from remote installs are not fetched.

Required fix steps:

1. Decide whether remote GitHub/URL installs should remain manifest-only or become archive-backed.
2. If asset-complete installs are desired, use packaged archives or a registry manifest format that declares companion files.
3. Update docs so "remote install" clearly distinguishes manifest-only installs from packaged installs.

### 4. Repo-wide quality debt remains outside the audited feature work

Evidence:

- `cargo clippy -p borgclaw-core -- -D warnings` still fails on pre-existing issues outside this audit tranche.

Representative blockers:

- `borgclaw-core/src/agent/mod.rs`
- `borgclaw-core/src/config/mod.rs`
- `borgclaw-core/src/scheduler/jobs.rs`
- `borgclaw-core/src/skills/browser.rs`
- `borgclaw-core/src/skills/github.rs`
- `borgclaw-core/src/skills/plugin.rs`
- `borgclaw-core/src/skills/stt.rs`

Required fix steps:

1. Open a dedicated lint-hardening tranche instead of mixing lint debt into product-contract work.
2. Fix one module family at a time.
3. Re-run `cargo clippy -p borgclaw-core -- -D warnings` after each slice.

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
   - keep remote archive installs listed as pending
4. Add superseded/historical framing to `docs/REMAINING_TASKS.md`.
5. Update `README.md` where the top-level product description should reflect real skill lifecycle limitations.
6. Keep `CHANGELOG.md` accurate for historical releases when a listed feature is only partial or placeholder.

## Current Recommended Priorities

1. Implement heartbeat manual trigger for real runtime execution or durable queueing.
2. Decide and implement the intended remote skill install model:
   - manifest-only
   - archive-by-URL
   - registry-first packaged installs
3. Resolve the repo-wide clippy debt in dedicated follow-up PRs.
4. Continue treating docs as contract and re-audit feature claims whenever a major tranche lands.
