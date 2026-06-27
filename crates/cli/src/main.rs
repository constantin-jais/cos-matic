//! The `aom` binary: parse args, dispatch to the compiler or the orchestrator.

mod cli;

use clap::Parser;
use miette::{IntoDiagnostic, miette};

use agent_o_matic::generate;
use cli::{Cli, Command, IncidentCommand, LibraryAction};
use orchestrator::forge::{self, GithubForge, RepoId};
use orchestrator::incident;

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
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }
            if check {
                println!("ok: {} file(s) up to date", report.files.len());
            }
            Ok(())
        }
        Command::Library { action } => match action {
            LibraryAction::List => {
                for (name, priority, description) in agent_o_matic::library::catalog() {
                    println!("{name:<20} (priority {priority:>3})  {description}");
                }
                Ok(())
            }
            LibraryAction::Show { name } => {
                print!("{}", agent_o_matic::library::content(&name)?);
                Ok(())
            }
        },
        Command::Goals { manifest } => {
            let (_root, manifest, tree) = generate::load_tree(&manifest)?;
            let outcomes = agent_o_matic::goals::evaluate(&tree, &manifest.goals)?;
            print_goals(&outcomes);
            let failures: Vec<String> = outcomes
                .iter()
                .filter(|o| o.is_blocking_failure())
                .map(|o| format!("  {}: {}", o.check, o.detail))
                .collect();
            if failures.is_empty() {
                Ok(())
            } else {
                Err(agent_o_matic::Error::GoalsFailed { failures }.into())
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

/// Print one line per goal outcome, marking hard-gate failures.
fn print_goals(outcomes: &[agent_o_matic::goals::GoalOutcome]) {
    use agent_o_matic::config::schema::GoalKind;
    for o in outcomes {
        let kind = match o.kind {
            GoalKind::HardGate => "hard_gate",
            GoalKind::Observability => "observability",
        };
        let status = if o.passed { "PASS" } else { "FAIL" };
        println!("goal [{kind}] {status}  {}: {}", o.check, o.detail);
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
