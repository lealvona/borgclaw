# Inspiration Study Skill

> Directive for studying upstream codebases and updating BorgClaw plans.

## Purpose

When working on BorgClaw, periodically study upstream inspirations to:
1. Learn new patterns and approaches
2. Update plans, specs, and roadmaps
3. Track progress against upstream implementations
4. Update dependencies and installed resources

## Workflow

### 1. Study Phase

```bash
# Clone or update inspiration repositories
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

```bash
# 1. Create feature branch
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

# 4. Push and create PR
git push -u origin TICKET-<number>-study-upstream
gh pr create --title "[TICKET-<number>] Update upstream inspiration analysis"
```

### 4. Dependency Updates

Check for dependency updates monthly:

```bash
# Check Rust dependencies
cargo outdated

# Check npm dependencies (if any)
npm outdated

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
# Check for tool updates
cd .local/tools/whisper.cpp && git log --since="30 days ago" --oneline

# Rebuild if needed
cd .local/tools/whisper.cpp && cmake --build build --config Release

# Verify tools still work
cargo test  # Run test suite to verify
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

## Output Format

After studying, document findings:

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

## Skill Metadata
- **Name**: inspiration-study
- **Version**: 1.0.0
- **Author**: BorgClaw Team
- **Tags**: research, documentation, upstream, planning
