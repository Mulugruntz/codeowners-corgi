pub mod aggregate;
pub mod cli;
pub mod codeowners;
pub mod error;
pub mod git;
pub mod migrate;
pub mod repo;
pub mod sync;

use cli::Command;
use error::Result;

pub fn run(command: Command) -> Result<i32> {
    let current_dir = std::env::current_dir()?;
    let repo = repo::RepoContext::discover(&current_dir)?;

    match command {
        Command::Sync => sync::run(&repo),
        Command::Aggregate => aggregate::run(&repo),
        Command::Migrate => migrate::run(&repo),
    }
}
