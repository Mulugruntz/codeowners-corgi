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

CORGI intentionally has exactly two crates:

    Cargo.toml                 # workspace + corgi-cli package
    src/                       # corgi-cli sources
      cli.rs
      main.rs
    tests/
      cli.rs                   # CLI behavior tests (small)
    crates/
      corgi-core/
        Cargo.toml
        src/                   # reconciliation engine (with inline unit tests)
        tests/
          behavior.rs          # core integration tests
          support/
            mod.rs             # TestRepo helper

- `corgi-cli` is the root package and produces the `corgi` binary. Keep it thin: argument parsing, command dispatch, top-level error rendering, and process exit behavior belong here.
- `corgi-core` owns repository discovery, CODEOWNERS parsing/modeling, Git-aware reconciliation, migration, and aggregation.
- Keep `corgi-core` independent of `clap` and other CLI concerns.
- Do not add another crate unless there is a concrete boundary that cannot be expressed cleanly as a module.
- Keep the CLI package at the workspace root. CORGI is published as a Rust pre-commit hook, and pre-commit installs the repository with `cargo install --bins`; a virtual workspace root would break that installation model.
- Shared dependency versions and package metadata belong in the root workspace tables.
- The `corgi` executable name is intentionally different from the package name `corgi-cli`.

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

This workspace targets Rust 2024 and uses resolver 3. Keep the root `corgi-cli` package as a workspace member rather than converting the root into a virtual manifest.

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

CORGI has three test layers. Use the lowest layer that can prove the behavior.

### Test placement

1. **Unit tests in `crates/corgi-core/src/*.rs`**

    * Use `#[cfg(test)] mod tests` in the module that owns the behavior.
    * Use these for parsing, matching, sorting, path conversion, generated-section handling, owner selection, Git-status parsing, and other deterministic logic.
    * Unit tests may access private functions. Do not make an internal function `pub` only to test it.
    * Prefer table-driven tests when validating multiple syntax or path cases.

2. **Core integration tests in `crates/corgi-core/tests/`**

    * Use these when behavior requires a real temporary repository, filesystem state, `.gitignore`, Git rename detection, or interaction among multiple core modules.
    * Call `corgi_core::sync`, `corgi_core::aggregate`, or `corgi_core::migrate` directly.
    * Do not spawn the `corgi` binary when the behavior belongs to `corgi-core`.
    * Use `tests/support/mod.rs` for the shared `TestRepo` helper.

3. **CLI tests in the root `tests/cli.rs`**

    * Reserve these for argument parsing, `--help`, `--version`, command dispatch, stderr formatting, and process exit codes.
    * A CLI test may exercise a small end-to-end smoke scenario, but reconciliation behavior should primarily be tested in `corgi-core`.

### Core behavioral invariants

Tests that modify corresponding behavior must preserve or intentionally update these invariants:

* Every managed non-ignored file belongs to exactly one package: the deepest package root containing it.
* Existing explicit ownership is preserved when re-syncing an unchanged file.
* Auto-assignment rules apply only to files without existing explicit ownership.
* The most specific matching auto-assignment rule wins.
* Equal-specificity behavior must remain deterministic (first match wins).
* Migration follows CODEOWNERS last-match-wins precedence for pattern resolution.
* Nested package roots take ownership away from ancestor package roots.
* Moving or renaming files across package boundaries requires an explicit regression test. Do not assume ownership can safely be copied across the boundary.
* `sync` must not modify the generated aggregate section of `.github/CODEOWNERS`.
* `aggregate` must not treat the previous generated section as source ownership data.
* Local `.github/CODEOWNERS` content before and after the generated section must be preserved.
* Output ordering must be deterministic.
* A successful fully reconciled operation is idempotent: running it again must not modify files.
* Process/core status code semantics: 0 = no change, 1 = changed/unresolved, 2 = fatal error (CLI only).

### CODEOWNERS syntax

CORGI follows GitHub CODEOWNERS syntax. Keep parser, matcher, migration, and rendering behavior consistent with that specification, and add focused tests for syntax changes.

### Test repository fixtures

Use temporary directories; never modify the checkout containing the tests.

Keep repeated Git/filesystem setup behind the shared `TestRepo` helper in `crates/corgi-core/tests/support/mod.rs`.

Configure repository-local Git identity where tests create commits. Do not rely on global Git username, email, branch-name defaults, or unrelated developer configuration.

Use real Git only for behavior that actually depends on Git. Pure parsing of Git command output should be unit-tested with byte fixtures.

### Test shape

Prefer one behavioral contract per test. Avoid combining unrelated behaviors into tests such as `handles_additions_deletions_errors_and_idempotency`. Prefer focused names such as:

    sync_adds_new_files
    sync_removes_deleted_entries
    sync_preserves_owner_on_rename_within_a_package
    migrate_uses_the_last_matching_pattern
    aggregate_preserves_content_after_the_generated_section
    parser_rejects_unsupported_codeowners_negation

For every bug fix, add a regression test that fails before the fix whenever practical.

### Idempotency

Distinguish:

* **Stable but unresolved**: A repository with unowned files consistently returns status 1 but does not modify files on the second run.
* **Fully reconciled and idempotent**: All files are owned, status 0, files are byte-identical after re-running.

### Failure behavior

For operations that rewrite manifests, test realistic fatal failures for atomicity. Prefer implementations that read, validate, compute desired state, then write, rather than writing partial results before later validation can fail.

### Property tests

Use `proptest` (dev-dependency of `corgi-core`) selectively for invariants like escape/split roundtrips, render/parse semantic roundtrips, sort idempotency, and renderer determinism. Keep explicit example-based regression tests for important real-world cases.

### Verification

During development, run the narrowest relevant test first:

    cargo test -p corgi-core <test-name>
    cargo test -p corgi-core
    cargo test -p corgi-cli --test cli

Before considering a Rust change complete, run:

    cargo fmt --all -- --check
    cargo check --workspace --all-targets
    cargo test --workspace --all-features
    cargo clippy --workspace --all-targets --all-features -- -D warnings

Do not claim a command passed unless it was actually executed successfully.

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