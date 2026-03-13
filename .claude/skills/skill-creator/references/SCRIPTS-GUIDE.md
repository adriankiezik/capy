# Scripts Guide for Agent Skills

Source: https://agentskills.io/skill-creation/using-scripts

## One-Off Commands (No scripts/ Directory Needed)

When an existing package does what you need, reference it directly in SKILL.md:

| Tool   | Example                              | Runtime     |
|--------|--------------------------------------|-------------|
| `uvx`  | `uvx ruff@0.8.0 check .`            | Python (uv) |
| `pipx` | `pipx run 'black==24.10.0' .`       | Python      |
| `npx`  | `npx eslint@9 --fix .`              | Node.js     |
| `bunx` | `bunx eslint@9 --fix .`             | Bun         |
| `deno` | `deno run npm:create-vite@6 my-app`  | Deno        |
| `go`   | `go run golang.org/x/tools/cmd/...`  | Go          |

Always pin versions for reproducibility.

## Self-Contained Scripts with Inline Dependencies

### Python (PEP 723 + uv)
```python
# /// script
# dependencies = [
#   "beautifulsoup4>=4.12,<5",
# ]
# requires-python = ">=3.10"
# ///
from bs4 import BeautifulSoup
# ... script logic
```
Run: `uv run scripts/extract.py`

### Deno (npm: specifiers)
```typescript
#!/usr/bin/env -S deno run
import * as cheerio from "npm:cheerio@1.0.0";
// ... script logic
```
Run: `deno run scripts/extract.ts`

### Bun (auto-install)
```typescript
#!/usr/bin/env bun
import * as cheerio from "cheerio@1.0.0";
// ... script logic
```
Run: `bun run scripts/extract.ts`

### Ruby (bundler/inline)
```ruby
require 'bundler/inline'
gemfile do
  source 'https://rubygems.org'
  gem 'nokogiri', '~> 1.16'
end
# ... script logic
```
Run: `ruby scripts/extract.rb`

## Designing Scripts for Agent Use

### Hard Requirements
- **No interactive prompts** — agents cannot respond to TTY input
- Accept all input via CLI flags, env vars, or stdin

### Best Practices
- **`--help` output** — brief description, flags, usage examples
- **Helpful errors** — say what went wrong + what to try
- **Structured output** — JSON/CSV to stdout, diagnostics to stderr
- **Idempotent** — "create if not exists" over "create and fail on duplicate"
- **`--dry-run`** — for destructive/stateful operations
- **Meaningful exit codes** — document in `--help`
- **Safe defaults** — destructive ops require `--confirm`/`--force`
- **Predictable output size** — support `--offset` for pagination, or `--output FILE`
