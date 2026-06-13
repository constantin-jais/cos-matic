//! Interactive setup wizard for Agent-O-Matic projects (`aom init`).
//!
//! Scaffolds a new project with optional L0–L3 levels, adapters, and safe-write
//! handling (never clobbers existing files).

use std::fs;
use std::path::Path;

use inquire::{Confirm, MultiSelect, Select, Text};
use miette::{IntoDiagnostic, Result, miette};

/// Configuration for the init wizard.
pub struct InitConfig {
    pub project_name: String,
    pub autonomy_level: String, // "L0", "L1", "L2", "L3"
    pub adapters: Vec<String>,  // "universal", "claude", "cursor"
    #[allow(dead_code)]
    pub repo: Option<String>, // Parsed but not yet written to manifest (future enhancement).
}

impl InitConfig {
    /// Build adapter targets TOML block.
    fn adapter_targets_toml(&self) -> String {
        let mut blocks = String::new();

        for adapter in &self.adapters {
            match adapter.as_str() {
                "universal" => {
                    blocks.push_str(
                        r#"
[[targets]]
name = "agents"
adapter = "universal"
output_file = "AGENTS.md"
profile = "default"
"#,
                    );
                }
                "claude" => {
                    blocks.push_str(
                        r#"
[[targets]]
name = "claude"
adapter = "claude"
output_file = "CLAUDE.md"
profile = "default"
"#,
                    );
                }
                "cursor" => {
                    blocks.push_str(
                        r#"
[[targets]]
name = "cursor"
adapter = "cursor"
output_dir = ".cursor/rules"
profile = "default"
"#,
                    );
                }
                _ => {} // Skipped; should not happen if validation passes.
            }
        }

        blocks
    }

    /// Render harness.toml template.
    fn render_harness_toml(&self) -> String {
        const TEMPLATE: &str = include_str!("../templates/harness.toml");
        TEMPLATE
            .replace("{{name}}", &self.project_name)
            .replace("{{level}}", &self.autonomy_level)
            .replace("{{adapter_targets}}", &self.adapter_targets_toml())
    }

    /// Render domains/core-values.md template.
    fn render_core_values_md(&self) -> String {
        const TEMPLATE: &str = include_str!("../templates/domains/core-values.md");
        TEMPLATE.replace("{{name}}", &self.project_name)
    }

    /// Render orchestrator-loop.yml template.
    fn render_orchestrator_loop_yml(&self) -> String {
        const TEMPLATE: &str = include_str!("../templates/workflows/orchestrator-loop.yml");
        TEMPLATE.to_string() // No placeholders; as-is.
    }
}

/// Validate the autonomy level.
fn validate_level(level: &str) -> Result<()> {
    match level {
        "L0" | "L1" | "L2" | "L3" => Ok(()),
        _ => Err(miette!(
            "invalid autonomy level `{level}`. Must be one of: L0, L1, L2, L3",
        )),
    }
}

/// Validate adapter names.
fn validate_adapters(adapters: &[String]) -> Result<()> {
    for adapter in adapters {
        match adapter.as_str() {
            "universal" | "claude" | "cursor" => {}
            _ => {
                return Err(miette!(
                    "invalid adapter `{adapter}`. Must be one of: universal, claude, cursor"
                ));
            }
        }
    }
    if adapters.is_empty() {
        return Err(miette!("at least one adapter is required"));
    }
    Ok(())
}

/// Validate project name (simple alphanumeric + hyphens).
fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(miette!("project name is required"));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(miette!(
            "project name `{name}` must be alphanumeric (hyphens and underscores allowed)"
        ));
    }
    Ok(())
}

/// Interactive prompt for the project name.
fn prompt_project_name(default: Option<&str>) -> Result<String> {
    let prompt_text = if let Some(def) = default {
        format!("Project name [{}]", def)
    } else {
        "Project name".to_string()
    };

    let name = Text::new(&prompt_text)
        .with_default(default.unwrap_or("my-project"))
        .prompt()
        .into_diagnostic()?;

    validate_project_name(&name)?;
    Ok(name)
}

/// Interactive prompt for autonomy level.
fn prompt_level(default: Option<&str>) -> Result<String> {
    let options = vec![
        "L0 (compile-only)",
        "L1 (bounded dispatch)",
        "L2 (gated loop)",
        "L3 (trusted autonomy)",
    ];
    let default_idx = if let Some(def) = default {
        options.iter().position(|o| o.starts_with(def)).unwrap_or(0)
    } else {
        0
    };

    let selected = Select::new("Autonomy level", options)
        .with_starting_cursor(default_idx)
        .prompt()
        .into_diagnostic()?;

    Ok(selected.split_whitespace().next().unwrap().to_string()) // Extract "L0", "L1", etc.
}

/// Interactive prompt for adapters (multi-select).
fn prompt_adapters(defaults: Option<&[String]>) -> Result<Vec<String>> {
    let options = vec!["universal", "claude", "cursor"];
    let checked_defaults = if let Some(defs) = defaults {
        options
            .iter()
            .enumerate()
            .filter(|(_, opt)| defs.contains(&opt.to_string()))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    } else {
        vec![0] // Default to universal only.
    };

    let selected = MultiSelect::new("Adapters", options)
        .with_default(&checked_defaults)
        .prompt()
        .into_diagnostic()?;

    Ok(selected.into_iter().map(|s| s.to_string()).collect())
}

/// Interactive prompt for GitHub repo.
fn prompt_repo() -> Result<Option<String>> {
    let include_repo = Confirm::new("Add GitHub repo (optional)?")
        .with_default(false)
        .prompt()
        .into_diagnostic()?;

    if !include_repo {
        return Ok(None);
    }

    let repo = Text::new("GitHub repo (owner/name)")
        .prompt()
        .into_diagnostic()?;

    if !repo.contains('/') {
        return Err(miette!("repo must be in format `owner/name`, got `{repo}`"));
    }

    Ok(Some(repo))
}

/// Gather inputs interactively or from flags.
pub fn gather_inputs(
    name_flag: Option<String>,
    level_flag: Option<String>,
    adapters_flag: Vec<String>,
    repo_flag: Option<String>,
    yes_mode: bool,
) -> Result<InitConfig> {
    if yes_mode {
        // Non-interactive: all required flags must be set.
        let name = name_flag.ok_or_else(|| miette!("--name is required in --yes mode"))?;
        let level = level_flag.unwrap_or_else(|| "L0".to_string());
        let adapters = if adapters_flag.is_empty() {
            vec!["universal".to_string()]
        } else {
            adapters_flag
        };

        validate_project_name(&name)?;
        validate_level(&level)?;
        validate_adapters(&adapters)?;

        return Ok(InitConfig {
            project_name: name,
            autonomy_level: level,
            adapters,
            repo: repo_flag,
        });
    }

    // Interactive mode: prompt for missing values.
    let project_name = if let Some(name) = name_flag {
        validate_project_name(&name)?;
        name
    } else {
        // Try to infer from current directory name.
        let default_name = std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "my-project".to_string());
        prompt_project_name(Some(&default_name))?
    };

    let autonomy_level = if let Some(level) = level_flag {
        validate_level(&level)?;
        level
    } else {
        prompt_level(Some("L0"))?
    };

    let adapters = if !adapters_flag.is_empty() {
        validate_adapters(&adapters_flag)?;
        adapters_flag
    } else {
        prompt_adapters(None)?
    };
    validate_adapters(&adapters)?;

    let repo = if repo_flag.is_some() {
        repo_flag
    } else {
        prompt_repo()?
    };

    Ok(InitConfig {
        project_name,
        autonomy_level,
        adapters,
        repo,
    })
}

/// Scaffold files to the target directory.
pub fn scaffold(config: &InitConfig, target_dir: &Path) -> Result<()> {
    let harness_path = target_dir.join("harness.toml");
    let domains_dir = target_dir.join("domains");
    let core_values_path = domains_dir.join("core-values.md");
    let workflow_path = target_dir.join(".github/workflows/orchestrator-loop.yml");

    let mut skipped = Vec::new();

    // Check for existing files (safe-write).
    if harness_path.exists() {
        skipped.push("harness.toml");
    }
    if core_values_path.exists() {
        skipped.push("domains/core-values.md");
    }
    if config.autonomy_level != "L0" && workflow_path.exists() {
        skipped.push(".github/workflows/orchestrator-loop.yml");
    }

    if !skipped.is_empty() {
        eprintln!("warning: the following files already exist and will be skipped (safe-write):");
        for f in &skipped {
            eprintln!("  {}", f);
        }
    }

    // Create harness.toml if not present.
    if !harness_path.exists() {
        let content = config.render_harness_toml();
        fs::write(&harness_path, content)
            .into_diagnostic()
            .map_err(|e| miette!("failed to write {}: {}", harness_path.display(), e))?;
        println!("created  {}", harness_path.display());
    }

    // Create domains/core-values.md if not present.
    if !core_values_path.exists() {
        fs::create_dir_all(&domains_dir)
            .into_diagnostic()
            .map_err(|e| miette!("failed to create {}: {}", domains_dir.display(), e))?;

        let content = config.render_core_values_md();
        fs::write(&core_values_path, content)
            .into_diagnostic()
            .map_err(|e| miette!("failed to write {}: {}", core_values_path.display(), e))?;
        println!("created  {}", core_values_path.display());
    }

    // Create workflow for L1/L2/L3.
    if config.autonomy_level != "L0" && !workflow_path.exists() {
        fs::create_dir_all(workflow_path.parent().unwrap())
            .into_diagnostic()
            .map_err(|e| miette!("failed to create .github/workflows: {}", e))?;

        let content = config.render_orchestrator_loop_yml();
        fs::write(&workflow_path, content)
            .into_diagnostic()
            .map_err(|e| miette!("failed to write {}: {}", workflow_path.display(), e))?;
        println!("created  {}", workflow_path.display());
    }

    Ok(())
}

/// Print the manual operator checklist for the user.
pub fn print_checklist(config: &InitConfig) {
    println!();
    println!("{}", "=".repeat(60));
    println!("Manual Operator Checklist for {}", config.autonomy_level);
    println!("{}", "=".repeat(60));
    println!();

    println!("[ ] 1. Commit and push this scaffold to your repository.");
    println!("[ ] 2. Review the generated harness.toml and domains/.");
    println!("[ ] 3. Run: aom generate --check");

    if config.autonomy_level != "L0" {
        println!("[ ] 4. If this is a sandbox repo (not your real repo),");
        println!("       set repository variable AOM_SANDBOX=true in Settings > Variables.");
        if config.autonomy_level != "L1" {
            println!("[ ] 5. Set repository secrets:");
            println!("       - AOM_BOT_TOKEN (fine-grained PAT for git push, PR, merge)");
            println!("       - ANTHROPIC_API_KEY (only if using fixer=claude)");
            println!(
                "       Note: AOM_CHECKS_TOKEN is supplied by the workflow from github.token;"
            );
            println!("       do not create it as a repository secret.");
        }
    }

    if config.autonomy_level == "L2" || config.autonomy_level == "L3" {
        println!("[ ] 6. Enable branch protection and require approvals for main.");
        println!("[ ] 7. Configure your CI checks (GitHub Actions).");
    }

    if config.autonomy_level == "L3" {
        println!("[ ] 8. Review docs/adr/0026-architecture-targets-seams-isolation-durability.md");
        println!("       (typed policy & isolation) before L3.");
    }

    println!();
    println!("For more details, see:");
    println!("  • docs/adr/0025-north-star-trustworthy-autonomy.md");
    if config.autonomy_level != "L0" {
        println!("  • .github/workflows/orchestrator-loop.yml");
    }
    println!();
}
