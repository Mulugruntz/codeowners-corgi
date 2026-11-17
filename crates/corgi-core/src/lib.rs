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
pub use repo::Platform;

/// Reconcile package CODEOWNERS manifests with the repository state.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn sync(start: &Path, dry_run: bool) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    sync::run(&repo)
}

/// Rebuild the generated aggregate section in `.github/CODEOWNERS`.
///
/// When `output_path` is `Some`, write the aggregate to the specified path
/// instead of the default `.github/CODEOWNERS`.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn aggregate(start: &Path, dry_run: bool, output_path: Option<&str>) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    aggregate::run(&repo, output_path)
}

/// Migrate conventional CODEOWNERS patterns into exhaustive CORGI manifests.
///
/// When `dry_run` is `true`, no files are written. The returned status code
/// matches the code that would be returned by an actual run.
pub fn migrate(start: &Path, dry_run: bool) -> Result<i32> {
    let repo = repo::RepoContext::discover(start, dry_run)?;
    migrate::run(&repo)
}
