# capy_game

Game-specific logic. The only crate that should contain content and design decisions.

## Scope

- Gameplay systems (player, enemies, items, scoring, progression).
- Game state machines (main menu, playing, paused, game over).
- Level/scene definitions and loading.
- Game-specific input action definitions.
- Content configuration (balance values, spawn rates, etc.).

## What Does NOT Belong Here

- Engine infrastructure (windowing, rendering pipeline, physics solver).
- Reusable abstractions — if something could serve another game, it belongs in a lower-level crate.
- Direct GPU or audio API calls — use `capy_render` and `capy_audio` interfaces.
