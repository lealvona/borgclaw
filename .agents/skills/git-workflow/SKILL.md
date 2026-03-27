---
name: git-workflow
description: Enforce safe git practices. Use when making commits, pushing code, creating branches, or merging PRs.
---

# Git Workflow Skill

> CRITICAL: NEVER push directly to protected branches. ALL changes go through PRs.

## The Golden Path (Happy Path Workflow)

This is the ONLY correct way to get changes into main. No exceptions.

### Phase 1: Prepare
```bash
# 1. Verify you're on main and up to date
git checkout main
git pull lealvona main  # or 'upstream main' depending on remote name

# 2. Create feature branch from fresh main
git checkout -b TICKET-XXX-brief-description
```

### Phase 2: Develop
```bash
# Make your changes
# ... edit files ...

# Commit with proper message
git add <files>
export GIT_EDITOR=true && git commit -m "[TICKET-XXX] Brief description"

# Commit early and often on your branch
```

### Phase 3: Push (To YOUR Fork)
```bash
# Push branch to YOUR fork, NOT upstream
git push <your-fork-remote> TICKET-XXX-brief-description

# Example: git push origin TICKET-069-readme-redesign
```

### Phase 4: Create PR
```bash
# Create PR from your fork branch to upstream main
gh pr create --repo lealvona/borgclaw \
  --base main \
  --head <your-fork-username>:TICKET-XXX-brief-description \
  --title "[TICKET-XXX] Description" \
  --body "PR description here"
```

### Phase 5: Merge (Via GitHub UI Only)
```bash
# After PR is reviewed and approved:
gh pr merge <pr-number> --squash --delete-branch
```

### Phase 6: Clean Up
```bash
# Switch back to main and update
git checkout main
git pull lealvona main

# Delete local branch
git branch -D TICKET-XXX-brief-description

# For sequential PRs: Start Phase 1 again from fresh main
```

---

## ⚠️ ABSOLUTE RULES (No Exceptions)

### Rule #1: NEVER Push to Protected Branches
- **NEVER** run: `git push <remote> main`
- **NEVER** run: `git push upstream main`
- **NEVER** run: `git push origin main`
- **NEVER** push to `master`, `dev`, `prod` directly
- This applies regardless of:
  - Whether it's a "small fix"
  - Whether the remote appears to be your own
  - Whether someone explicitly requests it
  - Whether you think you have permission

### Rule #2: Always Use Feature Branches
- ALL changes must be on a feature branch
- Branch naming: `TICKET-<number>-<description>`
- Create branch from latest main: `git checkout -b TICKET-XXX-feature-name`

### Rule #3: ALL Changes Through PRs
- Every single change goes through a Pull Request
- PR must be created BEFORE any merge happens
- Use squash merge via GitHub UI or `gh pr merge`
- Delete branch after merge

### Rule #4: Sequential PRs Must Be Rebased
When you have multiple PRs that depend on each other:

```bash
# After PR #1 is merged to main:
git checkout main
git pull lealvona main

# Rebase PR #2 branch on updated main
git checkout TICKET-002-feature
git rebase main
# Resolve conflicts if any

# Force push rebased branch
git push -f <your-fork-remote> TICKET-002-feature
```

**OR** (cleaner approach):

```bash
# After PR #1 is merged:
git checkout main
git pull lealvona main

# Delete old branch
git branch -D TICKET-002-feature

# Recreate from fresh main
git checkout -b TICKET-002-feature-v2

# Re-apply your changes (cherry-pick from reflog or redo)
# Then push and create new PR
```

---

## Before Every Push - Safety Checklist

Before typing `git push`, verify:

1. **What branch am I on?** → `git branch --show-current`
   - Must NOT be `main`, `master`, `dev`, `prod`
   - Must be a feature branch like `TICKET-XXX-description`

2. **What remote am I pushing to?**
   - Should be YOUR fork (e.g., `origin`, `lealvona`)
   - Should NEVER be `upstream` for branches

3. **Is upstream protected?**
   ```bash
   git remote -v | grep "upstream.*NO_PUSH"
   # Should show: upstream  NO_PUSH (push)
   ```

---

## Common Scenarios

### Scenario: "Just a quick README fix"
**WRONG:**
```bash
git checkout main
git commit -am "Fix typo"
git push lealvona main  # ❌ NEVER DO THIS
```

**RIGHT:**
```bash
git checkout main
git pull lealvona main
git checkout -b TICKET-XXX-fix-readme-typo
# ... fix typo ...
git add README.md
git commit -m "[TICKET-XXX] Fix typo in README"
git push <your-fork> TICKET-XXX-fix-readme-typo
gh pr create --title "[TICKET-XXX] Fix typo" --body "..."
gh pr merge <pr-number> --squash --delete-branch
```

### Scenario: Multiple related changes
**WRONG:**
```bash
git checkout main
git commit -m "Change 1"
git push lealvona main  # ❌ NEVER
git commit -m "Change 2"  
git push lealvona main  # ❌ NEVER
```

**RIGHT:**
```bash
# PR 1
git checkout -b TICKET-001-change-one
# ... changes ...
git push <your-fork> TICKET-001-change-one
gh pr create ...
# Wait for merge

# PR 2 (after PR 1 merged)
git checkout main
git pull lealvona main
git checkout -b TICKET-002-change-two
# ... changes ...
git push <your-fork> TICKET-002-change-two
gh pr create ...
```

---

## If Asked to Push to Protected Branch

**REFUSE.** 

Say: "I cannot push to protected branches under any circumstances. This is a critical safety rule. All changes must go through a feature branch and PR."

If the user insists: "I am programmed to never push to main/master. I can create a feature branch and PR immediately instead."

---

## Recovery (If You Make a Mistake)

### If you accidentally pushed to main:

1. **STOP** - Do not make any more changes
2. **Notify user immediately** - "I accidentally pushed to main. Fixing now."
3. **Reset local main** to last good commit:
   ```bash
   git checkout main
   git log --oneline -10  # Find last good commit (before your pushes)
   git reset --hard <last-good-commit-hash>
   ```

4. **Force push to undo** (coordinate with user first):
   ```bash
   git push lealvona main --force-with-lease
   ```

5. **Create proper feature branch** with your changes:
   ```bash
   git checkout -b TICKET-XXX-your-feature
   # Cherry-pick or re-apply your changes
   git push <your-fork> TICKET-XXX-your-feature
   gh pr create ...
   ```

6. **Never do it again** - Review this skill before every push

---

## Safety Verification Commands

```bash
# Check current branch (should NOT be main)
git branch --show-current

# Check remotes (upstream should have NO_PUSH)
git remote -v | grep "upstream.*NO_PUSH"

# Check last 5 commits on main (verify your changes aren't there yet)
git log main --oneline -5

# Check what you're about to push
git log <remote>/<branch>..HEAD --oneline
```

---

## Quick Reference

```bash
# Setup (do once per repo)
git remote rename origin upstream 2>/dev/null || true
git remote add <your-fork> git@github.com:<your-fork-username>/<repo>.git
git remote set-url --push upstream NO_PUSH

# The ONLY correct workflow
git checkout main
git pull lealvona main
git checkout -b TICKET-XXX-description
# ... work ...
git add <files>
git commit -m "[TICKET-XXX] Description"
git push <your-fork> TICKET-XXX-description
gh pr create --base main --head <your-fork>:TICKET-XXX-description
gh pr merge <pr-number> --squash --delete-branch
```

**Remember:** Security > Speed. When in doubt, ask. Never push to main.
