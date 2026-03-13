---
name: commit
description: >
  MANDATORY: You MUST invoke this skill for ALL git commits — whether the user
  says "commit", "save progress", "commit the change", "and commit it", uses
  /commit, or any other phrasing that results in creating a git commit.
  Never perform commit workflows manually; always use this skill instead.
  Enforces game-dev commit conventions with scoped prefixes
  (feat, fix, art, audio, level, ui, balance, perf, refactor, docs, chore, test).
---

# Game Dev Commit

## When to use

When the user asks to commit changes, save progress, or invokes /commit. Replaces default commit workflow with game-dev conventions.

## Format

```
<prefix>: <short summary>
```

- Lowercase prefix, colon, space
- Summary in lowercase imperative mood ("add", not "added"), max ~70 chars
- No trailing period

## Prefixes

- feat — new gameplay features, mechanics, systems
- fix — bug fixes
- art — sprites, textures, models, animations
- audio — music, sound effects, voiceover
- level — level design, maps, world building
- ui — menus, HUD, inventory screens
- balance — tuning numbers (damage, speed, drop rates)
- perf — optimization (frame rate, memory, load times)
- refactor — code restructuring without behavior change
- docs — design docs, READMEs
- chore — build config, dependencies, tooling
- test — adding or fixing tests

## Choosing a prefix

- Changed a damage value? balance
- New enemy type with AI? feat
- Enemy stuck on walls? fix
- New tileset? art
- Rearranged code, same behavior? refactor
- Particle system uses object pooling now? perf
- New dungeon layout? level
- Added a health bar? ui
- Boss background music? audio

## Optional body

For non-trivial changes, add a blank line after the summary then explain why. Wrap at 72 chars.

```
balance: reduce sword base damage from 50 to 35

Players were clearing early zones too quickly, trivializing the first
boss encounter. This brings melee TTK closer to ranged.
```

## Workflow

1. Run `git status` (never use -uall) and `git diff` (staged + unstaged) in parallel.
2. Run `git log --oneline -5` to match recent style.
3. Pick the single most appropriate prefix. If changes span categories, use the primary intent. If truly unrelated, suggest splitting into separate commits.
4. Draft the commit message.
5. Stage only relevant files by name — never `git add -A` or `git add .`
6. Commit using a HEREDOC for the message.
7. Run `git status` after to verify.

## Rules

- Never use `git add -A` or `git add .` — stage specific files
- Never amend unless user explicitly asks
- Never push unless user explicitly asks
- Never skip hooks (no --no-verify)
- If a pre-commit hook fails, fix, re-stage, create a NEW commit
- If no changes exist, tell the user — no empty commits
- Never commit secrets (.env, credentials, API keys)
