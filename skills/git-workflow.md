# Git Workflow Skill

> Directive for safe Git operations in multi-repository environments.

## Core Directive

**CRITICAL: Before ANY git push operation, verify repository ownership.**

## Rules

### Repository Ownership Check
1. Always check `git remote -v` before pushing
2. If repository is NOT personally owned (e.g., `upstream`, `origin` pointing to other users/orgs):
   - Set push target to `NO_PUSH`:
     ```bash
     git remote set-url --push upstream NO_PUSH
     ```
   - OR ensure you only push to your personal fork
3. NEVER push to repositories you don't own

### Ownership Detection

Identify repository owner from remote URL:
```bash
git remote -v
# origin  git@github.com:OWNER/repo.git (fetch)
# upstream git@github.com:UPSTREAM_OWNER/repo.git (fetch)
```

| Remote | Owner Type | Push Target |
|--------|------------|-------------|
| `origin` | Your fork | ✅ Allowed |
| `upstream` | Original repo | 🔴 NO_PUSH |

### Safe Workflow
```bash
# 1. ALWAYS check remotes first
git remote -v

# 2. Verify push targets
git remote get-url --push origin  # Should be your fork
git remote get-url --push upstream  # Should be NO_PUSH or your org's repo

# 3. If upstream needs NO_PUSH protection
git remote set-url --push upstream NO_PUSH

# 4. Now safe to push to your fork
git push origin YOUR-BRANCH
```

### Pre-Push Checklist
- [ ] `git remote -v` shows correct ownership
- [ ] Push target is your personal fork or `NO_PUSH` for others
- [ ] Working on a feature branch, not main/master
- [ ] Branch name follows `TICKET-<number>-description` convention

## Configuration

### Per-Repository Setup
For non-owned repositories, configure NO_PUSH:
```bash
# Check current push URL
git remote get-url --push upstream

# Set as NO_PUSH if not your repo
git remote set-url --push upstream NO_PUSH
```

### Global Git Alias (Recommended)
Add to your `~/.gitconfig`:
```bash
[alias]
  safe-push = "!f() { \
    owner=$(git remote get-url origin | cut -d: -f2 | cut -d/ -f1); \
    if [ \"$owner\" != \"$(whoami)\" ]; then \
      echo \"ERROR: Not pushing to your repository!\"; \
      echo \"Owner: $owner, Your user: $(whoami)\"; \
      return 1; \
    fi; \
    git push origin HEAD; \
  }; f"
```

### Pre-Push Hook (Recommended)
For additional safety, create `.git/hooks/pre-push`:
```bash
#!/bin/bash
owner=$(git remote get-url origin | cut -d: -f2 | cut -d/ -f1)
if [ "$owner" != "$(whoami)" ]; then
  echo "ERROR: Attempting to push to non-owned repository: $owner"
  echo "Your user: $(whoami)"
  exit 1
fi
```
Make executable: `chmod +x .git/hooks/pre-push`

## Emergency Recovery

If you accidentally pushed to wrong repo:
1. Contact repository owner immediately
2. Force push to your fork to overwrite if needed
3. Never assume "just this once" is acceptable

## Skill Metadata
- **Name**: git-workflow
- **Version**: 1.0.0
- **Tags**: git, safety, workflow, multi-repo
