# BorgClaw Setup & Secrets Plan

> Historical planning note as of March 31, 2026: large parts of this setup/secrets tranche landed through the March bootstrap/onboarding work, but this file still contains deferred ideas that are not part of the currently completed contract.

## Status Snapshot

Landed or largely landed:

- bootstrap/onboarding as the canonical setup path
- config-derived gateway/webhook port guidance in scripts and docs
- provider-profile-backed onboarding and secure secret storage
- helper installers for optional runtimes and Docker sandbox images

Still open or intentionally deferred:

- user-local install/uninstall helpers for BorgClaw binaries and launchers
- password-gated secret-store unlock flow (the current encrypted store is file-key based, not interactive-password gated)
- release/nightly automation as a documented product workflow

Use `ROADMAP.md` for active follow-up prioritization. The original plan items are retained below for historical context.

1. **TIGHTEN SETUP FLOW**
   - Make `bootstrap -> onboarding -> borgclaw` the single canonical path; bootstrap only builds once and reuses the built binary. Ensure `onboarding` text references the actual gateway port from config and no longer promotes redundant builds or `.env` secrets.
2. **SCRIPT INSTALL/UNINSTALL**
   - Add install/uninstall helpers for user-local binaries and launcher integration so users can install or remove BorgClaw cleanly.
3. **GATEWAY PORT DYNAMICS**
   - Replace hardcoded `3000` references with config-derived port lookups; update `scripts/gateway.*`, docs, and CLI summaries to show the real `websocket` port and webhook (Webhook defaults to 8080 fallback).
4. **EXCLUSIVE SECRET STORE**
   - Remove `.env` as a secret source (stay read-only for non-sensitive hints or drop). Ensure bootstrap/onboarding always record `config.security.secrets_path` and the doctor output references the actual secret store path.
5. **PASSWORD-GATED SECRET STORE**
   - During bootstrap/onboarding, prompt for a BorgClaw master password and derive (e.g., HKDF) the ChaCha20-Poly1305 key; store secrets only through this locked store. Require the password before `borgclaw secrets list` (and similar commands) expose secrets; keep helper reusable for scripts (e.g., pass via env). Add tests covering initialization, unlock, incorrect password, persistence.
6. **ONBOARDING SECRET UX**
   - Surface the secret-manager password flow, provider profile selection, gateway port, and next steps (doctor, gateway, install/uninstall) in onboarding completion messaging.
7. **DOCUMENTATION SURFACE**
   - Update `README.md`, `docs/quickstart.md`, `docs/onboarding.md`, `docs/gateway.md`, `docs/security.md`, and `AGENTS.md` with new workflow, install/uninstall guidance, port info, and password-protected secret store notes.
8. **NIGHTLY + RELEASE**
   - Draft a follow-up plan for nightly GitHub Actions builds with nightly tags, stable release workflow, and release notes, to be implemented after the setup/secrets tranche lands.

## Testing checklist

- `./scripts/bootstrap.sh` + confirm onboarding prompt flow (component workbench, port display)
- `./scripts/onboarding.sh --quick` with gateway port readback
- `./scripts/with-build-env.sh cargo fmt --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`
- `borgclaw secrets list` should prompt for the password and only succeed after unlock
- `./scripts/doctor.sh` should report gateway port and secret store state without exposing secrets
