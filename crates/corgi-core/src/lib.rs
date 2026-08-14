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

/// Reconcile package CODEOWNERS manifests with the repository state.
pub fn sync(start: &Path) -> Result<i32> {
    let repo = repo::RepoContext::discover(start)?;
    sync::run(&repo)
}

/// Rebuild the generated aggregate section in `.github/CODEOWNERS`.
pub fn aggregate(start: &Path) -> Result<i32> {
    let repo = repo::RepoContext::discover(start)?;
    aggregate::run(&repo)
}

/// Migrate conventional CODEOWNERS patterns into exhaustive CORGI manifests.
pub fn migrate(start: &Path) -> Result<i32> {
    let repo = repo::RepoContext::discover(start)?;
    migrate::run(&repo)
}
