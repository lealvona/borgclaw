# TICKET-051: Update Documentation for New Features

## Background
PRs #182, #183, and #184 introduced significant changes that need documentation updates.

## Documentation Updates Required

### 1. docs/security.md
Update to document new security features:

#### SSRF Protection (PR #184)
- [ ] Document `SsrfGuard` and its purpose
- [ ] List blocked IP ranges:
  - localhost/loopback (127.x.x.x, ::1, localhost)
  - Private IPv4 (10/8, 172.16/12, 192.168/16)
  - Link-local (169.254/16, fe80::/10)
  - Unique local IPv6 (fc00::/7)
- [ ] Document custom allowlist/blocklist configuration
- [ ] Add examples of SSRF attempts that are blocked

#### WASM Sandbox (PR #182)
- [ ] Document WASM as the primary sandbox mechanism
- [ ] Remove Docker sandbox references
- [ ] Document wasmtime integration
- [ ] Add configuration examples for `max_instances`

### 2. docs/skills.md
Update to document new Google Drive capabilities:

#### New Tools (PR #183)
- [ ] `google_create_folder` - Create folders
- [ ] `google_list_folders` - List folders
- [ ] `google_share_file` - Share files (with approval)
- [ ] `google_list_permissions` - View permissions
- [ ] `google_remove_permission` - Revoke access (with approval)
- [ ] `google_move_file` - Move files
- [ ] `google_copy_file` - Copy files
- [ ] `google_delete_file` - Delete files (with approval)
- [ ] `google_get_file_details` - Get file metadata
- [ ] `google_batch_upload` - Upload multiple files
- [ ] `google_batch_share` - Share multiple files

For each tool, include:
- Description
- Parameters
- Return values
- Example usage
- Approval requirements (if applicable)

#### Approval Gates
- [ ] Document which operations require approval
- [ ] Explain security rationale
- [ ] Show approval workflow

### 3. README.md
- [ ] Update feature list to include new Google Drive operations
- [ ] Update security section to mention SSRF protection
- [ ] Update architecture diagram (if exists) to show WASM sandbox
- [ ] Add example of batch operations

### 4. docs/architecture.md (if exists)
- [ ] Update container/sandbox section to reflect WASM-only approach
- [ ] Document SSRF protection as part of security layer

### 5. CHANGELOG.md (or create one)
- [ ] Document TICKET-047: Docker sandbox removal
- [ ] Document TICKET-048: Google Drive enhancements
- [ ] Document TICKET-049: SSRF protection

## Acceptance Criteria
- [ ] docs/security.md updated with SSRF and WASM documentation
- [ ] docs/skills.md includes all new Google Drive tools
- [ ] README.md reflects current feature set
- [ ] All documentation is accurate and tested
- [ ] Code examples compile and work
- [ ] Documentation follows existing style guide

## Related
- PR #182: Remove Docker sandbox
- PR #183: Enhanced Google Drive Operations  
- PR #184: SSRF Protection
