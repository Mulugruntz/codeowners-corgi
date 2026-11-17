//! Core CODEOWNERS reconciliation engine used by the `corgi` CLI.
//!
//! The public API is intentionally small: callers provide a path inside a Git
//! repository and select one of CORGI's three operations. Repository discovery,
//! parsing, reconciliation, and file updates remain private implementation
//! details so the crate can evolve without exposing its internal model.

mod aggregate;
mod codeowners;
mod error;
mod git;
mod migrate;
mod repo;
mod sync;

use std::path::Path;

pub use error::{CorgiError, Result};
pub use repo::{Platform, RepoContext};

/// Reconcile package CODEOWNERS manifests with the repository state.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn sync(start: &Path, dry_run: bool) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    sync::run(&repo)
}

/// Rebuild the generated aggregate section in the platform's default
/// `CODEOWNERS` file (`.github/`, `.gitlab/`, or `.bitbucket/`).
///
/// When `output_path` is `Some`, write the aggregate to the specified path
/// instead of the autodetected platform default.
///
/// When `platform` is `Some`, use it to decide merge semantics (GitHub merges
/// with prefix/suffix; other platforms overwrite). When `None`, the platform
/// is auto-detected from git remote URLs. If no git remotes are configured,
/// defaults to GitHub.
///
/// For GitHub, the generated body is merged with any existing local section
/// and suffix. For GitLab and Bitbucket the generated body is written
/// directly, replacing existing file content.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn aggregate(
    start: &Path,
    dry_run: bool,
    output_path: Option<&str>,
    platform: Option<Platform>,
) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    aggregate::run(&repo, output_path, platform)
}

/// Migrate conventional CODEOWNERS patterns into exhaustive CORGI manifests.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn migrate(start: &Path, dry_run: bool) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    migrate::run(&repo)
}
