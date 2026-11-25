use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use agent_studios_ipc_protocol::{AuthBinding, HmacSigner, ProtocolVersion};
use agent_studios_ipc_transport::connection::{IpcCommand, IpcResponse};
use agent_studios_ipc_transport::framing::{DEFAULT_MAX_FRAME_SIZE, recv_msg, send_msg};
use agent_studios_ipc_transport::server::IpcServer;
use agent_studios_provider::{
    AuthenticationScheme, EndpointProfile, ProtocolFamily, ProviderId, ProviderInstance,
    ProviderInstanceId,
};

#[tokio::test]
async fn test_extension_first_ipc_provider_discovery_fork_and_extensibility() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Local zero-billable mock /v1/models HTTP server
    tokio::spawn(async move {
        for _ in 0..4 {
            if let Ok((mut sock, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = sock.read(&mut buf).await;
                    let body = serde_json::json!({
                        "object": "list",
                        "data": [
                            { "id": "mock-gpt-4.1", "owned_by": "openai" },
                            { "id": "mock-o4-mini", "owned_by": "openai" }
                        ]
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        }
    });

    let workspace = tempfile::tempdir().unwrap();
    fs::write(workspace.path().join("AGENTS.md"), "# Workspace Rules\n").unwrap();
    let skill_dir = workspace
        .path()
        .join(".agent-studios")
        .join("skills")
        .join("audit");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: audit\ndescription: Audit repo\n---\nRun audit.\n",
    )
    .unwrap();

    let key = b"test-secret-key-32-bytes-minimum";
    let server = IpcServer::builder()
        .with_auth_key("ext-client", key)
        .with_default_repo_path(workspace.path())
        .build()
        .unwrap();

    let (client_stream, server_stream) = tokio::io::duplex(256 * 1024);
    let _server_task = server.handle_connection(server_stream);

    let (mut reader, mut writer) = tokio::io::split(client_stream);

    // 1. Negotiate & Authenticate
    send_msg(
        &mut writer,
        &IpcCommand::NegotiateCapabilities {
            protocol_version: ProtocolVersion::CURRENT,
            capabilities: vec!["event_streaming".into()],
        },
    )
    .await
    .unwrap();
    let agreed = match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::CapabilitiesNegotiated {
            agreed_capabilities,
            ..
        } => agreed_capabilities,
        other => panic!("unexpected negotiate response: {other:?}"),
    };

    let session_id = agent_studios_ipc_protocol::SessionId::generate();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let caps = if agreed.is_empty() {
        None
    } else {
        Some(agreed)
    };
    let binding = AuthBinding::new(
        "ext-client",
        session_id,
        "nonce-001",
        now_ms,
        b"",
        ProtocolVersion::CURRENT,
        caps,
    );
    let header = HmacSigner::sign(key, &binding).unwrap();
    send_msg(
        &mut writer,
        &IpcCommand::Authenticate {
            header,
            payload: None,
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::Authenticated { .. } => {}
        other => panic!("unexpected authenticate response: {other:?}"),
    }

    // 2. Upsert LiteLLM Provider & run real HTTP dynamic /v1/models discovery
    let inst_id = ProviderInstanceId::new();
    let inst = ProviderInstance::new(
        inst_id,
        ProviderId::new("litellm").unwrap(),
        "Local Mock LiteLLM",
        ProtocolFamily::OpenAiChatCompletions,
        EndpointProfile::new(format!("http://{addr}/v1")).unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    send_msg(
        &mut writer,
        &IpcCommand::UpsertProvider {
            provider: inst,
            models: vec![],
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::ProviderUpserted { provider } => assert_eq!(provider.id, inst_id),
        other => panic!("unexpected response: {other:?}"),
    }

    send_msg(
        &mut writer,
        &IpcCommand::DiscoverModels {
            provider_id: inst_id,
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::ModelsDiscovered {
            provider_id,
            models,
        } => {
            assert_eq!(provider_id, inst_id);
            assert_eq!(models.len(), 2);
            assert_eq!(models[0].id.as_str(), "mock-gpt-4.1");
        }
        other => panic!("unexpected discovery response: {other:?}"),
    }

    send_msg(
        &mut writer,
        &IpcCommand::TestProviderConnection {
            provider_id: inst_id,
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::ProviderConnectionTested { report } => {
            assert_eq!(report.instance_id, inst_id);
            assert_eq!(
                report.discovery.state,
                agent_studios_provider::CheckState::Passed
            );
        }
        other => panic!("unexpected connection test response: {other:?}"),
    }

    // 3. CreateStudio + ForkStudio
    send_msg(
        &mut writer,
        &IpcCommand::CreateStudio {
            name: "Main Session".into(),
        },
    )
    .await
    .unwrap();
    let source_id = match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::StudioCreated { studio } => studio.id,
        other => panic!("unexpected create studio response: {other:?}"),
    };

    send_msg(
        &mut writer,
        &IpcCommand::ForkStudio {
            source_studio_id: source_id,
            name: None,
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::StudioCreated { studio } => {
            assert_ne!(studio.id, source_id);
            assert_eq!(studio.name, "Main Session (Fork)");
        }
        other => panic!("unexpected fork studio response: {other:?}"),
    }

    // 4. ListExtensibility returns workspace skills + AGENTS.md
    send_msg(
        &mut writer,
        &IpcCommand::ListExtensibility {
            workspace_root: Some(workspace.path().to_string_lossy().into_owned()),
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::ExtensibilityListed {
            skills,
            instructions,
            ..
        } => {
            assert!(skills.iter().any(|s| s["name"] == "audit"));
            assert!(!instructions.is_empty());
        }
        other => panic!("unexpected extensibility response: {other:?}"),
    }

    // 5. RemoveProvider cleans up cascade
    send_msg(
        &mut writer,
        &IpcCommand::RemoveProvider {
            provider_id: inst_id,
        },
    )
    .await
    .unwrap();
    match recv_msg::<_, IpcResponse>(&mut reader, DEFAULT_MAX_FRAME_SIZE)
        .await
        .unwrap()
    {
        IpcResponse::ProviderRemoved {
            provider_id,
            removed,
        } => {
            assert_eq!(provider_id, inst_id);
            assert!(removed);
        }
        other => panic!("unexpected remove response: {other:?}"),
    }
}
