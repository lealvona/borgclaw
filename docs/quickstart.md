# BorgClaw Quickstart

This guide gets a local dev instance running in minutes.

## 1) Prerequisites

- Rust toolchain (`rustup`, `cargo`)
- Git

Verify:

```bash
cargo --version
rustc --version
```

## 2) Build

From repo root:

```bash
cargo build
```

## 3) Initialize config

```bash
cargo run --bin borgclaw -- init
```

This launches the interactive onboarding wizard with:

- color-coded required vs optional prompts
- live provider/model discovery (with fallback defaults)
- repeatable update/delete flows on subsequent runs
- automatic `.env` generation
- optional auto-start into REPL

Default config path:

- Linux/macOS: `~/.config/borgclaw/config.toml`
- Windows: `%APPDATA%\\borgclaw\\config.toml` (via `dirs::config_dir`)

## 4) Start CLI REPL

```bash
cargo run --bin borgclaw -- repl
```

Useful REPL commands:

- `help`
- `exit`

## 5) Start WebSocket gateway

```bash
cargo run --bin borgclaw-gateway
```

Gateway endpoint:

- `ws://localhost:18789/ws`

## 6) Convenience scripts

Use the scripts in `scripts/`:

- `scripts/bootstrap.sh` or `scripts/bootstrap.ps1`
- `scripts/onboarding.sh` or `scripts/onboarding.ps1`
- `scripts/repl.sh` or `scripts/repl.ps1`
- `scripts/gateway.sh` or `scripts/gateway.ps1`
- `scripts/doctor.sh` or `scripts/doctor.ps1`

Examples:

```bash
./scripts/bootstrap.sh
./scripts/onboarding.sh
./scripts/repl.sh
```

```powershell
./scripts/bootstrap.ps1
./scripts/onboarding.ps1
./scripts/repl.ps1
```

### Component Registrar (Title + Chapter)

Use onboarding in component mode to add/update/delete specific pieces:

```bash
cargo run --bin borgclaw -- init --component channel --chapter telegram --action add
cargo run --bin borgclaw -- init --component sandbox --chapter docker --action update
cargo run --bin borgclaw -- init --component channel --chapter telegram --action delete
```

## Troubleshooting

- Linker errors on Windows: ensure Visual Studio Build Tools / Windows SDK is installed.
- `cargo` not found: restart shell after `rustup` install, or add cargo bin to `PATH`.
- Gateway port conflict: stop process on `18789` or change source to bind another port.
