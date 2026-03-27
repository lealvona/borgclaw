# BorgClaw Deployment Guide

## Quick Deploy

```bash
# 1. Clone and enter directory
git clone https://github.com/lealvona/borgclaw.git
cd borgclaw

# 2. Build release binaries
cargo build --release

# 3. Copy binaries to system path (optional)
sudo cp target/release/borgclaw /usr/local/bin/
sudo cp target/release/borgclaw-gateway /usr/local/bin/

# 4. Initialize configuration
borgclaw init

# 5. Run system check
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

### Docker (Optional)

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
