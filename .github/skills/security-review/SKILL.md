---
name: security-review
description: Perform security checks before committing or pushing code. Use when preparing to commit changes, before pushing to remote, or when conducting code reviews to ensure no credentials or sensitive data is exposed.
---

# Security Review and Quality Checks

This skill performs comprehensive security and quality validation before committing or pushing code.

## When to Use

Use this skill when:
- Preparing to commit changes
- Before pushing to any remote
- Conducting code reviews
- User asks to "check security" or "verify changes"
- Implementing changes that handle sensitive data
- Modifying authentication or authorization code

## ⚠️ Scope Boundary

**Review security of current changes ONLY.**

- ✅ Check files modified in current work
- ✅ Ensure current changes don't expose secrets
- ✅ Validate security of new code added
- ❌ DO NOT audit entire codebase
- ❌ DO NOT fix unrelated security issues
- ❌ DO NOT expand scope to system-wide security

**Focused review:** Check what's changed, not everything that exists.

## Security Checklist

### Before Committing

**Code Integrity:**
- [ ] Self-review all changes
- [ ] No credentials, API keys, or secrets in code
- [ ] No hardcoded passwords or tokens
- [ ] No email addresses or PII in comments
- [ ] No internal URLs or IP addresses exposed
- [ ] Environment variables used for configuration
- [ ] Sensitive data properly encrypted or excluded

**Code Quality:**
- [ ] Run linters and formatters
- [ ] Execute tests locally
- [ ] No linter warnings or errors
- [ ] Test coverage meets standards
- [ ] Code follows project style guide

**Logic Review:**
- [ ] Input validation present
- [ ] Error handling appropriate
- [ ] No unhandled edge cases
- [ ] Security implications reviewed
- [ ] SQL injection protection (if applicable)
- [ ] XSS prevention (if applicable)
- [ ] CSRF protection (if applicable)

### Git Hygiene

- [ ] Commits are atomic and well-described
- [ ] Branch follows naming convention
- [ ] No merge conflicts
- [ ] Remote configuration verified (NO_PUSH on upstream)
- [ ] Push approved by user

### Documentation

- [ ] README updated for new features
- [ ] API changes documented
- [ ] Configuration examples updated
- [ ] Breaking changes noted

## Common Security Issues to Check

### 1. Credentials in Code
```bash
# Search for potential secrets
grep -r "password\|secret\|api_key\|token" .
grep -r "-----BEGIN.*KEY-----" .
```

### 2. Hardcoded URLs
```bash
# Find URLs that should be configurable
grep -r "http://\|https://" . | grep -v ".git\|node_modules"
```

### 3. Environment Variables
Verify sensitive config uses environment variables:
- ✅ `os.getenv('API_KEY')`
- ❌ `API_KEY = 'sk-abc123'`

### 4. Git History
```bash
# Check what's being committed
git diff --cached

# Verify no large files
git diff --cached --stat
```

## Emergency: If Secrets Are Committed

If secrets are accidentally committed:

1. **IMMEDIATELY** rotate/revoke the credential
2. **DO NOT** just commit a fix - secrets remain in history
3. **NOTIFY** security team
4. **Use git filter-repo** or similar to remove from history
5. **Force push** only after team coordination
6. **Document** incident in postmortem

## Automated Tools

Consider using:
- **git-secrets**: Prevents committing secrets
- **truffleHog**: Finds secrets in git history
- **detect-secrets**: Pre-commit hook for secrets
- **gitleaks**: Detect hardcoded secrets

## Validation Commands

```bash
# Check for staged changes with issues
git diff --cached | grep -i "password\|secret\|token\|api.key"

# Verify remote configuration
git remote -v | grep "NO_PUSH"

# Check file permissions
find . -type f -perm /111 | grep -v ".git"
```

## Response Format

**Output Style:** Scannable status report, clear pass/fail

**Standard format:**
```markdown
## Security Review

**Scope:** <n> files, <n> commits

### ✅ Passed
- No credentials/secrets detected
- Linters: 0 errors
- Tests: <n> passed
- Coverage: <n>%

### ❌ Blockers
- <specific issue> @ <file>:<line>
- <fix command or action required>

### ⚠️ Warnings  
- <non-blocking issue> @ <file>:<line>

**Status:** [✅ PASS | ❌ BLOCKED]
**Next:** [Commit | Fix blockers]
```

**No issues found:**
```
✅ Security review passed
Scope: <n> files, <n> LOC
Issues: None
Next: Commit ready
```
