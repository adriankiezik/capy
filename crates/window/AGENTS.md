# capy_window

Windowing backend. Wraps winit to provide window creation, event loop, and input event translation.

## Scope

- Window creation and management via winit.
- Event loop hosting (`ApplicationHandler` implementation).
- Translating winit events into ECS messages (`KeyboardInputMessage`, `MouseMotionMessage`).
- Key code translation (winit `KeyCode` → core `KeyCode`).
- Hook registries for window lifecycle callbacks (`OnAppResumed`, `OnWindowEvent`, `OnBeginFrame`, `OnEndFrame`, `WantsPointerInput`).
- `WindowPlugin` that registers the windowed runner.

## What Does NOT Belong Here

- Rendering implementation (belongs in `capy_render`).
- Engine orchestration, schedule definitions, or plugin lifecycle (belongs in `capy_engine`).
- Input abstraction or action mapping (belongs in `capy_input`).
- Game-specific logic or content.
- Window trait definition or `GameWindow` resource (belongs in `capy_core`).
