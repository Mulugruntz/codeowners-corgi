# AGENTS.md

## Objective

Maintain an idiomatic, boring, maintainable Rust codebase.

Prefer correctness, clarity, type safety, and simple ownership over cleverness.
Do not optimize prematurely or introduce abstractions without a concrete use case.

## Before making changes

1. Read the relevant `Cargo.toml` files and existing neighboring modules.
2. Understand the existing architecture before introducing a new pattern.
3. Search the repository for an existing implementation before adding a new abstraction.
4. Keep changes scoped to the requested behavior.
5. Do not perform unrelated refactors unless required for correctness.

## Repository structure

### General rules

- Start with a single crate unless there is a meaningful reason for multiple crates.
- Create a new crate only for a genuine architectural boundary such as:
    - independently reusable library code;
    - substantially different dependency requirements;
    - a proc-macro;
    - an executable/application boundary;
    - a component that benefits materially from separate compilation or testing.
- Do not create crates merely to represent architectural "layers".
- Prefer normal Rust modules for internal organization.

For a workspace, prefer:

    Cargo.toml
    crates/
      project-core/
      project-cli/
      project-server/

Each crate owns its own `src/`, `tests/`, examples, and crate-specific documentation.

### Libraries

- Put reusable behavior in library crates.
- Keep `lib.rs` primarily concerned with module declarations, re-exports, and crate documentation.
- Keep the public API deliberately small.
- Prefer private visibility, then `pub(crate)`, and use `pub` only for intentional API surface.
- Avoid exposing internal implementation types unnecessarily.

### Binaries

- Keep `main.rs` and files under `src/bin/` thin.
- Application startup should primarily:
    - parse configuration / arguments;
    - initialize infrastructure;
    - construct dependencies;
    - invoke library/application logic;
    - translate top-level errors into process exit behavior.
- Business logic should be testable without running the binary.

## Rust design

### Types

- Make invalid states difficult or impossible to represent.
- Prefer domain types/newtypes over primitive strings, integers, or booleans when semantics matter.
- Prefer enums over loosely related booleans or stringly-typed states.
- Prefer explicit state transitions when modeling workflows.
- Avoid boolean parameters when their meaning is unclear at the call site.

### Ownership

- Prefer borrowing over cloning.
- Do not add `.clone()` merely to satisfy the borrow checker without understanding the ownership issue.
- Prefer owned values where ownership genuinely simplifies an API.
- Do not introduce `Arc`, `Mutex`, `RwLock`, or atomics unless shared/concurrent ownership is actually required.

### Error handling

- Never silently discard errors.
- Avoid `unwrap()` and `expect()` in production paths unless the condition is a genuine invariant.
- If using `expect()`, explain the invariant in the message.
- Libraries should expose meaningful typed errors where callers need to react to error categories.
- Application/binary boundaries may use contextual/general-purpose errors when callers do not need structured recovery.
- Preserve the underlying error source where practical.

### API design

- Follow conventional Rust naming and API conventions.
- Prefer constructors and methods that make ownership semantics obvious.
- Use `as_`, `to_`, and `into_` consistently with Rust conventions.
- Prefer iterators and standard traits where they simplify interoperability.
- Do not expose implementation details solely for testing.
- Public APIs should have rustdoc when their purpose or invariants are not obvious.
- Public examples should compile as doctests where practical.

### Unsafe Rust

- Do not introduce `unsafe` unless there is a demonstrated need.
- Before adding `unsafe`, consider a safe implementation or established safe abstraction.
- Every unsafe block must document the safety invariant being relied upon.
- Unsafe code requires focused tests around its invariants.

## Dependencies

- Prefer the standard library when it provides a clear solution.
- Do not add a production dependency without considering:
    - maintenance/status;
    - transitive dependency cost;
    - feature requirements;
    - compile-time impact;
    - whether existing dependencies already solve the problem.
- In workspaces, define shared dependency versions in `[workspace.dependencies]`.
- Disable unnecessary default features where appropriate.
- Avoid wildcard dependency versions.
- Do not change dependency versions as an unrelated side effect.

## Cargo workspace conventions

Use workspace inheritance for shared metadata, dependencies, and lints where practical.

For a virtual workspace targeting Rust 2024, prefer:

    [workspace]
    resolver = "3"
    members = ["crates/*"]

Prefer shared values such as:

    [workspace.package]
    edition = "2024"

    [workspace.dependencies]
    # shared dependency versions

    [workspace.lints.rust]
    unsafe_code = "forbid"

Member crates should inherit shared settings rather than duplicate them.

Keep the project's declared MSRV explicit when the project has an MSRV policy.

## Testing

- Add or update tests for behavioral changes.
- Prefer focused unit tests close to implementation details.
- Use integration tests for externally observable crate behavior.
- Test error cases and boundary conditions, not only the happy path.
- Avoid tests that depend unnecessarily on timing, global state, network access, or execution order.
- Prefer deterministic test fixtures.
- A bug fix should normally include a regression test.

## Comments and documentation

Comments should explain:
- why something is necessary;
- invariants;
- non-obvious constraints;
- safety requirements;
- surprising tradeoffs.

Do not write comments that merely restate the code.

Keep README and public documentation synchronized with user-visible behavior.

## Code quality

Prefer:
- explicit code over clever code;
- small cohesive functions;
- composition over unnecessary traits/frameworks;
- standard-library idioms;
- exhaustive matching where useful;
- compiler-enforced invariants.

Avoid:
- speculative abstractions;
- premature genericity;
- "utils" modules that become unrelated dumping grounds;
- deeply nested modules without a clear domain reason;
- magic strings/numbers with domain meaning;
- duplicate representations of the same state;
- unnecessary allocation;
- unnecessary async code.

Do not suppress compiler or Clippy warnings merely to make checks pass.
Fix the underlying issue unless there is a documented reason for an allow.

## Verification

Before considering Rust changes complete, run the relevant subset of:

    cargo fmt --all -- --check
    cargo check --workspace --all-targets
    cargo test --workspace
    cargo clippy --workspace --all-targets -- -D warnings

When the project supports all features simultaneously:

    cargo test --workspace --all-features
    cargo clippy --workspace --all-targets --all-features -- -D warnings

If features are intentionally mutually exclusive, use the documented supported feature matrix instead of `--all-features`.

For public library API changes, also consider:

    cargo doc --workspace --no-deps

Do not claim checks passed unless they were actually executed successfully.

## Making changes

When implementing a feature:

1. Identify the smallest architectural boundary that owns the behavior.
2. Add or modify domain types first if the behavior introduces new states/invariants.
3. Implement the behavior.
4. Add tests.
5. Run formatting, checks, tests, and Clippy.
6. Review the diff for accidental complexity or unrelated changes.

Do not generate large amounts of scaffolding preemptively.

## Refactoring

Refactor only when it:
- removes demonstrated duplication;
- clarifies ownership or responsibilities;
- improves testability;
- enforces an invariant;
- simplifies an existing abstraction.

Do not introduce a trait for a single implementation unless there is another concrete reason
such as test substitution, dynamic dispatch, or a known extension point.

## Definition of done

A change is complete when:

- requested behavior is implemented;
- architecture remains coherent;
- public API changes are intentional;
- relevant tests exist and pass;
- formatting passes;
- Clippy passes according to project policy;
- documentation is updated where necessary;
- no unrelated changes remain in the diff.