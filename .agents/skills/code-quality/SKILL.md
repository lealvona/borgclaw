---
name: code-quality
description: Run pre-commit quality checks and verify tests, formatting, linting, and documentation before PR creation.
---

# Skill: Code Quality

> Directive for ensuring code quality, tests, and documentation before PR creation.

## Purpose

All code changes must include:
1. Comprehensive unit tests for new functionality
2. Integration tests where appropriate
3. Documentation updates in relevant `docs/` files
4. Passing test suite

## Mandatory Checklist

Before creating any PR, verify:

- [ ] **Tests Added**: Unit tests for all new functions/methods
- [ ] **Integration Tests**: For end-to-end workflows
- [ ] **Documentation Updated**: README, docs/*.md, inline comments
- [ ] **All Tests Pass**: `./scripts/with-build-env.sh cargo test --workspace` succeeds
- [ ] **Code Compiles**: No warnings (except pre-existing ones)
- [ ] **Feature Complete**: Not stubbed or placeholder code

## Test Requirements

### Unit Tests

- Test happy path and error cases
- Test boundary conditions
- Mock external dependencies
- Use descriptive test names

Example:
```rust
#[test]
fn package_skill_creates_valid_tar_gz() {
    // Test implementation
}

#[test]
fn package_skill_rejects_missing_skill_md() {
    // Test error handling
}
```

### Integration Tests

- Test complete workflows
- Use temporary directories
- Clean up test artifacts
- Test with realistic data

### Documentation Updates

Update these files as needed:
- `README.md` - User-facing features
- `docs/*.md` - Detailed documentation
- `docs/implementation-status.md` - Feature status
- `CHANGELOG.md` - Notable changes
- Inline code comments for complex logic

## Workflow

```bash
# 1. Implement feature
# 2. Add tests
./scripts/with-build-env.sh cargo test --workspace

# 3. Update documentation
# 4. Verify checklist
# 5. Create feature branch
git checkout -b TICKET-XXX-feature-name

# 6. Commit
git add .
git commit -m "[TICKET-XXX] Description"

# 7. Push and create PR (after checklist verified)
```

## Never Skip

- ❌ Never merge without tests
- ❌ Never merge without documentation
- ❌ Never merge with failing tests
- ❌ Never merge stubbed/placeholder code

## Anti-Patterns

- "I'll add tests later" → Add them now
- "The code is self-documenting" → Add documentation
- "It's just a small change" → Still needs tests
- "Tests passed locally" → Verify in CI

## Consequences

Code merged without tests or documentation:
- Creates technical debt
- Blocks future refactoring
- Confuses other developers
- Violates project standards

Base directory for this skill: `.agents/skills/code-quality`
