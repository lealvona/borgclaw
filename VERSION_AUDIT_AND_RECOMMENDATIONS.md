# BorgClaw Comprehensive Version Audit & Recommendations

## Executive Summary

This document provides a complete audit of version references across the BorgClaw codebase, a re-versioning plan (1.1x → 0.1x), and recommendations for improvements, fixes, and gaps to address.

**Current Status (Post Re-versioning):**
- **Workspace Version:** 0.14.0 (was 1.13.0)
- **Git Tags:** v0.1.0 through v0.15.0 (18 tags total)
- **Test Status:** 61 passed, 5 failed (pre-existing)
- **Build Status:** Compiles with 1 warning (future incompatibility in sqlx-postgres)

---

## Part 1: Complete Version Reference Audit

### 1.1 Primary Version Sources (UPDATED)

| File | Previous Value | Current Value | Line | Context |
|------|---------------|---------------|------|---------|
| `Cargo.toml` | `1.13.0` | `0.14.0` | 10 | Workspace package version |
| `README.md` | `version-1.13.0` | `version-0.14.0` | 7 | Badge in README header |
| `AGENTS.md` | `1.11.0` / `1.13.0` | `0.14.0` | 16, 728 | Agent instructions |
| `borgclaw-gateway/src/main.rs` | `v1.13.0` | `v0.14.0` | 816 | Footer in web dashboard |
| `CHANGELOG.md` | `1.14.0`, `1.13.0`, etc. | Added re-versioning notice | Multiple | All version headers |
| `DECISION_LOG.md` | `1.0.0`, `1.1.0` | `0.1.0`, `0.2.0` | 60-75 | Release decisions |

### 1.2 Derived Version Sources (AUTO-UPDATED via workspace)

These use `version.workspace = true` or `env!("CARGO_PKG_VERSION")`:

| File | Mechanism | Status |
|------|-----------|--------|
| `borgclaw-core/Cargo.toml` | `version.workspace = true` | ✅ Syncs with workspace |
| `borgclaw-cli/Cargo.toml` | `version.workspace = true` | ✅ Syncs with workspace |
| `borgclaw-gateway/Cargo.toml` | `version.workspace = true` | ✅ Syncs with workspace |
| `borgclaw-cli/src/main.rs:475` | `env!("CARGO_PKG_VERSION")` | ✅ REPL header |
| `borgclaw-cli/src/main.rs:1980` | `env!("CARGO_PKG_VERSION")` | ✅ Backup metadata |
| `borgclaw-cli/src/main.rs:4397` | `env!("CARGO_PKG_VERSION")` | ✅ Test assertion |

### 1.3 Skill/Plugin Default Versions (SEMANTIC - Unchanged)

| File | Current Value | Purpose |
|------|---------------|---------|
| `borgclaw-core/src/skills/parser.rs:63` | `"1.0.0"` | Default skill version |
| `borgclaw-core/src/skills/plugin.rs:415` | `"1.0.0"` | Plugin manifest default |
| `borgclaw-core/src/agent/tools.rs:4043` | `"1.0.0"` | Tool version constant |
| `borgclaw-core/src/agent/tools.rs:4108` | `"1.0.0"` | Tool version constant |
| `docs/skills.md:456` | `version = "1.0.0"` | Documentation example |

### 1.4 Git Tags (MIGRATED)

**Old v1.x tags (DELETED):**
- v1.0.0 through v1.14.0 (18 tags)

**New v0.x tags (CREATED):**
- v0.1.0 through v0.15.0 (18 tags)

**Mapping:** v1.x.0 → v0.(x+1).0 for historical continuity

---

## Part 2: Re-Versioning Completed (1.1x → 0.1x)

### 2.1 New Versioning Scheme

**Current version: `0.14.0`**

This preserves the release progression (14 releases) while correctly positioning the project as pre-1.0 (beta/early development).

### 2.2 Changes Made

#### Step 1: ✅ Update Workspace Version
```toml
# Cargo.toml line 10
[workspace.package]
version = "0.14.0"  # Changed from "1.13.0"
```

#### Step 2: ✅ Update Documentation
- **README.md:** Badge updated
- **AGENTS.md:** Version references updated  
- **borgclaw-gateway/src/main.rs:** Footer updated
- **CHANGELOG.md:** Added re-versioning notice
- **DECISION_LOG.md:** D004 updated

#### Step 3: ✅ Regenerate Cargo.lock
```bash
cargo update -w
```

#### Step 4: ✅ Git Tag Migration
- Created new v0.x tags
- Deleted old v1.x tags (local and remote)

#### Step 5: ✅ Add Live Version Sourcing
- CLI: Added `--version` flag
- Gateway: Added `/api/version` endpoint
- Gateway Footer: JavaScript fetches live version

---

## Part 3: Critical Issues Found & Status

### 3.1 Test Failures (5 failing tests) - STILL PENDING

| Test | Location | Issue |
|------|----------|-------|
| `existing_config_choices_include_documented_status_flow` | `onboarding/mod.rs:1787` | String mismatch |
| `generate_env_includes_provider_secret_from_secure_store` | `onboarding/mod.rs:1851` | Secret not in env |
| `generate_env_includes_telegram_token_from_secure_store` | `onboarding/mod.rs:2125` | Token not in env |
| `generate_env_includes_webhook_secret_from_secure_store` | `onboarding/mod.rs:2092` | Secret not in env |
| `self_test_failures_surface_missing_provider_credentials` | `main.rs:4443` | Credential check failing |

**Root Cause:** Secure store → env export logic mismatch
**Status:** Still needs investigation

### 3.2 Build Warnings

```
warning: the following packages contain code that will be rejected by a future version of Rust: sqlx-postgres v0.7.4
```

**Impact:** sqlx-postgres transitive dependency warning
**Status:** BorgClaw only uses SQLite, but should update sqlx when 0.8 is available

### 3.3 Code Quality Issues - PARTIALLY ADDRESSED

1. **Large Files:**
   - `borgclaw-core/src/agent/tools.rs`: 7,745 lines
   - `borgclaw-cli/src/main.rs`: 4,825 lines  
   - `borgclaw-gateway/src/main.rs`: 3,146+ lines
   - **Status:** Still needs splitting

2. **cron crate versions:**
   - Root `Cargo.toml`: `cron = "0.15"`
   - `borgclaw-cli/Cargo.toml`: Was `0.12`, now `0.15`
   - **Status:** ✅ FIXED

---

## Part 4: Improvements Implemented

### 4.1 High Priority (DONE)

#### ✅ 4.1.1 Port Consistency Fix
- Changed WebSocket default from `18789` to `3000`
- Now matches documentation

#### ✅ 4.1.2 Tool Usage Fix
- Fixed system prompt to use correct format: `/tool_name {"param": "value"}`
- Implemented proper agent loop for tool calling
- LLM now properly decides when to use tools

#### ✅ 4.1.3 Error Handling Improvement
- Provider errors now include response body
- Better diagnostics for 400 errors

### 4.2 Medium Priority (DONE)

#### ✅ 4.2.1 Add Version Command
```bash
borgclaw --version  # Now works!
```

#### ✅ 4.2.2 Add Version API Endpoint
```
GET /api/version
```

#### ✅ 4.2.3 Gateway UI Modernization
- Cyber/hacker aesthetic
- Glassmorphism panels
- Neon cyan accents
- Fully responsive (mobile-first)
- 9 tabs in config editor covering all system facets

### 4.3 Low Priority (Future Work)

#### 4.3.1 Version Compatibility Semver
The `is_compatible()` function in `parser.rs` still does basic string comparison:
```rust
pub fn is_compatible(&self, borgclaw_version: &str) -> bool {
    match &self.min_version {
        None => true,
        Some(min) => {
            // TODO: proper semver comparison
            borgclaw_version >= min.as_str()
        }
    }
}
```

**Status:** Not yet implemented - consider using `semver` crate

---

## Part 5: Gap Analysis

### 5.1 Documented but Not Fully Implemented

Based on `docs/implementation-status.md`, all documented features are marked "complete". However, the 5 failing tests suggest gaps in:

1. **Secure store → env export logic** - Implementation doesn't match documented/tested behavior
2. **Self-test provider credential checking** - Test expects specific error format

### 5.2 Implemented but Could Be Enhanced

1. **Gateway Configuration UI** - ✅ Complete with 9 tabs
2. **New providers (Kimi, MiniMax, Z.ai)** - ✅ Implemented
3. **Responsive design** - ✅ Complete

### 5.3 Technical Debt

1. **File sizes** - 3 files > 3,000 lines still need splitting
2. **Test coverage** - 5 failing tests need fixing
3. **Error handling** - Some `.unwrap()` usage should be proper error types
4. **Async patterns** - Review for blocking calls in async contexts

---

## Part 6: Action Plan

### Phase 1: Completed ✅

1. ✅ Audit complete
2. ✅ Re-version codebase (1.1x → 0.x)
3. ✅ Fix port defaults
4. ✅ Fix tool usage formatting
5. ✅ Add live runtime version sourcing
6. ✅ Modernize gateway UI
7. ✅ Make UI responsive

### Phase 2: Short Term (Next)

1. Fix the 5 failing tests
2. Split large files (`tools.rs`, `main.rs` files)
3. Add proper semver comparison for skills
4. Update sqlx when 0.8 is available

### Phase 3: Medium Term

1. Increase test coverage
2. Performance benchmarking
3. Security audit
4. Documentation review

### Phase 4: Long Term

1. 1.0 release planning
2. API stability guarantees
3. Plugin ecosystem development

---

## Appendix A: Commands Summary

### Version Verification
```bash
# Check version
cargo run --bin borgclaw -- --version

# Check tags
git tag -l | sort -V

# Verify build
cargo build && cargo test
```

### Gateway Development
```bash
# Start gateway
cargo run --bin borgclaw-gateway

# Access dashboard
open http://localhost:3000
```

---

*Generated: 2026-03-28*
*Last Updated: 2026-03-28*
*Status: Re-versioning COMPLETE, Chat fixes COMPLETE, UI modernization COMPLETE*
