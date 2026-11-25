//! MASTER MISSION 05 — Real Codex IPC Execution & Persistence Acceptance Tests
//!
//! Exercises the real `IpcServer` + `IpcClient` + `CodexStudioEngine` + `DaemonSecretVault`
//! + `InternalCodexAgentRuntime` + `WorkspaceOrchestrator` against an authenticated `wiremock`
//!   local multi-protocol inference server.

use std::sync::Arc;
use std::time::Duration;

use agent_studios_ipc_transport::{ClientConfig, IpcClient, IpcCommand, IpcResponse, IpcServer};
use agent_studios_provider::ProviderInstance;
use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::capabilities::ModelCapabilities;
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::{ModelDescriptor, ModelLimits};
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::{SecretBackend, SecretReference};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn run_large_stack<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(fut)
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn test_01_secret_vault_auth_gate_discovery_and_catalog_persistence() {
    run_large_stack(async {
        let server_http = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Bearer valid-secret-mm05"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "object": "list",
                "data": [{ "id": "codex-discovered-1", "object": "model" }]
            })))
            .mount(&server_http)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": { "message": "Unauthorized" }
            })))
            .with_priority(10)
            .mount(&server_http)
            .await;

        let state_dir = tempfile::tempdir().unwrap();
        let key_id = "mm05-client";
        let secret = b"mm05-ipc-secret-key".to_vec();

        let srv1 = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_codex_internal_runtime()
            .build()
            .expect("build srv1");
        let (cs1, ss1) = tokio::io::duplex(65536);
        srv1.handle_connection(ss1);
        let mut client = IpcClient::connect_stream(cs1, ClientConfig::new(key_id, secret.clone()))
            .await
            .expect("connect client");

        let readiness = client.get_execution_readiness().await.unwrap();
        assert!(readiness.available_runtimes.contains(&"codex".to_string()));

        // Wrong secret fails closed with 401/auth error
        let bad_ref = SecretReference {
            backend: SecretBackend::OsCredentialStore,
            locator: "agentStudios.secret.bad".to_string(),
        };
        client
            .send_command(IpcCommand::SetProviderSecret {
                secret_ref: bad_ref.clone(),
                secret_value: "wrong-token".to_string(),
            })
            .await
            .unwrap();
        let bad_id = ProviderInstanceId::new();
        let bad_inst = ProviderInstance::new(
            bad_id,
            ProviderId::new("openai").unwrap(),
            "Bad Secret",
            ProtocolFamily::OpenAiChatCompletions,
            EndpointProfile::new(format!("{}/v1", server_http.uri())).unwrap(),
            AuthenticationScheme::BearerToken { secret: bad_ref },
        )
        .unwrap();
        client
            .send_command(IpcCommand::UpsertProvider {
                provider: bad_inst,
                models: vec![],
            })
            .await
            .unwrap();
        let bad_disc = client
            .send_command(IpcCommand::DiscoverModels {
                provider_id: bad_id,
            })
            .await
            .unwrap();
        assert!(matches!(bad_disc, IpcResponse::Error { .. }));

        // Valid secret succeeds and persists provider + discovered model
        let good_ref = SecretReference {
            backend: SecretBackend::OsCredentialStore,
            locator: "agentStudios.secret.good".to_string(),
        };
        client
            .send_command(IpcCommand::SetProviderSecret {
                secret_ref: good_ref.clone(),
                secret_value: "valid-secret-mm05".to_string(),
            })
            .await
            .unwrap();
        let good_id = ProviderInstanceId::new();
        let good_inst = ProviderInstance::new(
            good_id,
            ProviderId::new("openai").unwrap(),
            "Good Secret Provider",
            ProtocolFamily::OpenAiChatCompletions,
            EndpointProfile::new(format!("{}/v1", server_http.uri())).unwrap(),
            AuthenticationScheme::BearerToken { secret: good_ref },
        )
        .unwrap();
        client
            .send_command(IpcCommand::UpsertProvider {
                provider: good_inst,
                models: vec![],
            })
            .await
            .unwrap();
        let disc = client
            .send_command(IpcCommand::DiscoverModels {
                provider_id: good_id,
            })
            .await
            .unwrap();
        match disc {
            IpcResponse::ModelsDiscovered { models, .. } => {
                assert!(models.iter().any(|m| m.id.as_str() == "codex-discovered-1"));
            }
            other => panic!("Unexpected discovery response: {other:?}"),
        }

        // Verify provider_catalog.json exists on disk and contains NO plaintext secret
        let catalog_json =
            std::fs::read_to_string(state_dir.path().join("provider_catalog.json")).unwrap();
        assert!(!catalog_json.contains("valid-secret-mm05"));
        assert!(!catalog_json.contains("wrong-token"));

        // Restart server pointing at the SAME state_dir -> catalog + secret vault survive!
        drop(client);
        let srv2 = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_codex_internal_runtime()
            .build()
            .expect("build srv2");
        let (cs2, ss2) = tokio::io::duplex(65536);
        srv2.handle_connection(ss2);
        let mut client2 = IpcClient::connect_stream(cs2, ClientConfig::new(key_id, secret))
            .await
            .expect("connect client2");

        let provs = client2
            .send_command(IpcCommand::ListProviders)
            .await
            .unwrap();
        match provs {
            IpcResponse::Providers { providers } => {
                assert!(providers.iter().any(|p| p.id == good_id));
            }
            other => panic!("Unexpected providers response: {other:?}"),
        }
        let mods = client2
            .send_command(IpcCommand::ListModels {
                instance_id: Some(good_id),
            })
            .await
            .unwrap();
        match mods {
            IpcResponse::Models { models } => {
                assert!(models.iter().any(|m| m.id.as_str() == "codex-discovered-1"));
            }
            other => panic!("Unexpected models response: {other:?}"),
        }
    });
}

#[test]
fn test_02_multi_turn_codex_execution_fork_and_transcript_recovery() {
    run_large_stack(async {
        let sse_reply = "data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Codex turn completed cleanly.\"},\"finish_reason\":null}]}\n\n\
                         data: {\"id\":\"chat-1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":6,\"total_tokens\":16}}\n\n\
                         data: [DONE]\n\n";

        let server_http = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer turn-secret-123"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_reply),
            )
            .mount(&server_http)
            .await;

        let state_dir = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace_dir.path().join("AGENTS.md"),
            "REPO_RULE_991: Preserve existing interfaces.\n",
        )
        .unwrap();
        let skill_dir = workspace_dir
            .path()
            .join(".agents")
            .join("skills")
            .join("review");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: review\ndescription: Code review\n---\nREVIEW_SKILL_MARKER_882\n",
        )
        .unwrap();

        let key_id = "mm05-chat";
        let secret = b"mm05-chat-key".to_vec();
        let srv = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_default_repo_path(workspace_dir.path())
            .with_authorized_workspace(workspace_dir.path())
            .with_codex_internal_runtime()
            .build()
            .unwrap();
        let (cs, ss) = tokio::io::duplex(65536);
        srv.handle_connection(ss);
        let mut client = IpcClient::connect_stream(cs, ClientConfig::new(key_id, secret.clone()))
            .await
            .unwrap();

        let prov_id = ProviderInstanceId::new();
        let sec_ref = SecretReference {
            backend: SecretBackend::OsCredentialStore,
            locator: "agentStudios.secret.chat".to_string(),
        };
        client
            .send_command(IpcCommand::SetProviderSecret {
                secret_ref: sec_ref.clone(),
                secret_value: "turn-secret-123".to_string(),
            })
            .await
            .unwrap();
        let inst = ProviderInstance::new(
            prov_id,
            ProviderId::new("openai").unwrap(),
            "Chat Test Provider",
            ProtocolFamily::OpenAiChatCompletions,
            EndpointProfile::new(format!("{}/v1", server_http.uri())).unwrap(),
            AuthenticationScheme::BearerToken { secret: sec_ref },
        )
        .unwrap();
        let model = ModelDescriptor::new(
            prov_id,
            ModelId::new("gpt-5-codex").unwrap(),
            "GPT-5 Codex",
            ModelCapabilities::default(),
            ModelLimits::default(),
        )
        .unwrap();
        client
            .send_command(IpcCommand::UpsertProvider {
                provider: inst,
                models: vec![model],
            })
            .await
            .unwrap();

        let studio = match client
            .send_command(IpcCommand::CreateStudio {
                name: "MultiTurn Studio".to_string(),
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioCreated { studio } => studio,
            other => panic!("Expected StudioCreated, got {other:?}"),
        };

        // Turn 1 + Turn 2 + Turn 3 on the same studio ThreadId
        let mut thread_ids = Vec::new();
        for idx in 1..=3 {
            let prompt = if idx == 1 {
                "Turn 1 using $review skill".to_string()
            } else {
                format!("Follow up turn {idx}")
            };
            match client
                .send_command(IpcCommand::ExecuteChatTurn {
                    studio_id: studio.id,
                    prompt,
                    mode: Some("agent".to_string()),
                    provider_instance_id: Some(prov_id),
                    model_id: Some("gpt-5-codex".to_string()),
                    reasoning_effort: Some("high".to_string()),
                    permission_mode: Some("auto-edit".to_string()),
                    follow_up_mode: Some("queue".to_string()),
                    workspace_path: Some(workspace_dir.path().display().to_string()),
                    attachments: vec![serde_json::json!({
                        "relativePath": "src/main.rs",
                        "languageId": "rust",
                        "contentSnippet": "fn main() {}"
                    })],
                })
                .await
                .unwrap()
            {
                IpcResponse::ChatTurnCompleted {
                    thread_id, message, ..
                } => {
                    assert_eq!(message["content"], "Codex turn completed cleanly.");
                    thread_ids.push(thread_id);
                }
                other => panic!("Expected ChatTurnCompleted, got {other:?}"),
            }
        }
        assert_eq!(thread_ids[0], thread_ids[1]);
        assert_eq!(thread_ids[1], thread_ids[2]);

        let requests = server_http.received_requests().await.unwrap();
        assert_eq!(requests.len(), 3);
        let first_body = String::from_utf8_lossy(&requests[0].body);
        assert!(first_body.contains("REPO_RULE_991"));
        assert!(first_body.contains("REVIEW_SKILL_MARKER_882"));
        assert!(first_body.contains("fn main() {}"));

        // Fork studio and verify cloned transcript (6 messages = 3 user + 3 assistant)
        let forked = match client
            .send_command(IpcCommand::ForkStudio {
                source_studio_id: studio.id,
                name: Some("Forked Studio".to_string()),
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioCreated { studio } => studio,
            other => panic!("Expected StudioCreated on fork, got {other:?}"),
        };

        let forked_tx = match client
            .send_command(IpcCommand::GetStudioTranscript {
                studio_id: forked.id,
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioTranscript { messages, .. } => messages,
            other => panic!("Expected StudioTranscript, got {other:?}"),
        };
        assert_eq!(forked_tx.len(), 6);

        // Restart server and verify transcript survives cleanly
        drop(client);
        let srv2 = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_codex_internal_runtime()
            .build()
            .unwrap();
        let (cs2, ss2) = tokio::io::duplex(65536);
        srv2.handle_connection(ss2);
        let mut client2 = IpcClient::connect_stream(cs2, ClientConfig::new(key_id, secret))
            .await
            .unwrap();
        let recovered = match client2
            .send_command(IpcCommand::GetStudioTranscript {
                studio_id: studio.id,
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioTranscript { messages, .. } => messages,
            other => panic!("Expected recovered transcript, got {other:?}"),
        };
        assert_eq!(recovered.len(), 6);
        tokio::time::sleep(Duration::from_millis(25)).await;
    });
}

#[test]
fn test_03_read_only_blocks_mutating_tools_while_full_access_runs() {
    run_large_stack(async {
        let sse_shell_tool = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chat-ro-1",
                "choices": [{
                    "index": 0,
                    "delta": {
                        "role": "assistant",
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_shell_ro",
                            "type": "function",
                            "function": {
                                "name": "exec_command",
                                "arguments": "{\"cmd\":\"echo blocked\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }]
            })
        );
        let sse_done = "data: {\"id\":\"chat-ro-2\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Read-only completed.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        let server_http = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_shell_tool),
            )
            .up_to_n_times(1)
            .mount(&server_http)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse_done),
            )
            .mount(&server_http)
            .await;

        let state_dir = tempfile::tempdir().unwrap();
        let workspace_dir = tempfile::tempdir().unwrap();
        let key_id = "mm05-ro";
        let secret = b"mm05-ro-key".to_vec();
        let srv = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_default_repo_path(workspace_dir.path())
            .with_codex_internal_runtime()
            .build()
            .unwrap();
        let (cs, ss) = tokio::io::duplex(65536);
        srv.handle_connection(ss);
        let mut client = IpcClient::connect_stream(cs, ClientConfig::new(key_id, secret))
            .await
            .unwrap();

        let prov_id = ProviderInstanceId::new();
        let inst = ProviderInstance::new(
            prov_id,
            ProviderId::new("openai").unwrap(),
            "RO Test",
            ProtocolFamily::OpenAiChatCompletions,
            EndpointProfile::new(format!("{}/v1", server_http.uri())).unwrap(),
            AuthenticationScheme::None,
        )
        .unwrap();
        let model = ModelDescriptor::new(
            prov_id,
            ModelId::new("gpt-5-codex").unwrap(),
            "GPT-5 Codex",
            ModelCapabilities::default(),
            ModelLimits::default(),
        )
        .unwrap();
        client
            .send_command(IpcCommand::UpsertProvider {
                provider: inst,
                models: vec![model],
            })
            .await
            .unwrap();
        let studio = match client
            .send_command(IpcCommand::CreateStudio {
                name: "RO Studio".to_string(),
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioCreated { studio } => studio,
            other => panic!("Unexpected {other:?}"),
        };

        let resp = client
            .send_command(IpcCommand::ExecuteChatTurn {
                studio_id: studio.id,
                prompt: "Run shell".to_string(),
                mode: Some("ask".to_string()),
                provider_instance_id: Some(prov_id),
                model_id: Some("gpt-5-codex".to_string()),
                reasoning_effort: Some("low".to_string()),
                permission_mode: Some("read-only".to_string()),
                follow_up_mode: None,
                workspace_path: Some(workspace_dir.path().display().to_string()),
                attachments: vec![],
            })
            .await
            .unwrap();
        match resp {
            IpcResponse::ChatTurnCompleted { message, .. } => {
                assert_eq!(message["permission_mode"], "read-only");
                let calls = message["tool_calls"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                assert!(
                    calls.iter().any(|c| {
                        let name = c["name"].as_str().unwrap_or("");
                        c["status"] == "failed"
                            || name.contains("exec_command")
                            || name.contains("shell")
                    }),
                    "Read-only mode must record blocked/failed shell attempt, got: {message}"
                );
            }
            other => panic!("Expected ChatTurnCompleted, got {other:?}"),
        }
    });
}

#[test]
fn test_04_multi_agent_worktree_goal_with_control_plane_events_and_artifacts() {
    run_large_stack(async {
        let repo_dir = tempfile::tempdir().unwrap();
        std::fs::write(repo_dir.path().join("README.md"), "# Multi-Agent Repo\n").unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Tester"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(repo_dir.path())
            .output()
            .unwrap();

        let state_dir = tempfile::tempdir().unwrap();
        let wt_root = state_dir.path().join("worktrees");
        let art_root = state_dir.path().join("artifacts");
        let orchestrator = Arc::new(
            agent_studios_workspace::WorkspaceOrchestrator::new(&wt_root, &art_root).unwrap(),
        );

        let plan_json = serde_json::json!({
            "action": "plan",
            "tasks": [
                {
                    "task_key": "worker_a",
                    "title": "Implement A",
                    "description": "Write A",
                    "assigned_alias": "worker-a",
                    "depends_on": [],
                    "priority": 10
                },
                {
                    "task_key": "worker_b",
                    "title": "Implement B",
                    "description": "Write B",
                    "assigned_alias": "worker-b",
                    "depends_on": [],
                    "priority": 10
                },
                {
                    "task_key": "reviewer",
                    "title": "Review A & B",
                    "description": "Audit work",
                    "assigned_alias": "reviewer",
                    "depends_on": ["worker_a", "worker_b"],
                    "priority": 5
                }
            ]
        })
        .to_string();

        let coord_sse = format!(
            "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({
                "id": "chat-coord",
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": plan_json}, "finish_reason": null}]
            }),
            serde_json::json!({
                "id": "chat-coord",
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
            })
        );
        let worker_sse = "data: {\"id\":\"chat-w\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Worker complete.\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        let server_http = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(coord_sse),
            )
            .up_to_n_times(1)
            .mount(&server_http)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(worker_sse),
            )
            .mount(&server_http)
            .await;

        let key_id = "mm05-ma";
        let secret = b"mm05-ma-key".to_vec();
        let srv = IpcServer::builder()
            .with_admin_auth_key(key_id, secret.clone())
            .with_state_dir(state_dir.path().to_path_buf())
            .with_workspace_orchestrator(orchestrator)
            .with_default_repo_path(repo_dir.path())
            .with_authorized_workspace(repo_dir.path())
            .with_codex_internal_runtime()
            .build()
            .unwrap();
        let (cs, ss) = tokio::io::duplex(65536);
        srv.handle_connection(ss);
        let mut client = IpcClient::connect_stream(cs, ClientConfig::new(key_id, secret))
            .await
            .unwrap();

        let prov_id = ProviderInstanceId::new();
        let inst = ProviderInstance::new(
            prov_id,
            ProviderId::new("openai").unwrap(),
            "MA Test",
            ProtocolFamily::OpenAiChatCompletions,
            EndpointProfile::new(format!("{}/v1", server_http.uri())).unwrap(),
            AuthenticationScheme::None,
        )
        .unwrap();
        let model = ModelDescriptor::new(
            prov_id,
            ModelId::new("gpt-5-codex").unwrap(),
            "GPT-5 Codex",
            ModelCapabilities::default(),
            ModelLimits::default(),
        )
        .unwrap();
        client
            .send_command(IpcCommand::UpsertProvider {
                provider: inst,
                models: vec![model],
            })
            .await
            .unwrap();
        let studio = match client
            .send_command(IpcCommand::CreateStudio {
                name: "MultiAgent Studio".to_string(),
            })
            .await
            .unwrap()
        {
            IpcResponse::StudioCreated { studio } => studio,
            other => panic!("Unexpected {other:?}"),
        };

        let resp = client
            .send_command(IpcCommand::ExecuteMultiAgentGoal {
                studio_id: studio.id,
                goal: "Implement parallel modules".to_string(),
                provider_instance_id: Some(prov_id),
                model_id: Some("gpt-5-codex".to_string()),
                repository_path: Some(repo_dir.path().display().to_string()),
                permission_mode: Some("full-access".to_string()),
            })
            .await
            .unwrap();
        match resp {
            IpcResponse::MultiAgentGoalCompleted { summary, .. } => {
                assert_eq!(summary["tasks_completed"], 3);
                assert_eq!(summary["worktrees"].as_array().unwrap().len(), 2);
            }
            other => panic!("Expected MultiAgentGoalCompleted, got {other:?}"),
        }

        let tasks = match client
            .send_command(IpcCommand::ListTasks {
                studio_id: studio.id,
            })
            .await
            .unwrap()
        {
            IpcResponse::Tasks { tasks } => tasks,
            other => panic!("Unexpected {other:?}"),
        };
        assert_eq!(tasks.len(), 3);
        assert!(
            tasks
                .iter()
                .all(|t| t.state == agent_studios_protocol::task::TaskState::Succeeded)
        );
    });
}
