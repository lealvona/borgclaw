---
name: code-quality-check
description: Validate code quality before marking work complete. Use when user asks to check quality, before finalizing a PR, or when preparing work for review.
---

# Code Quality Validation

This skill performs comprehensive code quality checks before marking work as complete.

## When to Use

Use this skill when:
- User asks to "check code quality" or "validate changes"
- Before marking PR as ready for review
- Completing a feature or ticket
- Preparing for final commit
- User requests pre-merge validation

## ⚠️ Scope Boundary

**This skill validates current changes ONLY.**

- ✅ Check quality of files modified in current task
- ✅ Ensure current changes meet standards
- ❌ DO NOT refactor unrelated code
- ❌ DO NOT fix quality issues in untouched files
- ❌ DO NOT expand scope beyond current PR

**If you find issues outside scope:** Document for follow-up, don't fix now.

## Quality Checklist

### Code Quality

**Style and Standards:**
- [ ] Code follows project style guide
- [ ] Naming conventions consistent
- [ ] No linter warnings or errors
- [ ] Formatting rules applied
- [ ] Comments clear and helpful
- [ ] No commented-out code blocks
- [ ] No debug print statements left in

**Testing:**
- [ ] Tests pass locally
- [ ] Coverage meets project standards
- [ ] New code has tests
- [ ] Edge cases covered
- [ ] Error conditions tested
- [ ] Integration tests updated if needed

**Documentation:**
- [ ] Public APIs documented
- [ ] Complex logic explained
- [ ] README updated if needed
- [ ] CHANGELOG updated for user-facing changes
- [ ] Migration guides provided for breaking changes

### Security

- [ ] No credentials in code
- [ ] No hardcoded secrets
- [ ] Input validation present
- [ ] Error handling appropriate
- [ ] Security implications reviewed
- [ ] Dependencies checked for vulnerabilities
- [ ] Authentication/authorization correct

### Git Hygiene

- [ ] Commits are atomic
- [ ] Commit messages descriptive
- [ ] Branch follows naming convention
- [ ] No merge conflicts
- [ ] Remote configuration verified
- [ ] Push approved by user

### Process

- [ ] Decisions logged in DECISION_LOG
- [ ] Ticket/issue updated with progress
- [ ] PR description complete and accurate
- [ ] Reviewers requested
- [ ] CI/CD passing
- [ ] Breaking changes documented

## Validation Steps

### 1. Run Linters
```bash
# Python
flake8 .
pylint src/
black --check .

# JavaScript/TypeScript
eslint .
prettier --check .

# Go
golint ./...
go vet ./...

# General
# Check project-specific linter configuration
```

### 2. Run Tests
```bash
# Python
pytest
pytest --cov=src tests/

# JavaScript
npm test
npm run test:coverage

# Go
go test ./...
go test -cover ./...

# Check test coverage meets threshold
```

### 3. Check Documentation
```bash
# Find undocumented public functions (Python example)
pydocstyle src/

# Verify README sections complete
# - Installation
# - Usage
# - Configuration
# - Contributing
# - License
```

### 4. Security Scan
```bash
# Check for secrets
git diff --cached | grep -i "password\|secret\|api.key\|token"

# Dependency vulnerabilities
npm audit  # JavaScript
pip check  # Python
go list -m all | nancy  # Go
```

### 5. Build Verification
```bash
# Ensure project builds successfully
npm run build
python setup.py build
go build ./...
```

## Common Issues to Fix

### Anti-Patterns
- **Magic numbers**: Replace with named constants
- **Long functions**: Extract into smaller units
- **Deep nesting**: Flatten control flow
- **Copy-paste code**: Extract to shared function
- **Unused imports**: Remove
- **Unused variables**: Remove or prefix with `_`

### Code Smells
- Functions longer than 50 lines
- Files longer than 500 lines
- Cyclomatic complexity > 10
- Too many parameters (> 5)
- Inconsistent error handling
- Missing null/undefined checks

### Documentation Gaps
- Public functions without docstrings
- Complex algorithms without explanation
- No examples in README
- Missing error code documentation
- Outdated comments after refactoring

## Automated Tools

Integrate quality tools:
- **SonarQube**: Code quality and security
- **CodeClimate**: Maintainability analysis
- **Coveralls**: Test coverage tracking
- **Snyk**: Dependency vulnerability scanning
- **ESLint/Pylint**: Language-specific linting

## Quality Gates

Define minimum standards:
- **Test Coverage**: ≥ 80%
- **Linter Errors**: 0
- **Critical Vulnerabilities**: 0
- **Code Complexity**: ≤ 10
- **Documentation**: All public APIs

## Response Format

**Output Style:** Metrics upfront, scannable issues list

**Standard format:**
```markdown
## Quality Check

**Metrics:**
- Linter: <n> errors, <n> warnings
- Tests: <n> passed / <n> total (<n>% coverage)
- Security: <n> issues
- Complexity: <n> violations

### ❌ Blockers (<n>)
- <issue> @ <file>:<line>
  Fix: <command or change needed>

### ⚠️ Warnings (<n>)
- <issue> @ <file>:<line>

**Status:** [✅ PASS | ⚠️ WARNINGS | ❌ BLOCKED]
**Next:** [Ready for review | Fix blockers | Address warnings]
```

**Perfect score:**
```
✅ Quality check passed
Linter: 0 errors, Tests: 100%, Coverage: <n>%
Next: Ready for review
```

## Pre-Review Checklist

Before requesting review:
1. **Self-review** all changes in diff
2. **Run full test suite** locally
3. **Check CI/CD** passes on pushed branch
4. **Update documentation** as needed
5. **Rebase on main** if behind
6. **Resolve conflicts** if any
7. **Squash commits** if needed (project dependent)
8. **Verify PR description** complete
