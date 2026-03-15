# Capy Project — Workspace

Rust game and game engine. Workspace with crates under `crates/`.
```

## Project Structure

```
crates/
  game/           # Binary — game entry point + game-specific logic
  world_editor/   # Binary — developer world editing tool
  engine/         # Abstract orchestrator — plugin lifecycle, schedules, runner
  window/         # Windowing backend — winit, event loop, window creation
  core/           # Shared types, math, ECS primitives
  render/         # GPU rendering backend
  audio/          # Audio playback
  physics/        # Physics simulation, collision
  input/          # Input abstraction (keyboard, mouse, gamepad)
  assets/         # Asset loading and caching
  net/            # Networking / multiplayer
  ui/             # In-game UI (menus, HUD)
  world/          # Voxel terrain generation
  shared/         # Reusable systems shared across binaries
```

## Dependency Direction

Dependencies flow **downward** — each layer may depend on any layer below it, never the reverse.

```
[game, world_editor]                                       ← binaries, may depend on any crate
  ↓
shared                                                     ← may depend on engine, window, subsystems, + core
  ↓
[engine, window]                                           ← window depends on engine + core; engine depends on core only
  ↓
[render, audio, physics, input, assets, net, ui, world]    ← may depend on core only
  ↓
core                                                       ← no workspace dependencies
```

`game` and `world_editor` are **composition roots** — binaries that wire together whichever crates they need. `engine` owns plugin lifecycle and schedule orchestration. `window` owns the winit event loop and window creation — it provides `WindowPlugin` which registers a windowed runner. `shared` holds reusable cross-binary systems and plugin adapters that glue subsystems to lifecycle hooks. Subsystem crates must not depend on each other or on `shared`.

## Boundaries

- **NEVER** add dependencies to `core` — it must remain dependency-free within the workspace.
- **Workspace-level external dependencies** (`bevy_ecs`, `glam`, etc.) are declared in the root `Cargo.toml`. Each crate imports them directly via `dep.workspace = true` — `core` does not re-export third-party types.
- **NEVER** put game-specific logic in engine crates (`engine`, `render`, `core`, etc.).
- **Ask first** before adding new external dependencies.
- **Ask first** before creating new crates.
- Each crate has its own `AGENTS.md` describing scope — respect those boundaries.

## Error Handling

- **Library crates** use `thiserror` — each crate with fallible APIs defines its own error enum in `src/error.rs` with a `Result<T>` alias.
- **Binary crates** (`capy_game`, `capy_world_editor`) use `anyhow` at the boundary with `.context()` for human-readable messages.
- **No `.unwrap()` / `.expect()`** — workspace clippy lints warn on both. Use `?` to propagate errors.
- Errors aggregate upward: e.g. `RenderError` wraps wgpu errors, `WindowError` wraps winit errors, `EngineError` wraps runner errors.
- Empty crates get error types when they gain real fallible APIs, not before.
