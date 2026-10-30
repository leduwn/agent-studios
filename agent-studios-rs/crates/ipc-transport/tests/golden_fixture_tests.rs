//! Golden Fixture Tests for Agent Studios M13 IPC Protocol
//!
//! Validates serialization, deserialization, and schema fidelity against
//! the canonical Rust implementation.

use agent_studios_ipc_protocol::{
    ApprovalDecision, ApprovalId, ApprovalStatus, ExecutionDefaultsReadiness,
    ExecutionReadinessReport, HmacSigner, IpcEvent, ModelReadiness, ProtocolVersion,
    RuntimeInstanceReadiness, SessionId,
};
use agent_studios_ipc_transport::{
    DaemonAdminBootstrapInfo, DaemonBootstrapInfo, IpcCommand, IpcResponse,
};
use agent_studios_protocol::cancellation::CancellationSummary;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{
    BatchTaskSpec, DependencyOutputPolicy, TaskExecutionSpec, TaskRecord, TaskState,
};
use agent_studios_provider::ProviderInstance;
use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::capabilities::ModelCapabilities;
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ModelId, ProviderId, ProviderInstanceId};
use agent_studios_provider::model::{ModelDescriptor, ModelLimits};
use agent_studios_provider::protocol::ProtocolFamily;
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};

#[test]
fn test_fixture_negotiate_capabilities_roundtrip() {
    let cmd = IpcCommand::NegotiateCapabilities {
        protocol_version: ProtocolVersion::new(1, 0, 0),
        capabilities: vec![
            "event_streaming".to_string(),
            "linearizable_replay".to_string(),
        ],
    };

    let json_str = serde_json::to_string_pretty(&cmd).expect("serialize NegotiateCapabilities");
    let val: Value = serde_json::from_str(&json_str).expect("parse json");

    assert_eq!(val["type"], "negotiate_capabilities");
    assert_eq!(val["protocol_version"]["major"], 1);
    assert_eq!(val["protocol_version"]["minor"], 0);
    assert_eq!(val["protocol_version"]["patch"], 0);
    assert_eq!(
        val["capabilities"],
        json!(["event_streaming", "linearizable_replay"])
    );

    let decoded: IpcCommand = serde_json::from_str(&json_str).expect("deserialize command");
    assert_eq!(cmd, decoded);

    let resp = IpcResponse::CapabilitiesNegotiated {
        selected_version: ProtocolVersion::new(1, 0, 0),
        agreed_capabilities: vec![
            "event_streaming".to_string(),
            "linearizable_replay".to_string(),
        ],
    };

    let resp_json = serde_json::to_string_pretty(&resp).expect("serialize CapabilitiesNegotiated");
    let resp_val: Value = serde_json::from_str(&resp_json).expect("parse json");

    assert_eq!(resp_val["type"], "capabilities_negotiated");
    assert_eq!(resp_val["selected_version"]["major"], 1);
    assert_eq!(
        resp_val["agreed_capabilities"],
        json!(["event_streaming", "linearizable_replay"])
    );
}

#[test]
fn test_fixture_hmac_canonical_digest_and_auth_header() {
    let secret = b"super-secret-key-1234567890abcdef";
    let version = ProtocolVersion::new(1, 0, 0);
    let key_id = "test-client-1";
    let session_id = SessionId::new("sess-9a7c0000-0000-0000-0000-000000000001");
    let nonce = "nonce-abcd-1234";
    let timestamp_millis = 1793400000000_i64;
    let payload = b"";
    let capabilities = vec![
        "event_streaming".to_string(),
        "linearizable_replay".to_string(),
    ];

    let digest_bytes = HmacSigner::compute_canonical_digest(
        &version,
        key_id,
        &session_id,
        nonce,
        timestamp_millis,
        Some(&capabilities),
        payload,
    );
    let digest_str = String::from_utf8(digest_bytes).expect("utf8 digest");

    // Format: {protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}
    let expected_payload_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // sha256 of empty bytes
    let expected_digest = format!(
        "1.0.0:{key_id}:{session_id}:{nonce}:{timestamp_millis}:event_streaming,linearizable_replay:{expected_payload_hash}"
    );
    assert_eq!(digest_str, expected_digest);

    let header = HmacSigner::sign_parts(
        secret,
        version,
        key_id,
        &session_id,
        nonce,
        timestamp_millis,
        payload,
        Some(capabilities),
    )
    .expect("sign header");

    let auth_cmd = IpcCommand::Authenticate {
        header: header.clone(),
        payload: None,
    };

    let auth_json = serde_json::to_string_pretty(&auth_cmd).expect("serialize Authenticate");
    let auth_val: Value = serde_json::from_str(&auth_json).expect("parse json");

    assert_eq!(auth_val["type"], "authenticate");
    assert_eq!(auth_val["header"]["key_id"], key_id);
    assert_eq!(auth_val["header"]["session_id"], session_id.to_string());
    assert_eq!(auth_val["header"]["nonce"], nonce);
    assert_eq!(auth_val["header"]["timestamp_millis"], timestamp_millis);
    assert_eq!(auth_val["header"]["signature"], header.signature);

    let auth_resp = IpcResponse::Authenticated {
        session_id: session_id.clone(),
    };
    let auth_resp_json = serde_json::to_string_pretty(&auth_resp).expect("serialize Authenticated");
    let auth_resp_val: Value = serde_json::from_str(&auth_resp_json).expect("parse json");

    assert_eq!(auth_resp_val["type"], "authenticated");
    assert_eq!(auth_resp_val["session_id"], session_id.to_string());
}

#[test]
fn test_fixture_ping_pong_health() {
    let ping = IpcCommand::Ping {
        payload: "heartbeat".to_string(),
        timestamp: 1793400000000,
    };
    let ping_json = serde_json::to_string_pretty(&ping).expect("serialize Ping");
    let ping_val: Value = serde_json::from_str(&ping_json).expect("parse json");
    assert_eq!(ping_val["type"], "ping");
    assert_eq!(ping_val["payload"], "heartbeat");
    assert_eq!(ping_val["timestamp"], 1793400000000_i64);

    let pong = IpcResponse::Pong {
        payload: "heartbeat".to_string(),
        client_timestamp: 1793400000000,
        server_timestamp: 1793400000005,
    };
    let pong_json = serde_json::to_string_pretty(&pong).expect("serialize Pong");
    let pong_val: Value = serde_json::from_str(&pong_json).expect("parse json");
    assert_eq!(pong_val["type"], "pong");
    assert_eq!(pong_val["payload"], "heartbeat");
    assert_eq!(pong_val["client_timestamp"], 1793400000000_i64);
    assert_eq!(pong_val["server_timestamp"], 1793400000005_i64);

    let health_cmd = IpcCommand::GetHealth;
    let health_json = serde_json::to_string(&health_cmd).expect("serialize GetHealth");
    assert_eq!(health_json, r#"{"type":"get_health"}"#);

    let health_resp = IpcResponse::Health {
        status: "healthy".to_string(),
        active_clients: 2,
        uptime_secs: 7200,
        memory_events_count: 150,
    };
    let health_resp_json = serde_json::to_string_pretty(&health_resp).expect("serialize Health");
    let health_resp_val: Value = serde_json::from_str(&health_resp_json).expect("parse json");
    assert_eq!(health_resp_val["type"], "health");
    assert_eq!(health_resp_val["status"], "healthy");
    assert_eq!(health_resp_val["active_clients"], 2);
    assert_eq!(health_resp_val["uptime_secs"], 7200);
}

#[test]
fn test_fixture_studios_lifecycle() {
    let create_cmd = IpcCommand::CreateStudio {
        name: "Studio Epsilon".to_string(),
    };
    let create_json = serde_json::to_string_pretty(&create_cmd).expect("serialize CreateStudio");
    let create_val: Value = serde_json::from_str(&create_json).expect("parse json");
    assert_eq!(create_val["type"], "create_studio");
    assert_eq!(create_val["name"], "Studio Epsilon");

    let studio_id = StudioId::new();
    let now = Utc.timestamp_opt(1793400000, 0).unwrap();
    let studio = Studio::with_id(studio_id, "Studio Epsilon", now);

    let resp_created = IpcResponse::StudioCreated {
        studio: studio.clone(),
    };
    let resp_created_json =
        serde_json::to_string_pretty(&resp_created).expect("serialize StudioCreated");
    let resp_created_val: Value = serde_json::from_str(&resp_created_json).expect("parse json");
    assert_eq!(resp_created_val["type"], "studio_created");
    assert_eq!(resp_created_val["studio"]["id"], studio_id.to_string());
    assert_eq!(resp_created_val["studio"]["name"], "Studio Epsilon");

    let list_cmd = IpcCommand::ListStudios;
    let list_json = serde_json::to_string(&list_cmd).expect("serialize ListStudios");
    assert_eq!(list_json, r#"{"type":"list_studios"}"#);

    let list_resp = IpcResponse::Studios {
        studios: vec![studio.clone()],
    };
    let list_resp_json = serde_json::to_string_pretty(&list_resp).expect("serialize Studios");
    let list_resp_val: Value = serde_json::from_str(&list_resp_json).expect("parse json");
    assert_eq!(list_resp_val["type"], "studios");
    assert_eq!(list_resp_val["studios"].as_array().unwrap().len(), 1);

    let get_cmd = IpcCommand::GetStudio { studio_id };
    let get_json = serde_json::to_string_pretty(&get_cmd).expect("serialize GetStudio");
    let get_val: Value = serde_json::from_str(&get_json).expect("parse json");
    assert_eq!(get_val["type"], "get_studio");
    assert_eq!(get_val["studio_id"], studio_id.to_string());

    let get_resp = IpcResponse::Studio {
        studio: Some(studio),
    };
    let get_resp_json = serde_json::to_string_pretty(&get_resp).expect("serialize Studio");
    let get_resp_val: Value = serde_json::from_str(&get_resp_json).expect("parse json");
    assert_eq!(get_resp_val["type"], "studio");
    assert_eq!(get_resp_val["studio"]["id"], studio_id.to_string());
}

#[test]
fn test_fixture_task_lifecycle_submit_and_cancel() {
    let studio_id = StudioId::new();
    let task_spec = BatchTaskSpec {
        key: "task-test-spec".to_string(),
        title: "Test Task Spec".to_string(),
        description: "Verify golden fixture serialization".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: None,
    };

    let submit_cmd = IpcCommand::SubmitTask {
        studio_id,
        task_spec,
    };
    let submit_json = serde_json::to_string_pretty(&submit_cmd).expect("serialize SubmitTask");
    let submit_val: Value = serde_json::from_str(&submit_json).expect("parse json");

    assert_eq!(submit_val["type"], "submit_task");
    assert_eq!(submit_val["studio_id"], studio_id.to_string());
    assert_eq!(submit_val["task_spec"]["key"], "task-test-spec");
    assert_eq!(submit_val["task_spec"]["title"], "Test Task Spec");
    assert_eq!(
        submit_val["task_spec"]["dependency_output_policy"],
        "task_success"
    );

    let task_id = TaskId::new();
    let submit_resp = IpcResponse::TasksSubmitted {
        task_ids: vec![task_id],
    };
    let submit_resp_json =
        serde_json::to_string_pretty(&submit_resp).expect("serialize TasksSubmitted");
    let submit_resp_val: Value = serde_json::from_str(&submit_resp_json).expect("parse json");
    assert_eq!(submit_resp_val["type"], "tasks_submitted");
    assert_eq!(submit_resp_val["task_ids"], json!([task_id.to_string()]));

    let cancel_cmd = IpcCommand::CancelTask {
        studio_id,
        task_id,
        reason: Some("Manual abort by operator".to_string()),
    };
    let cancel_json = serde_json::to_string_pretty(&cancel_cmd).expect("serialize CancelTask");
    let cancel_val: Value = serde_json::from_str(&cancel_json).expect("parse json");
    assert_eq!(cancel_val["type"], "cancel_task");
    assert_eq!(cancel_val["studio_id"], studio_id.to_string());
    assert_eq!(cancel_val["task_id"], task_id.to_string());
    assert_eq!(cancel_val["reason"], "Manual abort by operator");

    let cancel_resp = IpcResponse::TaskCancelled {
        summary: CancellationSummary {
            cancelled_tasks: vec![task_id],
            cancelled_runs: vec![RunId::new()],
            cancelled_approvals: vec![],
        },
    };
    let cancel_resp_json =
        serde_json::to_string_pretty(&cancel_resp).expect("serialize TaskCancelled");
    let cancel_resp_val: Value = serde_json::from_str(&cancel_resp_json).expect("parse json");
    assert_eq!(cancel_resp_val["type"], "task_cancelled");
    assert_eq!(
        cancel_resp_val["summary"]["cancelled_tasks"],
        json!([task_id.to_string()])
    );
    assert_eq!(
        cancel_resp_val["summary"]["cancelled_runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let list_tasks_cmd = IpcCommand::ListTasks { studio_id };
    let list_tasks_json =
        serde_json::to_string_pretty(&list_tasks_cmd).expect("serialize ListTasks");
    let list_tasks_val: Value = serde_json::from_str(&list_tasks_json).expect("parse json");
    assert_eq!(list_tasks_val["type"], "list_tasks");
    assert_eq!(list_tasks_val["studio_id"], studio_id.to_string());

    let now = Utc.timestamp_opt(1793400000, 0).unwrap();
    let task_record = TaskRecord {
        id: task_id,
        studio_id,
        parent_task_id: None,
        assigned_agent_id: None,
        title: "Test Task Spec".to_string(),
        description: "Verify golden fixture serialization".to_string(),
        state: TaskState::Ready,
        dependencies: vec![],
        created_at: now,
        updated_at: now,
        retry_count: 0,
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: None,
    };

    let tasks_resp = IpcResponse::Tasks {
        tasks: vec![task_record.clone()],
    };
    let tasks_resp_json = serde_json::to_string_pretty(&tasks_resp).expect("serialize Tasks");
    let tasks_resp_val: Value = serde_json::from_str(&tasks_resp_json).expect("parse json");
    assert_eq!(tasks_resp_val["type"], "tasks");
    assert_eq!(tasks_resp_val["tasks"].as_array().unwrap().len(), 1);

    let get_task_cmd = IpcCommand::GetTask { studio_id, task_id };
    let get_task_json = serde_json::to_string_pretty(&get_task_cmd).expect("serialize GetTask");
    let get_task_val: Value = serde_json::from_str(&get_task_json).expect("parse json");
    assert_eq!(get_task_val["type"], "get_task");

    let task_resp = IpcResponse::Task {
        task: Some(task_record),
    };
    let task_resp_json = serde_json::to_string_pretty(&task_resp).expect("serialize Task");
    let task_resp_val: Value = serde_json::from_str(&task_resp_json).expect("parse json");
    assert_eq!(task_resp_val["type"], "task");
    assert_eq!(task_resp_val["task"]["id"], task_id.to_string());
}

#[test]
fn test_fixture_runs_lifecycle() {
    let studio_id = StudioId::new();
    let task_id = TaskId::new();
    let run_id = RunId::new();
    let agent_id = AgentId::new();

    let list_runs_cmd = IpcCommand::ListRuns { studio_id };
    let list_runs_json = serde_json::to_string_pretty(&list_runs_cmd).expect("serialize ListRuns");
    let list_runs_val: Value = serde_json::from_str(&list_runs_json).expect("parse json");
    assert_eq!(list_runs_val["type"], "list_runs");
    assert_eq!(list_runs_val["studio_id"], studio_id.to_string());

    let run_record = RunRecord {
        id: run_id,
        task_id,
        agent_id,
        attempt: 1,
        state: RunState::Queued,
        started_at: None,
        finished_at: None,
        execution: None,
    };

    let runs_resp = IpcResponse::Runs {
        runs: vec![run_record.clone()],
    };
    let runs_resp_json = serde_json::to_string_pretty(&runs_resp).expect("serialize Runs");
    let runs_resp_val: Value = serde_json::from_str(&runs_resp_json).expect("parse json");
    assert_eq!(runs_resp_val["type"], "runs");
    assert_eq!(runs_resp_val["runs"].as_array().unwrap().len(), 1);

    let get_run_cmd = IpcCommand::GetRun { studio_id, run_id };
    let get_run_json = serde_json::to_string_pretty(&get_run_cmd).expect("serialize GetRun");
    let get_run_val: Value = serde_json::from_str(&get_run_json).expect("parse json");
    assert_eq!(get_run_val["type"], "get_run");

    let run_resp = IpcResponse::Run {
        run: Some(run_record),
    };
    let run_resp_json = serde_json::to_string_pretty(&run_resp).expect("serialize Run");
    let run_resp_val: Value = serde_json::from_str(&run_resp_json).expect("parse json");
    assert_eq!(run_resp_val["type"], "run");
    assert_eq!(run_resp_val["run"]["id"], run_id.to_string());
}

#[test]
fn test_fixture_providers_and_models() {
    let list_prov_cmd = IpcCommand::ListProviders;
    let list_prov_json = serde_json::to_string(&list_prov_cmd).expect("serialize ListProviders");
    assert_eq!(list_prov_json, r#"{"type":"list_providers"}"#);

    let inst_id = ProviderInstanceId::new();
    let prov_id = ProviderId::new("openai").unwrap();
    let provider_inst = ProviderInstance::new(
        inst_id,
        prov_id,
        "OpenAI Instance",
        ProtocolFamily::OpenAiResponses,
        EndpointProfile::new("https://api.openai.com/v1").unwrap(),
        AuthenticationScheme::None,
    )
    .unwrap();

    let provs_resp = IpcResponse::Providers {
        providers: vec![provider_inst.clone()],
    };
    let provs_resp_json = serde_json::to_string_pretty(&provs_resp).expect("serialize Providers");
    let provs_resp_val: Value = serde_json::from_str(&provs_resp_json).expect("parse json");
    assert_eq!(provs_resp_val["type"], "providers");
    assert_eq!(provs_resp_val["providers"].as_array().unwrap().len(), 1);

    let get_prov_cmd = IpcCommand::GetProvider {
        provider_id: inst_id,
    };
    let get_prov_json = serde_json::to_string_pretty(&get_prov_cmd).expect("serialize GetProvider");
    let get_prov_val: Value = serde_json::from_str(&get_prov_json).expect("parse json");
    assert_eq!(get_prov_val["type"], "get_provider");

    let prov_resp = IpcResponse::Provider {
        provider: Some(provider_inst),
    };
    let prov_resp_json = serde_json::to_string_pretty(&prov_resp).expect("serialize Provider");
    let prov_resp_val: Value = serde_json::from_str(&prov_resp_json).expect("parse json");
    assert_eq!(prov_resp_val["type"], "provider");

    let list_models_cmd = IpcCommand::ListModels {
        instance_id: Some(inst_id),
    };
    let list_models_json =
        serde_json::to_string_pretty(&list_models_cmd).expect("serialize ListModels");
    let list_models_val: Value = serde_json::from_str(&list_models_json).expect("parse json");
    assert_eq!(list_models_val["type"], "list_models");
    assert_eq!(list_models_val["instance_id"], inst_id.to_string());

    let model_desc = ModelDescriptor::new(
        inst_id,
        ModelId::new("gpt-4o-mini").unwrap(),
        "GPT-4o Mini",
        ModelCapabilities::unknown(),
        ModelLimits::default(),
    )
    .unwrap();

    let models_resp = IpcResponse::Models {
        models: vec![model_desc],
    };
    let models_resp_json = serde_json::to_string_pretty(&models_resp).expect("serialize Models");
    let models_resp_val: Value = serde_json::from_str(&models_resp_json).expect("parse json");
    assert_eq!(models_resp_val["type"], "models");
    assert_eq!(models_resp_val["models"].as_array().unwrap().len(), 1);
}

#[test]
fn test_fixture_approvals_lifecycle() {
    let approval_id = ApprovalId::generate();

    let resolve_approved = IpcCommand::ResolveApproval {
        approval_id: approval_id.clone(),
        decision: ApprovalDecision::Approved {
            modified_details: None,
            user_note: Some("Operator confirmed action".to_string()),
        },
    };
    let res_app_json =
        serde_json::to_string_pretty(&resolve_approved).expect("serialize ResolveApproval");
    let res_app_val: Value = serde_json::from_str(&res_app_json).expect("parse json");

    assert_eq!(res_app_val["type"], "resolve_approval");
    assert_eq!(res_app_val["approval_id"], approval_id.to_string());
    assert_eq!(res_app_val["decision"]["decision"], "approved");
    assert_eq!(
        res_app_val["decision"]["user_note"],
        "Operator confirmed action"
    );

    let resolve_rejected = IpcCommand::ResolveApproval {
        approval_id: approval_id.clone(),
        decision: ApprovalDecision::Rejected {
            reason: "Access denied by security policy".to_string(),
        },
    };
    let res_rej_json = serde_json::to_string_pretty(&resolve_rejected)
        .expect("serialize ResolveApproval rejected");
    let res_rej_val: Value = serde_json::from_str(&res_rej_json).expect("parse json");
    assert_eq!(res_rej_val["decision"]["decision"], "rejected");
    assert_eq!(
        res_rej_val["decision"]["reason"],
        "Access denied by security policy"
    );

    let resp_resolved = IpcResponse::ApprovalResolved {
        approval_id: approval_id.clone(),
        status: ApprovalStatus::Authorized,
    };
    let resp_res_json =
        serde_json::to_string_pretty(&resp_resolved).expect("serialize ApprovalResolved");
    let resp_res_val: Value = serde_json::from_str(&resp_res_json).expect("parse json");
    assert_eq!(resp_res_val["type"], "approval_resolved");
    assert_eq!(resp_res_val["approval_id"], approval_id.to_string());
    assert_eq!(resp_res_val["status"], "authorized");
}

#[test]
fn test_fixture_events_subscription() {
    let sub_cmd = IpcCommand::SubscribeEvents {
        from_sequence: Some(42),
    };
    let sub_json = serde_json::to_string_pretty(&sub_cmd).expect("serialize SubscribeEvents");
    let sub_val: Value = serde_json::from_str(&sub_json).expect("parse json");
    assert_eq!(sub_val["type"], "subscribe_events");
    assert_eq!(sub_val["from_sequence"], 42);

    let now = Utc.timestamp_opt(1793400000, 0).unwrap();
    let event = IpcEvent {
        sequence: 42,
        timestamp: now,
        event_type: "task_state_transitioned".to_string(),
        payload: json!({
            "task_id": "c0a80101-0000-0000-0000-000000000001",
            "from": "pending",
            "to": "ready"
        }),
    };
    let event_resp = IpcResponse::Event { event };
    let event_resp_json = serde_json::to_string_pretty(&event_resp).expect("serialize Event");
    let event_resp_val: Value = serde_json::from_str(&event_resp_json).expect("parse json");

    assert_eq!(event_resp_val["type"], "event");
    assert_eq!(event_resp_val["event"]["sequence"], 42);
    assert_eq!(
        event_resp_val["event"]["event_type"],
        "task_state_transitioned"
    );
    assert_eq!(
        event_resp_val["event"]["payload"]["task_id"],
        "c0a80101-0000-0000-0000-000000000001"
    );
}

#[test]
fn test_fixture_error_response() {
    let err_resp = IpcResponse::Error {
        message: "Permission denied: cross-session cancellation requires admin role".to_string(),
    };
    let err_json = serde_json::to_string_pretty(&err_resp).expect("serialize Error");
    let err_val: Value = serde_json::from_str(&err_json).expect("parse json");
    assert_eq!(err_val["type"], "error");
    assert_eq!(
        err_val["message"],
        "Permission denied: cross-session cancellation requires admin role"
    );
}

#[test]
fn test_fixture_execution_readiness_roundtrip() {
    let cmd = IpcCommand::GetExecutionReadiness;
    let cmd_json = serde_json::to_string_pretty(&cmd).expect("serialize GetExecutionReadiness");
    let cmd_val: Value = serde_json::from_str(&cmd_json).expect("parse json");
    assert_eq!(cmd_val["type"], "get_execution_readiness");

    let report = ExecutionReadinessReport {
        is_ready: true,
        available_runtimes: vec!["claude-code".to_string()],
        runtime_instances: vec![RuntimeInstanceReadiness {
            instance_id: "rt-inst-c0a80101-0000-0000-0000-000000000001".to_string(),
            implementation: "claude-code".to_string(),
            healthy: true,
        }],
        available_models: vec![ModelReadiness {
            provider_instance_id: "c0a80101-0000-0000-0000-000000000002".to_string(),
            model_id: "claude-3-5-sonnet".to_string(),
            display_name: Some("Claude 3.5 Sonnet".to_string()),
        }],
        authorized_workspaces: vec!["/path/to/repo".to_string()],
        defaults: ExecutionDefaultsReadiness {
            default_repo_path: Some("/path/to/repo".to_string()),
            default_provider_instance_id: Some("c0a80101-0000-0000-0000-000000000002".to_string()),
            default_model_id: Some("claude-3-5-sonnet".to_string()),
            configured_instance_id: Some(
                "rt-inst-c0a80101-0000-0000-0000-000000000001".to_string(),
            ),
        },
        blocking_reasons: vec![],
    };

    let resp = IpcResponse::ExecutionReadiness { readiness: report };
    let resp_json = serde_json::to_string_pretty(&resp).expect("serialize ExecutionReadiness");
    let resp_val: Value = serde_json::from_str(&resp_json).expect("parse json");
    assert_eq!(resp_val["type"], "execution_readiness");
    assert_eq!(resp_val["readiness"]["is_ready"], true);
    assert_eq!(
        resp_val["readiness"]["runtime_instances"][0]["implementation"],
        "claude-code"
    );
    assert_eq!(
        resp_val["readiness"]["available_models"][0]["model_id"],
        "claude-3-5-sonnet"
    );
    assert_eq!(
        resp_val["readiness"]["authorized_workspaces"][0],
        "/path/to/repo"
    );
    assert_eq!(
        resp_val["readiness"]["defaults"]["default_model_id"],
        "claude-3-5-sonnet"
    );

    // Verify roundtrip deserialization
    let deserialized_resp: IpcResponse =
        serde_json::from_str(&resp_json).expect("deserialize ExecutionReadiness");
    match deserialized_resp {
        IpcResponse::ExecutionReadiness { readiness: r } => {
            assert!(r.is_ready);
            assert_eq!(r.runtime_instances.len(), 1);
            assert_eq!(r.available_models.len(), 1);
            assert_eq!(r.authorized_workspaces.len(), 1);
            assert_eq!(
                r.defaults.default_model_id.as_deref(),
                Some("claude-3-5-sonnet")
            );
        }
        other => panic!("Unexpected response type: {:?}", other),
    }
}

#[test]
fn test_fixture_task_execution_spec_roundtrip() {
    let studio_id = StudioId::new();
    let exec_spec = TaskExecutionSpec {
        initial_prompt: Some("Solve bug in auth middleware".to_string()),
        repository_path: Some("/path/to/auth-repo".to_string()),
        runtime_instance_id: Some("rt-inst-test-01".to_string()),
        runtime_preference: Some("claude-code".to_string()),
        provider_instance_id: Some("prov-test-01".to_string()),
        model_id: Some("claude-3-7-sonnet".to_string()),
    };

    let task_spec = BatchTaskSpec {
        key: "task-m15-spec".to_string(),
        title: "Per-Task Execution Contract".to_string(),
        description: "Test description".to_string(),
        assigned_agent_id: None,
        parent_task_key: None,
        parent_task_id: None,
        dependency_keys: vec![],
        dependency_task_ids: vec![],
        dependency_output_policy: DependencyOutputPolicy::TaskSuccess,
        execution: Some(exec_spec.clone()),
    };

    let submit_cmd = IpcCommand::SubmitTask {
        studio_id,
        task_spec,
    };
    let submit_json = serde_json::to_string_pretty(&submit_cmd).expect("serialize SubmitTask");
    let submit_val: Value = serde_json::from_str(&submit_json).expect("parse json");

    assert_eq!(submit_val["type"], "submit_task");
    assert_eq!(
        submit_val["task_spec"]["execution"]["initial_prompt"],
        "Solve bug in auth middleware"
    );
    assert_eq!(
        submit_val["task_spec"]["execution"]["repository_path"],
        "/path/to/auth-repo"
    );
    assert_eq!(
        submit_val["task_spec"]["execution"]["runtime_preference"],
        "claude-code"
    );
    assert_eq!(
        submit_val["task_spec"]["execution"]["model_id"],
        "claude-3-7-sonnet"
    );

    let deserialized_cmd: IpcCommand =
        serde_json::from_str(&submit_json).expect("deserialize SubmitTask");
    match deserialized_cmd {
        IpcCommand::SubmitTask {
            studio_id: sid,
            task_spec: ts,
        } => {
            assert_eq!(sid, studio_id);
            assert_eq!(ts.execution, Some(exec_spec));
        }
        other => panic!("Unexpected command type: {:?}", other),
    }
}

#[test]
fn test_fixture_daemon_bootstrap_info() {
    let standard = DaemonBootstrapInfo {
        endpoint: r"\\.\pipe\agent-studios-default".to_string(),
        key_id: "ags-client".to_string(),
        client_key: "f81d4fae-7dec-11d0-a765-00a0c91e6bf6".to_string(),
        admin_key: None,
        pid: 12345,
        created_at: 1793400000,
    };

    let json_str = serde_json::to_string_pretty(&standard).expect("serialize DaemonBootstrapInfo");
    let val: Value = serde_json::from_str(&json_str).expect("parse json");

    assert_eq!(val["endpoint"], r"\\.\pipe\agent-studios-default");
    assert_eq!(val["key_id"], "ags-client");
    assert_eq!(val["client_key"], "f81d4fae-7dec-11d0-a765-00a0c91e6bf6");
    assert!(val.get("admin_key").is_none());
    assert_eq!(val["pid"], 12345);
    assert_eq!(val["created_at"], 1793400000);

    let debug_str = format!("{:?}", standard);
    assert!(debug_str.contains("[REDACTED]"));
    assert!(!debug_str.contains("f81d4fae-7dec-11d0-a765-00a0c91e6bf6"));

    let admin = DaemonAdminBootstrapInfo {
        endpoint: r"\\.\pipe\agent-studios-default".to_string(),
        key_id: "ags-client-admin".to_string(),
        admin_key: "a81d4fae-7dec-11d0-a765-00a0c91e6bf6".to_string(),
        pid: 12345,
        created_at: 1793400000,
    };

    let admin_json =
        serde_json::to_string_pretty(&admin).expect("serialize DaemonAdminBootstrapInfo");
    let admin_val: Value = serde_json::from_str(&admin_json).expect("parse json");
    assert_eq!(admin_val["key_id"], "ags-client-admin");
    assert_eq!(
        admin_val["admin_key"],
        "a81d4fae-7dec-11d0-a765-00a0c91e6bf6"
    );

    let admin_debug = format!("{:?}", admin);
    assert!(admin_debug.contains("[REDACTED]"));
    assert!(!admin_debug.contains("a81d4fae-7dec-11d0-a765-00a0c91e6bf6"));
}

#[test]
fn test_fixture_run_record_execution_serialization() {
    let _studio_id = StudioId::new();
    let task_id = TaskId::new();
    let run_id = RunId::new();
    let agent_id = AgentId::new();

    // 1. RunRecord with execution: Some(TaskExecutionSpec)
    let execution_spec = TaskExecutionSpec {
        initial_prompt: Some("Fix auth token validation".to_string()),
        repository_path: Some("/path/to/my-repo".to_string()),
        runtime_instance_id: Some("rt-inst-c0a80101-0000-0000-0000-000000000001".to_string()),
        runtime_preference: Some("opencode".to_string()),
        provider_instance_id: Some("c0a80101-0000-0000-0000-000000000002".to_string()),
        model_id: Some("claude-3-7-sonnet".to_string()),
    };

    let run_with_exec = RunRecord {
        id: run_id,
        task_id,
        agent_id,
        attempt: 1,
        state: RunState::Running,
        started_at: Some(Utc.timestamp_opt(1793400100, 0).single().unwrap()),
        finished_at: None,
        execution: Some(execution_spec),
    };

    let json_str =
        serde_json::to_string_pretty(&run_with_exec).expect("serialize RunRecord with execution");
    let val: Value = serde_json::from_str(&json_str).expect("parse json");

    assert_eq!(val["id"], run_id.to_string());
    assert_eq!(val["task_id"], task_id.to_string());
    assert_eq!(val["state"], "running");
    assert_eq!(
        val["execution"]["initial_prompt"],
        "Fix auth token validation"
    );
    assert_eq!(val["execution"]["repository_path"], "/path/to/my-repo");
    assert_eq!(
        val["execution"]["runtime_instance_id"],
        "rt-inst-c0a80101-0000-0000-0000-000000000001"
    );
    assert_eq!(val["execution"]["runtime_preference"], "opencode");
    assert_eq!(
        val["execution"]["provider_instance_id"],
        "c0a80101-0000-0000-0000-000000000002"
    );
    assert_eq!(val["execution"]["model_id"], "claude-3-7-sonnet");

    // Deserialization roundtrip
    let deserialized: RunRecord =
        serde_json::from_str(&json_str).expect("deserialize RunRecord with execution");
    assert_eq!(deserialized.id, run_id);
    assert_eq!(
        deserialized.execution.as_ref().unwrap().model_id.as_deref(),
        Some("claude-3-7-sonnet")
    );

    // 2. RunRecord with execution: None (backward compatibility)
    let run_no_exec = RunRecord {
        id: RunId::new(),
        task_id,
        agent_id,
        attempt: 1,
        state: RunState::Queued,
        started_at: None,
        finished_at: None,
        execution: None,
    };
    let json_no_exec =
        serde_json::to_string_pretty(&run_no_exec).expect("serialize RunRecord without execution");
    let val_no_exec: Value = serde_json::from_str(&json_no_exec).expect("parse json");
    assert!(val_no_exec["execution"].is_null());

    let deserialized_no_exec: RunRecord =
        serde_json::from_str(&json_no_exec).expect("deserialize RunRecord without execution");
    assert!(deserialized_no_exec.execution.is_none());

    // 3. IpcResponse::Run wrapping RunRecord
    let resp = IpcResponse::Run {
        run: Some(run_with_exec),
    };
    let resp_json = serde_json::to_string_pretty(&resp).expect("serialize IpcResponse::Run");
    let resp_val: Value = serde_json::from_str(&resp_json).expect("parse json");
    assert_eq!(resp_val["type"], "run");
    assert_eq!(
        resp_val["run"]["execution"]["model_id"],
        "claude-3-7-sonnet"
    );
}

#[test]
fn test_fixture_error_responses_serialization() {
    let err_resp = IpcResponse::Error {
        message: "Repository path '/invalid/path' is not authorized".to_string(),
    };
    let err_json = serde_json::to_string_pretty(&err_resp).expect("serialize IpcResponse::Error");
    let err_val: Value = serde_json::from_str(&err_json).expect("parse json");

    assert_eq!(err_val["type"], "error");
    assert_eq!(
        err_val["message"],
        "Repository path '/invalid/path' is not authorized"
    );

    let deserialized: IpcResponse =
        serde_json::from_str(&err_json).expect("deserialize IpcResponse::Error");
    match deserialized {
        IpcResponse::Error { message } => {
            assert!(message.contains("not authorized"));
        }
        other => panic!("Unexpected response variant: {:?}", other),
    }
}
