# capy_world_editor

Binary entry point for the world editor tool. Developer-facing application for viewing and modifying voxel worlds.

## Layer

`world_editor` is a **composition root** — a binary that sits at the top of the dependency graph alongside `app`. It wires together engine subsystems with editor-specific plugins.

## Scope

- Editor application bootstrap and window configuration.
- Editor-specific plugins and systems (camera controls, selection, gizmos, world manipulation).
- Editor UI and tool modes.
- CLI argument parsing (if added later).
- Top-level error handling.

## What Does NOT Belong Here

- Game logic — game-specific systems belong in `capy_game`.
- Engine infrastructure — windowing, event loop, scheduling belong in `capy_engine`.
- Reusable subsystem code — rendering, physics, world generation belong in their respective crates.
- Library code — this is a binary crate, not a library.
