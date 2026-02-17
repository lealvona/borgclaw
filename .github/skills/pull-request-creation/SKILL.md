---
name: pull-request-creation
description: Create well-structured pull requests with comprehensive descriptions. ALWAYS creates draft PRs by default. Use when ready to create a PR after pushing changes to a fork, or when user asks to create/draft a pull request.
---

# Pull Request Creation

This skill helps create well-structured, comprehensive pull requests that facilitate code review and maintain project documentation standards.

## ⚠️ CRITICAL: Always Create Draft PRs First

**Default behavior: ALWAYS use `--draft` flag when creating PRs.**

- ✅ Create all PRs as drafts initially
- ✅ Mark ready for review only when explicitly confirmed
- ✅ Allows time for CI/CD validation before review
- ✅ Enables self-review and final checks

**Mark ready when:**
- All CI/CD checks passing
- Self-review completed
- Description finalized
- User explicitly approves

## When to Use

Use this skill when:
- User has pushed changes to their fork
- User asks to "create a PR" or "make a pull request"
- Changes are ready for initial submission (as draft)
- Need to convert draft to ready-for-review

## ⚠️ Scope Boundary

**PR describes current changes ONLY.**

- ✅ Describe what this PR actually changes
- ✅ Link to current ticket/issue
- ✅ Include testing done for these changes
- ❌ DO NOT mention future work in description
- ❌ DO NOT include unrelated changes
- ❌ DO NOT expand scope in PR commits

**One PR, one purpose.** Additional work needs separate PRs.

## PR Requirements

### Title Format
```
[TICKET-###] Brief description of changes
```

- Include ticket/issue reference
- Keep under 72 characters
- Use imperative mood: "Add feature" not "Added feature"
- Be specific about what changed

### Description Structure

A comprehensive PR description should include:

#### 1. Summary
Brief overview of what changed (2-3 sentences)

#### 2. Problem Being Solved
- What issue/ticket this addresses
- Why the change was needed
- Context for reviewers

#### 3. Approach Taken
- How the problem was solved
- Key technical decisions
- Patterns or frameworks used

#### 4. Changes Made
- File-by-file summary for large PRs
- Breaking changes highlighted
- Configuration changes noted

#### 5. Testing Performed
- Unit tests added/updated
- Integration tests run
- Manual testing steps
- Edge cases verified

#### 6. Screenshots/Examples
If applicable:
- UI changes (before/after)
- CLI output examples
- API response samples

#### 7. Breaking Changes
If any:
- What breaks
- Migration path
- Deprecation timeline

#### 8. Blockers/Follow-up Work
- Known issues or limitations
- Follow-up tickets created
- Dependencies on other PRs

#### 9. Related Links
- Jira ticket/issue
- Related PRs
- Documentation
- Reference implementations

## PR Creation Commands

### Using GitHub CLI (Recommended)
```bash
# ALWAYS create draft PR first (DEFAULT BEHAVIOR)
gh pr create --draft \
  --repo <org>/<repo> \
  --base main \
  --head <your-fork-username>:<branch-name> \
  --title "[TICKET-###] Description" \
  --body-file PR_DESCRIPTION.md

# Mark draft PR as ready for review (ONLY after validation)
gh pr ready <pr-number>

# Add reviewers (after marking ready)
gh pr edit <pr-number> --add-reviewer user1,user2
```

**⚠️ NEVER create ready-for-review PRs directly unless explicitly instructed.**

### Using Git + Web Interface
1. Push to fork
2. Visit repository on GitHub
3. Click "Compare & pull request"
4. Fill in title and description
5. **CHECK "Create draft pull request"** (REQUIRED)
6. Create draft PR
7. Mark ready for review only after validation

## PR Template Example

```markdown
## Summary
Added sme-feed-file-export to Organization Folder pattern in jenkins-config to enable automatic PR pipeline job discovery.

## Changes
- Modified `pipeline.theorchard.io/jobs/orgFolder.groovy` (line 140)
- Single-line addition following pattern used by 160+ repositories
- Enables automatic PR job creation for sme-feed-file-export

## Impact
- Jenkins will automatically discover sme-feed-file-export repository
- PR pipeline jobs created automatically on PR creation
- GitHub status checks enabled for PRs

## Important Note
⚠️ **The sme-feed-file-export repository currently lacks a Jenkinsfile.** The pipeline job structure will be created but will fail with "no Jenkinsfile" error until a Jenkinsfile is added. Follow-up ticket required.

## Testing
- Syntax validated via git diff review
- Follows exact pattern of existing entries
- Automated JobDslScriptTest will validate groovy syntax in CI

## Related
- Jira: https://example.atlassian.net/browse/TICKET-###
- Repository: https://github.com/org/repo
- Follow-up work: Jenkinsfile creation (separate ticket)
```

## Best Practices

1. **ALWAYS start with draft PR** - allows validation before review
2. **Write description first** - helps clarify your changes
3. **Use checklists** for complex requirements
4. **Link everything** - tickets, docs, related PRs
5. **Highlight blockers** prominently
6. **Include visuals** when UI/output changes
7. **Wait for CI/CD** results before marking ready
8. **Self-review draft** before requesting reviews
9. **Mark ready** only when all checks pass and ready for eyes
10. **Request specific reviewers** for expertise areas
11. **Update description** as you address feedback

## Reviewer Selection

Consider requesting reviews from:
- **Code owners** (automatic via CODEOWNERS)
- **Subject matter experts** for specialized code
- **Team leads** for architectural decisions
- **Security reviewers** for sensitive changes
- **Documentation writers** for user-facing changes

## After Creating Draft PR

1. **Wait for CI/CD results** (linting, tests, builds)
2. **Review the changes yourself** on GitHub
3. **Fix any failures** identified by CI/CD
4. **Update ticket/issue** with PR link
5. **Mark ready for review** when:
   - All CI/CD checks passing ✅
   - Self-review completed ✅
   - Description accurate ✅
   - Ready for team review ✅
6. **Request reviewers** after marking ready
7. Add labels if applicable
8. Link to project board if used
9. Respond to review feedback promptly

---

## Response Format

**Output Style:** Essential PR details, clear next action

**After creation (ALWAYS draft):**
```
✅ Draft PR #<number> created
URL: <github-url>
Status: Draft (not yet ready for review)
CI/CD: Pending validation

Next steps:
1. Monitor CI/CD results
2. Review changes on GitHub
3. Mark ready when validated: gh pr ready <number>
4. Request reviewers: gh pr edit <number> --add-reviewer user1,user2
```

**On error:**
```
❌ PR creation failed
Issue: <specific error>
Fix: <command or action>
```
