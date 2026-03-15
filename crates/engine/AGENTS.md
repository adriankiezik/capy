# capy_engine

Abstract engine orchestrator. Manages plugin lifecycle, schedule registration, and runner dispatch.

## Scope

- Plugin registration and lifecycle (`EngineBuilder`).
- Schedule setup and runner abstraction (`Runner` resource).
- Schedule execution utilities (`schedule_runner`).
- Engine-level error types.

## What Does NOT Belong Here

- Windowing, event loop, or winit integration (belongs in `capy_window`).
- Rendering implementation (belongs in `capy_render`).
- Physics, audio, networking, or other subsystem internals.
- Game-specific logic or content.
- Input mapping or action bindings (belongs in `capy_input`).
