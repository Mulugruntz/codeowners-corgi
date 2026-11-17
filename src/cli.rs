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

#[derive(Clone, Debug, Subcommand)]
pub enum Command {
    /// Reconcile package CODEOWNERS manifests with the repository state.
    Sync {
        /// Perform a dry run without writing any files.
        #[arg(long)]
        dry_run: bool,
    },
    /// Rebuild the generated aggregate section in the platform's default CODEOWNERS file.
    ///
    /// Defaults to `.github/CODEOWNERS` for GitHub, `.gitlab/CODEOWNERS` for
    /// GitLab, and `.bitbucket/CODEOWNERS` for Bitbucket, based on the remote
    /// URL. Use `--output` to override.
    Aggregate {
        /// Perform a dry run without writing any files.
        #[arg(long)]
        dry_run: bool,
        /// Write the aggregate to a specific path instead of the default.
        #[arg(long)]
        output: Option<String>,
    },
    /// Migrate conventional CODEOWNERS manifests into exhaustive CORGI manifests.
    Migrate {
        /// Perform a dry run without writing any files.
        #[arg(long)]
        dry_run: bool,
    },
}
