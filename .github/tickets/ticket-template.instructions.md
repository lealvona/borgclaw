---
name: "[TICKET-ID] Template"
description: "Template for creating per-ticket instruction files. Copy this file and customize for your specific ticket."
---

# [TICKET-ID] Project-Specific Notes

<!-- 
TEMPLATE INSTRUCTIONS:
1. Copy this file to create a new per-ticket instruction file
2. Name it: [ticket-id].instructions.md (e.g., INT-2614.instructions.md, lowercase)
3. Replace ALL [PLACEHOLDERS] with actual values (including YAML frontmatter above)
4. Update the 'name' field in frontmatter with your ticket ID
5. Update the 'description' field with brief context (used for discovery)
6. Delete sections that don't apply to your ticket
7. Add additional sections as needed for your specific context
8. Keep focused on ONLY the current ticket scope
9. This file will be available in chat context automatically
-->

## ⚠️ SCOPE: Stay Focused on [TICKET-ID] ONLY

**This ticket:** [Brief description of the specific task]  
**Not in scope:** [List what should NOT be done as part of this ticket]  
**If you find issues:** Document as follow-up tickets, DO NOT fix them now

---

## Project Context
**Ticket:** [TICKET-ID] - [Brief ticket title]  
**Organization:** [organization-name]  
**Primary Repository:** [main-repo-name]  
**Affected Repository:** [affected-repo-name] (if applicable)  
**Date:** [Date work started]

---

## Repository Configuration

### [primary-repository]
- **Upstream:** git@github.com:[org]/[repo].git
- **Fork:** git@github.com:[your-fork-username]/[repo].git
- **Purpose:** [What this repository does]
- **Key File:** [Main file(s) being modified]

### [affected-repository] (if applicable)
- **Upstream:** git@github.com:[org]/[repo].git
- **Purpose:** [What this repository does]
- **Status:** [Current state or relevant information]

---

## Implementation Details

### Change Made
**File:** [path/to/file.ext]  
**Line:** [line number]  
**Change:** [Description of what was changed]

```diff
[Show relevant diff with context]
- old line
+ new line
```

### Pattern Used
**[Pattern or approach name]**
- [Key characteristic 1]
- [Key characteristic 2]
- [Key characteristic 3]

---

## Work Completed

### ✅ Completed Tasks
1. [Task 1 description]
2. [Task 2 description]
3. [Task 3 description]
4. [Continue listing completed work...]

### ⏳ Pending Tasks
1. [Pending task 1]
2. [Pending task 2]
3. [Pending task 3]

---

## Critical Findings

### 🚨 [Blocker/Issue Category]: [Brief Title]
**Issue:** [Description of the issue found]  
**Impact:** [What this affects]  
**Error:** [Expected error or problem]  
**Resolution:** [How this should be resolved]

**Verification Performed:**
```bash
# Commands used to verify the issue
[command 1]
# Result: [what was found]

[command 2]
# Result: [what was found]
```

**Requirements (for follow-up ticket):**
- [Requirement 1]
- [Requirement 2]
- [Requirement 3]

---

## Reference Implementations

### Similar Work/Repositories Using Same Pattern
1. **[repo-name]** ([relevant details])
   - [Why this is relevant]
   - [Key learnings or patterns]

2. **[repo-name-2]** ([relevant details])
   - [Why this is relevant]

---

## Technology Stack

### [repository-name]
- **Language:** [Programming language]
- **Database/Infrastructure:** [Key technologies]
- **Dependencies:** [Dependency management approach]
- **Testing:** [Testing framework and configuration]
- **Linting:** [Linting tools and configuration]
- **Deployment:** [Current deployment method/location]

### [repository-name-2] (if applicable)
- **Format:** [Format/language]
- **Testing:** [How it's tested]
- **Deployment:** [How it's deployed]

---

## [Infrastructure/System] Details (if applicable)

### After Changes Merge
**Timeline:** [Expected timeframe for changes to take effect]  
**Location:** [URL or location where changes can be verified]  
**Expected Behavior:**
- [What should happen]
- [Expected outcomes]
- [How to verify]

### Configuration Settings (if applicable)
- **Setting 1:** [Value and description]
- **Setting 2:** [Value and description]
- **Credentials:** [Credential references if applicable]

---

## Security Implementation

### Remote Configuration Applied
```bash
# Upstream (org) - read-only
upstream  git@github.com:[org]/[repo].git (fetch)
upstream  NO_PUSH (push)  # ← Prevents accidental pushes

# Fork (personal) - working remote
[fork-name]  git@github.com:[your-username]/[repo].git (fetch)
[fork-name]  git@github.com:[your-username]/[repo].git (push)
```

### Decision Log Entries
**[D001-D00X]** documented in `docs/DECISION_LOG.md`:
- D001: [Decision category and brief description]
- D002: [Decision category and brief description]
- [Continue listing key decisions...]

---

## Follow-Up Work Required

### Priority 1: [Follow-up Task Title] (HIGH/MEDIUM/LOW)
**Ticket:** [To be created or ticket number]  
**Repository:** [repository-name]  
**Template:** See `docs/FOLLOW_UP_TICKETS.md`

**[Example code/configuration if applicable]:**
```[language]
[Example implementation or configuration]
```

### Priority 2: [Follow-up Task Title] (HIGH/MEDIUM/LOW - Optional)
**Ticket:** [To be created or ticket number]  
**Features:**
- [Feature/enhancement 1]
- [Feature/enhancement 2]
- [Feature/enhancement 3]

---

## Reviewers & Team Contacts

### Review Requested From
- [Reviewer names and GitHub usernames]
- **Notes:** [Any relevant notes about reviewers or approval process]

### Ticket Management
- **Assignee:** [assignee-name]
- **Status:** [Current status]
- **Link:** [URL to ticket]

---

## Testing & Validation

### Post-Merge Validation Steps
1. **[Validation Step 1]**
   ```bash
   [Commands or steps to validate]
   ```

2. **[Validation Step 2]**
   - [What to check]
   - [Expected results]

3. **Expected State (Before [X])**
   - [Expected behavior]
   - [What should be visible]

4. **After [X] Completed**
   - [How to test]
   - [What to verify]
   - [Success criteria]

---

## Lessons Learned

### Key Insights from This Implementation
1. **[Insight 1]** - [Description and why it matters]
2. **[Insight 2]** - [Description and why it matters]
3. **[Insight 3]** - [Description and why it matters]
4. **[Insight 4]** - [Description and why it matters]

### Process Improvements Applied
- [Improvement 1 and rationale]
- [Improvement 2 and rationale]
- [Improvement 3 and rationale]

---

## Quick Commands Reference

### Repository Setup
```bash
cd [workspace-path]/[project-folder]
cd [repository-name]
git remote -v  # Verify configuration
```

### Check PR Status
```bash
gh pr view [PR-NUMBER] --repo [org]/[repo]
```

### View Specific Commit
```bash
cd [repository-name]
git show [commit-hash]
```

### [Other Useful Commands]
```bash
[command-description]
[actual-command]
```

---

## Related Documentation

### Project Files Created
- `.github/copilot.instructions.md` - Universal guidelines
- `.github/instructions/[ticket-id].instructions.md` - This file (project-specific)
- `docs/DECISION_LOG.md` - Architectural decision record
- `docs/FOLLOW_UP_TICKETS.md` - Templates for future work

### External References
- [Repo] PR #[number]: [URL]
- Jira [TICKET-ID]: [URL]
- [Other relevant links]
- sme-feed-file-export repo: https://github.com/theorchard/sme-feed-file-export
- Reference Jenkinsfile: https://github.com/theorchard/swf-feed-ingestion/blob/master/Jenkinsfile
- Scheduler deployment: https://scheduler.theorchard.io/job/sme-feed-file-exporter/

---

## End of Project-Specific Notes
