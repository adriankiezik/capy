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
4. Report findings grouped by severity: blocking, warning, nit.
5. For each finding, cite the file, line, and which rule it violates.

## Architectural Principles

Capy is a Rust game engine workspace. Crates are layered by abstraction level with strict boundaries.

Guiding rules:
- Each crate has one job — rendering, physics, audio, etc.
- Engine crates are reusable across games; game-specific logic lives only in `capy_game`
- Prefer composition over inheritance — combine small traits, don't build deep hierarchies
- Minimize coupling — crates communicate through traits and types defined in `capy_core`
- Design for `Send + Sync` by default to support future multithreading

## Dependency Direction

Dependencies flow inward. Higher-level crates depend on lower-level ones, never the reverse.

```
app -> engine -> [render, audio, physics, input, assets, net, ui, game] -> core
```

Review checks:
- No reverse dependencies (e.g., `core` depending on `engine`)
- No lateral dependencies between sibling subsystems unless architecturally justified (e.g., `render` must not depend on `physics`)
- `core` has zero workspace dependencies — always
- New external crate dependencies require justification

## Subsystem Responsibilities

Each crate owns a specific domain. Flag code that crosses boundaries.

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
- Cross-crate data types belong in `core`

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

## Public API Design

- Minimize public surface — `pub(crate)` by default, `pub` only for cross-crate API
- Builder pattern for types with many optional fields
- Return types that are useful without unwrapping — prefer `Result` over panicking
- Constructors validate invariants — invalid states should be unrepresentable
- Consistent method naming: `new`, `with_*`, `get_*` (only if ambiguous), `set_*`, `into_*`, `as_*`
- Breaking API changes require discussion

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
- [ ] Dependencies flow inward only
- [ ] No new external crates without justification
- [ ] Code lives in the correct crate per its AGENTS.md
- [ ] No game-specific logic in engine crates

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

// Error handling - to be added

Performance:
- [ ] No allocations in hot paths without reason
- [ ] Buffers reused where appropriate
- [ ] `&[T]` over `&Vec<T>` in signatures

Style:
- [ ] Naming follows conventions
- [ ] No stale comments
