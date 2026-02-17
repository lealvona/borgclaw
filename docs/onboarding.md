# Onboarding Guide

## Goals

BorgClaw onboarding is designed to be:

- repeatable
- color-coded for clarity
- update/delete friendly on subsequent runs
- capable of configuring provider, model, channels, sandbox, memory, and registries

## Run

```bash
cargo run --bin borgclaw -- init
```

## Subsequent Runs

When config exists, onboarding offers options to:

- update current settings
- add a new component
- delete a component
- keep config and regenerate `.env`

## Component Registrar

The title/chapter registrar is available through CLI arguments:

```bash
# add/update
cargo run --bin borgclaw -- init --component channel --chapter websocket --action add

# delete
cargo run --bin borgclaw -- init --component channel --chapter websocket --action delete
```

The registrar is persisted under `registrar.chapters` in `config.toml`.

## Provider Registry

On first run, onboarding writes a modular provider registry file next to config:

- `providers.toml`

This file can be edited to add/remove providers, API endpoints, and default models.

## .env Generation

Onboarding always generates `.env` in the current working directory, then saves config.

Common values include:

- `BORGCLAW_PROVIDER`
- `BORGCLAW_MODEL`
- provider API key variable(s) when entered in onboarding
