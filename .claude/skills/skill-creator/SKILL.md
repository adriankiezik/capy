---
name: skill-creator
description: >
  Guide for creating Agent Skills — Use this skill when the user wants to create a new skill,
  write a SKILL.md file, set up a skill directory, or learn how to author,
  test, and optimize agent skills. Also use when the user mentions "agent
  skills", "SKILL.md", or asks how to package instructions for AI agents.
---

# Creating Agent Skills

Skills are folders with instructions, scripts, and resources that agents discover and use. Format: SKILL.md with YAML frontmatter + markdown body.

## Plan

Answer before writing:
- What task does this skill solve? Be specific.
- What knowledge does the agent need? Domain, APIs, workflows, edge cases.
- What tools or scripts are needed? Bundled scripts or instructions only?

## Directory Structure

Minimum requirement: a folder with SKILL.md.

```
my-skill/
  SKILL.md            # required: metadata + instructions
  scripts/            # optional: executable code
  references/         # optional: detailed docs
  assets/             # optional: templates, resources
```

Skill locations:
- Project scope — `<project>/.claude/skills/` or `<project>/.agents/skills/`
- User scope — `~/.claude/skills/` or `~/.agents/skills/`

Cross-client interop uses the `.agents/` paths. Directory name must match the `name` frontmatter field.

## SKILL.md File

Two parts: YAML frontmatter, then markdown body.

### Frontmatter

```yaml
---
name: my-skill-name
description: >
  What this skill does and when to use it. Include keywords
  that help agents match relevant tasks.
---
```

Required fields:
- name — max 64 chars, lowercase a-z, numbers, hyphens only, must match directory name
- description — max 1024 chars, describes what the skill does and when to use it

Name rules:
- Lowercase letters, numbers, hyphens only
- No leading/trailing/consecutive hyphens
- Valid: pdf-processing, data-analysis, code-review
- Invalid: PDF-Processing, -pdf, pdf--processing

### Instruction Body

The markdown after frontmatter is what the agent reads on activation. No format restrictions — write what helps the agent succeed.

Recommended sections:

```
# Skill Title
## When to use
## Prerequisites
## Workflow
## Examples
## Edge Cases
```

Writing effective instructions:
- Be specific — `python3 scripts/extract.py --input FILE` beats "use the extraction script"
- Explain why — reasoning ("do X because Y causes Z") beats rigid rules ("ALWAYS do X")
- Show examples of good output
- Stay under 5000 tokens / 500 lines, move detail to references/
- Cover failure modes

## Description

The description is the only thing agents see before activation. Make it count.

Best practices:
- Use imperative phrasing — "Use this skill when..." not "This skill does..."
- Focus on user intent, not internals
- List concrete and implicit use cases
- Include trigger keywords users might say
- Stay under 1024 chars

Bad — too vague:
`description: Helps with PDFs.`

Good — specific about what and when:
```yaml
description: >
  Analyze CSV and tabular data files — compute summary statistics,
  add derived columns, generate charts, clean messy data. Use when
  the user has a CSV, TSV, or Excel file and wants to explore,
  transform, or visualize data.
```

## Scripts (Optional)

Bundle reusable logic in scripts/. Design rules:
- No interactive prompts — accept input via CLI flags, env vars, or stdin
- Include --help output so agents learn the interface
- Write clear error messages — what failed, what was expected, what to try
- Prefer structured output (JSON/CSV) over free-form text
- Data to stdout, diagnostics to stderr
- Make scripts idempotent — agents may retry
- Support --dry-run for destructive operations

Self-contained Python (PEP 723, run with `uv run scripts/my-script.py`):

```python
# /// script
# dependencies = ["beautifulsoup4>=4.12,<5"]
# ///
import sys
from bs4 import BeautifulSoup
```

Reference scripts from SKILL.md using relative paths.

## References (Optional)

Keep SKILL.md lean. Put detailed docs in references/:

```
my-skill/
  SKILL.md
  references/
    REFERENCE.md
    api-guide.md
    examples.md
```

Link from SKILL.md: `See [API guide](references/api-guide.md) for details.`

Keep references one level deep. Avoid nested reference chains.

## Optimizing .md for Token Efficiency

Skills are loaded into agent context. Every token costs capacity. Write markdown that is clear for humans and lean for models.

Guidelines:
- Prefer lists over tables — tables add `|`, `---`, padding; lists convey the same info with fewer tokens (10-25% savings)
- Skip decorative separators — `---`, `===`, `***` add tokens with zero meaning; use headers instead
- Avoid repeating context — state a subject once, then list its properties
- Keep headings short — headings repeat in context; "Inventory" beats "Overview of the Player Inventory Storage Management System"
- Avoid deep nesting — repeated `>`, `-`, spaces add up fast
- Use backticks sparingly — only for actual code identifiers, not emphasis
- Minimize code blocks — show simplified examples, reference full files instead of embedding them
- Move large data to .json/.csv/.yaml — never embed stat tables or item databases in markdown
- Use consistent vocabulary — pick one term per concept and stick with it; "inventory manager" everywhere, not sometimes "item storage controller"
- Write dense English — "Stores player items" beats "This system is designed with the intention of allowing the player to have the ability to store items"
- Cut blank lines — each costs a token; one between sections is enough
- Skip decorative markers — NOTE:, WARNING: without blockquote symbols
- Avoid emojis — they tokenize inefficiently

Biggest token costs in markdown (worst first):
- Large embedded tables
- Massive code blocks
- Repeated explanations
- Decorative formatting
- Long headings
- Deep nesting

Recommended layout for any system doc:

```
# System Name
## Purpose
Short explanation.
## Responsibilities
- item
- item
## Data
- structure
- format
## Related
link-to-other-doc.md
```
