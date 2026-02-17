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
- `scripts/repl.sh` or `scripts/repl.ps1`
- `scripts/gateway.sh` or `scripts/gateway.ps1`
- `scripts/doctor.sh` or `scripts/doctor.ps1`

Examples:

```bash
./scripts/bootstrap.sh
./scripts/repl.sh
```

```powershell
./scripts/bootstrap.ps1
./scripts/repl.ps1
```

## Troubleshooting

- Linker errors on Windows: ensure Visual Studio Build Tools / Windows SDK is installed.
- `cargo` not found: restart shell after `rustup` install, or add cargo bin to `PATH`.
- Gateway port conflict: stop process on `18789` or change source to bind another port.
