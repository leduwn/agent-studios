//! M16 Extensibility CLI Subcommands (`ags skill`, `ags mcp`, `ags hooks`, `ags instructions`)

use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

use agent_studios_extensibility::{
    HookSpec, InstructionResolver, McpRegistry, SkillProvenance, SkillRegistry, SkillScope,
};

use crate::config::get_config_dir;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum SkillScopeArg {
    Global,
    Project,
    Agent,
}

impl From<SkillScopeArg> for SkillScope {
    fn from(v: SkillScopeArg) -> Self {
        match v {
            SkillScopeArg::Global => SkillScope::Global,
            SkillScopeArg::Project => SkillScope::Project,
            SkillScopeArg::Agent => SkillScope::Agent,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum SkillCommands {
    /// List discovered skills across Global, Project, and Agent scopes
    List {
        #[arg(long)]
        json: bool,
    },
    /// Inspect full markdown instructions for an installed skill (progressive disclosure)
    Inspect { name: String },
    /// Safely install a skill from a local directory (never auto-executes scripts)
    Install {
        path: PathBuf,
        #[arg(long, default_value = "project", value_enum)]
        scope: SkillScopeArg,
    },
    /// Enable an installed skill
    Enable { name: String },
    /// Disable an installed skill
    Disable { name: String },
}

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// List configured MCP servers with secrets redacted
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum HooksCommands {
    /// List configured lifecycle hooks in workspace
    List {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args, Debug)]
pub struct InstructionsArgs {
    /// Active working directory inside repository/worktree
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Output resolved instructions as JSON
    #[arg(long)]
    pub json: bool,
}

fn current_project_root() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

pub fn handle_skill_command(cmd: SkillCommands) -> Result<(), Box<dyn std::error::Error>> {
    let global_home = get_config_dir();
    let project_root = current_project_root();
    let mut registry = SkillRegistry::new(global_home, project_root);
    let _ = registry.discover()?;

    match cmd {
        SkillCommands::List { json } => {
            let summaries = registry.summaries();
            if json {
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else if summaries.is_empty() {
                println!("No enabled skills discovered.");
            } else {
                for s in &summaries {
                    println!(
                        "[{:?}] {:<24} {} (enabled={})",
                        s.scope, s.name, s.description, s.enabled
                    );
                }
            }
        }
        SkillCommands::Inspect { name } => {
            let desc = registry
                .get(&name)
                .ok_or_else(|| format!("Skill '{name}' not found"))?;
            println!("{}", desc.load_full_instructions()?);
        }
        SkillCommands::Install { path, scope } => {
            let desc = registry.install_from_local_dir(
                &path,
                scope.into(),
                SkillProvenance::LocalFolder {
                    path: path.to_string_lossy().to_string(),
                },
            )?;
            println!(
                "✓ Installed skill '{}' ({:?}) safely without script execution",
                desc.manifest.name, desc.scope
            );
        }
        SkillCommands::Enable { name } => {
            registry.set_enabled(&name, true)?;
            println!("✓ Enabled skill '{name}'");
        }
        SkillCommands::Disable { name } => {
            registry.set_enabled(&name, false)?;
            println!("✓ Disabled skill '{name}'");
        }
    }
    Ok(())
}

pub fn handle_mcp_command(cmd: McpCommands) -> Result<(), Box<dyn std::error::Error>> {
    let global_home = get_config_dir();
    let project_root = current_project_root();
    let registry = McpRegistry::load_from_paths(&global_home, project_root.as_deref())?;

    match cmd {
        McpCommands::List { json } => {
            let redacted = registry.redacted_server_summaries();
            if json {
                println!("{}", serde_json::to_string_pretty(&redacted)?);
            } else if redacted.is_empty() {
                println!("No MCP servers configured.");
            } else {
                for item in &redacted {
                    println!(
                        "- {} (scope={}, tier={}, enabled={})",
                        item["id"], item["scope"], item["risk_tier"], item["enabled"]
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn handle_hooks_command(cmd: HooksCommands) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = current_project_root().unwrap_or_else(|| PathBuf::from("."));
    let hooks_path = project_root.join(".agent-studios").join("hooks.json");
    let hooks: Vec<HookSpec> = if hooks_path.is_file() {
        let raw = std::fs::read_to_string(&hooks_path)?;
        serde_json::from_str(&raw)?
    } else {
        Vec::new()
    };

    match cmd {
        HooksCommands::List { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&hooks)?);
            } else if hooks.is_empty() {
                println!("No lifecycle hooks configured in .agent-studios/hooks.json.");
            } else {
                for h in &hooks {
                    println!(
                        "- {} [{:?}] policy={:?} timeout={}ms enabled={}",
                        h.id, h.phase, h.policy, h.timeout_ms, h.enabled
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn handle_instructions_command(
    args: InstructionsArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let global_home = get_config_dir();
    let worktree_root = std::env::current_dir()?;
    let active_cwd = args.cwd.unwrap_or_else(|| worktree_root.clone());
    let resolution = InstructionResolver::resolve(Some(&global_home), &worktree_root, &active_cwd)?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&resolution)?);
    } else if resolution.entries.is_empty() {
        println!("No AGENTS.md or CLAUDE.md instruction files found.");
    } else {
        println!("{}", resolution.merged_prompt_context());
    }
    Ok(())
}
