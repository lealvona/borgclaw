# Inspiration Study Skill

> Directive for studying upstream codebases and updating BorgClaw plans.

## Purpose

When working on BorgClaw, periodically study upstream inspirations to:
1. Learn new patterns and approaches
2. Update plans, specs, and roadmaps
3. Track progress against upstream implementations
4. Update dependencies and installed resources

## Upstream Repositories

| Project | URL | Best BorgClaw Use |
|---|---|---|
| OpenClaw | https://github.com/openclaw/openclaw | Onboarding, gateway UX, managed skills, transport consistency |
| ZeroClaw | https://github.com/zeroclaw-labs/zeroclaw | Core Rust architecture, config shape, provider/channel/tool abstractions |
| NanoClaw | https://github.com/qwibitai/nanoclaw | Container-first isolation, deterministic local/bootstrap flows |
| IronClaw | https://github.com/nearai/ironclaw | Security pipeline, unified tool registry, defense-in-depth |
| PicoClaw | https://github.com/sipeed/picoclaw | Session/workspace layout, heartbeat semantics, subagent inheritance |
| TinyClaw | https://github.com/TinyAGI/tinyclaw | Multi-agent queueing, retries, isolated workspaces, dead-letter |

## Workflow

### 1. Study Phase

```bash
# Clone or update inspiration repositories to /tmp
git clone https://github.com/openclaw/openclaw /tmp/openclaw
git clone https://github.com/zeroclaw-labs/zeroclaw /tmp/zeroclaw
git clone https://github.com/qwibitai/nanoclaw /tmp/nanoclaw
git clone https://github.com/nearai/ironclaw /tmp/ironclaw
git clone https://github.com/sipeed/picoclaw /tmp/picoclaw
git clone https://github.com/TinyAGI/tinyclaw /tmp/tinyclaw

# For each repository, study:
# - Recent commits (last 30 days)
# - Release notes
# - New features or changes
# - Architecture patterns
# - Configuration formats
```

### 2. Documentation Updates

Update these files based on findings:

| File | When to Update |
|------|----------------|
| `docs/inspirations.md` | New patterns, architectural insights |
| `docs/implementation-status.md` | Completed features, new gaps |
| `ROADMAP.md` | Timeline adjustments, new priorities |
| `docs/2026-03-09-plan.md` | Sprint planning alignment |
| `CHANGELOG.md` | Version tracking |

### 3. Update Process

Follow AGENTS.md git workflow rules strictly — never push directly to main.

```bash
# 1. Start from latest main, create feature branch
git checkout main && git pull
git checkout -b TICKET-<number>-study-upstream

# 2. Update documentation
# - Review recent commits in each upstream repo
# - Note significant changes
# - Update inspiration matrix
# - Mark completed items in implementation status

# 3. Commit with reference
git add docs/
git commit -m "[RESEARCH] Update inspiration analysis from upstream study

- OpenClaw: Added notes on wizard UX maturity
- ZeroClaw: Updated architecture patterns
- Status: Marked 3 items as completed"

# 4. Run checks before submitting
cargo test && cargo fmt --check && cargo clippy -- -D warnings

# 5. Push and create PR
git push -u origin TICKET-<number>-study-upstream
gh pr create --title "[TICKET-<number>] Update upstream inspiration analysis"
```

### 4. Dependency Updates

Check for dependency updates monthly:

```bash
cargo outdated

# Review dependency changes in upstream repos
cd /tmp/openclaw && git log --since="30 days ago" --oneline -- Cargo.toml

# Update BorgClaw dependencies if beneficial
cargo update
```

### 5. Tool/Resource Updates

Check installed tools for updates:

| Tool | Location | Update Command |
|------|----------|----------------|
| whisper.cpp | `.local/tools/whisper.cpp` | `git pull && cmake rebuild` |
| playwright | `.local/tools/playwright` | `npm update` |
| Models | `.local/tools/*/models/` | Per model download script |

```bash
cd .local/tools/whisper.cpp && git log --since="30 days ago" --oneline
cd .local/tools/whisper.cpp && cmake --build build --config Release
cargo test
```

## Study Checklist

When reviewing an upstream repo:

- [ ] What new features did they add recently?
- [ ] How do they solve problems BorgClaw has?
- [ ] What architectural patterns are they using?
- [ ] How has their config/schema evolved?
- [ ] What security patterns do they implement?
- [ ] What onboarding improvements exist?
- [ ] How do they handle error cases BorgClaw struggles with?
- [ ] What documentation improvements could be copied?

## Key Upstream Priorities (as of March 2026)

These are the current highest-value things to watch upstream:

1. **OpenClaw**: Compaction correctness, transport-specific SSRF/media policy, managed skill lifecycle, gateway control-plane maturity
2. **ZeroClaw**: Runtime model switching, self-test/healthcheck depth, persistent gateway sessions, rollback-capable operations
3. **IronClaw**: Unified tool registry (built-in + MCP + WASM), security pipeline sequencing, rate-limit retry semantics
4. **PicoClaw**: Inherited workspace restrictions across subagents/heartbeat, symlink-aware allowed-root normalization, startup dependency checks
5. **TinyClaw**: Persisted schedule management, workspace-source-of-truth conventions, dead-letter/retry design
6. **NanoClaw**: Deterministic local/bootstrap flows, read-only capabilities/status surfaces

## Output Format

After studying, document findings in `docs/inspirations.md` under a dated section:

```markdown
## Upstream Follow-Up: YYYY-MM-DD

### OpenClaw
- **Recent**: [what they shipped]
- **Interesting**: [pattern worth noting]
- **BorgClaw Impact**: [what this means for us]

### ZeroClaw
- ...

### Status Changes
- [x] Feature A (was: in progress, now: complete in upstream)
- [ ] Feature B (still gap)

### Recommendations
1. Consider adopting [pattern] from OpenClaw
2. BorgClaw's approach to [problem] is [better/worse] because...
```

## Anti-Patterns

- Do not copy Node-first operational surfaces from OpenClaw/PicoClaw without translating to BorgClaw's Rust crate boundaries
- Do not copy ZeroClaw's config breadth blindly; only add knobs BorgClaw can actually enforce
- Do not copy NanoClaw's container-first model everywhere if BorgClaw needs local-first CLI/REPL ergonomics
- Do not copy TinyClaw's multi-agent behavior until BorgClaw defines ordering, retries, cancellation, and workspace isolation as hard contracts
- Borrow the contract, not the syntax

## Skill Metadata
- **Name**: inspiration-study
- **Version**: 2.0.0
- **Tags**: research, documentation, upstream, planning
