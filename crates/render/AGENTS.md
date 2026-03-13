# capy_render

Rendering backend. Manages the GPU pipeline, draw calls, and visual output.

## Scope

- GPU device/surface initialization.
- Render pipeline and shader management.
- Draw call submission and frame presentation.
- Camera and viewport management.
- Render resource types (meshes, textures, materials).

## What Does NOT Belong Here

- Window creation or event loop (belongs in `capy_engine`).
- Asset loading from disk (belongs in `capy_assets`). This crate receives already-loaded data.
- UI layout or widget logic (belongs in `capy_ui`).
- Game-specific rendering decisions (e.g., "draw the player health bar").
