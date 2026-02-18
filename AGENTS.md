# Project Instructions - Universal Guidelines

> **About This File**: These are instructions that apply to all tasks in this workspace. For task-specific workflows, see [Skills](.opencode/skills/) which load on-demand.

## Before Starting Any Task

**MANDATORY: Read all documentation and familiarize yourself with available skills before beginning work.**

When starting a new task or conversation:

1. **Read all project documentation:**
   - Review README.md for project overview
   - Read all files in `docs/` directory
   - Check for any project-specific guidelines

2. **Familiarize yourself with available skills:**
   - Review skill names and descriptions in `.opencode/skills/`
   - Understand what each skill does and when to use it
   - Note which skills apply to your current task
   - Skills will load automatically when relevant

3. **Understand the context:**
   - Identify the ticket/task scope and boundaries
   - Review related PRs, issues, or previous work
   - Note any blockers or dependencies
   - Understand technology stack and conventions

---

## Communication Standards

**Audience:** High-level engineers with domain expertise

**Response Principles:**
- **Fullsome yet concise** - Comprehensive information, minimal verbosity
- **Precision over explanation** - Use exact technical terms instead of descriptions
- **Essential information only** - Report current state, next actions, blockers
- **Scannable format** - Structured for rapid comprehension (bullets, tables, code blocks)
- **Actionable outputs** - Every response should clarify what to do next

**Examples:**

❌ Verbose: "I've identified that there's a potential issue with the configuration file where the database connection string might not be properly set, which could lead to connection failures when the application attempts to establish a connection to the database."

✅ Concise: "Missing `DATABASE_URL` in config. Add to `.env` before deployment."

❌ Verbose: "The tests are currently passing with a success rate of 100%, and the coverage metric indicates that approximately 87% of the codebase is being tested, which exceeds our target of 80%."

✅ Concise: "Tests: 142 passed, 0 failed. Coverage: 87% (target: 80%)."

**Status Reports:**
- ✅ / ❌ / ⏳ indicators for task status
- Bullet lists, not paragraphs
- Key metrics upfront
- Blockers highlighted
- Next action clear

**Technical Vocabulary:**
Use industry-standard terms without definition:
- ADR, YAGNI, SOLID, DRY
- Idempotent, immutable, atomic
- Upstream, fork, rebase, squash
- CI/CD, artifact, deployment
- Semantic versioning, SemVer
- Technical debt, refactor

---

## Scope Management

### ⚠️ CRITICAL: Stay Focused - Avoid Scope Creep

**Core Principle:** Complete the current task ONLY. Do not expand scope unless explicitly requested.

**Mandatory Rules:**

1. **Task Boundaries**
   - Execute ONLY what the current task/ticket requires
   - If you discover related issues: document them, don't fix them
   - Create follow-up tickets for additional work
   - Never "while we're at it" - stop immediately

2. **When You Notice Related Issues**
   - ✅ Document in comments or ADR
   - ✅ Add to follow-up tickets list
   - ✅ Mention in PR description
   - ❌ DO NOT fix them now
   - ❌ DO NOT refactor surrounding code
   - ❌ DO NOT add "nice to have" features

3. **Enhancement Requests**
   - If user asks "can we also...": clarify if it's in scope
   - If new requirement: create separate ticket
   - If blocker: discuss, don't assume

4. **Refactoring Boundaries**
   - Only refactor code directly touched by current task
   - Document larger refactoring needs as technical debt
   - Create separate tickets for broader improvements

5. **Testing Scope**
   - Test ONLY the changed functionality
   - Don't expand test coverage beyond current changes
   - Note missing tests for follow-up work

**Red Flags (Stop Immediately):**
- "While I'm here, I'll also..."
- "This would be better if we..."
- "I noticed we could improve..."
- "Let me also fix..."
- Touching files not directly related to task
- Adding features not in requirements

**Stay Laser-Focused:** One task, one change, one PR. Everything else is future work.

---

## Core Security Principles

### 🔒 CRITICAL: Git Safety Rules (MANDATORY FOR ALL PROJECTS)

#### Remote Configuration - THE MOST IMPORTANT SETUP COMMANDS
When cloning ANY upstream repository, **IMMEDIATELY** execute these commands:

```bash
# 1. Rename upstream remote (typically named 'origin' after clone)
git remote rename origin upstream

# 2. Add your personal fork as the working remote
# Replace <your-fork-username> with your actual GitHub username
git remote add <your-fork-username> git@github.com:<your-fork-username>/<repo>.git

# 3. ⭐ THE MOST IMPORTANT GIT COMMAND ⭐
# Disable push to upstream - prevents ALL accidental pushes
git remote set-url --push upstream NO_PUSH
```

**Why This Matters:**
- Prevents accidental pushes to organization/upstream repositories
- Creates technical safeguard beyond human vigilance
- Standard open-source contribution pattern
- Protects CI/CD infrastructure and production systems

**Verify Configuration:**
```bash
git remote -v
# Should show:
# <your-fork-username>  git@github.com:<your-fork-username>/<repo>.git (fetch)
# <your-fork-username>  git@github.com:<your-fork-username>/<repo>.git (push)
# upstream              git@github.com:<org>/<repo>.git (fetch)
# upstream              NO_PUSH (push)  ← CRITICAL: Must show NO_PUSH
```

#### Git Editor Requirements (AI Workflow Safety)
**CRITICAL:** Interactive editors block AI workflows and require human intervention.

```bash
# MANDATORY: Set GIT_EDITOR=true at the start of EVERY terminal command that invokes git
export GIT_EDITOR=true && git commit -m "message"
export GIT_EDITOR=true && git rebase --continue
export GIT_EDITOR=true && git merge --no-edit main
```

**Requirements:**
- NEVER allow git operations to open interactive editors (nano, vi, vim, emacs)
- ALL git commands MUST use: `export GIT_EDITOR=true && git <command>`
- Applies to: commit, rebase, merge, cherry-pick, revert, and any other git operation

#### Never Push Directly To:
- ❌ `main` / `master`
- ❌ `dev` / `develop`
- ❌ `qa` / `staging`
- ❌ `prod` / `production`
- ❌ Any upstream remote
- ❌ Any central/protected branch

#### Always:
- ✅ Work on feature branches in your fork
- ✅ Use descriptive branch names: `TICKET-###-brief-description`
- ✅ Require explicit user approval before ANY push
- ✅ Create PRs from fork → upstream
- ✅ Verify remote configuration before git operations

---

## Decision Tracking System

### DECISION_LOG.md - Architectural Decision Record
**Purpose:** Track all significant architectural and implementation decisions.

**Location:** Project root or `.github/` directory

**Format:**
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

**When to Log Decisions:**
- Architectural choices (technology, patterns, frameworks)
- Implementation approaches (especially when alternatives exist)
- Security or workflow changes
- Breaking changes or major refactors
- Tool/library selections
- Configuration methodology
- Any decision that affects future development

**Benefits:**
- Audit trail for architectural choices
- Knowledge transfer for team members
- Reference for similar future decisions
- Context preservation as team evolves

---

## Git Workflow - Fork-Based Development

### Initial Setup (Per Repository)
```bash
# 1. Clone from upstream
git clone git@github.com:<org>/<repo>.git
cd <repo>

# 2. Configure remotes (IMMEDIATELY after clone)
git remote rename origin upstream
git remote add <your-fork-username> git@github.com:<your-fork-username>/<repo>.git
git remote set-url --push upstream NO_PUSH

# 3. Verify
git remote -v
```

### Feature Development Workflow
```bash
# 1. Create feature branch
git checkout -b TICKET-###-feature-description

# 2. Make changes, commit frequently
git add <files>
git commit -m "[TICKET-###] Descriptive message"

# 3. Push to YOUR fork (require user approval first)
git push <your-fork-username> TICKET-###-feature-description

# 4. ALWAYS create DRAFT PR first from your-fork/branch → upstream/main
gh pr create --draft --repo <org>/<repo> --base main --head <your-fork-username>:TICKET-###-feature-description

# 5. Mark ready for review only after CI/CD validation
gh pr ready <pr-number>
```

### Staying in Sync
```bash
# Fetch upstream changes
git fetch upstream

# Update your main branch
git checkout main
git merge upstream/main

# Rebase your feature branch (if needed)
git checkout TICKET-###-feature-description
git rebase main
```

---

## Code Review & Quality Standards

### Before Committing
- ✅ Self-review all changes
- ✅ Run linters and formatters
- ✅ Execute tests locally
- ✅ Verify no credentials or sensitive data in code
- ✅ Check for hardcoded values that should be configuration
- ✅ Review security implications of changes

### Commit Message Format
```
[TICKET-###] Brief description (50 chars or less)

- Detailed point about change
- Another important detail
- Reference to issue/ticket if applicable

Related: Link to ticket/issue
```

### Pull Request Requirements
- **ALWAYS create as draft initially** (use `--draft` flag)
- Mark ready only after CI/CD validation passes
- Clear title with ticket reference: `[TICKET-###] Description`
- Comprehensive description including:
  - Summary of changes
  - Problem being solved
  - Approach taken
  - Testing performed
  - Breaking changes (if any)
  - Screenshots/examples (if applicable)
- Link to related tickets/issues
- Document blockers or follow-up work needed
- Request reviewers explicitly after marking ready

---

## Change Management Best Practices

### Incremental Changes
- Make atomic commits (one logical change per commit)
- Keep PRs focused and reviewable (< 400 lines preferred)
- Break large features into smaller, mergeable pieces
- Document partial implementations clearly

### Documentation Requirements
- Update README for feature additions
- Document API changes immediately
- Update configuration examples
- Maintain changelog for user-facing changes
- Include inline comments for complex logic

### Testing Strategy
- Write tests for new functionality
- Update tests for modified functionality
- Include edge cases and error conditions
- Verify backwards compatibility
- Test rollback procedures for critical changes

---

## Security Constraints

### Code Integrity
- Maintain backwards compatibility unless explicitly approved
- Verify changes don't break existing functionality
- Review security implications of all modifications
- Follow principle of least privilege
- Never commit credentials, API keys, or secrets

### Environment Separation
- Never test in production
- Use appropriate environment variables
- Maintain separate configurations per environment
- Document environment-specific requirements

### Access Control
- Work only in personal fork
- Never bypass branch protection rules
- Request appropriate permissions rather than circumventing
- Document required access levels for tasks

---

## Communication & Collaboration

### User Interaction Requirements
- **Always request approval before:**
  - Pushing to any remote (even your fork)
  - Making breaking changes
  - Deleting files or branches
  - Running destructive commands
  - Modifying production configurations

### Progress Updates
- Maintain task lists (TODO.md or similar)
- Update tickets/issues with progress regularly
- Communicate blockers immediately
- Document assumptions and decisions as they're made

### Team Coordination
- Tag relevant team members on PRs
- Update shared documentation promptly
- Communicate breaking changes widely
- Respond to review feedback professionally

---

## Skills Integration

### What Are Skills?

Skills are specialized capabilities stored in `.opencode/skills/` that load on-demand when relevant to your task. Unlike these always-on instructions, skills include:
- Task-specific workflows and procedures
- Scripts, templates, and examples
- Specialized domain knowledge
- Step-by-step guides for complex tasks

### Available Skills

This workspace provides the following skills (loaded automatically when needed):

- **`code-quality-check`** - Validate code quality before PRs
- **`decision-logging`** - Create architectural decision records
- **`emergency-procedures`** - Handle git emergencies and security incidents
- **`git-fork-setup`** - Configure fork-based workflow safely
- **`project-instructions-setup`** - Create per-ticket instruction files
- **`pull-request-creation`** - Generate comprehensive PR descriptions
- **`security-review`** - Security checks before commits

### When to Use Skills vs Instructions

**Use these instructions for:**
- ✅ General coding standards and guidelines
- ✅ Security rules that always apply
- ✅ Communication style and conventions
- ✅ Workflow principles (scope management, git safety)

**Use agent skills for:**
- ✅ Specific tasks with multiple steps
- ✅ Workflows that include scripts or examples
- ✅ Specialized procedures (code review, PR creation)
- ✅ Domain-specific knowledge (testing, deployment)

### How Skills Load

Skills use progressive disclosure:
1. **Discovery**: You always know available skills (names/descriptions)
2. **Loading**: Full instructions load only when task matches skill description
3. **Resources**: Additional files (scripts, templates) load only if referenced

You don't need to manually select skills—they activate automatically based on your request.

---

## Emergency Procedures

### Accidentally Pushed to Upstream
If you somehow push to upstream despite safeguards:
1. Immediately notify team lead
2. Do NOT force push to fix
3. Coordinate with team on resolution
4. Document incident for process improvement

### Remove Committed Secrets
If secrets are accidentally committed:
1. Immediately rotate/revoke the credential
2. Remove from git history (not just a new commit)
3. Notify security team
4. Force push only after coordination

### Broke Main/Master
If you break a protected branch:
1. Alert team immediately
2. Create hotfix or revert commit
3. Expedite review and merge
4. Document root cause in postmortem

---

## Continuous Improvement

### Lessons Learned
After completing significant work:
- Review what went well
- Identify pain points or bottlenecks
- Update these instructions if deficiencies found
- Share knowledge with team
- Propose process improvements

### Instruction Updates
These instructions should evolve:
- Capture new patterns and lessons
- Remove outdated practices
- Clarify ambiguous sections
- Add examples where helpful
- Keep security practices current

---

## Quick Reference Card

```bash
# Setup (do once per repo, replace <your-fork-username> with your GitHub username)
git remote rename origin upstream
git remote add <your-fork-username> git@github.com:<your-fork-username>/<repo>.git
git remote set-url --push upstream NO_PUSH

# Feature workflow
git checkout -b TICKET-###-description
# ... make changes ...
git add <files>
git commit -m "[TICKET-###] Description"
git push <your-fork-username> TICKET-###-description  # After user approval

# Verify safety
git remote -v | grep "upstream.*NO_PUSH"  # Should find match
```

**Remember:** Security > Speed. Take time to verify before pushing.

---

## BorgClaw Project Rules

### Project Overview
BorgClaw is a Rust-based personal AI agent framework combining the best features from OpenClaw-family frameworks.

### Architecture
- **Workspace**: Root `Cargo.toml` defines workspace with 3 crates
- **Core**: `borgclaw-core/` - traits, implementations
- **CLI**: `borgclaw-cli/` - REPL binary  
- **Gateway**: `borgclaw-gateway/` - WebSocket server

### Key Design Decisions

1. **Trait-Based Modularity**: All components defined as traits in `borgclaw-core/src/`. Implementations can be swapped via config.

2. **Security First**:
   - WASM sandbox for untrusted tools
   - Command blocklist via regex
   - Pairing codes for channel authentication
   - Prompt injection defense

3. **Memory**: SQLite + FTS5 for hybrid keyword search

### Development Commands

```bash
# Build all
cargo build

# Run CLI
cargo run --bin borgclaw -- repl

# Run gateway
cargo run --bin borgclaw-gateway
```
