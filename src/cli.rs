use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "corgi",
    version,
    about = "CODEOWNERS reconciler and aggregator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, Debug, Subcommand)]
pub enum Command {
    /// Reconcile package CODEOWNERS manifests with the repository state.
    Sync,
    /// Rebuild the generated aggregate section in .github/CODEOWNERS.
    Aggregate,
    /// Migrate conventional CODEOWNERS manifests into exhaustive CORGI manifests.
    Migrate,
}
