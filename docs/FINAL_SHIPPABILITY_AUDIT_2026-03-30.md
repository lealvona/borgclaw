# Final Shippability Audit: 2026-03-30

This document closes the March 2026 implementation train that repaired BorgClaw's documented contract, completed the remaining intended inspiration-derived runtime work, and re-verified the workspace from fresh `main`.

## Verification

The final audit re-ran the strict workspace checks:

- `cargo fmt --all`
- `TMPDIR=/home/lvona/src/borgclaw/.tmp cargo test --workspace -- --test-threads=1`
- `TMPDIR=/home/lvona/src/borgclaw/.tmp cargo clippy --workspace --all-targets -- -D warnings`

Result:

- formatting passed
- tests passed
- clippy passed

## Completed Train

The March 2026 PR train is now complete:

- documented memory backend contract repair
- PostgreSQL + pgvector memory support
- retrieval parity across memory backends
- runtime install helpers for optional memory dependencies
- Docker sandbox contract and hardening
- provider profile registry
- MiniMax multi-turn stability fix
- identity formats and structured transcript artifacts
- memory query extensions and external adapter support
- workspace-layered memory privacy
- PTY and background command runtime
- skills lifecycle completion
- gateway/onboarding control-plane completion
- structured fallback deliverables
- per-channel proxy settings

## Remaining Non-Goals

The following items are intentionally declined and not part of BorgClaw's required contract:

- AWS Bedrock provider support
- Composio integration
- Slack approval UI / buttons

## Residual Risk

One non-blocking release risk remains:

- `sqlx-postgres v0.7.4` still reports a Rust future-incompatibility warning related to a future toolchain change. It does not block shipping today, but it should be upgraded before a later Rust/toolchain bump.
