# Immediate Directives System

This directory contains the **immediate directives** mechanism for dynamic agent control during execution.

## Overview

The immediate directives system allows you to inject commands into an **actively executing agent** without stopping or restarting the workflow. The agent checks for directives **after every step** it takes.

## How It Works

### The File: `immediate.md`

- **Default State:** EMPTY (no content)
- **When You Need Control:** Add directives to the file
- **Agent Checks:** AFTER EVERY STEP (read, write, test, commit, search, etc.)
- **After Processing:** Agent clears the file and alerts you

### The Control File: `.stop`

- **Purpose:** Disables immediate directive checking
- **Create:** `touch .github/rules/.stop` to disable
- **Remove:** `rm .github/rules/.stop` to re-enable
- **Use Case:** Prevent interruptions during critical operations

### Workflow

```
Agent completes a step (e.g., reads file)
         ↓
Check if .github/rules/.stop exists
         ↓
    Exists?
    ↙     ↘
  YES      NO
   ↓        ↓
  Skip → Check immediate.md
         ↓
    Has content?
      ↙     ↘
    YES      NO
     ↓        ↓
  Process → Continue
   Clear  → Next Step
   Alert
```

## Usage

### 1. Create Your Directive

Edit `.github/rules/immediate.md` and add:

```markdown
## Directive: [Brief Title]
**Priority:** CRITICAL | HIGH | MEDIUM
**Action:** [Specific action to take]
**Context:** [Why this is needed]
```

### 2. Agent Processes (Automatically)

After its next step, the agent will:
1. Complete current step (e.g., read file, run test)
2. Check immediate.md (unless `.stop` file exists)
3. If directive found: Read and follow it
4. Clear the file (back to empty)
5. Alert you: "Found immediate directives: [summary]"
6. Continue with next step

### 3. File Returns to Empty State

The file is automatically emptied and ready for the next directive.

## Examples

### Example 1: Urgent Bug Fix

```markdown
## Directive: Fix Critical Bug
**Priority:** CRITICAL
**Action:** Fix null pointer exception in auth_handler.py line 142
**Context:** Production deployments failing, users cannot authenticate
```

### Example 2: Add Missing Tests

```markdown
## Directive: Add Test Coverage
**Priority:** HIGH
**Action:** Add unit tests for the new authentication module in tests/test_auth.py
**Context:** PR#46 needs 80%+ coverage before merge
```

### Example 3: Update Documentation

```markdown
## Directive: Document New Feature
**Priority:** MEDIUM
**Action:** Add usage examples for SSH key authentication to README.md
**Context:** Users asking how to configure SSH keys
```

## Best Practices

### DO ✅
- Use clear, specific action statements
- Provide context for why the directive exists
- Start with simple directives first
- Disable checking (`.stop` file) during critical operations

### DON'T ❌
- Leave multiple directives in the file (agent processes all, then clears)
- Use vague instructions ("fix the code")
- Forget to check that agent processed the directive
- Leave the file with content after agent processes it
- Interrupt during file writes or commits (use `.stop` first)

## Priority Levels

| Priority | When to Use | Expected Response |
|----------|-------------|-------------------|
| **CRITICAL** | Production issues, blocking work | After next step |
| **HIGH** | PR blockers, important features | After next step |
| **MEDIUM** | Improvements, documentation | After next step |

## Disabling Directive Checking

### When to Disable

Use the `.stop` file when:
- Agent is in middle of critical multi-step operation
- You don't want interruptions during specific work
- Performing sensitive git operations

```bash
# Disable checking
touch .github/rules/.stop

# Agent will now skip immediate.md checks between steps
```

### How to Re-enable

```bash
# Re-enable checking
rm .github/rules/.stop

# Agent will resume checking after each step
```

## Advanced Usage

### Chaining Directives

For multi-step work:

**Step 1:** Add first directive
**After Processing:** Add next directive  
**Continue:** Until all steps complete

### Conditional Directives

```markdown
## Directive: Conditional Fix
**Priority:** HIGH
**Action:** IF tests are failing, fix them; ELSE add new feature X
**Context:** Depends on current test status
```

### Emergency Override

For urgent changes that override current work:

```markdown
## Directive: STOP CURRENT WORK
**Priority:** CRITICAL
**Action:** Stop all current tasks. Run full test suite and report results.
**Context:** Need immediate validation before release
```

## Troubleshooting

### "Agent didn't process my directive"

**Checklist:**
1. Is there content in `.github/rules/immediate.md`?
2. Did you save the file after adding the directive?
3. Is checking enabled? (no `.github/rules/.stop` file)
4. Has the agent completed at least one step since adding it?
5. Is the directive format correct?

**Solution:** Wait for agent to complete current step, or send a prompt to trigger action

### "Directive too complex"

**Problem:** Trying to give multiple unrelated tasks

**Solution:** Break into multiple directives, add them one at a time

## FAQ

**Q: Can I add directives while agent is working?**  
A: Yes! That's the primary use case. Agent checks after each step.

**Q: What's a "step"?**  
A: Any discrete action: read file, write file, run test, commit, search, etc.

**Q: How do I prevent interruptions during critical work?**  
A: Create `.github/rules/.stop` file to disable checking temporarily.

**Q: What if I put something invalid in the file?**  
A: Agent will read it, may not understand it, and will still clear the file. Use proper format.

**Q: Can I use this for every request?**  
A: You can, but normal prompts are usually easier. Use for mid-execution control or urgent overrides.

**Q: Does checking slow down the agent?**  
A: Minimal overhead. Create `.stop` file if you notice issues.

---

**Last Updated:** 2026-02-14  
**Version:** 1.0
