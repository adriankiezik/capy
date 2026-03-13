# capy_input

Input abstraction layer. Translates raw platform events into a clean input API.

## Scope

- Keyboard, mouse, and gamepad state tracking.
- Input action mapping (e.g., "Jump" → Space / Button A).
- Axis and button abstractions.
- Input event buffering and polling.

## What Does NOT Belong Here

- Raw winit event handling (belongs in `capy_engine` — this crate receives processed input).
- UI focus or text input for menus (belongs in `capy_ui`).
- Game-specific action definitions (belongs in `capy_game`; this crate provides the mapping system).
