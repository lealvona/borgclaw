# Inspiration Study Skill

> Directive for studying upstream codebases and updating project plans.

## Purpose

Periodically study upstream projects to:
1. Learn new patterns and approaches
2. Update plans, specs, and roadmaps
3. Track progress against upstream implementations
4. Update dependencies and installed resources

## Workflow

### 1. Identify Upstream Sources

Your project documentation should list upstream inspirations:
```bash
# Check YOUR docs for upstream references
cat docs/inspirations.md
cat ROADMAP.md
```

### 2. Clone or Update Upstream Repos

```bash
# Create temporary study directory
STUDY_DIR=$(mktemp -d)
cd "$STUDY_DIR"

# Clone upstream repositories (check YOUR docs for actual URLs)
git clone https://github.com/UPSTREAM_ORG/UPSTREAM_REPO.git

# For each repository:
cd UPSTREAM_REPO
git remote -v
```

### 3. Study Phase

For each upstream repository, examine:

```bash
# Recent commits (last 30 days)
git log --since="30 days ago" --oneline

# Release notes
git log --oneline --grep="Release" | head -10
# OR check GitHub Releases page

# Architecture or key files
ls -la
cat README.md

# Recent file changes
git diff --stat HEAD~30..HEAD
```

### 4. Documentation Updates

Based on findings, update these files in YOUR project:

| File | When to Update |
|------|----------------|
| `docs/inspirations.md` | New patterns, architectural insights |
| `docs/implementation-status.md` | Completed features, new gaps |
| `ROADMAP.md` | Timeline adjustments, new priorities |
| `docs/SPRINT_PLAN.md` | Sprint planning alignment |
| `CHANGELOG.md` | Version tracking |

### 5. Update Process

```bash
# 1. Create feature branch
git checkout -b TICKET-<number>-study-upstream

# 2. Update documentation based on findings
# Edit relevant docs to reflect upstream changes

# 3. Commit with clear reference
git add docs/
git commit -m "[RESEARCH] Update inspiration analysis from upstream study

- UPSTREAM_REPO: Added notes on [feature/pattern]
- UPSTREAM_OTHER: Updated [architecture/config]
- Status: Marked N items as completed"

# 4. Push and create PR
git push origin TICKET-<number>-study-upstream
gh pr create --title "[TICKET-<number>] Update upstream inspiration analysis"
```

### 6. Dependency Updates

Check for dependency updates regularly:

```bash
# Check Rust dependencies
cargo outdated

# Check npm dependencies (if applicable)
npm outdated

# Review dependency changes in upstream repos
cd /path/to/upstream/repo
git log --since="30 days ago" --oneline -- Cargo.toml package.json

# Update YOUR project if beneficial
cargo update
npm update
```

### 7. Tool/Resource Updates

Check installed tools for updates:

```bash
# List installed tools in your project
ls -la .local/tools/

# Check each tool's repo for updates
cd .local/tools/TOOL_NAME && git log --since="30 days ago" --oneline

# Rebuild/update if needed
cd TOOL_NAME && cmake --build build --config Release

# Verify tools still work
cargo test
```

## Study Checklist

When reviewing an upstream repo:

- [ ] What new features did they add recently?
- [ ] How do they solve problems YOUR project has?
- [ ] What architectural patterns are they using?
- [ ] How has their config/schema evolved?
- [ ] What security patterns do they implement?
- [ ] What onboarding improvements exist?
- [ ] How do they handle error cases YOUR project struggles with?
- [ ] What documentation improvements could be copied?
- [ ] Have any features been completed that were on YOUR roadmap?

## Output Format

After studying, document findings in YOUR project:

```markdown
## Upstream Follow-Up: YYYY-MM-DD

### UPSTREAM_REPO
- **Recent**: [what they shipped]
- **Interesting**: [pattern worth noting]
- **Impact**: [what this means for our project]

### UPSTREAM_OTHER
- ...

### Status Changes
- [x] Feature A (was: in progress, now: complete in upstream)
- [ ] Feature B (still gap)

### Recommendations
1. Consider adopting [pattern] from UPSTREAM
2. Our approach to [problem] is [better/worse] because...
```

## Skill Metadata
- **Name**: inspiration-study
- **Version**: 1.0.0
- **Tags**: research, documentation, upstream, planning
