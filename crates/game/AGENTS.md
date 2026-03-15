# capy_game

Binary entry point for the game. Combines game-specific logic with application bootstrap.

## Layer

`game` is a **composition root** — a binary that sits at the top of the dependency graph alongside `world_editor`. It wires together engine subsystems with game-specific plugins and systems.

## Scope

- Application bootstrap and startup configuration.
- Game-specific plugins and systems (gameplay, state machines, level loading).
- Game-specific input action definitions.
- Content configuration (balance values, spawn rates, etc.).
- CLI argument parsing (if added later).
- Top-level error handling / panic hooks.

## What Does NOT Belong Here

- Engine infrastructure (windowing, event loop) — belongs in `capy_engine`.
- Reusable subsystem code (rendering, physics, world generation) — belongs in respective crates.
- Reusable cross-binary systems — belongs in `capy_shared`.
