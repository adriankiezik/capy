# capy_assets

Asset loading, caching, and lifecycle management.

## Scope

- Loading files from disk (textures, models, audio, configs, levels).
- Asset caching and reference counting.
- Hot-reloading support.
- Asset handles and typed references.
- Format parsing and deserialization.

## What Does NOT Belong Here

- GPU resource creation from loaded data (belongs in `capy_render`).
- Audio decoding into playback buffers (belongs in `capy_audio`).
- Game-specific asset organization or content decisions.
