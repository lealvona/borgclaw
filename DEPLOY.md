# BorgClaw Deployment Guide

## Quick Deploy

```bash
# 1. Clone and enter directory
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw

# 2. Run bootstrap (installs dependencies)
./scripts/bootstrap.sh

# 3. Build release binaries
cargo build --release

# 4. Copy binaries to system path (optional)
sudo cp target/release/borgclaw /usr/local/bin/
sudo cp target/release/borgclaw-gateway /usr/local/bin/

# 5. Initialize configuration
borgclaw init

# 6. Run system check
borgclaw doctor
borgclaw self-test
```

## Production Setup

### Systemd Service (Linux)

Create `/etc/systemd/system/borgclaw.service`:

```ini
[Unit]
Description=BorgClaw AI Agent
After=network.target

[Service]
Type=simple
User=borgclaw
WorkingDirectory=/var/lib/borgclaw
ExecStart=/usr/local/bin/borgclaw-gateway
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable borgclaw
sudo systemctl start borgclaw
```

### Optional Tool Installation

BorgClaw includes helper scripts to install optional dependencies:

### Whisper.cpp (Speech-to-Text)
```bash
./scripts/install-whisper.sh
```
Installs whisper.cpp for local speech-to-text processing.

### Bitwarden CLI (Secret Vault)
```bash
./scripts/install-bitwarden.sh
```
Installs Bitwarden CLI for external secret vault integration. Automatically configures PATH in your shell profile.

### Playwright (Browser Automation)
```bash
./scripts/install-playwright.sh
```
Installs Playwright for web browser automation capabilities.

## Docker (Optional)

See `docker/` directory for containerized deployment.

## Configuration

Edit `~/.config/borgclaw/config.toml`:

```toml
[agent]
model = "claude-sonnet-4-20250514"
provider = "anthropic"

[channels.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"

[security]
wasm_sandbox = true
prompt_injection_defense = true
```

## Monitoring

```bash
# Check status
borgclaw status

# View logs
journalctl -u borgclaw -f

# Runtime diagnostics
borgclaw runtime
```
