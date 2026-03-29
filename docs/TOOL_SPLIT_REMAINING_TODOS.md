# Tool Split Remaining Todos

> Created: 2026-03-29
> Resume point after merged tranches for `TICKET-087` and early `TICKET-086`.

## Current State

- Completed: tranche 1 `TICKET-087-semver-and-tool-split-plan`
- Completed: tranche 2 `TICKET-086-tools-types-extraction`
- Completed: tranche 3a `TICKET-086-memory-tool-module`
- Completed: tranche 3b `TICKET-086-tools-core-modules`
- Completed: tranche 4a `TICKET-086-tools-service-runtime-modules`
- Completed: tranche 4b `TICKET-086-tools-github-google-modules`
- Ready to land: tranche 4c `TICKET-086-tools-browser-media-modules`

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
- [ ] Commit tranche 4c
- [ ] Push tranche 4c branch
- [ ] Open tranche 4c PR
- [ ] Merge tranche 4c PR

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
