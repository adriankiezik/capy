# capy_core

Foundation crate. Shared types, math primitives, and core abstractions used across all other crates.

## Scope

- Math types (vectors, matrices, quaternions, transforms).
- ECS primitives or trait definitions.
- Common error types.
- Utility traits and small helpers shared across crates.
- Type aliases and newtypes for domain concepts (EntityId, Time, etc.).

## What Does NOT Belong Here

- Anything with side effects (I/O, rendering, audio, networking).
- Platform-specific code.
- External dependencies — this crate must remain dependency-free within the workspace. Minimize external crate dependencies; prefer `no_std`-compatible choices.
- High-level abstractions that only one crate would use.

## Constraints

- **Zero internal workspace dependencies.** Every other crate may depend on `core`, so cycles are fatal.
- Keep types `Send + Sync` by default for future multithreading.
