# Git Workflow Skill

> CRITICAL: NEVER push to main or master. EVER. Under any conditions.

## Absolute Rules

### Rule #1: NEVER Touch Main/Master
- **NEVER** push to `main` or `master` branch
- **NEVER** merge directly to `main` or `master`  
- **NEVER** force push to `main` or `master`
- This applies regardless of:
  - Whether you think it's a "small fix"
  - Whether the remote says it's "your" repo
  - Whether someone asked you to
  - ANY other circumstance

### Rule #2: Always Use Feature Branches
- ALL changes must be on a feature branch
- Branch naming: `TICKET-<number>-<description>`
- Create branch from latest main: `git checkout -b TICKET-XXX-feature-name`
- Make changes, commit, push branch, create PR

### Rule #3: PRs Only
- All changes go through Pull Requests
- PR must be reviewed before merge
- Squash and merge via GitHub UI
- Delete branch after merge

## Workflow

```bash
# 1. Check remotes - if fork and upstream are same, just work locally
git remote -v

# 2. Always start from fresh main on a feature branch
git checkout main
git pull
git checkout -b TICKET-XXX-your-feature

# 3. Make changes, commit
git add .
git commit -m "[TICKET-XXX] Description"

# 4. Push branch (not main!)
git push -u origin TICKET-XXX-your-feature

# 5. Create PR via GitHub UI or gh CLI
gh pr create --title "[TICKET-XXX] Description" --body "..."

# 6. Wait for review, merge via GitHub UI
```

## Why This Matters

- Main branch protection prevents broken code
- Code review catches mistakes
- Audit trail for debugging
- Feature branches can be discarded if wrong

## If Someone Asks You to Push to Main

Refuse. Say: "I cannot push to main under any circumstances. All changes must go through a feature branch and PR."

## Recovery

If you accidentally pushed to main:
1. Immediately notify the user
2. Reset your local main: `git reset --hard HEAD~1`
3. Create proper feature branch: `git checkout -b TICKET-XXX-fix`
4. Never do it again

## Skill Metadata
- **Name**: git-workflow
- **Version**: 2.0.0
- **Tags**: git, safety, workflow
- **Updated**: March 2026 - Made rules absolute and explicit
