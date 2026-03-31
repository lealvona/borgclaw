# Implementation Plan: Docker Sandbox Contract and Remaining Hardening

> Historical note as of March 31, 2026: this plan is complete and retained as an audit artifact. Use `ROADMAP.md` for the active extension roadmap.

This document is the execution source of truth for the Docker sandbox tranche started on March 30, 2026.

It assumes the merged baseline from PRs `#251` through `#257`, and focuses on the remaining gaps that are still either unimplemented, incomplete, or stale in repo guidance.

## Gap Matrix

| Area | Status | Source | Implementation Owner | Tests | Docs |
|---|---|---|---|---|---|
| Docker sandbox for shell execution | Missing | `docs/inspirations.md`, `CUT_CONTENT.MD` | Core runtime + security | Required | Required |
| Background execution sandbox inheritance | Partial | `docs/inspirations.md`, `ROADMAP.md` | Core runtime | Required | Required |
| Approval parity across immediate/deferred execution | Partial | `docs/inspirations.md`, `ROADMAP.md` | Core runtime | Required | Required |
| Docker runtime install/bootstrap support | Missing | `docs/inspirations.md` appendix | Scripts | Required | Required |
| Stale internal deployment guidance | Stale | `.agents/skills/deployment-onboarding/SKILL.md` | Docs/skills | Regression-only | Required |
| Stale roadmap/tool-split history | Stale | `ROADMAP.md`, `docs/TOOL_SPLIT_REMAINING_TODOS.md` | Docs | N/A | Required |

## Remaining Runtime Work

1. Add typed `security.docker` config parsing and validation.
2. Route `execute_command` through a shared host-vs-Docker execution adapter.
3. Keep Docker execution inside the existing security pipeline.
4. Make scheduled, heartbeat, and sub-agent command execution inherit the same path automatically.
5. Expose Docker runtime state through CLI status/doctor/self-test and gateway config/doctor surfaces.
6. Add runtime install helpers and a local sandbox image definition.

## Contract Repairs

1. Update security, onboarding, README, and gateway docs to describe the implemented Docker contract.
2. Update status/roadmap docs so historical March gaps are not still presented as live missing work.
3. Repair internal operator/agent onboarding guidance so it matches the live config contract.

## Explicitly Culled Features

Do not revive these as-is:

- bare `docker_sandbox = true`
- obsolete `Channel` trait examples
- obsolete public `Tool` trait examples
- stale gateway event wording already archived in `CUT_CONTENT.MD`

Replacement target:

- typed `[security.docker]` config with explicit image, mount, network, env, and timeout policy
