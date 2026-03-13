# capy_engine

Main engine crate. Orchestrates subsystem initialization, the main loop, and shutdown.

## Scope

- Window creation and management.
- Event loop and frame timing.
- Subsystem orchestration (init, update, shutdown ordering).
- Engine configuration and lifecycle.

## What Does NOT Belong Here

- Rendering implementation (belongs in `capy_render`).
- Physics, audio, networking, or other subsystem internals.
- Game-specific logic or content.
- Input mapping or action bindings (belongs in `capy_input`).
