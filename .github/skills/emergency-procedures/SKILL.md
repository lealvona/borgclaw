---
name: emergency-procedures
description: Handle critical situations like accidental pushes to upstream, committed secrets, or broken protected branches. Use when user reports an emergency, accidentally pushed to wrong remote, or committed sensitive data.
---

# Emergency Procedures

This skill provides immediate response procedures for critical git and security incidents.

## When to Use

Use this skill when:
- User accidentally pushed to upstream
- Secrets committed to repository
- Protected branch (main/master) is broken
- Force push needed to fix critical issue
- User reports "I made a mistake" or "emergency"
- Sensitive data exposed in commits

## ⚠️ Scope Boundary

**Handle the emergency ONLY. Do not expand scope.**

- ✅ Fix the immediate crisis
- ✅ Prevent further damage
- ✅ Document what happened
- ❌ DO NOT "improve" other things while fixing
- ❌ DO NOT refactor surrounding code
- ❌ DO NOT add preventive measures beyond immediate fix

**Emergency mode:** Fix the problem, nothing else. Enhancements are follow-up work.

## Emergency Types

### 🚨 Level 1: Accidental Push to Upstream

**Symptoms:**
- Push went to upstream instead of fork
- Changes appear on protected branch
- CI/CD triggered unexpectedly

**Immediate Actions:**
1. **DO NOT panic** - can be fixed
2. **DO NOT force push** without coordination
3. **Immediately notify team lead** via Slack/Teams
4. **Document what happened** - which commits, which branch

**Resolution Path:**
1. Coordinate with team on resolution
2. If no one pulled changes yet: may revert
3. If changes distributed: create revert commit
4. For hotfix: expedite proper PR process
5. Document incident for process improvement

**Prevention:**
```bash
# Verify remote configuration
git remote -v | grep "NO_PUSH"

# Should show: upstream  NO_PUSH (push)
# If not, run: git remote set-url --push upstream NO_PUSH
```

---

### 🔐 Level 1: Secrets Committed to Repository

**Symptoms:**
- API key, password, or token in committed code
- Credentials visible in git diff or history
- Security scanner alert

**CRITICAL FIRST STEP:**
**Immediately rotate/revoke the credential** - assume it is compromised

**Immediate Actions:**
1. **ROTATE/REVOKE** the credential (do this FIRST)
2. **DO NOT** just commit a new fix - secret remains in history
3. **Notify security team** immediately
4. **Document** what was exposed and when

**Resolution:**
```bash
# Option 1: Using git-filter-repo (recommended)
git filter-repo --path-glob '**/config.py' --invert-paths

# Option 2: Using BFG Repo-Cleaner
bfg --delete-files config.py

# Option 3: Interactive rebase (if recent)
git rebase -i HEAD~5  # Adjust number as needed
# Edit commits to remove secrets
```

**After Cleanup:**
1. Force push only after team coordination
2. All developers must re-clone
3. Update .gitignore to prevent recurrence
4. Add pre-commit hooks (git-secrets)
5. Security postmortem required

**Prevention:**
- Use `.gitignore` for sensitive files
- Use environment variables for secrets
- Enable pre-commit hooks
- Use secret scanning tools

---

### 💥 Level 1: Broken Main/Master Branch

**Symptoms:**
- Protected branch builds failing
- Tests broken on main
- Production outage due to merge

**Immediate Actions:**
1. **Alert team immediately** - use emergency channel
2. **Identify breaking commit** via CI/CD logs
3. **Choose fix strategy** based on urgency

**Fix Strategies:**

**Option A: Revert Commit (Fastest)**
```bash
# Revert the breaking commit
git revert <commit-sha>
git push origin main

# Preserves history, safest approach
```

**Option B: Hotfix PR (If fix is simple)**
```bash
# Create hotfix branch
git checkout -b hotfix/broken-build
# Fix the issue
git commit -m "Hotfix: Fix broken build"
# Expedite review and merge
```

**Option C: Force Revert (Last resort)**
```bash
# Only if approved by tech lead
git reset --hard <last-good-commit>
git push --force origin main

# Requires all devs to re-sync
```

**After Resolution:**
1. Verify CI/CD passes
2. Monitor for side effects
3. Postmortem within 24 hours
4. Improve testing to prevent recurrence

---

### ⚠️ Level 2: Large Files Committed

**Symptoms:**
- Git push very slow
- Repository size ballooned
- Files larger than 100MB committed

**Resolution:**
```bash
# Find large files
git rev-list --objects --all \
  | git cat-file --batch-check='%(objecttype) %(objectname) %(objectsize) %(rest)' \
  | sed -n 's/^blob //p' \
  | sort -nk2 \
  | tail -20

# Remove large files using filter-repo
git filter-repo --path <large-file> --invert-paths

# Or use BFG
bfg --strip-blobs-bigger-than 50M
```

---

### ⚠️ Level 2: Wrong Files Committed

**Symptoms:**
- Unintended files in commit
- Build artifacts or logs committed
- Binary files in repository

**Resolution (before push):**
```bash
# Undo last commit, keep changes
git reset --soft HEAD~1

# Unstage specific files
git reset HEAD <file>

# Amend commit to exclude files
git commit --amend
```

**Resolution (after push to fork):**
```bash
# Force push is OK to your fork
git reset --hard HEAD~1
git push --force origin <branch>
```

---

## General Emergency Protocol

### Step 1: Assess Severity
- **Critical**: Affects production, exposes secrets
- **High**: Breaks protected branch, blocks team
- **Medium**: Affects only your work, easily reversible

### Step 2: Stop the Bleeding
- Revoke compromised credentials
- Revert breaking changes
- Prevent further damage

### Step 3: Communicate
- Notify affected team members
- Use appropriate emergency channels
- Provide status updates

### Step 4: Fix
- Choose appropriate fix strategy
- Get necessary approvals
- Execute carefully
- Verify fix works

### Step 5: Document
- What happened
- How it was fixed
- How to prevent recurrence
- Update runbooks/procedures

### Step 6: Learn
- Conduct postmortem (blameless)
- Identify process gaps
- Implement preventive measures
- Update training materials

---

## Emergency Contacts Template

Document your team's emergency contacts:

```markdown
## Emergency Contacts

**Security Issues:**
- Team: security@org.com
- Slack: #security-alerts
- On-call: PagerDuty rotation

**Build/Deploy Issues:**
- Team: devops@org.com
- Slack: #devops-emergency
- On-call: Check wiki

**Database Issues:**
- Team: dba@org.com
- Slack: #database-alerts
- On-call: PagerDuty DBA group
```

---

## Prevention > Response

**Best Practices:**
1. Always verify remote configuration
2. Use pre-commit hooks for secret detection
3. Run tests before merging to protected branches
4. Require PR reviews before merge
5. Use branch protection rules
6. Maintain good .gitignore
7. Regular training on git best practices

---

## Response Format

**Output Style:** Emergency triage, immediate action steps

**Incident response:**
```
🚨 EMERGENCY: <incident type>

**Severity:** [Critical | High | Medium]
**Impact:** <affected systems/data>

**IMMEDIATE ACTION:**
1. <step> [DONE | PENDING]
2. <step> [DONE | PENDING]

**STATUS:** <current state>
**NEXT:** <critical next step>
**ETA:** <timeline to resolution>
```

**All clear:**
```
✅ Emergency resolved
Incident: <type>
Action taken: <summary>
Next: Postmortem within 24h
```
