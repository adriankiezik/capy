# capy_render

Rendering backend. Manages the GPU pipeline, draw calls, and visual output.

## Scope

- GPU device/surface initialization.
- Render pipeline and shader management.
- Draw call submission and frame presentation.
- Camera and viewport management.
- Render resource types (meshes, textures, materials).

## Architecture

State is split into separate ECS resources rather than a single monolithic renderer struct:

- **GpuContext** — shared wgpu fundamentals (device, queue, surface, config). Every rendering feature depends on this.
- **StreamingPipeline** — voxel ray-march compute pass. Owns its compute pipeline, bind groups, all GPU buffers (camera, streaming info, chunk data), and the storage/depth textures that other passes read.
- **BlitPipeline** — fullscreen blit render pass. Reads the storage texture from streaming and presents it to the screen.

Cross-cutting systems (resize, render frame) live at the top level of `systems/`. Feature-specific systems live in `systems/<feature>/` subdirectories. Resources follow the same pattern under `resources/<feature>/`.

### Adding a new rendering feature

1. Create a pipeline resource in `resources/<feature>/` that owns its GPU state.
2. Create systems in `systems/<feature>/` for init, resize, and per-frame work.
3. Register the new systems in `render_plugin.rs` at the correct point in the schedule chain.

No existing files need modification beyond the plugin registration.

### Render frame order

Startup: init gpu → init streaming → init blit (chained, order matters).
Per frame: upload camera → resize → render frame (compute pass + blit pass + present).

## What Does NOT Belong Here

- Window creation or event loop (belongs in `capy_engine`).
- Asset loading from disk (belongs in `capy_assets`). This crate receives already-loaded data.
- UI layout or widget logic (belongs in `capy_ui`).
- Game-specific rendering decisions (e.g., "draw the player health bar").
