//! The `aom` binary: parse args, dispatch to the compiler or the orchestrator.

mod cli;

use std::path::PathBuf;

use clap::Parser;
use miette::IntoDiagnostic;

use agent_o_matic::generate;
use cli::{Cli, Command, GateCommand, GoalsCommand};
use orchestrator::{gate, goals};

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

        Command::Goals {
            command: GoalsCommand::Report { config },
        } => {
            let src = std::fs::read_to_string(&config).into_diagnostic()?;
            let g = goals::parse(&src).into_diagnostic()?;
            print!("{}", goals::render_markdown(&g, &goals::Metrics::new()));
            Ok(())
        }

        Command::Gate {
            command: GateCommand::Run { config },
        } => {
            let src = std::fs::read_to_string(&config).into_diagnostic()?;
            let g = goals::parse(&src).into_diagnostic()?;
            let runner = gate::CargoRunner {
                repo_root: PathBuf::from("."),
            };
            let report = gate::run(&g, &runner);
            print!("{}", report.markdown);
            if report.all_green {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
    }
}
