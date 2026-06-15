//! The `cosmatic` binary: parse args, dispatch to the compiler or the orchestrator.

mod cli;
mod init;

use clap::Parser;
use miette::{IntoDiagnostic, miette};

use cos_matic::generate;
use cli::{Cli, Command, IncidentCommand, LibraryAction};
use orchestrator::automerge::Gate;
use orchestrator::forge::{self, GithubForge, RepoId};
use orchestrator::{automerge, deploy, dispatch, incident, pipeline};

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init {
            name,
            level,
            adapters,
            repo,
            yes,
        } => {
            let config = init::gather_inputs(name, level, adapters, repo, yes)?;
            let target_dir = std::env::current_dir().into_diagnostic()?;
            init::scaffold(&config, &target_dir)?;
            init::print_checklist(&config);
            Ok(())
        }
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
                for (name, priority, description) in cos_matic::library::catalog() {
                    println!("{name:<20} (priority {priority:>3})  {description}");
                }
                Ok(())
            }
            LibraryAction::Show { name } => {
                print!("{}", cos_matic::library::content(&name)?);
                Ok(())
            }
        },
        Command::Goals { manifest } => {
            let (_root, manifest, tree) = generate::load_tree(&manifest)?;
            let outcomes = cos_matic::goals::evaluate(&tree, &manifest.goals)?;
            print_goals(&outcomes);
            let failures: Vec<String> = outcomes
                .iter()
                .filter(|o| o.is_blocking_failure())
                .map(|o| format!("  {}: {}", o.check, o.detail))
                .collect();
            if failures.is_empty() {
                Ok(())
            } else {
                Err(cos_matic::Error::GoalsFailed { failures }.into())
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
        Command::Dispatch {
            issue,
            title,
            body,
            repo,
        } => {
            let repo_id = resolve_repo(repo.as_deref())?;
            // Kill-switch: set cosmatic_DISPATCH_DISABLED to refuse every dispatch.
            let env = dispatch::Envelope {
                enabled: std::env::var_os("cosmatic_DISPATCH_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_attempts: 1,
            };
            let req = dispatch::FixRequest {
                issue,
                title,
                body,
                repo: repo_id.clone(),
            };
            let repo_root = std::env::current_dir().into_diagnostic()?;
            // cosmatic_FIXER=stub uses the deterministic no-LLM fixer (no Anthropic key).
            let report = if std::env::var("cosmatic_FIXER").as_deref() == Ok("stub") {
                dispatch::dispatch(&dispatch::StubFixer { repo_root }, &env, &req)
            } else {
                dispatch::dispatch(&dispatch::ClaudeFixer { repo_root }, &env, &req)
            }
            .into_diagnostic()?;

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .into_diagnostic()?
                .as_secs();
            if let Some(dir) = incident::default_journal_dir() {
                dispatch::append_audit(&dir, issue, &repo_id, &report, ts).into_diagnostic()?;
            }

            match report {
                dispatch::DispatchReport::Refused { reason } => {
                    eprintln!("refused: {reason}");
                    std::process::exit(2);
                }
                dispatch::DispatchReport::Attempted { branch, summary } => {
                    println!(
                        "attempted: {summary}\nbranch `{branch}` is ready for review — \
                         gate it and merge it yourself (dispatch never merges)."
                    );
                    Ok(())
                }
            }
        }
        Command::Automerge { branch, repo } => {
            let repo_id = resolve_repo(repo.as_deref())?;
            // Kill-switch: set cosmatic_AUTOMERGE_DISABLED to refuse every merge.
            let env = automerge::MergeEnvelope {
                enabled: std::env::var_os("cosmatic_AUTOMERGE_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_merges: 1,
            };
            let req = automerge::MergeRequest {
                branch,
                repo: repo_id.clone(),
            };
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .into_diagnostic()?;
            // Build the forge inside the runtime — octocrab's HTTP client needs a
            // reactor at construction.
            let outcome = rt
                .block_on(async {
                    let forge = GithubForge::from_env().map_err(|e| automerge::MergeError(e.0))?;
                    automerge::auto_merge(
                        &automerge::ForgeGate::new(&forge),
                        &automerge::ForgeMerger::new(&forge),
                        &env,
                        &req,
                        0,
                    )
                    .await
                })
                .into_diagnostic()?;

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .into_diagnostic()?
                .as_secs();
            if let Some(dir) = incident::default_journal_dir() {
                automerge::append_audit(&dir, &req, &outcome, ts).into_diagnostic()?;
            }

            match outcome {
                automerge::MergeOutcome::Refused { reason } => {
                    eprintln!("refused: {reason}");
                    std::process::exit(2);
                }
                automerge::MergeOutcome::Merged { reference } => {
                    println!("merged: {reference}");
                    Ok(())
                }
            }
        }
        Command::Deploy { target, repo } => {
            let repo_id = resolve_repo(repo.as_deref())?;
            let cmd = |key: &str| -> miette::Result<String> {
                std::env::var(key)
                    .map_err(|_| miette!("set {key} (the deploy is configured by command)"))
            };
            let deployer = deploy::CommandDeployer {
                canary_cmd: cmd("cosmatic_DEPLOY_CANARY")?,
                promote_cmd: cmd("cosmatic_DEPLOY_PROMOTE")?,
                rollback_cmd: cmd("cosmatic_DEPLOY_ROLLBACK")?,
            };
            let smoke = deploy::CommandSmoke {
                smoke_cmd: cmd("cosmatic_DEPLOY_SMOKE")?,
            };
            let env = deploy::DeployEnvelope {
                enabled: std::env::var_os("cosmatic_DEPLOY_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_deploys: 1,
            };
            let req = deploy::DeployRequest {
                target,
                repo: repo_id.clone(),
            };
            let outcome = deploy::deploy(&deployer, &smoke, &env, &req, 0).into_diagnostic()?;

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .into_diagnostic()?
                .as_secs();
            if let Some(dir) = incident::default_journal_dir() {
                deploy::append_audit(&dir, &req, &outcome, ts).into_diagnostic()?;
            }

            match outcome {
                deploy::DeployOutcome::Refused { reason } => {
                    eprintln!("refused: {reason}");
                    std::process::exit(2);
                }
                deploy::DeployOutcome::RolledBack { reason } => {
                    eprintln!("rolled back: {reason}");
                    std::process::exit(1);
                }
                deploy::DeployOutcome::Promoted { reference } => {
                    println!("promoted: {reference}");
                    Ok(())
                }
            }
        }
        Command::Loop {
            issue,
            title,
            body,
            repo,
            dry_run,
            max_iterations,
        } => {
            let repo_id = resolve_repo(repo.as_deref())?;
            if dry_run {
                let branch = format!("aom/fix/issue-{issue}");
                println!(
                    "[dry-run] loop for issue #{issue} on {}/{} — no fix, merge, or deploy will run.",
                    repo_id.owner, repo_id.name
                );
                println!(
                    "  1. dispatch  -> would create `{branch}` and run a bounded fixer. [skipped]"
                );
                println!("  2. publish   -> would push `{branch}` and open a PR. [skipped]");
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .into_diagnostic()?;
                // Build the forge inside the runtime — octocrab's HTTP client
                // needs a reactor at construction.
                let verdict = rt
                    .block_on(async {
                        let forge =
                            GithubForge::from_env().map_err(|e| automerge::MergeError(e.0))?;
                        automerge::ForgeGate::new(&forge)
                            .verdict(&automerge::MergeRequest {
                                branch: branch.clone(),
                                repo: repo_id.clone(),
                            })
                            .await
                    })
                    .into_diagnostic()?;
                let green = matches!(verdict, automerge::Verdict::Green);
                println!(
                    "  3. automerge -> real gate verdict for `{branch}`: {verdict:?} -> would {}.",
                    if green { "merge" } else { "REFUSE, stop" }
                );
                if !green {
                    println!(
                        "     note: dry-run skips publish, so no PR exists yet — a real run \
                         publishes first, then the gate sees the PR's checks."
                    );
                    return Ok(());
                }
                println!(
                    "  4. deploy    -> would canary-deploy, smoke-test, and promote-or-rollback. [skipped]"
                );
                return Ok(());
            }
            let env = pipeline::LoopEnvelope {
                enabled: std::env::var_os("cosmatic_LOOP_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_iterations,
            };
            let req = pipeline::LoopRequest {
                issue,
                title,
                body,
                repo: repo_id.clone(),
            };
            let repo_root = std::env::current_dir().into_diagnostic()?;
            let deploy_canary = std::env::var("cosmatic_DEPLOY_CANARY").unwrap_or_default();
            let deploy_promote = std::env::var("cosmatic_DEPLOY_PROMOTE").unwrap_or_default();
            let deploy_rollback = std::env::var("cosmatic_DEPLOY_ROLLBACK").unwrap_or_default();
            let deploy_smoke = std::env::var("cosmatic_DEPLOY_SMOKE").unwrap_or_default();
            // The loop core is async (the forge is); block on it once here — the
            // single async boundary — exactly as the incident command does. The
            // forge is built inside the runtime: octocrab's HTTP client needs a
            // reactor at construction.
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .into_diagnostic()?;
            let outcome = rt
                .block_on(async {
                    let stages = pipeline::RealStages {
                        repo_root,
                        forge: GithubForge::from_env().map_err(|e| pipeline::LoopError(e.0))?,
                        deploy_canary,
                        deploy_promote,
                        deploy_rollback,
                        deploy_smoke,
                    };
                    pipeline::run_until_done(&stages, &env, &req).await
                })
                .into_diagnostic()?;

            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .into_diagnostic()?
                .as_secs();
            if let Some(dir) = incident::default_journal_dir() {
                pipeline::append_audit(&dir, &req, &outcome, ts).into_diagnostic()?;
            }

            match outcome {
                pipeline::LoopOutcome::Refused { reason } => {
                    eprintln!("refused: {reason}");
                    std::process::exit(2);
                }
                pipeline::LoopOutcome::Stopped { stage, reason } => {
                    eprintln!("stopped at {stage}: {reason}");
                    std::process::exit(1);
                }
                pipeline::LoopOutcome::Completed { branch } => {
                    println!("completed: {branch} — dispatched, merged, and deployed");
                    Ok(())
                }
            }
        }
    }
}

/// Print one line per goal outcome, marking hard-gate failures.
fn print_goals(outcomes: &[cos_matic::goals::GoalOutcome]) {
    use cos_matic::config::schema::GoalKind;
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
