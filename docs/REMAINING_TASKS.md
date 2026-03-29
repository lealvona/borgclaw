# Remaining Tasks for BorgClaw v0.14.0

> Document created: 2026-03-28
> Based on analysis of codebase during TICKET-086 and TICKET-087 implementation

## TICKET-087: Semver Comparison for Skill Versioning ✅ COMPLETE

**Status:** Implemented and tested

**Changes Made:**
- Added `semver = "1.0"` dependency to `borgclaw-core/Cargo.toml`
- Updated `SkillManifest::is_compatible()` to use proper semver comparison
- Added `normalize_version()` helper to handle partial versions (e.g., "1.8" → "1.8.0")
- Updated `version_gte()` function with proper semver logic
- All 5 parser tests pass

**Implementation Details:**
```rust
// Before (string comparison - problematic):
borgclaw_version >= min.as_str()

// After (proper semver):
let current = semver::Version::parse(&normalize_version(current_version))?;
let minimum = semver::Version::parse(&normalize_version(min))?;
current >= minimum
```

---

## TICKET-086: Split tools.rs (7,752 lines) ⏳ INCOMPLETE

**Status:** Attempted but rolled back due to complexity. Detailed analysis below.

### Current Structure

File: `borgclaw-core/src/agent/tools.rs` (7,752 lines)

**Core Types Defined:**
1. `ToolRetryPolicy` (lines 23-92) - Retry configuration
2. `Tool` (lines 96-153) - Tool definition with schema
3. `ToolSchema` (lines 155-198) - JSON schema for parameters
4. `PropertySchema` (lines 203-313) - Individual property schemas
5. `ToolCall` (lines 316-366) - Parsed tool invocation
6. `ToolResult` (lines 369-443) - Tool execution result
7. `ToolRuntime` (lines 368-382 in backup) - Runtime context
8. `ToolInvocationContext` (lines 384-389 in backup) - Invocation metadata

**Tool Implementation Functions (86 total):**

| Category | Count | Functions |
|----------|-------|-----------|
| Memory | 8 | memory_store, memory_recall, memory_delete, memory_keys, memory_groups, memory_clear_group, solution_store, solution_find |
| File | 3 | read_file, write_file, delete_file |
| Shell | 1 | execute_command |
| Web | 4 | web_search, fetch_url, url_shorten, url_expand |
| GitHub | 22 | github_list_repos, github_get_repo, github_list_branches, github_create_branch, github_list_prs, github_create_pr, github_prepare_delete_branch, github_delete_branch, github_prepare_merge_pr, github_merge_pr, github_list_issues, github_create_issue, github_list_releases, github_get_file, github_create_file, github_update_file, github_delete_file, github_close_issue |
| Google | 17 | google_list_messages, google_get_message, google_send_email, google_delete_email, google_trash_email, google_search_files, google_download_file, google_list_events, google_upload_file, google_create_event, google_update_event, google_delete_event, google_create_folder, google_list_folders, google_share_file, google_list_permissions, google_remove_permission, google_move_file, google_copy_file, google_delete_file, google_get_file_details |
| Browser | 12 | browser_navigate, browser_click, browser_fill, browser_wait_for, browser_get_text, browser_get_html, browser_get_url, browser_eval_js, browser_go_back, browser_go_forward, browser_reload, browser_screenshot |
| Media | 11 | stt_transcribe, stt_transcribe_url, tts_speak, tts_speak_stream, tts_list_voices, image_generate, image_analyze, image_analyze_file, qr_encode, qr_encode_url |
| Plugin | 2 | plugin_list, plugin_invoke |
| Schedule | 4 | schedule_task, run_scheduled_tasks, approve_tool, message |

### Target Structure

```
borgclaw-core/src/agent/tools/
├── mod.rs           # Core types + registry
├── types.rs         # Tool, ToolCall, ToolResult, schemas
├── runtime.rs       # ToolRuntime, ToolInvocationContext
├── retry.rs         # ToolRetryPolicy
├── memory.rs        # Memory operations (8 functions)
├── file.rs          # File operations (3 functions)
├── shell.rs         # Command execution (1 function)
├── web.rs           # Web search/fetch (4 functions)
├── github.rs        # GitHub API (22 functions)
├── google.rs        # Google Workspace (17 functions)
├── browser.rs       # Browser automation (12 functions)
├── media.rs         # STT/TTS/Image/QR (11 functions)
├── plugin.rs        # WASM plugins (2 functions)
└── schedule.rs      # Scheduling (4 functions)
```

### Blockers and Issues Encountered

#### Issue 1: Complex ToolRuntime Dependencies

**Problem:** The `ToolRuntime` struct has many fields with complex initialization:

```rust
pub struct ToolRuntime {
    pub workspace_root: PathBuf,
    pub workspace_policy: crate::config::WorkspacePolicyConfig,
    pub memory: Arc<SqliteMemory>,
    pub heartbeat: Arc<HeartbeatEngine>,
    pub scheduler: Arc<Mutex<Scheduler>>,
    pub plugins: Arc<PluginRegistry>,
    pub skills: crate::config::SkillsConfig,
    pub mcp_servers: HashMap<String, crate::config::McpServerConfig>,
    pub security: Arc<SecurityLayer>,
    pub audit: Arc<crate::security::AuditLogger>,
    pub invocation: Option<Arc<ToolInvocationContext>>,
}
```

**Impact:** Moving this to a separate file requires re-exporting or importing:
- `SqliteMemory` from `crate::memory`
- `HeartbeatEngine` from `crate::memory::heartbeat`
- `Scheduler` from `crate::scheduler`
- `PluginRegistry` from `crate::skills`
- `SecurityLayer` from `crate::security`

**Solution:** Keep `ToolRuntime` in `mod.rs` and only extract implementation functions.

#### Issue 2: Trait Object Types

**Problem:** `BrowserSkill` is a trait, needs `Arc<dyn BrowserSkill>`:

```rust
// Current code uses:
pub browser: Option<Arc<BrowserSkill>>,  // This doesn't work

// Should be:
pub browser: Option<Arc<dyn BrowserSkill>>,  // dyn keyword needed
```

**Impact:** If we move `ToolRuntime` to a submodule, we need proper imports and trait object syntax.

**Solution:** Fix the type declaration to use `Arc<dyn BrowserSkill>`.

#### Issue 3: Recursive Types with Box

**Problem:** `ToolCall` and `ToolResult` have recursive references:

```rust
pub struct ToolCall {
    pub result: Option<ToolResult>,  // Recursive!
}

pub struct ToolResult {
    pub tool_call: Option<ToolCall>,  // Recursive!
}
```

**Error:** `recursive types have infinite size`

**Solution:** Use `Box` for indirection:
```rust
pub result: Option<Box<ToolResult>>,
pub tool_call: Option<Box<ToolCall>>,
```

#### Issue 4: Tool Function Signatures

**Problem:** Tool implementation functions have inconsistent signatures:

```rust
// Some take arguments hashmap:
async fn memory_store(
    arguments: &HashMap<String, serde_json::Value>,
    runtime: &ToolRuntime,
) -> ToolResult

// Some take runtime only:
async fn memory_keys(runtime: &ToolRuntime) -> ToolResult

// Some take no runtime:
async fn qr_encode(arguments: &HashMap<String, serde_json::Value>) -> ToolResult
```

**Impact:** Need to standardize or maintain multiple wrapper types.

**Solution:** Standardize on `(&ToolCall, &ToolRuntime) -> ToolResult` pattern used in new module structure.

#### Issue 5: Security Layer API Mismatches

**Problem:** The security layer has sync methods that were incorrectly called with `.await`:

```rust
// Incorrect:
security.check_command(&command).await

// Correct:
match security.check_command(&command) {
    CommandCheck::Blocked(reason) => ...,
    CommandCheck::Allowed => ...,
}
```

**Also:** `check_path_access` method doesn't exist - need to use `check_file_access` or similar.

#### Issue 6: GitHub Client API Differences

**Problem:** The actual `GitHubClient` API differs from what was assumed:

| Assumed | Actual |
|---------|--------|
| `list_pull_requests()` | `list_prs()` |
| `create_pull_request()` | `create_pr()` |
| `merge_pull_request()` | `merge_pr()` |
| `delete_branch(repo, branch)` | `delete_branch(repo, branch, token)` (3 args) |
| `close_issue(number)` | `close_issue(number)` but needs `u32` not `u64` |

**Problem:** Return types also differ:
- `GitHubRepo` doesn't have `stars`, `forks` fields
- `GitHubIssue` uses `html_url` not `url`
- `GitHubPullRequest` uses `html_url` not `url`

#### Issue 7: Memory API Differences

**Problem:** Memory operations differ from assumptions:

| Assumed | Actual |
|---------|--------|
| `MemoryEntry::with_key()` | Doesn't exist - set field directly |
| `memory.store(entry) -> Result<String, _>` | Returns `Result<(), _>` |
| `memory.delete(id) -> Result<bool, _>` | Returns `Result<(), _>` |
| `memory.keys(group)` | `memory.keys()` (no group param) |

#### Issue 8: Tool Registry Pattern

**Problem:** The new module structure uses a `register()` function pattern:

```rust
// New pattern in target structure:
pub fn register(tools: &mut Vec<Tool>) {
    tools.push(Tool::new("memory_store", ...));
}
```

**But current code uses:**
```rust
pub fn builtin_tools() -> Vec<Tool> {
    vec![
        Tool::new("memory_store", ...),
        ...
    ]
}
```

**Impact:** Need to either:
1. Keep the current `builtin_tools()` pattern
2. Migrate to `register()` pattern (preferred for modularity)

### Recommended Approach for Splitting

Instead of a complete rewrite, use an incremental approach:

#### Phase 1: Extract Type Definitions (Safe)

Create `borgclaw-core/src/agent/tools/types.rs`:
```rust
// Move these unchanged:
- ToolRetryPolicy
- Tool
- ToolSchema
- PropertySchema
- ToolCall
- ToolResult
```

#### Phase 2: Extract Implementation Modules (Careful)

For each category, create a submodule that ONLY contains:
1. The `register()` function to add tools to registry
2. The async implementation functions

Keep in `mod.rs`:
- `ToolRuntime` struct and methods
- `ToolInvocationContext`
- `builtin_tools()` function (calls all register functions)
- `execute_tool()` match statement (calls implementation functions)

Example for `memory.rs`:
```rust
use super::{Tool, ToolCall, ToolResult, ToolSchema, PropertySchema, ToolRuntime};
use crate::memory::{new_entry_for_group, MemoryQuery};

pub fn register(tools: &mut Vec<Tool>) {
    tools.push(Tool::new("memory_store", ...));
    tools.push(Tool::new("memory_recall", ...));
    // ... etc
}

pub async fn memory_store(call: &ToolCall, runtime: &ToolRuntime) -> ToolResult {
    // Implementation here
}

pub async fn memory_recall(call: &ToolCall, runtime: &ToolRuntime) -> ToolResult {
    // Implementation here
}
// ... etc
```

#### Phase 3: Update mod.rs

```rust
pub mod types;
mod memory;
mod file;
mod shell;
mod web;
mod github;
mod google;
mod browser;
mod media;
mod plugin;
mod schedule;

pub use types::*;

pub fn builtin_tools() -> Vec<Tool> {
    let mut tools = Vec::new();
    memory::register(&mut tools);
    file::register(&mut tools);
    shell::register(&mut tools);
    web::register(&mut tools);
    github::register(&mut tools);
    google::register(&mut tools);
    browser::register(&mut tools);
    media::register(&mut tools);
    plugin::register(&mut tools);
    schedule::register(&mut tools);
    tools
}

pub async fn execute_tool(call: &ToolCall, runtime: &ToolRuntime) -> ToolResult {
    match call.name.as_str() {
        "memory_store" => memory::memory_store(call, runtime).await,
        "memory_recall" => memory::memory_recall(call, runtime).await,
        // ... etc for all tools
        _ => ToolResult::error(format!("Unknown tool: {}", call.name)),
    }
}
```

### Pre-Split Checklist

Before attempting the split again:

- [ ] Verify all GitHub client method names match actual API
- [ ] Verify all Memory trait method signatures
- [ ] Verify SecurityLayer method signatures (sync vs async)
- [ ] Document all tool function signatures needed
- [ ] Create test to verify `builtin_tools()` returns expected count
- [ ] Run `cargo check` after each module extraction
- [ ] Run full test suite after each phase

### Testing Strategy

1. **Before split:** Create comprehensive test for `builtin_tools()`
2. **During split:** After each module, verify test still passes
3. **After split:** Full test suite must pass

### Risk Mitigation

1. **Keep backup:** Always keep `tools.rs.bak` until complete
2. **Incremental commits:** One module per commit
3. **Feature branch:** Work on `TICKET-086-tool-split` branch
4. **Code review:** Review each module extraction

---

## Other Remaining Tasks

### Code Quality

- [ ] Remove unused import: `DEFAULT_PLUGIN_VERSION` in `plugin.rs` (line 3)
- [ ] Fix unused variable warnings in test code
- [ ] Run `cargo clippy -- -D warnings` and fix issues
- [ ] Run `cargo fmt --check` and fix formatting

### Documentation

- [ ] Update `docs/skills.md` with semver compatibility notes
- [ ] Add documentation for `version_gte()` and `normalize_version()`
- [ ] Document the new tool registry pattern

### Testing

- [ ] Add more edge case tests for semver comparison:
  - Pre-release versions (e.g., "1.0.0-alpha")
  - Build metadata (e.g., "1.0.0+build123")
  - Invalid version strings
- [ ] Add integration test for skill loading with min_version

---

## Summary

| Task | Status | Notes |
|------|--------|-------|
| TICKET-087 (Semver) | ✅ Complete | All tests pass |
| TICKET-086 (Tool split) | ⏳ Blocked | Needs incremental approach |
| Code quality | ⏳ Pending | Minor warnings to fix |
| Documentation | ⏳ Pending | Update skills.md |
| Additional tests | ⏳ Pending | Edge cases for semver |

**Next Priority:** Implement the incremental tool split approach outlined above, starting with Phase 1 (type definitions).
