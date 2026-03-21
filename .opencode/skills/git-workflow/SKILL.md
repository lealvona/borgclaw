---
name: git-workflow
description: Enforce safe git practices. Use when making commits, pushing code, creating branches, or merging PRs.
---

# Git Workflow Skill

> CRITICAL: NEVER push directly to protected branches.

## Absolute Rules

### Rule #1: Never Touch Protected Branches
- **NEVER** push to `main`, `master`, `dev`, `prod` directly
- **NEVER** merge directly to protected branches
- **NEVER** force push to protected branches
- This applies regardless of:
  - Whether it's a "small fix"
  - Whether the remote appears to be your own
  - Whether someone requests it

### Rule #2: Always Use Feature Branches
- ALL changes must be on a feature branch
- Branch naming: `TICKET-<number>-<description>`
- Create branch from latest main: `git checkout -b TICKET-XXX-feature-name`

### Rule #3: All Changes Through PRs
- All changes go through Pull Requests
- PR must be reviewed before merge
- Use squash and merge via GitHub UI
- Delete branch after merge

## Workflow

```bash
# 1. Check remotes
git remote -v

# 2. Start from fresh main on a feature branch
git checkout main
git pull
git checkout -b TICKET-XXX-your-feature

# 3. Make changes, commit
git add .
export GIT_EDITOR=true && git commit -m "[TICKET-XXX] Description"

# 4. Push branch (not main!)
git push -u origin TICKET-XXX-your-feature

# 5. Create PR
gh pr create --title "[TICKET-XXX] Description" --body "..."

# 6. Wait for review, merge via GitHub UI
```

## If Asked to Push to Protected Branch

Refuse. Say: "I cannot push to protected branches under any circumstances. All changes must go through a feature branch and PR."

## Recovery

If you accidentally pushed to protected branch:
1. Immediately notify the user
2. Reset your local branch: `git reset --hard HEAD~1`
3. Create proper feature branch: `git checkout -b TICKET-XXX-fix`
4. Never do it again

## Safety Verification

```bash
git remote -v | grep "upstream.*NO_PUSH"  # Verify upstream push protection
```
