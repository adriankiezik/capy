# Capy Project — Workspace

Rust game and game engine. Workspace with crates under `crates/`.
```

## Project Structure

```
crates/
  app/       # Binary entry point — calls capy_engine::run()
  engine/    # Window, event loop, orchestrates subsystems
  core/      # Shared types, math, ECS primitives
  render/    # GPU rendering backend
  audio/     # Audio playback
  physics/   # Physics simulation, collision
  input/     # Input abstraction (keyboard, mouse, gamepad)
  assets/    # Asset loading and caching
  net/       # Networking / multiplayer
  ui/        # In-game UI (menus, HUD)
  game/      # Game-specific logic and systems
```

## Dependency Direction

Dependencies flow **inward** — higher-level crates depend on lower-level ones, never the reverse.

```
app → engine → [render, audio, physics, input, assets, net, ui, game] → core
```

## Boundaries

- **NEVER** add dependencies to `core` — it must remain dependency-free within the workspace.
- **NEVER** put game-specific logic in engine crates (`engine`, `render`, `core`, etc.).
- **Ask first** before adding new external dependencies.
- **Ask first** before creating new crates.
- Each crate has its own `AGENTS.md` describing scope — respect those boundaries.

## Error Handling

- **Library crates** use `thiserror` — each crate with fallible APIs defines its own error enum in `src/error.rs` with a `Result<T>` alias.
- **Binary crate** (`capy_app`) uses `anyhow` at the boundary with `.context()` for human-readable messages.
- **No `.unwrap()` / `.expect()`** — workspace clippy lints warn on both. Use `?` to propagate errors.
- Errors aggregate upward: e.g. `RenderError` wraps wgpu errors, `EngineError` wraps `RenderError` + winit errors.
- Empty crates get error types when they gain real fallible APIs, not before.
