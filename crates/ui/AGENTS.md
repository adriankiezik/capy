# capy_ui

In-game user interface system.

## Scope

- UI widget layout and rendering (buttons, text, panels, sliders).
- HUD elements and overlays.
- UI event handling (click, hover, focus).
- Text rendering and font management.
- Theming and styling.

## Directory Layout

- `src/debug/` — Self-enclosed egui integration for developer tools (world editor, debug overlays). Owns the full egui pipeline including GPU rendering via `egui-wgpu`. This is intentional: keeping egui rendering here avoids leaking egui dependencies into `capy_render`, and debug UI is only wired in by binaries that opt into it.

## What Does NOT Belong Here

- Game-specific screen flow or menu content (belongs in `capy_game`).
- Debug/editor-specific UI content — the UI *systems* (panels, widgets) belong in the binary or plugin that uses them (e.g., `world_editor`). This crate provides the egui platform and rendering infrastructure.
