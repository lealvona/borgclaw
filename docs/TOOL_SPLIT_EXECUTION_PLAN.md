# Tool Split Execution Plan

> Created: 2026-03-28
> Scope: Execute the remaining work from `docs/REMAINING_TASKS.md` in small, mergeable tranches.

## Goal

Split `borgclaw-core/src/agent/tools.rs` incrementally without pushing to `main`, while preserving the existing semver work and keeping each PR small enough to verify and review.

## Rules

- Never push directly to `main`
- Every tranche gets its own feature branch
- Every branch is pushed and merged through a PR before starting the next tranche
- Run compilation/tests relevant to the tranche before opening each PR
- Keep unrelated refactors out of scope
- If a tranche exposes unexpected API mismatches, stop expanding scope and document the blocker in the active PR

## Tranches

### Tranche 1: Preserve Current Work + Planning

Branch: `TICKET-087-semver-and-tool-split-plan`

Deliverables:
- Commit existing semver comparison changes already present in the worktree
- Commit `docs/REMAINING_TASKS.md`
- Commit this execution plan

Checks:
- `cargo test -p borgclaw-core parser`
- `cargo fmt --check`

### Tranche 2: Extract Shared Tool Types

Branch: `TICKET-086-tools-types-extraction`

Deliverables:
- Create `borgclaw-core/src/agent/tools/`
- Move shared type definitions into `tools/types.rs`
- Update `agent/mod.rs` exports via `tools/mod.rs`
- Keep `ToolRuntime`, `ToolInvocationContext`, registry assembly, and dispatch in `mod.rs`
- Add or keep a registry count/coverage test for `builtin_tools()`

Checks:
- `cargo check -p borgclaw-core`
- `cargo test -p borgclaw-core builtin_tools`
- `cargo fmt --check`

### Tranche 3: Extract Low-Risk Tool Modules

Branch: `TICKET-086-tools-core-modules`

Deliverables:
- Extract `memory.rs`
- Extract `file.rs`
- Extract `shell.rs`
- Extract `web.rs`
- Convert those modules to `register()` plus implementation functions
- Keep `execute_tool()` in `mod.rs`, but dispatch into extracted modules

Checks:
- `cargo check -p borgclaw-core`
- Focused tests for memory/file/shell/web tool behavior
- `cargo fmt --check`

### Tranche 4: Extract Remaining Tool Modules

Branch: `TICKET-086-tools-service-modules`

Deliverables:
- Extract `github.rs`
- Extract `google.rs`
- Extract `browser.rs`
- Extract `media.rs`
- Extract `plugin.rs`
- Extract `schedule.rs`
- Finish `builtin_tools()` assembly through per-module `register()` calls
- Leave runtime/security sanitization logic centralized in `mod.rs`

Checks:
- `cargo check`
- `cargo test -p borgclaw-core`
- `cargo fmt --check`

### Tranche 5: Cleanup, Docs, and Edge Cases

Branch: `TICKET-086-tools-cleanup-and-docs`

Deliverables:
- Remove known warnings such as unused imports/variables
- Add semver edge-case tests
- Update `docs/skills.md` with semver compatibility notes
- Document the final tool registry pattern
- Run stricter verification and fix tranche-related findings

Checks:
- `cargo test --workspace`
- `cargo clippy -- -D warnings`
- `cargo fmt --check`

## Execution Checklist

- [ ] Review current worktree and preserve existing changes
- [ ] Create `TICKET-087-semver-and-tool-split-plan`
- [ ] Commit tranche 1 changes
- [ ] Push tranche 1 branch
- [ ] Open tranche 1 PR
- [ ] Merge tranche 1 PR
- [ ] Refresh local `main`
- [ ] Create `TICKET-086-tools-types-extraction`
- [ ] Extract shared tool types
- [ ] Validate tranche 2
- [ ] Push tranche 2 branch
- [ ] Open tranche 2 PR
- [ ] Merge tranche 2 PR
- [ ] Refresh local `main`
- [ ] Create `TICKET-086-tools-core-modules`
- [ ] Extract memory/file/shell/web modules
- [ ] Validate tranche 3
- [ ] Push tranche 3 branch
- [ ] Open tranche 3 PR
- [ ] Merge tranche 3 PR
- [ ] Refresh local `main`
- [ ] Create `TICKET-086-tools-service-modules`
- [ ] Extract github/google/browser/media/plugin/schedule modules
- [ ] Validate tranche 4
- [ ] Push tranche 4 branch
- [ ] Open tranche 4 PR
- [ ] Merge tranche 4 PR
- [ ] Refresh local `main`
- [ ] Create `TICKET-086-tools-cleanup-and-docs`
- [ ] Add warning fixes, docs updates, and semver edge-case tests
- [ ] Validate tranche 5
- [ ] Push tranche 5 branch
- [ ] Open tranche 5 PR
- [ ] Merge tranche 5 PR
- [ ] Refresh local `main`
- [ ] Confirm final workspace state

## Notes

- `upstream` is not configured with a `NO_PUSH` placeholder in the current repo state, so all pushes must be explicit to `lealvona` feature branches only.
- The worktree already contains semver-related modifications; tranche 1 exists partly to capture that work cleanly before the tool split starts.
- If a later tranche becomes too large, split it again rather than growing the PR.
