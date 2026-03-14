# capy_shared

Reusable systems and utilities shared across multiple binaries.

## Layer

`shared` sits at the same tier as `engine` and `game` — above subsystem crates, below binaries. Both `app` and `world_editor` may depend on it.

## Scope

- Cross-binary systems that don't belong in any single subsystem (e.g., fly camera controller).
- Shared utility systems used by multiple composition roots.

## What Does NOT Belong Here

- Subsystem logic (rendering, physics, input translation) — belongs in the respective subsystem crate.
- Game-specific logic — belongs in `capy_game`.
- Shared types or ECS primitives — belongs in `capy_core`.
- Engine infrastructure (windowing, event loop) — belongs in `capy_engine`.

## Constraints

- May depend on `capy_core` and subsystem crates only.
- Must not depend on `capy_game`, `capy_engine`, or binary crates.
