//! M16 Extensibility Integration Tests (Skills, MCP, Lifecycle Hooks, Hierarchical Instructions)

use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

use agent_studios_extensibility::{
    HookError, HookPolicy, HookScope, HookSpec, InstructionError, InstructionResolver,
    LifecycleHookPhase, McpConfigFile, McpError, McpInvocationDecision, McpRegistry, McpRiskTier,
    McpScope, McpServerConfig, McpTransportConfig, SkillError, SkillProvenance, SkillRegistry,
    SkillScope, execute_phase_hooks,
};

#[test]
fn test_m16_skills_progressive_disclosure_scopes_install_and_dependencies() {
    let global_home = tempdir().unwrap();
    let project_dir = tempdir().unwrap();
    let external_skill_src = tempdir().unwrap();

    // Global skill: base-skill
    let global_base = global_home.path().join("skills").join("base-skill");
    fs::create_dir_all(&global_base).unwrap();
    fs::write(
        global_base.join("SKILL.md"),
        r#"---
name: base-skill
description: Base global skill summary
version: 1.0.0
triggers: [base]
---
# Base Full Instructions
Detailed instructions that should only load on demand.
"#,
    )
    .unwrap();

    // External project skill depending on base-skill
    fs::write(
        external_skill_src.path().join("SKILL.md"),
        r#"---
name: rust-audit
description: Audits Rust crates
version: 0.2.0
tools: [exec_command]
dependencies: [base-skill]
triggers: [audit, clippy]
---
# Rust Audit Full Body
Run `cargo clippy --workspace --all-targets -- -D warnings`.
"#,
    )
    .unwrap();
    // Script inside skill must NEVER be executed on install
    let scripts_dir = external_skill_src.path().join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(scripts_dir.join("setup.sh"), "#!/bin/sh\nexit 1\n").unwrap();

    let mut reg = SkillRegistry::new(global_home.path(), Some(project_dir.path().to_path_buf()));
    reg.install_from_local_dir(
        external_skill_src.path(),
        SkillScope::Project,
        SkillProvenance::LocalFolder {
            path: external_skill_src.path().to_string_lossy().to_string(),
        },
    )
    .expect("safe install should succeed without executing scripts");

    let discovered = reg.discover().expect("discover");
    assert_eq!(discovered.len(), 2);

    // Progressive disclosure: summaries contain metadata, not body
    let summaries = reg.summaries();
    assert_eq!(summaries.len(), 2);
    assert!(summaries.iter().any(|s| s.name == "rust-audit"));

    let full_body = reg
        .get("rust-audit")
        .unwrap()
        .load_full_instructions()
        .unwrap();
    assert!(full_body.contains("cargo clippy --workspace"));

    // Dependencies valid when both enabled
    assert!(reg.validate_dependencies().is_ok());

    // Disabling dependency breaks validation
    reg.set_enabled("base-skill", false).unwrap();
    let dep_err = reg.validate_dependencies().unwrap_err();
    assert!(matches!(dep_err, SkillError::MissingDependency { .. }));
}

#[tokio::test]
async fn test_m16_mcp_config_redaction_approval_and_stdio_invocation() {
    let global_home = tempdir().unwrap();
    let project_dir = tempdir().unwrap();
    fs::create_dir_all(project_dir.path().join(".agent-studios")).unwrap();

    let mut env_map = std::collections::BTreeMap::new();
    env_map.insert(
        "API_TOKEN".to_string(),
        "sk-secret-token-1234567890".to_string(),
    );

    // Use cross-platform inline node or python/powershell? Test pure config & approval + echo-style stdio
    let mut global_cfg = McpConfigFile::default();
    global_cfg.servers.insert(
        "local-tools".to_string(),
        McpServerConfig {
            id: "local-tools".to_string(),
            name: Some("Local Tools".to_string()),
            enabled: true,
            timeout_secs: 10,
            risk_tier: McpRiskTier::High,
            transport: McpTransportConfig::Stdio {
                command: "node".to_string(),
                args: vec![
                    "-e".to_string(),
                    "process.stdin.once('data', d => { const r = JSON.parse(d); if (r.method === 'tools/list') { console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result:{tools:[{name:'ping_tool',description:'Ping with sk-secret-abcdef1234567890abcdef'}]}})); } else { console.log(JSON.stringify({jsonrpc:'2.0',id:r.id,result:{content:[{type:'text',text:'pong Bearer my-secret-jwt-token'}]}})); } });".to_string(),
                ],
                env: env_map,
            },
            scope: McpScope::Global,
        },
    );

    fs::write(
        global_home.path().join("mcp.json"),
        serde_json::to_string_pretty(&global_cfg).unwrap(),
    )
    .unwrap();

    let reg = McpRegistry::load_from_paths(global_home.path(), Some(project_dir.path()))
        .expect("load mcp registry");

    // 1. Secret redaction in summaries
    let summaries = reg.redacted_server_summaries();
    let serialized = serde_json::to_string(&summaries).unwrap();
    assert!(!serialized.contains("sk-secret-token-1234567890"));
    assert!(serialized.contains("[REDACTED]"));

    // 2. Approval interception fails closed without approval
    let decision = reg
        .evaluate_approval("local-tools", "ping_tool", Some(McpRiskTier::Low), false)
        .unwrap();
    assert!(matches!(
        decision,
        McpInvocationDecision::RequiresApproval {
            risk_tier: McpRiskTier::High,
            ..
        }
    ));

    let blocked = reg
        .invoke_tool_stdio(
            "local-tools",
            "ping_tool",
            serde_json::json!({}),
            project_dir.path(),
            Some(McpRiskTier::Low),
            false,
        )
        .await;
    assert!(matches!(blocked, Err(McpError::ApprovalRequired { .. })));

    // 3. Stdio tool discovery & approved tool invocation with secret redaction on output
    if let Ok(tools) = reg
        .discover_tools_stdio("local-tools", project_dir.path())
        .await
    {
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ping_tool");
        assert!(
            !tools[0]
                .description
                .contains("sk-secret-abcdef1234567890abcdef")
        );

        let call_res = reg
            .invoke_tool_stdio(
                "local-tools",
                "ping_tool",
                serde_json::json!({}),
                project_dir.path(),
                Some(McpRiskTier::High),
                true,
            )
            .await
            .expect("approved stdio invocation");
        assert!(!call_res.is_error);
        assert!(call_res.output.contains("pong"));
        assert!(!call_res.output.contains("my-secret-jwt-token"));
    }
}

#[tokio::test]
async fn test_m16_lifecycle_hooks_execution_fail_closed_and_timeout() {
    let dir = tempdir().unwrap();

    #[cfg(windows)]
    let (ok_cmd, ok_args, fail_cmd, fail_args, slow_cmd, slow_args) = (
        "cmd",
        vec!["/C", "echo", "hook-ok"],
        "cmd",
        vec!["/C", "exit", "1"],
        "powershell",
        vec!["-NoProfile", "-Command", "Start-Sleep -Seconds 3"],
    );
    #[cfg(not(windows))]
    let (ok_cmd, ok_args, fail_cmd, fail_args, slow_cmd, slow_args) = (
        "sh",
        vec!["-c", "echo hook-ok"],
        "sh",
        vec!["-c", "exit 1"],
        "sh",
        vec!["-c", "sleep 3"],
    );

    let ok_hook = HookSpec {
        id: "pre-turn-ok".to_string(),
        phase: LifecycleHookPhase::PreTurn,
        scope: HookScope::Project,
        command: ok_cmd.to_string(),
        args: ok_args.into_iter().map(str::to_string).collect(),
        timeout_ms: 5000,
        policy: HookPolicy::FailClosed,
        enabled: true,
    };

    let results = execute_phase_hooks(&[ok_hook], LifecycleHookPhase::PreTurn, dir.path())
        .await
        .expect("ok hook");
    assert_eq!(results.len(), 1);
    assert!(results[0].succeeded);
    assert!(results[0].stdout.contains("hook-ok"));

    let fail_hook = HookSpec {
        id: "post-task-fail".to_string(),
        phase: LifecycleHookPhase::PostTask,
        scope: HookScope::Project,
        command: fail_cmd.to_string(),
        args: fail_args.into_iter().map(str::to_string).collect(),
        timeout_ms: 5000,
        policy: HookPolicy::FailClosed,
        enabled: true,
    };
    let fail_res =
        execute_phase_hooks(&[fail_hook], LifecycleHookPhase::PostTask, dir.path()).await;
    assert!(matches!(fail_res, Err(HookError::FailedClosed { .. })));

    let timeout_hook = HookSpec {
        id: "reconcile-timeout".to_string(),
        phase: LifecycleHookPhase::OnReconciliation,
        scope: HookScope::Global,
        command: slow_cmd.to_string(),
        args: slow_args.into_iter().map(str::to_string).collect(),
        timeout_ms: 150,
        policy: HookPolicy::FailClosed,
        enabled: true,
    };
    let timeout_res = execute_phase_hooks(
        &[timeout_hook],
        LifecycleHookPhase::OnReconciliation,
        dir.path(),
    )
    .await;
    assert!(matches!(timeout_res, Err(HookError::TimedOut { .. })));
}

#[test]
fn test_m16_hierarchical_instructions_and_worktree_boundary_enforcement() {
    let global_home = tempdir().unwrap();
    let outside_dir = tempdir().unwrap();
    let worktree_root = tempdir().unwrap();
    let sub_dir = worktree_root.path().join("backend").join("src");
    fs::create_dir_all(&sub_dir).unwrap();

    fs::write(global_home.path().join("AGENTS.md"), "Global rules").unwrap();
    fs::write(worktree_root.path().join("AGENTS.md"), "Root project rules").unwrap();
    fs::write(
        worktree_root.path().join("backend").join("CLAUDE.md"),
        "Backend Claude rules",
    )
    .unwrap();
    fs::write(sub_dir.join("AGENTS.md"), "Src specific rules").unwrap();

    let res =
        InstructionResolver::resolve(Some(global_home.path()), worktree_root.path(), &sub_dir)
            .expect("resolve hierarchical instructions");

    assert_eq!(res.entries.len(), 4);
    let merged = res.merged_prompt_context();
    let g_idx = merged.find("Global rules").unwrap();
    let r_idx = merged.find("Root project rules").unwrap();
    let b_idx = merged.find("Backend Claude rules").unwrap();
    let s_idx = merged.find("Src specific rules").unwrap();
    assert!(g_idx < r_idx && r_idx < b_idx && b_idx < s_idx);

    // Boundary check: active_cwd outside worktree_root must fail closed
    let escaped: PathBuf = outside_dir.path().to_path_buf();
    let err =
        InstructionResolver::resolve(Some(global_home.path()), worktree_root.path(), &escaped)
            .unwrap_err();
    assert!(matches!(err, InstructionError::BoundaryViolation(_)));
}
