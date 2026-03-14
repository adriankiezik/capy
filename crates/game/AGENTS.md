# capy_game

Game-specific logic. The only crate that should contain content and design decisions.

## Layer

`game` sits **above** the subsystem crates in the dependency graph:

```
app                                                        ← binary, may depend on any crate
  ↓
[engine, game]                                             ← may depend on subsystems + core
  ↓
[render, audio, physics, input, assets, net, ui, world]    ← may depend on core only
  ↓
core                                                       ← no workspace dependencies
```

It may depend on `core` plus any subsystem crate it needs (e.g., `world` for terrain generation). It is **not** a peer of subsystem crates — it composes them into gameplay.

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
