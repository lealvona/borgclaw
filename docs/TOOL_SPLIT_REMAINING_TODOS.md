# Tool Split Remaining Todos

> Created: 2026-03-29
> Resume point after merged tranches for `TICKET-087` and early `TICKET-086`.
>
> Historical note: the remaining tranche-5 commit/push/PR boxes and the listed clippy blockers were cleared in later March 29 follow-up work. This file is kept only as the historical execution ledger for the tool split.

## Current State

- Completed: tranche 1 `TICKET-087-semver-and-tool-split-plan`
- Completed: tranche 2 `TICKET-086-tools-types-extraction`
- Completed: tranche 3a `TICKET-086-memory-tool-module`
- Completed: tranche 3b `TICKET-086-tools-core-modules`
- Completed: tranche 4a `TICKET-086-tools-service-runtime-modules`
- Completed: tranche 4b `TICKET-086-tools-github-google-modules`
- Completed: tranche 4c `TICKET-086-tools-browser-media-modules`
- In progress: tranche 5 `TICKET-086-tools-cleanup-docs`

## Small-Step Execution Plan

### Tranche 3b: Core Tool Modules

- [x] Extract `borgclaw-core/src/agent/tools/file.rs`
- [x] Extract `borgclaw-core/src/agent/tools/shell.rs`
- [x] Extract `borgclaw-core/src/agent/tools/web.rs`
- [x] Move registry assembly for those tools into module `register()` functions
- [x] Update `execute_tool()` dispatch to call extracted modules
- [x] Keep shared approval, path, and truncation helpers centralized in `mod.rs`
- [x] Run focused `cargo test -p borgclaw-core builtin_tools`
- [x] Run focused command and scheduler tests that cover moved behavior
- [x] Commit tranche 3b
- [x] Push tranche 3b branch
- [x] Open tranche 3b PR
- [x] Merge tranche 3b PR

### Tranche 4a: Service Runtime Modules

- [x] Extract `plugin.rs`
- [x] Extract `mcp.rs`
- [x] Extract `schedule.rs`
- [x] Centralize `builtin_tools()` around per-module `register()` calls for these services
- [x] Run `cargo check`
- [x] Run focused tool tests in a non-sandboxed environment
- [x] Commit tranche 4a
- [x] Push tranche 4a branch
- [x] Open tranche 4a PR
- [x] Merge tranche 4a PR

### Tranche 4b: GitHub and Google Modules

- [x] Extract `github.rs`
- [x] Extract `google.rs`
- [x] Run `cargo check`
- [x] Run focused GitHub and Google tool tests
- [x] Commit tranche 4b
- [x] Push tranche 4b branch
- [x] Open tranche 4b PR
- [x] Merge tranche 4b PR

### Tranche 4c: Browser and Media Modules

- [x] Extract `browser.rs`
- [x] Extract `media.rs`
- [x] Run `cargo check`
- [x] Run focused browser and media tool tests
- [x] Commit tranche 4c
- [x] Push tranche 4c branch
- [x] Open tranche 4c PR
- [x] Merge tranche 4c PR

### Tranche 5: Cleanup and Docs

- [x] Remove tranche-related warnings and dead helpers in the tool-split surface
- [x] Add semver edge-case tests where coverage is still thin
- [x] Update `docs/skills.md` with semver compatibility notes
- [x] Document the final tool registry/module pattern
- [x] Run `cargo test --workspace` or capture remaining blockers
- [x] Run `cargo clippy -- -D warnings` or capture remaining blockers
- [x] Run `cargo fmt --check`
- [ ] Commit tranche 5
- [ ] Push tranche 5 branch
- [ ] Open tranche 5 PR
- [ ] Merge tranche 5 PR

## Remaining Global Quality Blockers

- `cargo clippy -p borgclaw-core -- -D warnings` still fails on pre-existing repo-wide issues outside the tool-split tranche, including:
- `borgclaw-core/src/agent/mod.rs`: `clippy::needless_borrow`
- `borgclaw-core/src/config/mod.rs`: `clippy::module_inception`
- `borgclaw-core/src/scheduler/jobs.rs`: `clippy::derivable_impls`
- `borgclaw-core/src/skills/browser.rs`: `clippy::derivable_impls`
- `borgclaw-core/src/skills/github.rs`: `clippy::too_many_arguments`
- `borgclaw-core/src/skills/plugin.rs`: `clippy::if_same_then_else`
- `borgclaw-core/src/skills/stt.rs`: `clippy::needless_borrows_for_generic_args`
