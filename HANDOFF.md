# BorgClaw Codebase Handoff

**Date:** 2026-03-31  
**Version:** 0.17.0  
**Status:** All tests passing (490+ tests)

---

## Recent Work Completed (Last 13 PRs)

### TICKET-120: Bootstrap --force Flag (#286)
- Added `--force` flag to `./scripts/bootstrap.sh` for clean rebuilds
- Usage: `./scripts/bootstrap.sh --force` runs `cargo clean --release` then builds
- Also added `--help` flag for usage information

### TICKET-119: Bootstrap Auto-Rebuild Fix (#285)
- Fixed issue where bootstrap wouldn't rebuild when source files were newer than binaries
- Added `source_is_newer()` function that compares mtime of `.rs` and `Cargo.toml` files vs binaries
- Auto-triggers `cargo clean --release` when source is detected as newer

### TICKET-118: Setup Scripts Improvements (#284)
**Major overhaul of setup scripts:**

1. **bootstrap.sh** - System initialization (builds ALL targets, creates global keys/stores)
2. **onboarding.sh** - User configuration (clear questions, external setup docs, input validation)
3. **install-github.sh** - GitHub PAT setup with API validation
4. **install-google-oauth.sh** - Google OAuth step-by-step guide
5. **lib/validation.sh** - Reusable validation library

Key features:
- Port availability checking with multiple fallback methods
- Token validation (GitHub, Telegram) before accepting
- Clear external setup instructions with direct links
- Provider selection with format validation

### TICKET-117: OAuth Happy Path (#283)
**Google Workspace OAuth implementation:**
- `OAuthPendingStore` - Thread-safe storage with 10-minute expiry
- `OAuthState` - Tracks session_id, user_id, channel, group_id for cross-channel routing
- `google_authenticate` tool - Generates OAuth URL with unique state parameter
- Gateway `/oauth/callback` endpoint - Handles Google redirect, validates state, exchanges code
- Token exchange via `GoogleAuth::exchange_code()`
- Success/failure HTML responses with postMessage support

**⚠️ KNOWN LIMITATION:** OAuth callback needs channel notification system (see "Pending Work")

### TICKET-116: Tool Parser Fix (#282)
- Strip `<think>` blocks before parsing tool commands
- Prevents MiniMax and other models' reasoning blocks from interfering with tool detection

### TICKET-115: Comprehensive Tool Parser (#281)
- Handles 7+ LLM output formats:
  - XML `<invoke name="...">` format
  - MiniMax native `[TOOL_CALL]` format
  - Markdown code blocks
  - `[/!tool_invocation]` format
  - Path normalization (`/../tool` → `/tool`)
  - Space handling (`/ tool` → `/tool`)

### TICKET-114: Provider Uniformity (#279)
- All 7 providers (Anthropic, OpenAI, Google, Kimi, MiniMax, Z) now support:
  - Think block stripping
  - API base URL env var overrides
  - Consistent error handling

### TICKET-113, 112, 111: Earlier Parser/Markdown Work
- Tool call parsing fixes for various LLM formats
- Markdown support per channel type
- Think block stripping from responses

---

## Current State

### Architecture
```
borgclaw/
├── borgclaw-core/      # Core library (agent, tools, memory, channels, security)
├── borgclaw-cli/       # CLI binary (REPL, commands)
├── borgclaw-gateway/   # WebSocket gateway server (Axum)
├── scripts/            # Setup and utility scripts
│   ├── bootstrap.sh    # System initialization
│   ├── onboarding.sh   # User configuration
│   ├── install-*.sh    # Component installers
│   └── lib/            # Script libraries
├── skills/             # Project-specific skill definitions
└── docs/               # Documentation
```

### Dependency Flow
```
borgclaw-cli ──────┐
                   ├──> borgclaw-core (library)
borgclaw-gateway ──┘
```

### Build Status
- ✅ All 490 tests passing
- ✅ Clippy clean (no warnings in gateway)
- ✅ Release builds working
- ✅ Bootstrap builds both binaries

### Key Configuration Files
- `~/.config/borgclaw/config.toml` - Runtime configuration
- `~/.borgclaw/secrets.enc` - Encrypted secrets store
- `~/.borgclaw/secrets.enc.key` - Encryption key (BACK THIS UP!)

---

## Pending Work / Known Issues

### 1. OAuth Callback Routing And Token Scoping (PRIORITY)
**Locations:** `borgclaw-gateway/src/main.rs`, `borgclaw-core/src/skills/google.rs`

The March 31 follow-up repaired the broken callback-state handoff by persisting pending OAuth state next to the Google token path, and the gateway now sends a direct completion message back to Telegram when the originating channel is Telegram.

**Still open:**
- WebSocket / CLI / browser-originated OAuth flows do not get a live in-band callback message; they still rely on the browser success page and `postMessage`
- Google tokens are still persisted to a shared token path rather than being scoped per user/session for multi-user operation
- Gateway WebSocket connections track metadata only, not outbound session actors, so callback-driven push messages cannot yet target active WebSocket clients

### 2. Token Persistence
- OAuth tokens from `exchange_code()` are saved to disk
- Pending callback state now survives across the tool-runtime and gateway boundary
- Token ownership is still not associated with specific users/sessions for multi-user support

### 3. WebSocket Port Validation Edge Cases
- Port validation works for most cases
- Edge case: If gateway is running but on different port, detection might miss it
- Consider adding process-based detection

### 4. Documentation Updates
- Some docs may reference old setup procedures
- New scripts (`install-github.sh`, `install-google-oauth.sh`) need documentation links

---

## Scripts Reference

### Setup Scripts
| Script | Purpose | When to Run |
|--------|---------|-------------|
| `bootstrap.sh` | Build all targets, create keys/stores | First time setup |
| `bootstrap.sh --force` | Clean rebuild | When source changed |
| `onboarding.sh` | User configuration | After bootstrap |
| `onboarding.sh --quick` | Minimal config | Skip optional integrations |
| `doctor.sh` | System health check | Anytime |

### Install Scripts
| Script | External Setup Required |
|--------|------------------------|
| `install-github.sh` | GitHub PAT at https://github.com/settings/tokens |
| `install-google-oauth.sh` | Google Cloud Console OAuth credentials |
| `install-playwright.sh` | Node.js 18+ |
| `install-ollama.sh` | None (auto-installs) |
| `install-pgvector.sh` | Docker |
| `install-docker-sandbox.sh` | Docker |

### Runtime Scripts
| Script | Purpose |
|--------|---------|
| `repl.sh` | Start interactive CLI |
| `gateway.sh` | Start WebSocket gateway |
| `clean-build-cache.sh` | Trim or clean build cache |

---

## Testing

### Run All Tests
```bash
cargo test
```

### Run Specific Crate
```bash
cargo test -p borgclaw-core
cargo test -p borgclaw-gateway
```

### Run With Output
```bash
cargo test -- --nocapture
```

### Check/Lint
```bash
cargo check
cargo clippy
```

---

## Key Code Locations

### OAuth Implementation
- `borgclaw-core/src/skills/google.rs` - `OAuthState`, `OAuthPendingStore`, `GoogleAuth`
- `borgclaw-core/src/agent/tools/google.rs` - `google_authenticate()` tool
- `borgclaw-gateway/src/main.rs` - `oauth_callback_handler()`

### Tool Parser
- `borgclaw-core/src/agent/tools/mod.rs` - `parse_tool_command()`, format detection
- `borgclaw-core/src/agent/mod.rs` - `strip_think_blocks()` integration

### Validation Library
- `scripts/lib/validation.sh` - Port, token, OAuth URL validation functions

---

## Environment Variables

### Provider API Keys (set via `borgclaw secrets set`)
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`
- `GOOGLE_API_KEY`
- `KIMI_API_KEY`
- `MINIMAX_API_KEY`
- `Z_API_KEY`

### Other Secrets
- `GITHUB_TOKEN` - GitHub PAT
- `GOOGLE_CLIENT_ID` - OAuth client ID
- `GOOGLE_CLIENT_SECRET` - OAuth client secret
- `TELEGRAM_BOT_TOKEN` - Telegram bot token

### Configuration Overrides
- `BORGCLAW_CONFIG` - Custom config file path
- `BORGCLAW_PROVIDER` - Override provider
- `BORGCLAW_MODEL` - Override model

---

## For the Next Agent

### If Continuing OAuth Work
1. Look at `oauth_callback_handler()` in `borgclaw-gateway/src/main.rs`
2. The `OAuthState` struct has `session_id`, `user_id`, `channel`, `group_id`
3. Pending OAuth state is now stored in a deterministic `.pending-oauth.json` file alongside the configured Google token path
4. Remaining work is live callback delivery for non-Telegram channels plus per-user token scoping

### If Adding New Features
- Follow existing patterns in `borgclaw-core/src/agent/tools/`
- Add tests in `#[cfg(test)] mod tests` blocks
- Update `AGENTS.md` if changing architecture
- Update relevant docs in `docs/`

### If Fixing Bugs
- Run `cargo test` to ensure no regressions
- Run `./scripts/doctor.sh` to verify setup
- Check `DECISION_LOG.md` for architectural context

### Common Commands
```bash
# Quick check
./scripts/doctor.sh

# Full test
./scripts/with-build-env.sh cargo test

# Check formatting
cargo fmt --check

# Clean build
./scripts/bootstrap.sh --force
```

---

## Contact / Resources

- **Repository:** `lealvona/borgclaw`
- **Main Branch:** Protected, all changes via PR
- **Golden Path:** `git checkout main && git pull && git checkout -b TICKET-XXX-desc`
- **AGENTS.md:** Primary agent documentation
- **DECISION_LOG.md:** Architectural decisions

---

*End of Handoff Document*
