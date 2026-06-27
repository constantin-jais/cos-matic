//! The `aom` binary: parse args, dispatch to the compiler, print a report.

mod cli;

use clap::Parser;
use cli::{Cli, Command};

use agent_o_matic::generate;

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Generate {
            manifest,
            check,
            force,
        } => {
            let report = generate::run(&generate::Options {
                manifest_path: manifest,
                check,
                force,
            })?;
            for file in &report.files {
                println!("{:>9}  {}", file.action.label(), file.path);
            }
            if check {
                println!("ok: {} target(s) up to date", report.files.len());
            }
            Ok(())
        }
    }
}
