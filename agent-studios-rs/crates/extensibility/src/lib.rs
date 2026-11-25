//! Agent Studios Extensibility Engine (Milestone M16)
//!
//! Provides deterministic, security-bounded extensibility across:
//! - Skills (`SKILL.md` frontmatter, progressive disclosure, Global/Project/Agent scopes, safe install)
//! - Model Context Protocol (`mcp.json` loader, stdio/HTTP transport configs, secret redaction, approval tiers)
//! - Lifecycle Hooks (`pre_turn`, `post_task`, `on_reconciliation`, `on_approval` with hard timeouts & fail-closed policies)
//! - Hierarchical Instructions (`AGENTS.md` / `CLAUDE.md` discovery strictly contained within worktree boundaries)

pub mod hooks;
pub mod instructions;
pub mod mcp;
pub mod skills;

pub use hooks::{
    HookError, HookExecutionResult, HookPolicy, HookScope, HookSpec, LifecycleHookPhase,
    execute_hook, execute_phase_hooks,
};
pub use instructions::{
    InstructionEntry, InstructionError, InstructionFileKind, InstructionResolution,
    InstructionResolver,
};
pub use mcp::{
    McpConfigFile, McpError, McpInvocationDecision, McpRegistry, McpRiskTier, McpScope,
    McpServerConfig, McpToolCallResult, McpToolDescriptor, McpTransportConfig,
};
pub use skills::{
    SkillDescriptor, SkillError, SkillManifest, SkillProvenance, SkillRegistry, SkillScope,
};
