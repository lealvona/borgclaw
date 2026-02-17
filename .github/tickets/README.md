# Per-Ticket Instruction Files

This directory contains project-specific instruction files that provide context for individual tickets or projects.

## Purpose

Per-ticket instruction files help AI assistants understand:
- Specific ticket scope and boundaries
- Repository configurations and relationships
- Implementation details and decisions made
- Work completed and pending tasks
- Critical findings and blockers
- Follow-up work required

## Usage

### Creating a New Instruction File

1. **Copy the template**
   ```bash
   cp ticket-template.instructions.md [TICKET-ID].instructions.md
   ```

2. **Replace all placeholders** with actual values:
   - `[TICKET-ID]` → Your ticket number (e.g., INT-2614)
   - `[organization-name]` → Organization name
   - `[repo-name]` → Repository names
   - `[your-fork-username]` → Your GitHub username
   - All other bracketed placeholders

3. **Customize sections**:
   - Delete sections that don't apply
   - Add additional sections as needed
   - Keep it focused and relevant

4. **Keep it updated** as work progresses

### File Naming Convention

Use the ticket ID as the filename:
```
INT-2614.instructions.md
JIRA-123.instructions.md
PROJ-456.instructions.md
```

## Template Structure

The template includes sections for:
- **Scope Definition** - What IS and ISN'T in scope
- **Project Context** - Basic ticket information
- **Repository Configuration** - Git setup and remotes
- **Implementation Details** - What changed and why
- **Work Status** - Completed and pending tasks
- **Critical Findings** - Blockers and issues
- **Reference Implementations** - Similar work to learn from
- **Technology Stack** - Languages, tools, frameworks
- **Security Implementation** - Git safety and security measures
- **Follow-Up Work** - Next steps and future tickets
- **Testing & Validation** - How to verify changes
- **Lessons Learned** - Insights for future work
- **Quick Commands** - Useful commands for this work

## Best Practices

### DO:
✅ Create a new instruction file for each ticket  
✅ Update the file as work progresses  
✅ Document blockers and critical findings immediately  
✅ Include specific commands and examples  
✅ Link to related PRs, tickets, and documentation  
✅ Keep scope section prominent and clear  

### DON'T:
❌ Reuse the same file for multiple tickets  
❌ Include information about unrelated work  
❌ Let the file become stale or outdated  
❌ Include sensitive information (credentials, secrets)  
❌ Document future work in detail (use follow-up tickets)  

## Integration with AI Assistants

These instruction files are automatically loaded by GitHub Copilot and similar AI assistants when:
- They match the pattern `*.instructions.md`
- They're in the `.github/instructions/` directory
- The workspace is active

The AI will use this context to:
- Understand ticket scope and boundaries
- Make informed decisions aligned with project patterns
- Avoid scope creep
- Follow established conventions
- Suggest appropriate next steps

## Examples

See `ticket-template.instructions.md` for the complete template structure with all available sections.

## Maintenance

- **Review monthly**: Remove instruction files for completed tickets older than 3 months
- **Archive important context**: Move key insights to project-wide documentation
- **Update template**: Improve the template based on lessons learned
