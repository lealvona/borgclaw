---
name: git-fork-setup
description: Configure git remotes for fork-based development with upstream push protection. Use when setting up a new repository clone to prevent accidental pushes to upstream.
---

# Git Fork Setup with Upstream Push Protection

This skill helps you safely configure git remotes when cloning from an upstream repository to prevent accidental pushes.

## When to Use

Use this skill when:
- Cloning a repository for the first time
- User mentions setting up a fork or configuring remotes
- Need to establish safe git remote configuration
- Starting work on a repository with fork-based workflow

## ⚠️ Scope Boundary

**Configure git remotes ONLY. Do not expand setup.**

- ✅ Set up upstream and origin remotes
- ✅ Configure push protection
- ✅ Verify remote configuration
- ❌ DO NOT initialize additional tooling
- ❌ DO NOT configure CI/CD
- ❌ DO NOT set up development environment

**Focus:** Git remote setup only. Other setup is separate work.

## Critical Security Command

The **MOST IMPORTANT** command to prevent accidental upstream pushes:

```bash
git remote set-url --push upstream NO_PUSH
```

## Complete Setup Procedure

After cloning from upstream, execute these commands **IMMEDIATELY**:

### Step 1: Rename origin to upstream
```bash
git remote rename origin upstream
```

### Step 2: Add your fork as working remote
```bash
# Replace <your-fork-username> with actual GitHub username
git remote add <your-fork-username> git@github.com:<your-fork-username>/<repo>.git
```

### Step 3: Disable push to upstream (CRITICAL)
```bash
git remote set-url --push upstream NO_PUSH
```

### Step 4: Verify configuration
```bash
git remote -v
```

## Expected Output

Verification should show:
```
<your-fork-username>  git@github.com:<your-fork-username>/<repo>.git (fetch)
<your-fork-username>  git@github.com:<your-fork-username>/<repo>.git (push)
upstream              git@github.com:<org>/<repo>.git (fetch)
upstream              NO_PUSH (push)  ← CRITICAL: Must show NO_PUSH
```

## Why This Matters

- **Prevents accidental pushes** to organization/upstream repositories
- **Creates technical safeguard** beyond human vigilance
- **Standard pattern** for open-source contribution
- **Protects CI/CD** infrastructure and production systems

## Never Push Directly To

- ❌ `main` / `master`
- ❌ `dev` / `develop`
- ❌ `qa` / `staging` / `prod` / `production`
- ❌ Any upstream remote
- ❌ Any central/protected branch

## Always

- ✅ Work on feature branches in your fork
- ✅ Use descriptive branch names: `TICKET-###-brief-description`
- ✅ Require explicit user approval before ANY push
- ✅ Create PRs from fork → upstream
- ✅ Verify remote configuration before git operations

---

## Response Format

**Output Style:** Concise, actionable commands

**After setup, confirm:**
```
✅ Remote configuration complete
- Fork: <username>/<repo> (push enabled)
- Upstream: <org>/<repo> (NO_PUSH)

Next: git checkout -b <branch-name>
```

**On error:**
```
❌ Remote setup failed
Issue: <specific problem>
Fix: <exact command to run>
```
