# CORGI

[![GitHub Release](https://img.shields.io/github/v/release/Mulugruntz/codeowners-corgi)](https://github.com/Mulugruntz/codeowners-corgi/releases/latest)
[![Tests](https://github.com/Mulugruntz/codeowners-corgi/actions/workflows/rust-tests.yaml/badge.svg)](https://github.com/Mulugruntz/codeowners-corgi/actions/workflows/rust-tests.yaml)
[![Lint & format](https://github.com/Mulugruntz/codeowners-corgi/actions/workflows/rust-lint-and-format.yaml/badge.svg)](https://github.com/Mulugruntz/codeowners-corgi/actions/workflows/rust-lint-and-format.yaml)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org/)
![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)

![CORGI mascot, a corgi jumping to the right while carrying a CODEOWNERS file](docs/logo-corgi-400px.png)

**Keep `CODEOWNERS` exhaustive, deterministic, and synchronized with the files Git actually sees.**

CORGI (**Co**de**O**wners **R**econciler for **G**it **I**ntegrity) is an opinionated Rust CLI for maintaining exhaustive `CODEOWNERS` manifests.

Instead of relying on broad wildcard rules as the final source of truth, CORGI reconciles ownership against the repository itself: adding new files, removing deleted ones, preserving ownership across renames where appropriate, respecting nested package boundaries and Git ignore rules, and reporting files that still need an owner.

It can also aggregate package-level manifests into the repository-wide `CODEOWNERS` file each platform reads.

```mermaid
flowchart TD
    A["Git repository<br/>+ <code>CODEOWNERS</code>"] --> B["<code>corgi sync</code>"]

    B --> C["Add new files"]
    B --> D["Remove deleted files"]
    B --> E["Reconcile renames"]
    B --> F["Report unowned files"]

    C --> G["Exhaustive package<br/><code>CODEOWNERS</code>"]
    D --> G
    E --> G
    F --> G

    G --> H["<code>corgi aggregate</code>"]
    H --> I["<code>.github/CODEOWNERS</code> / <code>.gitlab/CODEOWNERS</code> / <code>.bitbucket/CODEOWNERS</code>"]
```

### The model

* Every directory containing a `CODEOWNERS` file is a package root.
* Every managed, non-ignored file gets one explicit ownership entry.
* Nested package roots take ownership away from ancestor package roots.
* `# Rule[auto-assign]: ...` rules assign owners to new files without overwriting existing explicit ownership.
* `corgi sync` reconciles package manifests with Git-aware repository state.
* `corgi aggregate` builds the repository-wide generated section in the platform's default `CODEOWNERS` location (`.github/`, `.gitlab/`, or `.bitbucket/`).
* `corgi migrate` converts conventional wildcard-based manifests into CORGI's exhaustive format.

## Commands

```bash
corgi sync
corgi aggregate
corgi migrate
```

Each command accepts `--dry-run` to preview what would change without writing any files. The exit code still reflects the expected outcome (0 = no changes, 1 = changes would be made, 2 = fatal error).

## Model

- Every directory containing a `CODEOWNERS` file is a package root.
- Every managed non-ignored file in a package gets one explicit entry.
- Nested package roots take ownership away from ancestor package roots.
- `# Rule[auto-assign]: ...` comments are auto-assignment rules for new files only.
- Platform `CODEOWNERS` files keep their local ownership outside the generated section and store the repository-wide aggregate inside the generated section.
- When a platform `CODEOWNERS` is the only manifest (no root `/CODEOWNERS`), its local section is promoted to cover the entire repository so every file remains owned by the file the platform actually reads.

## Workflow

1. Run `corgi sync` to reconcile package manifests with Git-aware file state.
2. Run `corgi aggregate` to rebuild the generated aggregate in the platform's default `CODEOWNERS` file.
3. Run `corgi migrate` once to convert conventional wildcard manifests into CORGI's exhaustive format.

`corgi sync` returns `1` when it updates a manifest or when at least one file remains unowned.
`corgi aggregate` returns `1` when it updates the platform's default `CODEOWNERS` file.

## Auto-assignment rules

Rules use CODEOWNERS-style patterns and only apply inside the package where they are declared.
The most specific matching rule wins, and rules never overwrite an existing explicit file assignment.

```text
# Rule[auto-assign]: /src/** @org/backend
/src/lib.rs @org/backend
```

## File visibility

CORGI uses the `ignore` crate's WalkBuilder to discover files. This means:

- Repository-local `.gitignore` and `.git/info/exclude` are honored.
- **Machine-global Git ignores are not loaded.** CORGI explicitly disables global gitignore
  (`core.excludesFile` from the global Git config) so that output is repository-deterministic
  and does not depend on developer-specific machine configuration.
- **Tracked-then-ignored files are not managed.** If a file was committed and later added to
  `.gitignore`, the WalkBuilder follows `.gitignore` rules and does not consult Git's index.
  The file will be excluded from CORGI's output. This is a known limitation of the
  filesystem-walker approach.

## Platform `CODEOWNERS` files

Each platform reads its `CODEOWNERS` from a different location:

- **GitHub**: `.github/CODEOWNERS`
- **GitLab**: `.gitlab/CODEOWNERS`
- **Bitbucket**: `.bitbucket/CODEOWNERS`

Use a local section plus generated section markers:

```text
# Local .github ownership/rules:
/.github/workflows/ci.yml @org/platform

# BEGIN CORGI GENERATED
/README.md @org/root
# END CORGI GENERATED
```

`corgi sync` only reconciles the local section.
`corgi aggregate` only rebuilds the generated section and ignores the previous generated contents as input.
When the aggregate would be empty, `corgi aggregate` omits the generated section entirely.

## Architecture

The repository is a two-crate Cargo workspace:

- `corgi-cli` is the root package and produces the `corgi` binary. It owns argument parsing, command dispatch, top-level error rendering, and process exit behavior.
- `corgi-core` lives in `crates/corgi-core` and owns Git repository discovery, CODEOWNERS parsing, reconciliation, migration, and aggregation. Its internal modules are private; the crate exposes only the operations used by the CLI plus its error type.

The CLI intentionally remains the root package instead of making the workspace root virtual. CORGI is distributed as a Rust pre-commit hook, and pre-commit installs Rust hook repositories with `cargo install --bins` from the repository root.

## pre-commit / prek

Either a `.pre-commit-config.yaml` file:

```yaml
- repo: https://github.com/Mulugruntz/codeowners-corgi
  rev: v0.1.0
  hooks:
    - id: corgi-sync
      language_version: system
    - id: corgi-aggregate
      language_version: system
```

Or a `prek.toml` file:

```toml
[[repos]]
repo = "https://github.com/Mulugruntz/codeowners-corgi"
rev = "v0.1.0"
hooks = [
    { id = "corgi-sync", language_version = "system" },
    { id = "corgi-aggregate", language_version = "system" },
]
```

Use only `corgi-sync` (and omit `corgi-aggregate`) when another aggregation tool manages the repository-wide `CODEOWNERS` output.

## CODEOWNERS compatibility

`CODEOWNERS` is not a single standardized format. GitHub, GitLab, and Bitbucket share a common core syntax, but differ in file discovery, pattern matching, owner references, and provider-specific extensions.

The table below shows whether **CORGI can safely manage each feature when used with that repository provider**.

**Legend:** ✅ supported by CORGI · ❌ supported by the provider, but not currently supported by CORGI · blank = the provider does not provide that feature

| Feature                                          | GitHub | GitLab | Bitbucket |
|--------------------------------------------------|:------:|:------:|:---------:|
| **Repository integration**                       |        |        |           |
| Native repository-wide `CODEOWNERS` output       |   ✅   |   ✅   |    ✅     |
| Provider `CODEOWNERS` discovery / fallback rules |   ✅   |  ❌¹   |    ❌²    |
| **Core syntax**                                  |        |        |           |
| Full-line comments — `# comment`                 |   ✅   |   ✅   |    ✅     |
| Inline comments — `*.rs @team # comment`         |  ❌³   |        |    ❌³    |
| Multiple owners per rule                         |   ✅   |   ✅   |    ✅     |
| Email addresses as owners                        |   ✅   |   ✅   |    ✅     |
| User / team / group owner references             |   ✅   |   ✅   |    ✅     |
| Root-anchored paths — `/src/lib.rs`              |   ✅   |   ✅   |    ✅     |
| Relative / unanchored paths — `README.md`        |   ✅   |   ✅   |    ✅     |
| Directory patterns — `/docs/`                    |   ✅   |   ✅   |    ✅     |
| Wildcards — `*.rs`                               |   ✅   |   ✅   |    ✅     |
| Single-character wildcard — `file?.rs`           |   ✅   |   ✅   |    ✅     |
| Globstar — `/src/**`                             |   ✅   |   ✅   |    ✅     |
| Last matching flat rule wins                     |   ✅   |  ✅⁴   |    ✅     |
| **Pattern extensions**                           |        |        |           |
| Character classes — `/file[0-9].rs`              |        |   ✅   |           |
| Exclusion patterns — `!generated/**`             |        |   ❌   |           |
| **GitLab extensions**                            |        |        |           |
| Sections — `[Documentation]`                     |        |   ❌   |           |
| Optional sections — `^[Documentation]`           |        |   ❌   |           |
| Required approval count — `[Documentation][2]`   |        |   ❌   |           |
| Section default owners — `[Documentation] @docs` |        |   ❌   |           |
| Role owners — `@@maintainer`                     |        |  ✅⁵   |           |
| **Bitbucket extensions**                         |        |        |           |
| Group reviewer strategy — `:all`                 |        |        |    ✅⁵    |
| Group reviewer strategy — `:random(2)`           |        |        |    ✅⁵    |
| Group reviewer strategy — `:least_busy(3)`       |        |        |    ✅⁵    |
| Repository-defined teams — `@teams/frontend`     |        |        |    ✅⁵    |
| **CORGI extensions**                             |        |        |           |
| Auto-assignment — `# Rule[auto-assign]: ...`     |   ✅   |   ✅   |    ✅     |

¹ GitLab searches `/CODEOWNERS`, `docs/CODEOWNERS`, then `.gitlab/CODEOWNERS`. Additionally, a root `/CODEOWNERS` used as a CORGI package manifest would take precedence over `.gitlab/CODEOWNERS`, so GitLab aggregation requires careful placement of package manifests.

² Bitbucket only reads `.bitbucket/CODEOWNERS`, which CORGI generates by default when it detects a Bitbucket remote.

³ GitHub and Bitbucket support inline comments, but CORGI does not preserve them when rewriting a manifest. Use full-line comments in CORGI-managed files.

⁴ GitLab uses last-match-wins within each section, but multiple sections are evaluated independently. Flat manifests work with CORGI's model; GitLab section semantics do not.

⁵ CORGI treats owner references as opaque tokens. Provider-specific forms such as GitLab roles and Bitbucket reviewer strategies or `@teams/...` references are preserved, but CORGI does not interpret or validate their provider-specific meaning.

### Provider support

CORGI's repository-wide aggregation workflow supports **GitHub**, **GitLab**, and **Bitbucket**.

CORGI inspects the repository's Git remotes and selects the appropriate output location (`.github/CODEOWNERS`, `.gitlab/CODEOWNERS`, or `.bitbucket/CODEOWNERS`). Use `--output` to override the default for repositories with mixed providers.

Most of the common `CODEOWNERS` syntax is also compatible with all three providers, and provider-specific owner references are generally preserved. For GitLab, full first-class support additionally requires support for its extended rule semantics.

## Development

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```
