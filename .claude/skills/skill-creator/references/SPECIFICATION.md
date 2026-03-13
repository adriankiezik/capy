# Agent Skills Specification Reference

Source: https://agentskills.io/specification

## SKILL.md Format

The `SKILL.md` file must contain YAML frontmatter followed by Markdown content.

### Frontmatter Fields

#### `name` (required)
- 1-64 characters
- Unicode lowercase alphanumeric (`a-z`, `0-9`) and hyphens (`-`)
- Must not start or end with a hyphen
- Must not contain consecutive hyphens (`--`)
- Must match the parent directory name

#### `description` (required)
- 1-1024 characters
- Should describe both what the skill does and when to use it
- Should include specific keywords for agent task matching

#### `license` (optional)
- License name or reference to a bundled license file
- Example: `Apache-2.0` or `Proprietary. LICENSE.txt has complete terms`

#### `compatibility` (optional)
- 1-500 characters
- Environment requirements: intended product, system packages, network access
- Example: `Requires git, docker, jq, and access to the internet`

#### `metadata` (optional)
- Map from string keys to string values
- For additional properties not in the spec
- Example: `author: example-org`, `version: "1.0"`

#### `allowed-tools` (optional, experimental)
- Space-delimited list of pre-approved tools
- Example: `Bash(git:*) Bash(jq:*) Read`

## Directory Conventions

### Standard Directories

- `scripts/` — Executable code (Python, Bash, JavaScript, etc.)
- `references/` — Additional documentation loaded on demand
- `assets/` — Static resources (templates, images, data files)

### Scan Locations

Project-level:
- `<project>/.<client>/skills/`
- `<project>/.agents/skills/`

User-level:
- `~/.<client>/skills/`
- `~/.agents/skills/`

### Name Collision Resolution
- Project-level skills override user-level skills
- Within same scope, first-found or last-found (be consistent)

## Progressive Disclosure Tiers

1. **Catalog** (~100 tokens): name + description loaded at startup
2. **Instructions** (<5000 tokens recommended): full SKILL.md body on activation
3. **Resources** (varies): scripts, references, assets loaded on demand

## File References

- Use relative paths from the skill root directory
- Keep references one level deep from SKILL.md
- Avoid deeply nested reference chains

## Validation

Use the skills-ref library: `skills-ref validate ./my-skill`
Repository: https://github.com/agentskills/agentskills/tree/main/skills-ref
