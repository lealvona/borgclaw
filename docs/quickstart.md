# Quick Start Guide

Get BorgClaw running in 5 minutes.

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust | 1.75+ | Install via [rustup](https://rustup.rs) |
| Git | 2.0+ | For cloning and updates |

### Optional
| Tool | Purpose |
|------|---------|
| Node.js 18+ | Playwright browser automation |
| signal-cli | Signal messaging channel |
| bw (Bitwarden CLI) | Vault integration |
| op (1Password CLI) | Secondary vault |

## Step 1: Clone

```bash
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw
```

## Step 2: Bootstrap

**Linux/macOS:**
```bash
./scripts/bootstrap.sh
```

**Windows:**
```powershell
.\scripts\bootstrap.ps1
```

This will:
- Check prerequisites
- Build the workspace
- Prepare the local workspace and helper scripts

## Step 3: Configure

Run the interactive onboarding wizard:

```bash
./scripts/onboarding.sh
```

You'll be prompted for:
1. **AI Provider** - OpenAI, Anthropic, Google, or Ollama
2. **API Key** - Stored in config or vault
3. **Model Selection** - Choose from available models
4. **Channels** - Enable Telegram, Signal, etc.

Alternatively, create config manually:

```bash
mkdir -p ~/.config/borgclaw
cat > ~/.config/borgclaw/config.toml << 'EOF'
[agent]
model = "claude-sonnet-4-20250514"
provider = "anthropic"

[security]
wasm_sandbox = true
EOF
```

Set your API key:
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

## Step 4: Run

### REPL Mode (Interactive)

```bash
./scripts/repl.sh
# Or: ./scripts/with-build-env.sh cargo run --bin borgclaw -- repl
```

### Gateway Mode (WebSocket)

```bash
./scripts/gateway.sh
# Or: ./scripts/with-build-env.sh cargo run --bin borgclaw-gateway
```

Gateway endpoint: `ws://localhost:3000/ws`

## Step 5: Verify

```bash
./scripts/doctor.sh    # Linux/macOS
.\scripts\doctor.ps1   # Windows
```

Expected output:
```
=== Required Tools ===
✓ rustc: 1.75.0
✓ cargo: 1.75.0
✓ git: 2.43.0

=== Build Status ===
✓ Code compiles successfully

✅ All checks passed!
```

## Optional Components

### Playwright (Browser Automation)

```bash
./scripts/install-playwright.sh    # Linux/macOS
.\scripts\install-playwright.ps1   # Windows
```

### Whisper.cpp (Local STT)

```bash
./scripts/install-whisper.sh    # Linux/macOS
.\scripts\install-whisper.ps1   # Windows
```

## Troubleshooting

### Build fails with "linker not found"
- **Ubuntu/Debian**: `sudo apt install build-essential`
- **macOS**: `xcode-select --install`
- **Windows**: Install Visual Studio Build Tools

### Build artifacts consume too much disk
- Run `./scripts/clean-build-cache.sh` to trim incremental caches and stale temp scratch
- Run `./scripts/clean-build-cache.sh --all` for a full `cargo clean`
- Prefer `./scripts/with-build-env.sh cargo ...` for manual Cargo commands so temp files stay in the repo cache instead of spilling into `/tmp`

### "Permission denied" on scripts
```bash
chmod +x scripts/*.sh
```

### Gateway port conflict
Stop process on port 3000 or change the configured WebSocket port.

## Next Steps

- [Configure channels](channels.md) - Telegram, Signal, Webhook
- [Set up memory](memory.md) - Session and solution patterns
- [Add integrations](integrations.md) - GitHub, Google, Browser
- [Review security](security.md) - WASM sandbox, vault
