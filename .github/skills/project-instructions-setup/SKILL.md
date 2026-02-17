---
name: project-instructions-setup
description: Create per-ticket or per-project instruction files following the established pattern. Use when starting a new ticket, need to document project-specific context, or setting up new workspace instructions.
---

# Project-Specific Instructions Setup

This skill helps create per-ticket or per-project instruction files to capture context, configuration, and specific requirements.

## When to Use

Use this skill when:
- Starting work on a new ticket that requires context
- Setting up project-specific guidelines
- Documenting complex multi-repository changes
- User needs to create instruction files
- Establishing custom workflows for specific features

## ⚠️ Scope Boundary

**Create instruction file for current ticket ONLY.**

- ✅ Document context for current ticket
- ✅ Include relevant constraints and requirements
- ✅ Reference related tickets as blockers/dependencies
- ❌ DO NOT document unrelated projects
- ❌ DO NOT expand into general documentation
- ❌ DO NOT create instructions for future work

**One ticket, one instruction file.** Keep it focused.

## File Structure Pattern

```
.github/
├── copilot.instructions.md          # Universal guidelines
└── instructions/
    ├── <ticket-id>.instructions.md  # Ticket-specific notes
    └── <project>.instructions.md    # Project-specific guidelines
```

## When to Create Instructions

**Per-Ticket Instructions:**
- Working on specific ticket/issue requiring context
- Complex multi-repository changes
- Custom workflows for specific features
- Migration or refactoring work
- Anything requiring historical context

**Project-Wide Instructions:**
- New project setup
- Technology stack specifics
- Team conventions
- Testing requirements
- Deployment procedures

## File Naming Convention

- Use **lowercase** ticket/project ID
- Examples: `int-2614.instructions.md`, `proj-123.instructions.md`, `deployment.instructions.md`
- Store in: `.github/instructions/`
- One file per ticket/context

## Per-Ticket Instructions Template

Create file: `.github/instructions/<ticket-id>.instructions.md`

```markdown
# <TICKET-ID> Project-Specific Notes

## Project Context
**Ticket:** <TICKET-ID> - Brief description
**Organization:** <org-name>
**Primary Repository:** <repo>
**Date:** <start-date>

---

## Repository Configuration

### <repo-name>
- **Upstream:** git@github.com:<org>/<repo>.git
- **Fork:** git@github.com:<your-fork-username>/<repo>.git
- **Purpose:** Brief description
- **Key Files:** List important files

---

## Implementation Details

### Changes Made
**File:** path/to/file
**Line:** line number(s)
**Change:** Description of what changed

\```diff
- old code
+ new code
\```

### Pattern/Approach Used
Explain the architectural pattern or approach

---

## Work Completed

### ✅ Completed Tasks
1. Task 1
2. Task 2

### ⏳ Pending Tasks
1. Pending task 1
2. Pending task 2

---

## Critical Findings

### 🚨 Blockers
**Issue:** Description
**Impact:** What this affects
**Resolution:** How to fix

---

## Technology Stack
- Language/Framework details
- Dependencies
- Testing tools
- Build system

---

## Related Documentation
- Links to PRs
- Links to tickets
- Reference implementations
- External documentation

---

## Quick Commands Reference

### Repository Setup
\```bash
cd <workspace-path>
git remote -v  # Verify configuration
\```

### Check Status
\```bash
# Commands specific to this ticket/project
\```
```

## Project-Wide Instructions Template

Create file: `.github/instructions/<project-name>.instructions.md`

```markdown
# <Project Name> Guidelines

## Technology Stack
- **Language:** Python 3.9+
- **Framework:** Django 4.0
- **Database:** PostgreSQL 14
- **Cache:** Redis 7
- **Testing:** pytest, coverage

---

## Development Setup

### Prerequisites
- Python 3.9 or higher
- PostgreSQL 14
- Redis 7
- Docker (optional)

### Installation
\```bash
# Clone repository
git clone git@github.com:<org>/<repo>.git

# Create virtual environment
python -m venv venv
source venv/bin/activate

# Install dependencies
pip install -r requirements.txt
pip install -r requirements-dev.txt

# Setup database
createdb <project>_dev
python manage.py migrate

# Run tests
pytest
\```

---

## Code Standards

### Style Guide
- Follow PEP 8
- Maximum line length: 100 characters
- Use Black for formatting
- Use isort for import sorting

### Linting
\```bash
# Run all checks
black .
isort .
flake8 .
pylint src/
mypy src/
\```

### Testing
- Minimum coverage: 80%
- Write tests for all new features
- Update tests when modifying code
- Test edge cases and error conditions

---

## Git Workflow

### Branch Naming
- Feature: `feature/TICKET-###-description`
- Bugfix: `bugfix/TICKET-###-description`
- Hotfix: `hotfix/description`

### Commit Messages
\```
[TICKET-###] Brief description

- Detailed point 1
- Detailed point 2
\```

---

## CI/CD

### Pipeline Stages
1. Lint (flake8, pylint, mypy)
2. Test (pytest with coverage)
3. Build (Docker image)
4. Deploy (staging on merge to develop)

### Environment Variables
- `DATABASE_URL`: PostgreSQL connection
- `REDIS_URL`: Redis connection
- `SECRET_KEY`: Django secret
- `DEBUG`: Enable debug mode (dev only)

---

## Deployment

### Staging
- Automatic on merge to `develop`
- URL: https://staging.example.com
- Database: staging RDS

### Production
- Manual approval required
- Triggered from `main` branch
- URL: https://example.com
- Database: production RDS

---

## Monitoring

### Logs
- Application: CloudWatch Logs
- Access: S3 bucket
- Errors: Sentry

### Metrics
- APM: New Relic
- Uptime: Pingdom
- Analytics: Google Analytics

---

## Common Tasks

### Run Development Server
\```bash
python manage.py runserver
\```

### Database Migrations
\```bash
# Create migration
python manage.py makemigrations

# Apply migrations
python manage.py migrate

# Rollback migration
python manage.py migrate <app> <migration>
\```

### Run Tests
\```bash
# All tests
pytest

# With coverage
pytest --cov=src tests/

# Specific test
pytest tests/test_feature.py::test_function
\```

---

## Troubleshooting

### Common Issues

**Database connection fails:**
- Check DATABASE_URL environment variable
- Verify PostgreSQL is running
- Check database exists

**Import errors:**
- Ensure virtual environment activated
- Run `pip install -r requirements.txt`
- Check Python version compatibility

---

## Team Contacts
- **Tech Lead:** @username
- **DevOps:** #devops-channel
- **Code Reviews:** #code-review-channel
```

## What to Include in Per-Ticket vs Universal

### ✅ Include in Per-Ticket Instructions
- Specific repository URLs
- Actual usernames and organization names
- Concrete file paths and line numbers
- Links to PRs, tickets, related work
- Team member contacts
- Project-specific commands
- Known issues or blockers

### ❌ Do NOT Include in Universal Instructions
- Specific usernames or email addresses
- Specific organization names
- Specific repository names
- Concrete file paths from your project
- Project-specific commands
- Personal workspace paths

## Creation Workflow

When user starts new ticket work:

1. **Create directory if needed:**
   ```bash
   mkdir -p .github/instructions
   ```

2. **Create instruction file:**
   ```bash
   touch .github/instructions/<ticket-id>.instructions.md
   ```

3. **Use template** from above

4. **Fill in sections** as work progresses

5. **Update regularly** with findings and decisions

6. **Reference in commits:**
   ```
   [TICKET-###] Implementation
   
   See .github/instructions/ticket-###.instructions.md for details
   ```

## Best Practices

1. **Create early** - set up instructions at ticket start
2. **Update often** - document as discoveries happen
3. **Be specific** - include actual values, not placeholders
4. **Link everything** - PRs, tickets, docs, references
5. **Include commands** - exact commands used
6. **Document blockers** - note issues encountered
7. **Clean up** - archive completed ticket instructions

---

## Response Format

**Output Style:** Instruction file status, key sections populated

**After creation:**
```
✅ Instructions created: .github/instructions/<ticket-id>.instructions.md
Sections: <count> / <total>
Populated: [Context, Repositories, Implementation]
Pending: [Testing, Deployment]

Next: Update as work progresses
```

**Status check:**
```
Instructions: .github/instructions/<ticket-id>.instructions.md
Last updated: <timestamp>
Completeness: <n>%
Outstanding: <list sections needing updates>
```
