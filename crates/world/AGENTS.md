# capy_world

CPU-side voxel world representation. Generates, stores, and serializes voxel data for the renderer.

## Scope

- Voxel grid storage and access.
- Terrain generation (Perlin noise heightfields).
- Sparse octree (sparse64tree) construction and serialization.
- DAG reduction for memory-efficient GPU upload.
- Material definitions and color palettes.

## What Does NOT Belong Here

- GPU buffer creation or upload (belongs in `capy_render`).
- Chunk streaming, loading, or caching policies (belongs in `capy_engine` or a future `capy_assets`).
- Game-specific world logic like biomes or structures (belongs in `capy_game`).
