# Tool Split Remaining Todos

> Created: 2026-03-29
> Resume point after merged tranches for `TICKET-087` and early `TICKET-086`.

## Current State

- Completed: tranche 1 `TICKET-087-semver-and-tool-split-plan`
- Completed: tranche 2 `TICKET-086-tools-types-extraction`
- Completed: tranche 3a `TICKET-086-memory-tool-module`
- Completed: tranche 3b `TICKET-086-tools-core-modules`
- In progress: tranche 4a `TICKET-086-tools-service-runtime-modules`

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
- [ ] Commit tranche 4a
- [ ] Push tranche 4a branch
- [ ] Open tranche 4a PR
- [ ] Merge tranche 4a PR

### Tranche 4b: Remaining Service Tool Modules

- [ ] Extract `github.rs`
- [ ] Extract `google.rs`
- [ ] Extract `browser.rs`
- [ ] Extract `media.rs`
- [ ] Run `cargo check`
- [ ] Run `cargo test -p borgclaw-core`
- [ ] Commit tranche 4b
- [ ] Push tranche 4b branch
- [ ] Open tranche 4b PR
- [ ] Merge tranche 4b PR

### Tranche 5: Cleanup and Docs

- [ ] Remove tranche-related warnings and dead helpers
- [ ] Add semver edge-case tests where coverage is still thin
- [ ] Update `docs/skills.md` with semver compatibility notes
- [ ] Document the final tool registry/module pattern
- [ ] Run `cargo test --workspace`
- [ ] Run `cargo clippy -- -D warnings`
- [ ] Run `cargo fmt --check`
- [ ] Commit tranche 5
- [ ] Push tranche 5 branch
- [ ] Open tranche 5 PR
- [ ] Merge tranche 5 PR
