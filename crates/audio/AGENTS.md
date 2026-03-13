# capy_audio

Audio playback and spatial sound.

## Scope

- Audio device initialization and output stream.
- Sound playback (play, pause, stop, volume, looping).
- Spatial/positional audio.
- Audio mixing and channel management.

## What Does NOT Belong Here

- Loading audio files from disk (belongs in `capy_assets`). This crate receives decoded audio data.
- Music selection or game-specific sound triggering logic (belongs in `capy_game`).
