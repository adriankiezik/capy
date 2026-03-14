# capy_app

Binary entry point for the game. Thin launcher — delegates immediately to `capy_engine::run()`.

## Scope

- Application bootstrap and startup configuration.
- CLI argument parsing (if added later).
- Top-level error handling / panic hooks.

## What Does NOT Belong Here

- Game logic, rendering, input handling, or any subsystem code.
- Library code — this is a binary crate.
- Direct dependencies on subsystem crates (only depend on `capy_engine`).
