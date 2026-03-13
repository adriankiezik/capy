---
name: skill-creator
description: >
  Guide for creating Agent Skills — Use this skill when the user wants to create a new skill,
  write a SKILL.md file, set up a skill directory, or learn how to author,
  test, and optimize agent skills. Also use when the user mentions "agent
  skills", "SKILL.md", or asks how to package instructions for AI agents.
---

# How to Create an Agent Skill

Agent Skills are folders of instructions, scripts, and resources that AI agents
can discover and use to perform tasks more accurately and efficiently. Skills
use a simple, open format based on a `SKILL.md` file with YAML frontmatter
and Markdown instructions.

## Step 1: Plan the Skill

Before writing anything, answer these questions:

1. **What task does this skill help with?** Be specific (e.g., "generating PDF reports" not "helping with files").
2. **What knowledge does the agent need?** Domain expertise, API details, workflow steps, edge cases.
3. **What tools or scripts are needed?** Will the skill need bundled scripts, or just instructions?

## Step 2: Create the Directory Structure

A skill is a directory containing at minimum a `SKILL.md` file:

```
my-skill/
├── SKILL.md          # Required: metadata + instructions
├── scripts/          # Optional: executable code
├── references/       # Optional: additional documentation
└── assets/           # Optional: templates, resources
```

### Where to place skills

| Scope   | Path                          | Purpose                        |
|---------|-------------------------------|--------------------------------|
| Project | `<project>/.claude/skills/`   | Skills specific to a project   |
| Project | `<project>/.agents/skills/`   | Cross-client interoperability  |
| User    | `~/.claude/skills/`           | Available across all projects  |
| User    | `~/.agents/skills/`           | Cross-client interoperability  |

The directory name **must match** the `name` field in the frontmatter.

## Step 3: Write the SKILL.md File

The `SKILL.md` file has two parts: YAML frontmatter and Markdown body.

### Frontmatter (Required Fields)

```yaml
---
name: my-skill-name
description: >
  What this skill does and when to use it. Be specific and include
  keywords that help agents identify relevant tasks.
---
```

### Frontmatter Field Reference

| Field           | Required | Constraints                                                    |
|-----------------|----------|----------------------------------------------------------------|
| `name`          | Yes      | Max 64 chars. Lowercase letters, numbers, hyphens only. Must not start/end with hyphen. No consecutive hyphens. Must match directory name. |
| `description`   | Yes      | Max 1024 chars. Non-empty. Describes what the skill does AND when to use it. |

### Name Rules

- Only lowercase letters (`a-z`), numbers, and hyphens (`-`)
- Cannot start or end with a hyphen
- No consecutive hyphens (`--`)
- Must match the parent directory name

**Valid:** `pdf-processing`, `data-analysis`, `code-review`
**Invalid:** `PDF-Processing`, `-pdf`, `pdf--processing`

## Step 4: Write the Instruction Body

The Markdown body after the frontmatter is what the agent reads when the
skill is activated. There are no format restrictions — write whatever helps
the agent perform the task effectively.

### Recommended Structure

```markdown
# Skill Title

## When to use this skill
Describe the situations where this skill applies.

## Prerequisites
List any tools, packages, or setup needed.

## Workflow
Step-by-step instructions the agent should follow.

## Examples
Show example inputs and expected outputs.

## Edge Cases
Document known gotchas and how to handle them.
```

### Writing Effective Instructions

- **Be specific and actionable.** "Run `python3 scripts/extract.py --input FILE`" beats "use the extraction script."
- **Explain the why.** Reasoning-based instructions ("Do X because Y tends to cause Z") work better than rigid directives ("ALWAYS do X").
- **Use examples.** Show the agent what good output looks like.
- **Keep it concise.** Target under 5000 tokens / 500 lines. Move detailed reference material to separate files in `references/`.
- **Cover edge cases.** Document what to do when things go wrong.

## Step 5: Write an Effective Description

The `description` field is critical — it's the **only thing** agents see before
deciding whether to activate your skill.

### Description Best Practices

1. **Use imperative phrasing:** "Use this skill when..." not "This skill does..."
2. **Focus on user intent:** Describe what the user is trying to achieve, not internal mechanics.
3. **Be specific but broad:** List concrete use cases AND implicit ones.
4. **Include trigger keywords:** Mention terms users might use even if they don't name the domain directly.
5. **Stay under 1024 characters.**

### Before and After

```yaml
# Bad — too vague
description: Helps with PDFs.

# Good — specific about what and when
description: >
  Analyze CSV and tabular data files — compute summary statistics,
  add derived columns, generate charts, and clean messy data. Use
  this skill when the user has a CSV, TSV, or Excel file and wants
  to explore, transform, or visualize the data, even if they don't
  explicitly mention "CSV" or "analysis."
```

## Step 6: Add Scripts (Optional)

Bundle executable scripts in a `scripts/` directory for reusable logic.

### Script Design Rules for Agent Use

1. **No interactive prompts.** Agents run in non-interactive shells. Accept all input via CLI flags, env vars, or stdin.
2. **Include `--help` output.** This is how agents learn the script's interface.
3. **Write helpful error messages.** Say what went wrong, what was expected, and what to try.
4. **Use structured output.** Prefer JSON/CSV over free-form text.
5. **Separate data from diagnostics.** Structured data to stdout, progress/warnings to stderr.
6. **Make scripts idempotent.** Agents may retry. "Create if not exists" is safer than "create and fail on duplicate."
7. **Support `--dry-run` for destructive operations.**

### Self-Contained Scripts (Inline Dependencies)

Python with PEP 723 (run with `uv run scripts/my-script.py`):

```python
# /// script
# dependencies = [
#   "beautifulsoup4>=4.12,<5",
# ]
# ///
import sys
from bs4 import BeautifulSoup
# ... script logic
```

Reference scripts from SKILL.md using relative paths:

```markdown
## Available scripts
- **`scripts/validate.sh`** — Validates configuration files
- **`scripts/process.py`** — Processes input data

## Workflow
1. Run validation: `bash scripts/validate.sh "$INPUT_FILE"`
2. Process results: `uv run scripts/process.py --input results.json`
```

## Step 7: Add References (Optional)

Put detailed documentation in `references/` to keep the main SKILL.md lean:

```
my-skill/
├── SKILL.md
└── references/
    ├── REFERENCE.md      # Detailed technical reference
    ├── api-guide.md      # API documentation
    └── examples.md       # Extended examples
```

Reference them from SKILL.md:

```markdown
See [the API guide](references/api-guide.md) for endpoint details.
```

Keep file references one level deep. Avoid deeply nested reference chains.
