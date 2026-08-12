use clap::Parser;

use corgi::{cli::Cli, run};

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
