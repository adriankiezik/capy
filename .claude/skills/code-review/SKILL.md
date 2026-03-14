---
name: code-review
description: >
  Review code changes against Capy engine architecture and Rust best practices.
  Use when the user asks to review code, review a PR, check a diff, audit
  changes, or says "review this". Covers dependency direction, crate boundaries,
  trait design, ownership, error handling, performance, safety, and naming.
---

# Code Review

## When to use

When the user asks to review code, audit changes, check a PR, or validate that changes follow project conventions. Also use proactively before committing significant changes.

## Purpose

Ensure every change respects Capy's architectural boundaries, Rust idioms, and project conventions. A review catches dependency violations, leaky abstractions, unsafe misuse, and naming drift before they compound.

## Workflow

1. Read the diff — `git diff` for unstaged, `git diff --cached` for staged, or `git diff main...HEAD` for branch review.
2. Identify which crates are touched. Read each crate's `AGENTS.md` to confirm scope.
3. Walk through every section of the checklist below.
4. Verify before flagging. For every potential finding:
   - Trace actual producers and consumers across crates. A type or field that looks domain-specific may be the only correct solution given dependency constraints between sibling subsystems.
   - When questioning placement in a crate, confirm which crates actually read and write the item. "Written by crate A, read by crate B" makes it cross-crate by definition, even if the content is domain-specific.
   - When a pattern looks non-idiomatic, consider what the concrete alternatives would require. If every alternative introduces worse trade-offs (version coupling, dependency leaks, code duplication), the pattern is a justified design choice, not a violation.
   - Do not flag something as wrong if you cannot name a strictly better alternative that works within the existing architectural constraints.
5. Report findings grouped by severity: blocking, warning. Do NOT include nitpicks or code style issues.
5. For each finding, cite the file, line, and which rule it violates.
6. Do NOT include positive comments, praise, or "looks good" notes — only report issues.
7. If there are no issues, just say the changes look good and there are no issues. Do not pad the review.

## Architectural Principles

Capy is a Rust game engine workspace. Crates are layered by abstraction level with strict boundaries.

Guiding rules:
- Each crate has one job — rendering, physics, audio, etc.
- Engine crates are reusable across games; game-specific logic lives only in `capy_game`
- Prefer composition over inheritance — combine small traits, don't build deep hierarchies
- Minimize coupling — crates communicate through traits and types defined in `capy_core`
- Design for `Send + Sync` by default to support future multithreading

## Dependency Direction

Dependencies flow downward. Each layer may depend on any layer below it, never the reverse.

```
[app, world_editor]                                        <- binaries, may depend on any crate
  |
[engine, game, shared]                                     <- may depend on subsystems + core
  |
[render, audio, physics, input, assets, net, ui, world]    <- may depend on core only
  |
core                                                       <- no workspace dependencies
```

`app` and `world_editor` are composition roots — binaries that wire together whichever crates they need. `game` and `shared` sit above the subsystem crates. `game` is game-specific composition; `shared` holds reusable cross-binary systems (e.g., fly camera controller). Neither is a peer of subsystem crates.

Review checks:
- No reverse dependencies (e.g., `core` depending on `engine`, or a subsystem depending on `game`)
- No lateral dependencies between sibling subsystems (e.g., `render` must not depend on `physics`)
- `game` may depend on subsystem crates — it is game-specific composition, not a reusable subsystem
- `shared` may depend on subsystem crates — it holds reusable cross-binary systems, not game-specific logic
- Binaries (`app`, editors, tools) may depend on any crate — they are composition roots
- `core` has zero workspace dependencies — always
- New external crate dependencies require justification

## Subsystem Responsibilities

Each crate owns a specific domain. Flag code that crosses boundaries.

### Orchestrator crates must not absorb subsystem logic

A crate whose job is coordination (e.g., event loop, scheduling, lifecycle) must stay a pure orchestrator. It delegates work to subsystem crates — it never implements their logic inline.

- No cross-domain state — a struct should only own state that belongs to its domain. If a field conceptually belongs to another subsystem, it should be an ECS resource managed by that subsystem's crate, not a field on the orchestrator.
- Delegate, don't inline — when an orchestrator receives events or data meant for a subsystem, it forwards them. It does not interpret, transform, or act on them directly.
- No hardcoded game-specific values in engine crates — literal key bindings, movement mappings, magic numbers, or policy decisions (e.g., cursor behavior) are game-level configuration. They belong in the game crate or in configurable resources, never baked into engine or subsystem crates.
- Watch imports — if a crate starts importing types that belong to another subsystem's domain, the logic using those types almost certainly belongs in that other crate instead.

## Abstraction and Trait Rules

- Define shared traits in `core` so multiple crates can implement them independently
- Trait objects (`dyn Trait`) for runtime polymorphism; generics (`impl Trait`) for static dispatch and zero-cost abstraction
- Keep traits small and focused — prefer multiple small traits over one large one
- Avoid trait inheritance chains deeper than 2 levels
- Default implementations are fine for convenience but must not hide important behavior
- Orphan rule: implement traits only where you own the trait or the type

## Data Flow and Communication

- Subsystems receive data through function arguments or shared resources, not by reaching into other subsystems
- Event/message passing for loose coupling between subsystems (e.g., input events -> game logic)
- Avoid global mutable state — pass context explicitly
- Frame data flows: input -> game logic -> physics -> render. Respect this ordering.
- Cross-crate data types belong in `core` — a type that looks domain-specific is correctly placed in `core` if it serves as a data contract between sibling subsystems that cannot depend on each other. Always check whether a core type is consumed by multiple crates before flagging it as misplaced.
- Core resource types must only contain fields consumed by more than one crate. If a field is only read by a single subsystem (e.g., a near-clip plane only used by render), it belongs in that subsystem, not in core.

## Ownership and Shared State

- Prefer owned data and borrowing over `Rc`/`Arc` where possible
- `Arc<Mutex<T>>` only when shared mutable state across threads is truly needed — justify each use
- Use the borrow checker as a design tool — if lifetimes get complex, the data model may need rethinking
- Resources (GPU handles, audio devices) should have clear single owners with well-defined lifetimes
- Avoid `'static` lifetimes unless the data genuinely lives for the program's duration
- Clone intentionally, not to silence borrow checker errors

## Module and Crate Organization

- One concept per file — a file named `camera.rs` should define camera-related types and logic
- `lib.rs` re-exports the public API; keep it thin
- Use `mod.rs` or inline modules to group related items
- Internal helpers go in private modules — only expose what other crates need
- Avoid circular module dependencies within a crate
- Feature flags only for genuinely optional capabilities, not to paper over design issues
- No public exports that are unused
- Workspace-level external dependencies (`bevy_ecs`, `glam`, etc.) are imported directly by each crate — `core` does not re-export third-party types, only its own types and traits

### ECS Convention: `resources/`, `plugins/` and `systems/` directories

Rules:
- Every ECS resource type (`#[derive(Resource)]` or non-send resource) goes in `resources/<name>.rs`
- Every public system function goes in `systems/<name>.rs`
- Every plugin implementation goes in `plugins/<name>.rs`
- Each `mod.rs` only declares submodules and re-exports — no logic
- Shared resource types used across multiple crates belong in `capy_core/src/resources/`
- Schedule labels belong in `capy_core/src/schedule/`, one per file
- Crate-internal types that aren't resources or systems live in their own files at `src/` level (e.g., `app.rs`, `builder.rs`)

## Public API Design

- Minimize public surface — `pub(crate)` by default, `pub` only for cross-crate API
- Builder pattern for types with many optional fields
- Return types that are useful without unwrapping — prefer `Result` over panicking
- Constructors validate invariants — invalid states should be unrepresentable
- Consistent method naming: `new`, `with_*`, `get_*` (only if ambiguous), `set_*`, `into_*`, `as_*`

## Performance Guidelines

- Avoid allocations in hot loops — reuse buffers, prefer stack allocation
- Prefer iterators over indexed loops for clarity and optimization
- Profile before optimizing — don't sacrifice readability for speculative performance
- Batch GPU operations; minimize state changes
- Use `#[inline]` only for small, frequently-called, cross-crate functions — let the compiler decide otherwise
- Pool frequently allocated objects (entities, particles, network messages)
- Prefer `&[T]` over `&Vec<T>` in function signatures

## Naming Conventions

- Types: `PascalCase` — `RenderPipeline`, `PhysicsWorld`
- Functions/methods: `snake_case` — `create_pipeline`, `step_simulation`
- Constants: `SCREAMING_SNAKE_CASE` — `MAX_ENTITIES`, `DEFAULT_GRAVITY`
- Modules/files: `snake_case` — `render_pipeline.rs`
- Crates: `capy_*` prefix — `capy_render`, `capy_core`
- Traits: describe capability, usually adjective or noun — `Renderable`, `Collidable`, `System`
- Generic parameters: single letter for simple (`T`, `E`), descriptive for complex (`Vertex`, `Backend`)
- Avoid abbreviations except widely known ones (`id`, `ctx`, `tx`/`rx`, `cfg`)
- Boolean methods: `is_*`, `has_*`, `can_*`, `should_*`

## Code Review Checklist

Use this as a final pass. Each item maps to a section above.

Architecture:
- [ ] Dependencies flow downward only — no reverse dependencies
- [ ] No lateral dependencies between sibling subsystems
- [ ] Binaries are composition roots — may depend on any crate
- [ ] `game` depends only on `core`, `shared`, and subsystem crates — not on `engine` or `app`
- [ ] `shared` depends only on `core` and subsystem crates — not on `game`, `engine`, or `app`
- [ ] No new external crates without justification
- [ ] Code lives in the correct crate per its AGENTS.md
- [ ] No game-specific logic in engine crates
- [ ] New resource types are in `resources/<name>.rs`, re-exported from `resources/mod.rs`
- [ ] New system functions are in `systems/<name>.rs`, re-exported from `systems/mod.rs`
- [ ] New plugin types are in `plugins/<name>.rs`, re-exported from `plugins/mod.rs`
- [ ] Cross-crate resources and schedule labels are in `capy_core`, not in subsystem crates
- [ ] Core resource fields are consumed by more than one crate — single-consumer fields belong in the consuming crate
- [ ] Structs only own state belonging to their domain — no cross-domain fields
- [ ] Orchestrator crates delegate to subsystems, not inline their logic
- [ ] No hardcoded game-specific values (key bindings, magic numbers, policy decisions) in engine crates
- [ ] No imports from another subsystem's domain — if present, the logic belongs in that subsystem
- [ ] External types (`bevy_ecs`, `glam`) imported directly, not re-exported through `core`

Traits and abstractions:
- [ ] Shared traits defined in `core`
- [ ] Traits are small and focused
- [ ] No deep inheritance chains

Data and ownership:
- [ ] No unnecessary `Arc`/`Mutex`/`Clone`
- [ ] Clear resource ownership and lifetimes
- [ ] Data flows through arguments, not globals

API:
- [ ] Minimal public surface — `pub(crate)` preferred
- [ ] Constructors enforce invariants
- [ ] No panicking in library code

Error handling:
- [ ] Library crates use `thiserror` with a crate-level error enum in `src/error.rs`
- [ ] Binary crates use `anyhow` with `.context()` at the boundary
- [ ] No `.unwrap()` or `.expect()` — use `?` to propagate
- [ ] Errors wrap lower-level errors via `#[from]` — no information loss
- [ ] No error types in crates that have no fallible APIs yet

Performance:
- [ ] No allocations in hot paths without reason
- [ ] Buffers reused where appropriate
- [ ] `&[T]` over `&Vec<T>` in signatures

Style:
- [ ] Naming follows conventions
- [ ] No stale comments
