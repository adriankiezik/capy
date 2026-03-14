# Capy Project — Workspace

Rust game and game engine. Workspace with crates under `crates/`.
```

## Project Structure

```
crates/
  app/            # Binary — game entry point
  world_editor/   # Binary — developer world editing tool
  engine/         # Window, event loop, orchestrates subsystems
  core/           # Shared types, math, ECS primitives
  render/         # GPU rendering backend
  audio/          # Audio playback
  physics/        # Physics simulation, collision
  input/          # Input abstraction (keyboard, mouse, gamepad)
  assets/         # Asset loading and caching
  net/            # Networking / multiplayer
  ui/             # In-game UI (menus, HUD)
  world/          # Voxel terrain generation
  game/           # Game-specific logic and systems
```

## Dependency Direction

Dependencies flow **downward** — each layer may depend on any layer below it, never the reverse.

```
[app, world_editor]                                        ← binaries, may depend on any crate
  ↓
[engine, game]                                             ← may depend on subsystems + core
  ↓
[render, audio, physics, input, assets, net, ui, world]    ← may depend on core only
  ↓
core                                                       ← no workspace dependencies
```

`app` and `world_editor` are **composition roots** — binaries that wire together whichever crates they need. `game` sits above the subsystem crates as game-specific composition. Subsystem crates must not depend on each other or on `game`.

## Boundaries

- **NEVER** add dependencies to `core` — it must remain dependency-free within the workspace.
- **Workspace-level external dependencies** (`bevy_ecs`, `glam`, etc.) are declared in the root `Cargo.toml`. Each crate imports them directly via `dep.workspace = true` — `core` does not re-export third-party types.
- **NEVER** put game-specific logic in engine crates (`engine`, `render`, `core`, etc.).
- **Ask first** before adding new external dependencies.
- **Ask first** before creating new crates.
- Each crate has its own `AGENTS.md` describing scope — respect those boundaries.

## Error Handling

- **Library crates** use `thiserror` — each crate with fallible APIs defines its own error enum in `src/error.rs` with a `Result<T>` alias.
- **Binary crates** (`capy_app`, `capy_world_editor`) use `anyhow` at the boundary with `.context()` for human-readable messages.
- **No `.unwrap()` / `.expect()`** — workspace clippy lints warn on both. Use `?` to propagate errors.
- Errors aggregate upward: e.g. `RenderError` wraps wgpu errors, `EngineError` wraps `RenderError` + winit errors.
- Empty crates get error types when they gain real fallible APIs, not before.
