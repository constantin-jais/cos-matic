//! The `bolt-cosmatic` binary: parse args, dispatch to the compiler or the orchestrator.

mod cli;
mod init;

use clap::Parser;
use miette::{IntoDiagnostic, miette};

use bolt_cos_matic::generate;
use cli::{
    Cli, Command, HandoffAction, IncidentCommand, InspectAction, LibraryAction, MaturityAction,
    StackAction,
};
use orchestrator::automerge::Gate;
use orchestrator::branch_policy::AttemptBranch;
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
                for (name, priority, description) in bolt_cos_matic::library::catalog() {
                    println!("{name:<20} (priority {priority:>3})  {description}");
                }
                Ok(())
            }
            LibraryAction::Show { name } => {
                print!("{}", bolt_cos_matic::library::content(&name)?);
                Ok(())
            }
        },
        Command::Goals { manifest } => {
            let (_root, manifest, tree) = generate::load_tree(&manifest)?;
            let outcomes = bolt_cos_matic::goals::evaluate(&tree, &manifest.goals)?;
            print_goals(&outcomes);
            let failures: Vec<String> = outcomes
                .iter()
                .filter(|o| o.is_blocking_failure())
                .map(|o| format!("  {}: {}", o.check, o.detail))
                .collect();
            if failures.is_empty() {
                Ok(())
            } else {
                Err(bolt_cos_matic::Error::GoalsFailed { failures }.into())
            }
        }
        Command::Maturity { action } => match action {
            MaturityAction::Validate { claim, json } => {
                let report = bolt_cos_matic::maturity::validate_file(&claim)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_maturity_report(&report);
                }
                if report.is_valid() {
                    Ok(())
                } else {
                    Err(miette!(
                        "maturity validation failed: {} error(s)",
                        report
                            .findings
                            .iter()
                            .filter(|finding| finding.severity
                                == bolt_cos_matic::maturity::FindingSeverity::Error)
                            .count()
                    ))
                }
            }
            MaturityAction::Report { dir, json } => {
                let report = bolt_cos_matic::maturity::report_dir(&dir)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_maturity_workspace_report(&report);
                }
                if report.is_valid() {
                    Ok(())
                } else {
                    Err(miette!(
                        "maturity report contains blocking validation errors"
                    ))
                }
            }
        },
        Command::Handoff { action } => match action {
            HandoffAction::Validate { payload, json } => {
                let report = bolt_cos_matic::handoff::validate_file(&payload)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_handoff_report(&report);
                }
                if report.is_valid() {
                    if !json {
                        println!("ok: handoff payload is valid for planning");
                    }
                    Ok(())
                } else {
                    Err(miette!(
                        "handoff validation failed: {} error(s)",
                        report
                            .findings
                            .iter()
                            .filter(|finding| finding.severity
                                == bolt_cos_matic::handoff::FindingSeverity::Error)
                            .count()
                    ))
                }
            }
            HandoffAction::Plan {
                payload,
                dry_run,
                evidence_reports,
                evidence_manifests,
                human_approvals,
                approval_key_registry,
                json,
            } => {
                if !dry_run {
                    return Err(miette!(
                        "handoff plan requires --dry-run; implementation execution is forbidden in MVP"
                    ));
                }
                let plan = bolt_cos_matic::handoff::dry_run_plan_file_with_evidence_sources_and_approval_keys(
                    &payload,
                    &evidence_reports,
                    &evidence_manifests,
                    &human_approvals,
                    approval_key_registry.as_deref(),
                )?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&plan).into_diagnostic()?);
                } else {
                    print_handoff_report(&plan.report);
                }
                if !plan.report.is_valid() {
                    return Err(miette!(
                        "handoff dry-run refused: validation has blocking errors"
                    ));
                }
                if !json {
                    println!("dry-run gates:");
                    for gate in &plan.gates {
                        println!("  - {} [{}] {}", gate.code, gate.status, gate.detail);
                    }
                    println!("dry-run tasks:");
                    for task in &plan.tasks {
                        println!("  - {}", task.title);
                    }
                    println!("ok: dry-run plan produced; no execution performed");
                }
                Ok(())
            }
        },
        Command::Inspect { action } => match action {
            InspectAction::AdrRequired { root, policy } => {
                let policy = if let Some(path) = policy {
                    bolt_cos_matic::inspect::load_adr_required_policy(&path)?
                } else {
                    bolt_cos_matic::inspect::default_adr_required_policy()
                };
                let report = bolt_cos_matic::inspect::inspect_adr_required(&root, &policy)?;
                if report.is_clean() {
                    println!(
                        "ok: ADR coverage clean ({} file(s) checked)",
                        report.checked_files
                    );
                    Ok(())
                } else {
                    for finding in &report.findings {
                        eprintln!("{} [{}] {}", finding.path, finding.trigger, finding.reason);
                    }
                    Err(miette!(
                        "ADR coverage failed: {} finding(s)",
                        report.findings.len()
                    ))
                }
            }
            InspectAction::LanguageOwnership { root, policy } => {
                let policy = if let Some(path) = policy {
                    bolt_cos_matic::inspect::load_language_ownership_policy(&path)?
                } else {
                    bolt_cos_matic::inspect::default_language_ownership_policy()
                };
                let report = bolt_cos_matic::inspect::inspect_language_ownership(&root, &policy)?;
                if report.is_clean() {
                    println!(
                        "ok: language ownership clean ({} file(s) checked)",
                        report.checked_files
                    );
                    Ok(())
                } else {
                    for finding in &report.findings {
                        eprintln!(
                            "{} [{}:{}] {}",
                            finding.path, finding.zone, finding.extension, finding.reason
                        );
                    }
                    Err(miette!(
                        "language ownership failed: {} finding(s)",
                        report.findings.len()
                    ))
                }
            }
            InspectAction::FrontendStrict { root, policy } => {
                let policy = if let Some(path) = policy {
                    bolt_cos_matic::inspect::load_frontend_strict_policy(&path)?
                } else {
                    bolt_cos_matic::inspect::default_frontend_strict_policy()
                };
                let report = bolt_cos_matic::inspect::inspect_frontend_strict(&root, &policy)?;
                if report.is_clean() {
                    println!(
                        "ok: frontend strict clean ({} file(s) checked)",
                        report.checked_files
                    );
                    Ok(())
                } else {
                    for finding in &report.findings {
                        eprintln!("{} {}", finding.path, finding.reason);
                    }
                    Err(miette!(
                        "frontend strict failed: {} finding(s)",
                        report.findings.len()
                    ))
                }
            }
            InspectAction::ShellDebt { root, policy } => {
                let policy = if let Some(path) = policy {
                    bolt_cos_matic::inspect::load_shell_debt_policy(&path)?
                } else {
                    bolt_cos_matic::inspect::default_shell_debt_policy()
                };
                let report = bolt_cos_matic::inspect::inspect_shell_debt(&root, &policy)?;
                if report.is_clean() {
                    println!(
                        "ok: shell debt clean ({} script(s) checked)",
                        report.checked_scripts
                    );
                    Ok(())
                } else {
                    for finding in &report.findings {
                        eprintln!(
                            "{} [{} line(s)] {}",
                            finding.path, finding.lines, finding.reason
                        );
                    }
                    Err(miette!(
                        "shell debt failed: {} finding(s)",
                        report.findings.len()
                    ))
                }
            }
        },
        Command::Stack { action } => match action {
            StackAction::ProjectStatus { root, json } => {
                let report = bolt_cos_matic::stack::project_status(&root)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_project_status(&report);
                }
                Ok(())
            }
            StackAction::Detect { root, json } => {
                let report = bolt_cos_matic::stack::stack_detect(&root)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_stack_detect(&report);
                }
                Ok(())
            }
            StackAction::Scorecard { root, json } => {
                let report = bolt_cos_matic::stack::stack_scorecard(&root)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_stack_scorecard(&report);
                }
                if report.decision == bolt_cos_matic::stack::StackDecision::NoGo {
                    Err(miette!("stack scorecard is NO_GO"))
                } else {
                    Ok(())
                }
            }
            StackAction::DependencyAudit { root, json } => {
                let report = bolt_cos_matic::stack::dependency_audit(&root)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_dependency_audit(&report);
                }
                if report.has_failures() {
                    Err(miette!("dependency audit has blocking findings"))
                } else {
                    Ok(())
                }
            }
            StackAction::LocalSmoke {
                root,
                commands,
                timeout_seconds,
                json,
            } => run_local_smoke(&root, &commands, timeout_seconds, json),
            StackAction::DbSecurityCheck {
                root,
                database_url,
                allow_db_connection,
                json,
            } => {
                let report = bolt_cos_matic::stack::db_security_check(
                    &root,
                    &bolt_cos_matic::stack::DbSecurityCheckOptions {
                        database_url_requested: database_url.is_some(),
                        allow_db_connection,
                    },
                )?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_db_security_check(&report);
                }
                if report.has_failures() {
                    Err(miette!("db security check has blocking findings"))
                } else {
                    Ok(())
                }
            }
            StackAction::AdrGenerate {
                title,
                accepted_decision_ref,
                context,
                decision,
                consequences,
                reversibility,
                json,
            } => {
                let report =
                    bolt_cos_matic::stack::adr_generate(&bolt_cos_matic::stack::AdrDraftRequest {
                        title,
                        accepted_decision_ref,
                        context,
                        decision,
                        consequences,
                        reversibility,
                    });
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print!("{}", report.markdown);
                    if !report.missing_fields.is_empty() {
                        eprintln!("missing fields: {}", report.missing_fields.join(", "));
                    }
                }
                if report.is_complete() {
                    Ok(())
                } else {
                    Err(miette!(
                        "ADR draft has missing fields: {}",
                        report.missing_fields.join(", ")
                    ))
                }
            }
            StackAction::DeployDryRun {
                root,
                commands,
                json,
            } => {
                let report = bolt_cos_matic::stack::deploy_dry_run(&root, &commands)?;
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).into_diagnostic()?
                    );
                } else {
                    print_deploy_dry_run(&report);
                }
                if report.has_failures() {
                    Err(miette!("deploy dry-run has blocking findings"))
                } else {
                    Ok(())
                }
            }
        },
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
            // Kill-switch: set BOLT_COSMATIC_DISPATCH_DISABLED to refuse every dispatch.
            let env = dispatch::Envelope {
                enabled: std::env::var_os("BOLT_COSMATIC_DISPATCH_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_attempts: 1,
            };
            if !env.enabled {
                exit_refused("dispatch is disabled (kill-switch)");
            }
            let req = dispatch::FixRequest {
                issue,
                title,
                body,
                repo: repo_id.clone(),
            };
            let repo_root = std::env::current_dir().into_diagnostic()?;
            // BOLT_COSMATIC_FIXER=stub uses the deterministic no-LLM fixer (no Anthropic key).
            let report = if std::env::var("BOLT_COSMATIC_FIXER").as_deref() == Ok("stub") {
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
            // Kill-switch: set BOLT_COSMATIC_AUTOMERGE_DISABLED to refuse every merge.
            let env = automerge::MergeEnvelope {
                enabled: std::env::var_os("BOLT_COSMATIC_AUTOMERGE_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_merges: 1,
            };
            if !env.enabled {
                exit_refused("auto-merge is disabled (kill-switch)");
            }
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
            let env = deploy::DeployEnvelope {
                enabled: std::env::var_os("BOLT_COSMATIC_DEPLOY_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_deploys: 1,
            };
            if !env.enabled {
                exit_refused("deploy is disabled (kill-switch)");
            }
            let cmd = |key: &str| -> miette::Result<String> {
                std::env::var(key)
                    .map_err(|_| miette!("set {key} (the deploy is configured by command)"))
            };
            let deployer = deploy::CommandDeployer {
                canary_cmd: cmd("BOLT_COSMATIC_DEPLOY_CANARY")?,
                promote_cmd: cmd("BOLT_COSMATIC_DEPLOY_PROMOTE")?,
                rollback_cmd: cmd("BOLT_COSMATIC_DEPLOY_ROLLBACK")?,
            };
            let smoke = deploy::CommandSmoke {
                smoke_cmd: cmd("BOLT_COSMATIC_DEPLOY_SMOKE")?,
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
                let branch = AttemptBranch::new(&format!("issue-{issue}"), issue, 1)
                    .map_err(|e| miette!("branch policy: {e}"))?
                    .as_str()
                    .to_string();
                println!(
                    "[dry-run] loop for issue #{issue} on {}/{} — no fix, merge, or deploy will run.",
                    repo_id.owner, repo_id.name
                );
                println!(
                    "  1. dispatch  -> would create `{branch}` and run a bounded fixer. [skipped]"
                );
                println!("  2. publish   -> would push `{branch}` and open a PR. [skipped]");

                let maybe_verdict = if std::env::var_os("GITHUB_TOKEN").is_some()
                    || std::env::var_os("GH_TOKEN").is_some()
                {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .into_diagnostic()?;
                    Some(
                        rt.block_on(async {
                            let forge =
                                GithubForge::from_env().map_err(|e| automerge::MergeError(e.0))?;
                            automerge::ForgeGate::new(&forge)
                                .verdict(&automerge::MergeRequest {
                                    branch: branch.clone(),
                                    repo: repo_id.clone(),
                                })
                                .await
                        })
                        .into_diagnostic()?,
                    )
                } else {
                    None
                };

                let Some(verdict) = maybe_verdict else {
                    println!(
                        "  3. automerge -> skipped remote gate check: no GitHub token in dry-run. [skipped]"
                    );
                    println!(
                        "  4. deploy    -> would canary-deploy, smoke-test, and promote-or-rollback. [skipped]"
                    );
                    return Ok(());
                };

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
                enabled: std::env::var_os("BOLT_COSMATIC_LOOP_DISABLED").is_none(),
                allowlist: vec![repo_id.clone()],
                max_iterations,
            };
            if !env.enabled {
                exit_refused("loop is disabled (kill-switch)");
            }
            let req = pipeline::LoopRequest {
                issue,
                title,
                body,
                repo: repo_id.clone(),
            };
            let repo_root = std::env::current_dir().into_diagnostic()?;
            let deploy_canary = std::env::var("BOLT_COSMATIC_DEPLOY_CANARY").unwrap_or_default();
            let deploy_promote = std::env::var("BOLT_COSMATIC_DEPLOY_PROMOTE").unwrap_or_default();
            let deploy_rollback =
                std::env::var("BOLT_COSMATIC_DEPLOY_ROLLBACK").unwrap_or_default();
            let deploy_smoke = std::env::var("BOLT_COSMATIC_DEPLOY_SMOKE").unwrap_or_default();
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

fn exit_refused(reason: &str) -> ! {
    eprintln!("refused: {reason}");
    std::process::exit(2);
}

/// Print a deterministic maturity validation summary.
fn print_maturity_report(report: &bolt_cos_matic::maturity::MaturityReport) {
    println!(
        "{}: current={} target={} status={}",
        report.project, report.current_level, report.target_level, report.status
    );
    if let Some(next) = report.next_level {
        println!("next: {next}");
    }
    if !report.blocked_by.is_empty() {
        println!("blocked by:");
        for blocker in &report.blocked_by {
            println!("  - {blocker}");
        }
    }
    println!(
        "evidence: {} ref(s), learning_yield: {} item(s)",
        report.evidence_count, report.learning_yield_count
    );
    for finding in &report.findings {
        eprintln!(
            "{:?} [{}] {}",
            finding.severity, finding.code, finding.detail
        );
    }
}

fn print_maturity_workspace_report(report: &bolt_cos_matic::maturity::MaturityWorkspaceReport) {
    for item in &report.reports {
        println!(
            "{:<22} current={:<3} target={:<3} status={}",
            item.project, item.current_level, item.target_level, item.status
        );
        if !item.blocked_by.is_empty() {
            println!("  blocked_by: {}", item.blocked_by.join("; "));
        }
        for finding in &item.findings {
            eprintln!(
                "{} {:?} [{}] {}",
                item.project, finding.severity, finding.code, finding.detail
            );
        }
    }
}

/// Print a deterministic handoff validation summary.
fn print_handoff_report(report: &bolt_cos_matic::handoff::HandoffReport) {
    println!(
        "handoff: id={} package={} hash={}",
        report.handoff_id.as_deref().unwrap_or("<missing>"),
        report.package_id.as_deref().unwrap_or("<missing>"),
        report.package_hash.as_deref().unwrap_or("<missing>")
    );
    if let Some(goal) = &report.planning_goal {
        println!("goal: {goal}");
    }
    if !report.requested_outputs.is_empty() {
        println!("requested outputs: {}", report.requested_outputs.join(", "));
    }
    for finding in &report.findings {
        eprintln!(
            "{} [{}] {}",
            finding.severity.label(),
            finding.code,
            finding.message
        );
    }
}

fn print_project_status(report: &bolt_cos_matic::stack::ProjectStatusReport) {
    println!(
        "project_status: target={} mode={}",
        report.target, report.mode
    );
    if report.git.is_repo {
        println!(
            "git: branch={} dirty_entries={}",
            report
                .git
                .branch
                .as_deref()
                .unwrap_or("<detached-or-unknown>"),
            report.git.dirty_entries.len()
        );
    } else {
        println!("git: not a repository");
    }
    if !report.detected_scripts.is_empty() {
        println!("detected scripts:");
        for script in &report.detected_scripts {
            println!("  - {script}");
        }
    }
    print_stack_findings(&report.findings);
    print_next_actions(&report.next_actions);
}

fn print_stack_detect(report: &bolt_cos_matic::stack::StackDetectReport) {
    println!(
        "stack_detect: target={} mode={}",
        report.target, report.mode
    );
    for component in &report.components {
        println!(
            "component: {} {} confidence={} evidence={}",
            component.kind,
            component.name,
            component.confidence,
            component.evidence.join(", ")
        );
    }
    if !report.suggested_commands.is_empty() {
        println!("suggested local commands:");
        for command in &report.suggested_commands {
            println!("  - {command}");
        }
    }
    if !report.missing_gates.is_empty() {
        println!("missing gates:");
        for gate in &report.missing_gates {
            println!("  - {gate}");
        }
    }
    print_stack_findings(&report.findings);
}

fn print_stack_scorecard(report: &bolt_cos_matic::stack::StackScorecardReport) {
    println!(
        "stack_scorecard: target={} decision={} mode={}",
        report.target, report.decision, report.mode
    );
    for axis in &report.axes {
        println!("axis: {} {}", axis.status.label(), axis.axis);
        for missing in &axis.missing_evidence {
            println!("  missing: {missing}");
        }
    }
    print_stack_findings(&report.findings);
    print_next_actions(&report.next_actions);
}

fn print_dependency_audit(report: &bolt_cos_matic::stack::DependencyAuditReport) {
    println!(
        "dependency_audit: target={} mode={}",
        report.target, report.mode
    );
    if !report.manifests.is_empty() {
        println!("manifests:");
        for manifest in &report.manifests {
            println!("  - {manifest}");
        }
    }
    print_stack_findings(&report.findings);
    if !report.waiver_candidates.is_empty() {
        println!("waiver candidates:");
        for waiver in &report.waiver_candidates {
            println!("  - {waiver}");
        }
    }
}

fn print_db_security_check(report: &bolt_cos_matic::stack::DbSecurityCheckReport) {
    println!(
        "db_security_check: target={} mode={} backend={}",
        report.target, report.mode, report.backend
    );
    if report.database_url_requested {
        println!(
            "database connection: {} (performed={})",
            if report.refused_db_connection {
                "refused"
            } else {
                "not executed"
            },
            report.db_connection_performed
        );
    }
    if !report.sql_files.is_empty() {
        println!("sql files:");
        for file in &report.sql_files {
            println!("  - {file}");
        }
    }
    if !report.accepted_fixtures.is_empty() {
        println!("accepted fixtures:");
        for file in &report.accepted_fixtures {
            println!("  - {file}");
        }
    }
    print_stack_findings(&report.findings);
    print_next_actions(&report.next_actions);
}

fn print_deploy_dry_run(report: &bolt_cos_matic::stack::DeployDryRunReport) {
    println!(
        "deploy_dry_run: target={} mode={} dry_run_only={}",
        report.target, report.mode, report.dry_run_only
    );
    if !report.commands.is_empty() {
        println!("commands classified (not executed):");
        for command in &report.commands {
            println!("  - {}: {}", command.status, command.command);
            if let Some(reason) = &command.reason {
                println!("    reason: {reason}");
            }
        }
    }
    print_stack_findings(&report.findings);
    print_next_actions(&report.next_actions);
}

fn print_stack_findings(findings: &[bolt_cos_matic::stack::StackFinding]) {
    for finding in findings {
        eprintln!(
            "{} [{}] {}",
            finding.severity.label(),
            finding.axis,
            finding.message
        );
    }
}

fn print_next_actions(actions: &[String]) {
    if actions.is_empty() {
        return;
    }
    println!("next actions:");
    for action in actions {
        println!("  - {action}");
    }
}

fn run_local_smoke(
    root: &std::path::Path,
    commands: &[String],
    timeout_seconds: u64,
    json: bool,
) -> miette::Result<()> {
    if commands.is_empty() {
        return Err(miette!("local-smoke requires at least one --cmd"));
    }
    let mut results = Vec::new();
    let mut failed = false;
    for command in commands {
        if let Some(reason) = refused_smoke_command(command) {
            failed = true;
            results.push(serde_json::json!({
                "command": redact_command_for_report(command),
                "status": "refused",
                "reason": reason,
                "exit_code": null,
            }));
            continue;
        }
        let output = run_smoke_command(root, command, timeout_seconds)?;
        let code = output.status.code();
        if !output.status.success() {
            failed = true;
        }
        results.push(serde_json::json!({
            "command": redact_command_for_report(command),
            "status": if output.status.success() { "pass" } else { "fail" },
            "exit_code": code,
            "stdout": redact_output(&String::from_utf8_lossy(&output.stdout)),
            "stderr": redact_output(&String::from_utf8_lossy(&output.stderr)),
        }));
    }
    let report = serde_json::json!({
        "tool": "local_smoke",
        "version": "0.1",
        "mode": "local_only",
        "target": root.to_string_lossy(),
        "timeout_seconds": timeout_seconds,
        "timeout_enforced": true,
        "redactions_applied": true,
        "results": results,
    });
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).into_diagnostic()?
        );
    } else {
        println!("local_smoke: target={} mode=local_only", root.display());
        for result in report["results"].as_array().unwrap_or(&Vec::new()) {
            println!(
                "  - {}: {}",
                result["status"].as_str().unwrap_or("unknown"),
                result["command"].as_str().unwrap_or("<missing>")
            );
            if let Some(reason) = result["reason"].as_str() {
                println!("    reason: {reason}");
            }
        }
    }
    if failed {
        Err(miette!(
            "local smoke failed or refused at least one command"
        ))
    } else {
        Ok(())
    }
}

fn run_smoke_command(
    root: &std::path::Path,
    command: &str,
    timeout_seconds: u64,
) -> miette::Result<std::process::Output> {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", command])
            .current_dir(root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    } else {
        std::process::Command::new("sh")
            .args(["-c", command])
            .current_dir(root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }
    .into_diagnostic()?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_seconds);
    loop {
        if child.try_wait().into_diagnostic()?.is_some() {
            return child.wait_with_output().into_diagnostic();
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(miette!(
                "local smoke command timed out after {timeout_seconds}s: {command}"
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

fn refused_smoke_command(command: &str) -> Option<&'static str> {
    let lowered = command.to_lowercase();
    let forbidden_words = [
        "deploy",
        "push",
        "provision",
        "apply",
        "clever",
        "aws",
        "gcloud",
        "az",
        "vercel",
        "netlify",
        "fly",
    ];
    let forbidden_phrases = [
        "terraform apply",
        "pulumi up",
        "kubectl apply",
        "docker push",
        "git push",
        "npm publish",
        "cargo publish",
    ];
    if command_contains_secret_material(&lowered) {
        Some("command appears to include secret material")
    } else if contains_shell_metacharacter(command) {
        Some(
            "command contains shell metacharacters; use a simple local command without expansion or chaining",
        )
    } else if forbidden_words
        .iter()
        .any(|word| contains_shell_word(&lowered, word))
        || forbidden_phrases
            .iter()
            .any(|phrase| lowered.contains(phrase))
    {
        Some("command looks like provisioning, publishing, deploy, or hyperscaler access")
    } else {
        None
    }
}

fn redact_command_for_report(command: &str) -> String {
    if command_contains_secret_material(&command.to_lowercase()) {
        "<redacted>".to_string()
    } else {
        command.to_string()
    }
}

fn command_contains_secret_material(lowered: &str) -> bool {
    let secret_needles = [
        "token",
        "secret",
        "password",
        "passwd",
        "authorization",
        "api_key",
        "api-key",
        "apikey",
        "access_key",
        "access-key",
        "private_key",
        "client_secret",
        "bearer ",
        "ghp_",
        "ghs_",
        "github_pat_",
        "glpat-",
        "akia",
        "sk-",
    ];
    secret_needles.iter().any(|needle| lowered.contains(needle))
        || lowered
            .split_whitespace()
            .any(|part| part.contains("://") && part.contains('@'))
}

fn contains_shell_metacharacter(command: &str) -> bool {
    command.chars().any(|ch| {
        matches!(
            ch,
            ';' | '&'
                | '|'
                | '<'
                | '>'
                | '$'
                | '`'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | '*'
                | '?'
                | '~'
                | '!'
                | '\\'
                | '\''
                | '"'
                | '\n'
                | '\r'
        )
    })
}

fn contains_shell_word(command: &str, word: &str) -> bool {
    command.match_indices(word).any(|(start, _)| {
        let before = command[..start].chars().next_back();
        let after = command[start + word.len()..].chars().next();
        !before.is_some_and(is_command_word_char) && !after.is_some_and(is_command_word_char)
    })
}

fn is_command_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn redact_output(output: &str) -> String {
    output
        .lines()
        .take(80)
        .map(|line| {
            let lowered = line.to_lowercase();
            if command_contains_secret_material(&lowered) {
                "<redacted>".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Print one line per goal outcome, marking hard-gate failures.
fn print_goals(outcomes: &[bolt_cos_matic::goals::GoalOutcome]) {
    use bolt_cos_matic::config::schema::GoalKind;
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
