# Capy

A voxel game/engine written in Rust, built around a GPU raymarched voxel renderer. Successor to an earlier public voxel_renderer prototype, now split into a proper multi-crate workspace.

## How it works

- **Renderer** (`crates/render`): wgpu + WGSL compute-shader raymarcher. Rays are walked through the voxel world with a chunk-level DDA, against a sparse voxel tree (`crates/world`) that reduces duplicate subtrees into a DAG for compact GPU-resident storage.
- **Passes**: primary ray trace + shadow rays + reflection rays into a G-buffer, screen-space GTAO for ambient occlusion, a lighting pass (with underwater/water-surface screen-space distortion, water shading), and a final blit to present.
- **Upscaling**: optional NVIDIA DLSS and AMD FSR support behind Cargo feature flags (`dlss`, `fsr`); off by default and not required to build or run.
- **World**: terrain is generated on the CPU (`crates/world`) from a heightfield and baked into a sparse64tree/DAG structure; a separate `world_editor` binary (egui-based) lets you paint, mask, and place voxels and saves edits to disk as region/chunk files (`crates/assets`).
- **Engine**: a small plugin-based ECS engine (`bevy_ecs`) that owns the window (winit), input, and schedule; `game` and `world_editor` are the two binaries wired on top of it. `net`, `physics`, and `ui` are present as workspace crates but are mostly scaffolding today.

## Status

Personal project, work in progress. APIs, file formats, and shader structure change frequently. Not optimized for external contributors; expect rough edges, especially around the optional DLSS/FSR builds which require vendor SDKs not included in this repo.

## Requirements

- Rust (edition 2024, so a recent stable toolchain, e.g. 1.85+).
- A GPU/driver with Vulkan, DX12, or Metal support (via wgpu).
- Windows-only for the `dlss` and `fsr` Cargo features (they link against the NVIDIA DLSS SDK / require `DLSS_SDK` + `VULKAN_SDK` env vars, and static DXC for FSR). Not needed for a normal build.

## Build & run

This is a Cargo workspace with no root binary, so pick one of the two binaries explicitly:

```
cargo run --release -p capy_game          # the game
cargo run --release -p capy_world_editor  # the voxel world editor
```

## Controls (game)

- `W`/`A`/`S`/`D` — move
- `Space` / `Left Shift` — move up / down
- Mouse — look around

## Controls (world editor)

Uses an egui-based tool UI (brushes, masks, color picker, path/place tools) with its own shortcut set defined in `crates/world_editor/src/systems/shortcuts.rs`.

<!-- TODO: screenshot -->
