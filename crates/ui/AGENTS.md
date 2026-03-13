# capy_ui

In-game user interface system.

## Scope

- UI widget layout and rendering (buttons, text, panels, sliders).
- HUD elements and overlays.
- UI event handling (click, hover, focus).
- Text rendering and font management.
- Theming and styling.

## What Does NOT Belong Here

- Game-specific screen flow or menu content (belongs in `capy_game`).
- Low-level draw calls (belongs in `capy_render` — this crate describes *what* to draw, render draws it).
- Debug/editor tooling (consider a separate `editor` crate if needed).
