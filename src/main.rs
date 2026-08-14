mod cli;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    let exit_code = match run(cli.command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error}");
            2
        }
    };

    std::process::exit(exit_code);
}

fn run(command: Command) -> corgi_core::Result<i32> {
    let current_dir = std::env::current_dir()?;

    match command {
        Command::Sync => corgi_core::sync(&current_dir),
        Command::Aggregate => corgi_core::aggregate(&current_dir),
        Command::Migrate => corgi_core::migrate(&current_dir),
    }
}
