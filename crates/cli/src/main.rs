//! The `aom` binary: parse args, dispatch to the compiler or the orchestrator.

mod cli;

use std::path::PathBuf;

use clap::Parser;
use miette::{IntoDiagnostic, miette};

use agent_o_matic::generate;
use cli::{Cli, Command, GateCommand, GoalsCommand, IncidentCommand};
use orchestrator::forge::{self, GithubForge, RepoId};
use orchestrator::{gate, goals, incident};

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

        Command::Incident {
            command:
                IncidentCommand::Open {
                    kind,
                    title,
                    body,
                    severity,
                    key,
                    repo,
                    labels,
                },
        } => {
            let repo_id = resolve_repo(repo.as_deref())?;
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .into_diagnostic()?
                .as_secs();
            let key = key.unwrap_or_else(|| title.clone());
            let inc = incident::Incident::new(kind, severity, title, body, &key, ts);

            if let Some(dir) = incident::default_journal_dir() {
                incident::append_journal(&inc, &dir).into_diagnostic()?;
            }

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .into_diagnostic()?;
            let (issue, created) = rt
                .block_on(async {
                    let forge = GithubForge::from_env()?;
                    forge::open_or_reuse(&forge, &repo_id, &inc, &labels).await
                })
                .into_diagnostic()?;

            println!(
                "{} #{} {}",
                if created { "created" } else { "reused" },
                issue.number,
                issue.url
            );
            Ok(())
        }
    }
}

/// Resolve the target repo: `--repo owner/name`, else the `origin` remote.
fn resolve_repo(repo: Option<&str>) -> miette::Result<RepoId> {
    if let Some(r) = repo {
        let (owner, name) = r
            .split_once('/')
            .ok_or_else(|| miette!("--repo must be `owner/name`, got `{r}`"))?;
        return Ok(RepoId {
            owner: owner.to_string(),
            name: name.to_string(),
        });
    }
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .into_diagnostic()?;
    if !out.status.success() {
        return Err(miette!(
            "could not read the `origin` remote; pass --repo owner/name"
        ));
    }
    let url = String::from_utf8_lossy(&out.stdout);
    RepoId::parse_remote(url.trim()).ok_or_else(|| {
        miette!(
            "could not parse a GitHub repo from `origin` ({})",
            url.trim()
        )
    })
}
