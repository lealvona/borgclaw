# TICKET-050: Add Tests for New Google Drive Operations

## Background
PR #183 added 10 new Google Drive tool wrappers and DriveClient methods. These need comprehensive test coverage.

## New Operations to Test

### DriveClient Methods (borgclaw-core/src/skills/google.rs)
- [ ] `create_folder()` - Create folders with optional parent
- [ ] `list_folders()` - List folders in directory
- [ ] `share_file()` - Share files with users
- [ ] `list_permissions()` - View file permissions
- [ ] `remove_permission()` - Revoke access
- [ ] `move_file()` - Move files between folders
- [ ] `copy_file()` - Copy files
- [ ] `delete_file()` - Delete/trash files
- [ ] `get_file_details()` - Get metadata with web links
- [ ] `batch_upload()` - Upload multiple files
- [ ] `batch_share()` - Share multiple files

### Tool Wrappers (borgclaw-core/src/agent/tools.rs)
- [ ] `google_create_folder` tool
- [ ] `google_list_folders` tool
- [ ] `google_share_file` tool (approval required)
- [ ] `google_list_permissions` tool
- [ ] `google_remove_permission` tool (approval required)
- [ ] `google_move_file` tool
- [ ] `google_copy_file` tool
- [ ] `google_delete_file` tool (approval required)
- [ ] `google_get_file_details` tool
- [ ] `google_batch_upload` tool
- [ ] `google_batch_share` tool

## Test Requirements

### Unit Tests
- Mock Google API responses
- Test success cases for each operation
- Test error handling (API errors, network failures)
- Test parameter validation
- Test batch operation edge cases (empty lists, partial failures)

### Integration Tests
- Follow existing patterns in `borgclaw-core/src/skills/google.rs` tests
- Use local test stubs (don't hit real Google API)
- Test approval gate integration for destructive operations

### Test Data
```rust
// Example test fixtures
const TEST_FILE_ID: &str = "test_file_123";
const TEST_FOLDER_ID: &str = "test_folder_456";
const TEST_USER_EMAIL: &str = "test@example.com";
```

## Acceptance Criteria
- [ ] All new DriveClient methods have unit tests
- [ ] All new tool wrappers have integration tests
- [ ] Tests cover success and error cases
- [ ] Tests verify approval gates work correctly
- [ ] Code coverage for new code > 80%
- [ ] All tests pass: `cargo test --workspace`

## Related
- PR #183: Enhanced Google Drive Operations
- File: borgclaw-core/src/skills/google.rs
- File: borgclaw-core/src/agent/tools.rs
