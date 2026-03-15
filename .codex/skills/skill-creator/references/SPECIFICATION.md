# Agent Skills Specification

Source: https://agentskills.io/specification

## SKILL.md Format

YAML frontmatter followed by markdown content.

### Required Fields

name:
- 1-64 chars, lowercase a-z, 0-9, hyphens
- No leading/trailing/consecutive hyphens
- Must match parent directory name

description:
- 1-1024 chars
- Describes what the skill does and when to use it
- Include keywords for agent task matching

### Optional Fields

- license — license name or reference to bundled file (e.g. `Apache-2.0`)
- compatibility — 1-500 chars, environment requirements: intended product, system packages, network access
- metadata — string key-value map for additional properties (e.g. `author: example-org`, `version: "1.0"`)
- allowed-tools — space-delimited pre-approved tools (experimental, e.g. `Bash(git:*) Read`)

## Directory Conventions

Standard directories:
- scripts/ — executable code
- references/ — additional docs loaded on demand
- assets/ — static resources (templates, images, data)

Scan locations (project level):
- `<project>/.<client>/skills/`
- `<project>/.agents/skills/`

Scan locations (user level):
- `~/.<client>/skills/`
- `~/.agents/skills/`

Name collisions: project-level overrides user-level.

## Progressive Disclosure

1. Catalog (~100 tokens) — name + description loaded at startup
2. Instructions (<5000 tokens) — full SKILL.md body on activation
3. Resources (varies) — scripts, references, assets loaded on demand

## File References

- Use relative paths from skill root
- Keep one level deep from SKILL.md
- Avoid nested reference chains

## Validation

Use skills-ref library: `skills-ref validate ./my-skill`
Repo: https://github.com/agentskills/agentskills/tree/main/skills-ref
