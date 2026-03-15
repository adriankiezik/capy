# capy_render

Rendering backend. Manages the GPU pipeline, draw calls, and visual output.

## Scope

- GPU device/surface initialization.
- Render pipeline and shader management.
- Draw call submission and frame presentation.
- Camera and viewport management.
- Render resource types (meshes, textures, materials).

## Architecture

State is split into separate ECS resources rather than a single monolithic renderer struct. Internal resources (GPU context, pipeline state) are `NonSend` and not re-exported. External crates only interact through the public API in `lib.rs`.

Cross-cutting systems (resize, render frame) live at the top level of `systems/`. Feature-specific systems live in `systems/<feature>/` subdirectories. Resources follow the same pattern under `resources/<feature>/`.

### Adding a new rendering feature

1. Create a pipeline resource in `resources/<feature>/` that owns its GPU state.
2. Create systems in `systems/<feature>/` for init, resize, and per-frame work.
3. Register the new systems in `render_plugin.rs` at the correct point in the schedule chain.

No existing files need modification beyond the plugin registration.

### Render frame order

Startup: init gpu → init streaming → init blit (chained, order matters).
Per frame: upload camera → resize → custom compute passes → render frame → overlays + present.

## Usage from External Binaries

### Plugin ordering

The render plugin must be registered **after** the window plugin (it needs the window handle at startup) and **before** any plugin that wants to use GPU resources or register callbacks.

### Input resources (from core)

The render crate reads a small set of ECS resources defined in core. The window handle is required; camera and mesh data are optional (missing ones log a warning and render an empty void). Check `lib.rs` re-exports and the startup systems to see exactly which resources are consumed.

### Provided resources

The crate inserts several public `Send` resources that external code can use:
- **GPU access** — cloned device + queue handles for custom GPU work.
- **Shared voxel buffers** — the GPU buffers backing the main voxel data, readable by custom compute passes.
- **Callback registries** — for injecting custom compute passes and render overlays (see below).

All internal pipeline state is `NonSend` and hidden. External code must not try to access it.

### Extension points

There are two callback registries exported from `lib.rs`:

1. **Compute pass callbacks** — register a function that encodes custom compute work each frame. These run before the main render pass. Used by the world editor for GPU picking.
2. **Render overlay callbacks** — register a function that draws on top of the final frame (e.g., egui UI). These run at the very end before present.

Both are registered during plugin setup via static methods on the callback registry resources.

### Shader composition

The crate exports a helper that builds compute shaders with a common WGSL prefix (camera uniforms, AABB helpers, DAG traversal, etc.) already injected. External crates use this to write minimal custom shaders that reuse the renderer's traversal logic. It also exports helpers for creating and updating camera uniform buffers.

### Scheduling

External systems that update camera or mesh data should run in the **Update** schedule. The render schedule runs after Update and expects those resources to already be up to date.

## What Does NOT Belong Here

- Window creation or event loop (belongs in `capy_engine`).
- Asset loading from disk (belongs in `capy_assets`). This crate receives already-loaded data.
- UI layout or widget logic (belongs in `capy_ui`).
- Game-specific rendering decisions (e.g., "draw the player health bar").
