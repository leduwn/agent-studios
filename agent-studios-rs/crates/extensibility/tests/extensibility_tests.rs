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

#[tokio::test]
async fn test_m16_mcp_streamable_http_handshake_tools_resources_prompts_and_redaction() {
    use wiremock::matchers::{body_string_contains, header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(header("authorization", "Bearer top-secret-header"))
        .and(body_string_contains("\"method\":\"initialize\""))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("mcp-session-id", "sess-http-001")
                .set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 0,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "serverInfo": { "name": "wiremock-mcp" },
                        "capabilities": { "tools": {}, "resources": {}, "prompts": {} }
                    }
                })),
        )
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains(
            "\"method\":\"notifications/initialized\"",
        ))
        .respond_with(ResponseTemplate::new(202))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"tools/list\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {
                "tools": [{
                    "name": "http_search",
                    "description": "Search repo (secret sk-secret-abcdef1234567890abcdef)"
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"tools/call\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": {
                "content": [{ "type": "text", "text": "matched Bearer super-secret-jwt-value" }],
                "isError": false
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"resources/list\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "result": {
                "resources": [{
                    "uri": "file:///README.md",
                    "name": "README",
                    "description": "Repo overview",
                    "mimeType": "text/markdown"
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"resources/read\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "result": {
                "contents": [{
                    "uri": "file:///README.md",
                    "mimeType": "text/markdown",
                    "text": "Agent Studios docs with key sk-secret-abcdef1234567890abcdef"
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"prompts/list\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "result": {
                "prompts": [{ "name": "audit-pr", "description": "Security review prompt" }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(body_string_contains("\"method\":\"prompts/get\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {
                "description": "Audit instructions",
                "messages": [{ "role": "user", "content": { "type": "text", "text": "Review crate" } }]
            }
        })))
        .mount(&server)
        .await;

    let mut headers = std::collections::BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer top-secret-header".to_string(),
    );

    let mut reg = McpRegistry::default();
    reg.register_server(McpServerConfig {
        id: "remote-http".to_string(),
        name: Some("Remote HTTP MCP".to_string()),
        enabled: true,
        timeout_secs: 5,
        risk_tier: McpRiskTier::High,
        transport: McpTransportConfig::Http {
            url: server.uri(),
            headers,
        },
        scope: McpScope::Project,
    });

    let dir = tempdir().unwrap();

    let init = reg
        .initialize_server("remote-http", dir.path())
        .await
        .unwrap();
    assert_eq!(init.protocol_version, "2024-11-05");
    assert_eq!(init.server_name.as_deref(), Some("wiremock-mcp"));

    let tools = reg.discover_tools("remote-http", dir.path()).await.unwrap();
    assert_eq!(tools.len(), 1);
    assert!(
        !tools[0]
            .description
            .contains("sk-secret-abcdef1234567890abcdef")
    );

    // Fail-closed approval check applies uniformly to HTTP servers
    let blocked = reg
        .invoke_tool(
            "remote-http",
            "http_search",
            serde_json::json!({ "q": "daemon" }),
            dir.path(),
            Some(McpRiskTier::Low),
            false,
        )
        .await;
    assert!(matches!(blocked, Err(McpError::ApprovalRequired { .. })));

    let res = reg
        .invoke_tool(
            "remote-http",
            "http_search",
            serde_json::json!({ "q": "daemon" }),
            dir.path(),
            Some(McpRiskTier::High),
            true,
        )
        .await
        .unwrap();
    assert!(res.output.contains("matched"));
    assert!(!res.output.contains("super-secret-jwt-value"));

    let resources = reg.list_resources("remote-http", dir.path()).await.unwrap();
    assert_eq!(resources[0].uri, "file:///README.md");

    let contents = reg
        .read_resource("remote-http", "file:///README.md", dir.path())
        .await
        .unwrap();
    assert!(contents.text.contains("Agent Studios docs"));
    assert!(!contents.text.contains("sk-secret-abcdef1234567890abcdef"));

    let prompts = reg.list_prompts("remote-http", dir.path()).await.unwrap();
    assert_eq!(prompts[0].name, "audit-pr");

    let prompt = reg
        .get_prompt("remote-http", "audit-pr", serde_json::json!({}), dir.path())
        .await
        .unwrap();
    assert_eq!(prompt.messages.len(), 1);
}
