# Agent Skills

This directory contains Agent Skills that GitHub Copilot loads on-demand when relevant to your task.

## What Are Agent Skills?

Agent Skills are specialized capabilities that include:
- Task-specific workflows and procedures  
- Scripts, templates, and examples
- Specialized domain knowledge
- Step-by-step guides for complex tasks

Unlike the always-on instructions in `copilot.instructions.md`, skills load progressively:
1. **Discovery**: Copilot always knows available skills (names/descriptions)
2. **Loading**: Full instructions load only when task matches description
3. **Resources**: Additional files load only if referenced

## Available Skills

- **[code-quality-check](code-quality-check/)** - Validate code quality before PRs
- **[decision-logging](decision-logging/)** - Create architectural decision records
- **[emergency-procedures](emergency-procedures/)** - Handle git emergencies and security incidents
- **[git-fork-setup](git-fork-setup/)** - Configure fork-based workflow safely
- **[project-instructions-setup](project-instructions-setup/)** - Create per-ticket instruction files
- **[pull-request-creation](pull-request-creation/)** - Generate comprehensive PR descriptions
- **[security-review](security-review/)** - Security checks before commits

## How to Use Skills

You don't need to manually activate skills. Simply:
1. Ask Copilot to perform a task
2. If your request matches a skill's description, it loads automatically
3. The skill's instructions guide Copilot's response

For example:
- "Check code quality before I create a PR" → Loads `code-quality-check`
- "Set up my git remotes safely" → Loads `git-fork-setup`
- "Create a PR for this work" → Loads `pull-request-creation`

## Creating a New Skill

To add a skill:

1. **Create directory structure**:
   ```
   .github/skills/
   └── your-skill-name/
       ├── SKILL.md          (required)
       ├── script.sh         (optional)
       ├── template.txt      (optional)
       └── examples/         (optional)
   ```

2. **Create SKILL.md with YAML frontmatter**:
   ```markdown
   ---
   name: your-skill-name
   description: Clear description of what the skill does and when to use it
   ---

   # Skill Instructions

   Detailed instructions for how to use this skill...

   ## When to Use
   - Situation 1
   - Situation 2

   ## Steps
   1. First step
   2. Second step

   ## Examples
   [Example code or output]
   ```

3. **Test the skill** by asking Copilot to perform the related task

## Skill Naming Guidelines

- Use lowercase letters
- Use hyphens for spaces (e.g., `web-app-testing`)
- Maximum 64 characters
- Be specific and descriptive

## Description Guidelines

- Be specific about capabilities AND use cases
- Help Copilot decide when to load the skill
- Maximum 1024 characters
- Include trigger phrases users might say

## Best Practices

### DO:
✅ **Keep skills focused** - One skill, one specialized capability  
✅ **Include examples** - Show expected inputs and outputs  
✅ **Reference included files** - Link to scripts/templates in the skill directory  
✅ **Test thoroughly** - Verify the skill loads when expected  
✅ **Update descriptions** - Improve them based on usage patterns  

### DON'T:
❌ **Duplicate instructions** - Don't repeat what's in copilot.instructions.md  
❌ **Make skills too broad** - Split large skills into focused ones  
❌ **Include secrets** - Never put credentials in skill files  
❌ **Forget YAML frontmatter** - Name and description are required  

## Portability

Agent Skills follow the [open standard](https://agentskills.io/) specification, meaning skills you create work across:
- GitHub Copilot in VS Code
- GitHub Copilot CLI
- GitHub Copilot coding agent
- Other skills-compatible AI agents

## Community Skills

Explore community-contributed skills:
- [github/awesome-copilot](https://github.com/github/awesome-copilot) - Community collection
- [anthropics/skills](https://github.com/anthropics/skills) - Reference skills

Always review shared skills before using them to ensure they meet your requirements and security standards.

## Related Documentation

- [VS Code Agent Skills Documentation](https://code.visualstudio.com/docs/copilot/customization/agent-skills)
- [Agent Skills Standard](https://agentskills.io/)
- [../copilot.instructions.md](../copilot.instructions.md) - Always-on workspace instructions
- [../instructions/](../instructions/) - Per-ticket instruction files
