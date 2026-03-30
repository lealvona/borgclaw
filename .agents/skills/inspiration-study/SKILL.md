---
name: inspiration-study
description: Study upstream codebases and update plans. Use when reviewing external projects, updating documentation, or planning new features.
---

# Inspiration Study Skill

> Directive for studying upstream codebases and updating project plans.

## Purpose

When working on this project, periodically study upstream inspirations to:
1. Learn new patterns and approaches
2. Update plans, specs, and roadmaps
3. Track progress against upstream implementations
4. Update dependencies and installed resources

## Core Principle: Preserve Unimplemented Inspiration

**CRITICAL**: The `docs/inspirations.md` file MUST retain ALL "What [Project] should copy" items until they are actually implemented. Never remove or modify these items to say "now implemented" unless:

1. The feature exists in the codebase
2. The feature is documented in `docs/implementation-status.md` as `complete`
3. Tests verify the feature works

When reviewing inspiration updates:
- **ADD** new upstream patterns to "What [Project] should copy" sections
- **NEVER DELETE** existing "What [Project] should copy" items
- **NEVER CHANGE** items to "now implemented" unless verified in codebase
- Only mark items complete in `docs/implementation-status.md` when implemented

## Upstream Repositories

| Project | URL | Best Use |
|---------|-----|----------|
| OpenClaw | https://github.com/openclaw/openclaw | Onboarding, gateway UX, managed skills |
| ZeroClaw | https://github.com/zeroclaw-labs/zeroclaw | Core Rust architecture, config shape |
| NanoClaw | https://github.com/qwibitai/nanoclaw | Container-first isolation, local flows |
| IronClaw | https://github.com/nearai/ironclaw | Security pipeline, unified tool registry |
| PicoClaw | https://github.com/sipeed/picoclaw | Session/workspace layout, heartbeat |
| TinyClaw | https://github.com/TinyAGI/tinyclaw | Multi-agent queueing, retries |

## Workflow

### 1. Study Phase

```bash
# Clone or update inspiration repositories
git clone https://github.com/openclaw/openclaw /tmp/openclaw
git clone https://github.com/zeroclaw-labs/zeroclaw /tmp/zeroclaw
# ... clone other upstream repos as needed

# For each repository, study:
# - Recent commits (last 30 days)
# - Release notes
# - New features or changes
# - Architecture patterns
# - Configuration formats
```

### 2. Documentation Updates

| File | When to Update |
|------|----------------|
| `docs/inspirations.md` | Add new patterns. NEVER remove existing items. |
| `docs/implementation-status.md` | Only mark features `complete` when actually implemented |
| `ROADMAP.md` | Timeline adjustments, new priorities |

### 3. Update Process

```bash
# 1. Start from latest main, create feature branch
git checkout main && git pull
git checkout -b TICKET-<number>-study-upstream

# 2. Update documentation
# - Review recent commits in each upstream repo
# - Add new patterns to "What [Project] should copy" sections
# - Check implementation-status.md to verify before marking complete

# 3. Commit with reference
git add docs/
export GIT_EDITOR=true && git commit -m "[TICKET-<number>] Update inspiration analysis"

# 4. Run checks
./scripts/with-build-env.sh cargo test --workspace
./scripts/with-build-env.sh cargo fmt --check
./scripts/with-build-env.sh cargo clippy --workspace --all-targets -- -D warnings

# 5. Push and create PR
git push -u origin TICKET-<number>-study-upstream
gh pr create --title "[TICKET-<number>] Update upstream inspiration analysis"
```

### 5. Tool/Resource Updates

Check installed tools for updates:
```bash
# Check for updates in tools directory
ls -la .local/tools/*/.

# Update if needed per tool-specific instructions
```

## Study Checklist

When reviewing an upstream repo:
- [ ] What new features did they add recently?
- [ ] How do they solve problems this project has?
- [ ] What architectural patterns are they using?
- [ ] How has their config/schema evolved?
- [ ] What security patterns do they implement?
- [ ] What onboarding improvements exist?
- [ ] How do they handle error cases this project struggles with?
- [ ] What documentation improvements could be copied?
- [ ] Is this pattern already implemented? (verify before claiming)

## Output Format

After studying, document findings in `docs/inspirations.md`:

```markdown
## Upstream Follow-Up: YYYY-MM-DD

### [Project Name]
- **Recent**: [what they shipped]
- **Interesting**: [pattern worth noting]
- **[Project] Impact**: [what this means for us]
- **New "What [Project] should copy" items**: [add any new items]

### Implementation Verification
- [ ] Verify: Feature X is/isn't implemented (search codebase first)
```

## Verification Before Marking Complete

Before claiming any "What [Project] should copy" item is "now implemented":

```bash
# Search the codebase to verify the feature exists
grep -r "feature_name" <crate>/src/

# Check implementation-status.md shows it as complete
grep "feature_name" docs/implementation-status.md
```

## Anti-Patterns

- **NEVER** remove "What [Project] should copy" items just because they're old
- **NEVER** change items to "now implemented" without verifying in codebase
- Do not copy patterns blindly; translate to project's architecture
- Borrow the contract, not the syntax
