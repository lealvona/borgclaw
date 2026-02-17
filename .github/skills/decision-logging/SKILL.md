---
name: decision-logging
description: Create and maintain architectural decision records (ADR) to track significant implementation choices. Use when making architectural decisions, choosing between approaches, or documenting important technical choices.
---

# Decision Logging System

This skill helps create and maintain architectural decision records to track all significant technical and implementation decisions.

## When to Use

Use this skill when:
- Making architectural choices (technology, patterns, frameworks)
- Choosing between multiple implementation approaches
- Making security or workflow changes
- Implementing breaking changes or major refactors
- Selecting tools or libraries
- Deciding on configuration methodology
- Any decision that affects future development

## ⚠️ Scope Boundary

**Log decisions for current task ONLY.**

- ✅ Document decisions made during current work
- ✅ Record trade-offs for current implementation
- ❌ DO NOT document unrelated architectural decisions
- ❌ DO NOT expand into broader system design
- ❌ DO NOT create ADRs for future work

**Stay focused:** Document what's decided now, not what could be decided later.

## Decision Log Format

Create or update `docs/DECISION_LOG.md` with this structure:

```markdown
# Decision Log - [Project/Ticket Name]

## D001: [Decision Title]
**Decision**: What was decided

**Rationale**: Why this approach was chosen
- Key reasoning points
- Business/technical drivers

**Alternatives Considered**:
- Option A (rejected - reason)
- Option B (rejected - reason)

**Impact**:
- Technical implications
- Team/workflow changes
- Dependencies created/resolved

---
```

## Location

- Primary: `docs/DECISION_LOG.md`
- Alternative: `.github/DECISION_LOG.md`
- Per-ticket: `docs/<ticket-id>-decisions.md`

## Decision Numbering

- Use sequential numbering: D001, D002, D003...
- Continue sequence across all decisions in the file
- Never reuse decision numbers

## What to Include

**Each decision must have:**
1. **Title**: Clear, concise description
2. **Decision**: What was chosen
3. **Rationale**: Why this approach (with bullet points)
4. **Alternatives Considered**: What else was evaluated and why rejected
5. **Impact**: Consequences and implications

**Optional sections:**
- **Implementation**: How the decision is applied
- **Verification**: How to confirm implementation
- **Next Steps**: Follow-up actions required
- **Related Decisions**: Links to related decisions

## Benefits

- **Audit trail** for architectural choices
- **Knowledge transfer** for team members
- **Reference** for similar future decisions
- **Context preservation** as team evolves
- **Onboarding aid** for new team members
- **Justification** for technical debt or complexity

## Examples

**Good Decision Log Entry:**
```markdown
## D007: Missing Jenkinsfile Discovery
**Decision**: Document that sme-feed-file-export lacks a Jenkinsfile; must be created before jenkins-config changes are effective

**Rationale**: 
- Verified sme-feed-file-export repository has no Jenkinsfile
- Git history shows Jenkinsfile never existed
- Organization Folder pattern requires Jenkinsfile for pipeline jobs
- Pipeline will fail without it

**Alternatives Considered**:
- Proceed without verification (rejected - prevents surprises)
- Assume Jenkinsfile exists (rejected - violates verification principle)

**Impact**: 
- BLOCKER: Must create Jenkinsfile in sme-feed-file-export first OR
- Add to jenkins-config knowing it won't work until Jenkinsfile created
- Two-repository change instead of single-repository change
```

## Best Practices

1. **Write decisions immediately** while context is fresh
2. **Be specific** about technical details
3. **Document alternatives** even if briefly
4. **Update decisions** if circumstances change (add addendum)
5. **Link decisions** that relate to each other
6. **Include dates** for time-sensitive decisions
7. **Be honest** about tradeoffs and technical debt

---

## Response Format

**Output Style:** Structured ADR entries, technical precision

**When logging decision:**
```
✅ Decision D<###> logged: <title>
Location: docs/DECISION_LOG.md
Alternatives: <count> evaluated

Next: Reference in commit message
```

**Entry structure:** Decision, Rationale (bullets), Alternatives (bullets), Impact (bullets)
