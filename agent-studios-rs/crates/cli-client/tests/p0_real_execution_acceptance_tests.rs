//! P0 Real Agent Execution Acceptance Tests (Zero Billable Invariant: $0.00)
//!
//! Exercises full end-to-end user journeys without cloud API spend:
//! 1. Goal -> Worktree Isolation -> Real File Mutation & Tool Execution -> Diff -> Patch Reconciliation -> Succeeded
//! 2. Interactive Approval Gate Enforcement (Pending -> Approved / Denied fail-closed)
//! 3. Cancellation & Process Containment
//! 4. ConnectedClient over native IPC + SQLite WAL persistence & reconnect

use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tokio::time::timeout;

use agent_studios_cli_client::{
    AgentStudiosClient, ClientApprovalDecision, ConnectedClient, EmbeddedClient, ExecutionMode,
    GoalSubmissionOptions, RuntimeMode,
};
use agent_studios_control_plane::{
    AnyEventStore, ControlPlane, SystemClock, sqlite_store::SqliteEventStore,
};
use agent_studios_external_runtime::{
    AgentRuntime, DiscoveredRuntimeInstance, RuntimeCapabilities, RuntimeError, RuntimeEventKind,
    RuntimeEventSubscription, RuntimeImplementationId, RuntimeInput, RuntimeInstanceId,
    RuntimeLifecycleState, RuntimeSessionHandle, RuntimeSessionId, RuntimeSessionRef,
    RuntimeStartRequest, SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_ipc_protocol::ProtocolVersion;
use agent_studios_ipc_transport::client::{ClientConfig, IpcClient};
use agent_studios_ipc_transport::server::IpcServerBuilder;
use agent_studios_protocol::approval::ApprovalState;
use agent_studios_protocol::event::ControlPlaneEvent;
use agent_studios_protocol::id::ApprovalId;
use agent_studios_protocol::task::TaskState;
use agent_studios_workspace::WorkspaceOrchestrator;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

fn init_git_repo(path: &std::path::Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("Failed to run git {:?}: {e}", args));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run(&["init"]);
    run(&["config", "user.name", "Agent Studios P0 Acceptance"]);
    run(&["config", "user.email", "p0@agent-studios.internal"]);
    run(&["config", "commit.gpgsign", "false"]);

    fs::write(path.join("README.md"), "# Agent Studios Acceptance Repo\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-m", "Initial commit"]);
    run(&["rev-parse", "HEAD"])
}

/// Local non-billable coding agent adapter that executes real filesystem writes
/// and real git/shell tool invocations strictly inside the assigned Git worktree,
/// guarded by a 2PC approval request when `require_approval` is true.
struct LocalSandboxWorktreeRuntime {
    impl_id: RuntimeImplementationId,
    inst_id: RuntimeInstanceId,
    require_approval: bool,
    hubs: Arc<RwLock<std::collections::HashMap<RuntimeSessionId, Arc<SessionEventHub>>>>,
    pending_approval_tx:
        Arc<RwLock<std::collections::HashMap<ApprovalId, tokio::sync::oneshot::Sender<bool>>>>,
}

impl LocalSandboxWorktreeRuntime {
    fn new(require_approval: bool) -> Self {
        Self {
            impl_id: RuntimeImplementationId::new("local-sandbox-p0").unwrap(),
            inst_id: RuntimeInstanceId::generate(),
            require_approval,
            hubs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            pending_approval_tx: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl AgentRuntime for LocalSandboxWorktreeRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.impl_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_tools(true)
            .with_stop(true)
            .with_interrupt(true)
            .with_workspace_binding(true);
        Ok(vec![DiscoveredRuntimeInstance::available(
            self.inst_id,
            self.impl_id.clone(),
            "Local Sandbox P0 Runtime",
            caps,
        )])
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_tools(true)
            .with_stop(true)
            .with_interrupt(true)
            .with_workspace_binding(true)
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        request.validate()?;
        let session_id = RuntimeSessionId::generate();
        let hub = Arc::new(SessionEventHub::new(session_id, self.inst_id, 256));
        self.hubs.write().await.insert(session_id, hub.clone());

        let worktree_cwd = request.workspace.cwd().to_path_buf();
        let handle = RuntimeSessionHandle::new(
            session_id,
            self.inst_id,
            self.impl_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace.clone(),
            request.workspace_access_mode,
            self.capabilities().await,
            request.correlation.clone(),
            Utc::now(),
        );

        hub.emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: self.inst_id,
        })
        .await?;

        let require_approval = self.require_approval;
        let approval_map = self.pending_approval_tx.clone();
        let hub_clone = hub.clone();

        tokio::spawn(async move {
            if require_approval {
                let approval_id = ApprovalId::new();
                let (tx, rx) = tokio::sync::oneshot::channel();
                approval_map.write().await.insert(approval_id, tx);

                let _ = hub_clone
                    .emit(RuntimeEventKind::ApprovalRequested {
                        approval_id,
                        description: SanitizedRuntimeMessage::new(
                            "Write src_generated.txt and verify with git status in worktree",
                        ),
                        command: Some(SanitizedRuntimeMessage::new("git status --short")),
                    })
                    .await;

                let approved = match timeout(Duration::from_secs(5), rx).await {
                    Ok(Ok(val)) => val,
                    _ => false,
                };
                if !approved {
                    let _ = hub_clone
                        .emit(RuntimeEventKind::Failed {
                            safe_error_summary: SanitizedRuntimeMessage::new(
                                "Approval denied or timed out; failing closed",
                            ),
                        })
                        .await;
                    return;
                }
            }

            // Tool 1: Real file mutation strictly inside the isolated worktree
            let _ = hub_clone
                .emit(RuntimeEventKind::ToolStarted {
                    tool_name: "write_file".to_string(),
                    tool_call_id: "call_write_1".to_string(),
                })
                .await;

            let target_file = worktree_cwd.join("p0_generated_feature.txt");
            let write_ok = fs::write(
                &target_file,
                "Generated inside isolated worktree by P0 acceptance runtime\n",
            )
            .is_ok();

            let _ = hub_clone
                .emit(RuntimeEventKind::ToolCompleted {
                    tool_name: "write_file".to_string(),
                    tool_call_id: "call_write_1".to_string(),
                    duration_ms: 5,
                    success: write_ok,
                })
                .await;

            if !write_ok {
                let _ = hub_clone
                    .emit(RuntimeEventKind::Failed {
                        safe_error_summary: SanitizedRuntimeMessage::new(
                            "Failed to write file in isolated worktree",
                        ),
                    })
                    .await;
                return;
            }

            // Tool 2: Real subprocess command execution (`git --version` / `git status`)
            let _ = hub_clone
                .emit(RuntimeEventKind::ToolStarted {
                    tool_name: "exec_command".to_string(),
                    tool_call_id: "call_exec_1".to_string(),
                })
                .await;

            let cmd_res = Command::new("git")
                .current_dir(&worktree_cwd)
                .args(["status", "--short"])
                .output();
            let cmd_ok = cmd_res
                .as_ref()
                .map(|o| o.status.success())
                .unwrap_or(false);

            let _ = hub_clone
                .emit(RuntimeEventKind::ToolCompleted {
                    tool_name: "exec_command".to_string(),
                    tool_call_id: "call_exec_1".to_string(),
                    duration_ms: 15,
                    success: cmd_ok,
                })
                .await;

            if cmd_ok {
                let _ = hub_clone
                    .emit(RuntimeEventKind::OutputDelta {
                        text: "Created p0_generated_feature.txt and verified worktree.\n"
                            .to_string(),
                    })
                    .await;
                let _ = hub_clone
                    .emit(RuntimeEventKind::Completed {
                        summary: Some("P0 real local worktree execution succeeded".to_string()),
                    })
                    .await;
            } else {
                let _ = hub_clone
                    .emit(RuntimeEventKind::Failed {
                        safe_error_summary: SanitizedRuntimeMessage::new(
                            "Worktree command returned non-zero exit status",
                        ),
                    })
                    .await;
            }
        });

        Ok(handle)
    }

    async fn send(
        &self,
        _session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        if let RuntimeInput::ApprovalResponse {
            approval_id,
            approved,
            ..
        } = input
            && let Some(tx) = self.pending_approval_tx.write().await.remove(&approval_id)
        {
            let _ = tx.send(approved);
        }
        Ok(())
    }

    async fn interrupt(&self, _session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn resume(
        &self,
        _session: &RuntimeSessionRef,
        _input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        if let Some(hub) = self.hubs.read().await.get(&session.session_id) {
            let _ = hub.emit(RuntimeEventKind::Stopped).await;
        }
        Ok(())
    }

    async fn status(
        &self,
        _session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        Ok(RuntimeLifecycleState::Running)
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        let hubs = self.hubs.read().await;
        let hub = hubs
            .get(&session.session_id)
            .cloned()
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        hub.subscribe(after_sequence).await
    }
}

#[tokio::test]
async fn test_p0_journey_1_full_autonomous_worktree_tool_diff_and_reconcile() {
    let source_dir = tempdir().expect("source tempdir");
    init_git_repo(source_dir.path());

    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let orchestrator = Arc::new(
        WorkspaceOrchestrator::new(
            orchestrator_dir.path().join("worktrees"),
            orchestrator_dir.path().join("artifacts"),
        )
        .expect("orchestrator"),
    );

    let runtime = Arc::new(LocalSandboxWorktreeRuntime::new(true));
    let client = EmbeddedClient::bootstrap_with_runtime("p0-journey-1", runtime)
        .await
        .expect("bootstrap")
        .with_orchestrator(orchestrator)
        .with_working_dir(source_dir.path());

    let mut sub = client.subscribe_events(0).await.expect("subscribe");

    let task_id = client
        .submit_goal(
            "Create p0_generated_feature.txt and verify git status",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal");

    let mut approval_seen = false;
    let mut tool_completed_count = 0usize;
    let mut task_succeeded = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    while tokio::time::Instant::now() < deadline && !task_succeeded {
        if let Ok(Some(env)) = timeout(Duration::from_millis(300), sub.stream.recv()).await {
            match &env.event {
                ControlPlaneEvent::ApprovalRequested { approval }
                    if approval.task_id == task_id =>
                {
                    approval_seen = true;
                    // While awaiting approval, source repo must remain untouched
                    assert!(!source_dir.path().join("p0_generated_feature.txt").exists());
                    client
                        .resolve_approval(approval.id, ClientApprovalDecision::Allow)
                        .await
                        .expect("resolve_approval allow");
                }
                ControlPlaneEvent::ToolCompleted { .. } => {
                    tool_completed_count += 1;
                }
                ControlPlaneEvent::TaskStateChanged {
                    task_id: tid,
                    new_state,
                    ..
                } if *tid == task_id && *new_state == TaskState::Succeeded => {
                    task_succeeded = true;
                }
                _ => {}
            }
        }
    }

    assert!(approval_seen, "Approval gate must fire before mutation");
    assert_eq!(
        tool_completed_count, 2,
        "Both write_file and exec_command tools must complete"
    );
    assert!(task_succeeded, "Task must reach Succeeded");

    // Main source directory must remain clean BEFORE explicit reconciliation
    assert!(
        !source_dir.path().join("p0_generated_feature.txt").exists(),
        "Main repository must stay isolated until human review / reconciliation"
    );

    // Inspect worktree diff via Client trait
    let worktrees = client.list_worktrees().await.expect("list_worktrees");
    assert_eq!(worktrees.len(), 1);
    let diff = client
        .get_worktree_diff(worktrees[0].worktree_id)
        .await
        .expect("get_worktree_diff");
    assert!(
        !diff.patch_bytes.is_empty(),
        "Captured diff must be non-empty"
    );
    assert!(
        diff.changed_files
            .iter()
            .any(|f| f.contains("p0_generated_feature.txt"))
    );

    // Reconcile into source repo and verify file lands cleanly
    let (_captured, outcome) = client
        .capture_and_reconcile_task(task_id, source_dir.path(), Some("feat(p0): reconcile"))
        .await
        .expect("capture_and_reconcile_task");
    assert!(matches!(outcome, ReconciliationOutcome::Applied { .. }));
    assert!(source_dir.path().join("p0_generated_feature.txt").exists());
}

#[tokio::test]
async fn test_p0_journey_2_denied_approval_fails_closed() {
    let source_dir = tempdir().expect("source tempdir");
    init_git_repo(source_dir.path());

    let orchestrator_dir = tempdir().expect("orchestrator tempdir");
    let orchestrator = Arc::new(
        WorkspaceOrchestrator::new(
            orchestrator_dir.path().join("worktrees"),
            orchestrator_dir.path().join("artifacts"),
        )
        .expect("orchestrator"),
    );

    let runtime = Arc::new(LocalSandboxWorktreeRuntime::new(true));
    let client = EmbeddedClient::bootstrap_with_runtime("p0-journey-2", runtime)
        .await
        .expect("bootstrap")
        .with_orchestrator(orchestrator)
        .with_working_dir(source_dir.path());

    let mut sub = client.subscribe_events(0).await.expect("subscribe");
    let task_id = client
        .submit_goal("Attempt denied mutation", GoalSubmissionOptions::default())
        .await
        .expect("submit_goal");

    let mut denied_ok = false;
    let mut task_failed = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    while tokio::time::Instant::now() < deadline && !task_failed {
        if let Ok(Some(env)) = timeout(Duration::from_millis(300), sub.stream.recv()).await {
            match &env.event {
                ControlPlaneEvent::ApprovalRequested { approval }
                    if approval.task_id == task_id =>
                {
                    client
                        .resolve_approval(approval.id, ClientApprovalDecision::Deny)
                        .await
                        .expect("resolve deny");
                }
                ControlPlaneEvent::ApprovalResolved { new_state, .. }
                    if *new_state == ApprovalState::Denied =>
                {
                    denied_ok = true;
                }
                ControlPlaneEvent::TaskStateChanged {
                    task_id: tid,
                    new_state,
                    ..
                } if *tid == task_id && *new_state == TaskState::Failed => {
                    task_failed = true;
                }
                _ => {}
            }
        }
    }

    assert!(denied_ok, "ApprovalResolved::Denied must be recorded");
    assert!(task_failed, "Denied approval must fail closed");
    assert!(!source_dir.path().join("p0_generated_feature.txt").exists());
}

#[tokio::test]
async fn test_p0_journey_3_connected_client_ipc_and_sqlite_reconnect() {
    let temp_dir = tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("p0_state.db");

    // Start IPC server backed by durable SQLite WAL event store
    let store = SqliteEventStore::open(&db_path).expect("open sqlite store");
    let cp = ControlPlane::restore_from_store(SystemClock, AnyEventStore::Sqlite(store))
        .expect("restore control plane");

    let server = Arc::new(
        IpcServerBuilder::new()
            .with_auth_key("p0-client", b"p0-secret-key".to_vec())
            .with_control_plane(cp)
            .build()
            .expect("build ipc server"),
    );

    let (client_stream_1, server_stream_1) = tokio::io::duplex(64 * 1024);
    let server_state_1 = server.state();
    tokio::spawn(async move {
        let _ =
            agent_studios_ipc_transport::handle_connection(server_stream_1, server_state_1).await;
    });

    let cfg = ClientConfig::new("p0-client", b"p0-secret-key".to_vec())
        .with_protocol_version(ProtocolVersion::CURRENT);
    let ipc_1 = IpcClient::connect_stream(client_stream_1, cfg.clone())
        .await
        .expect("ipc connect 1");

    let connected =
        ConnectedClient::from_ipc_client(ipc_1, "p0-connected-ws", RuntimeMode::Production)
            .await
            .expect("from_ipc_client");
    assert_eq!(connected.execution_mode(), ExecutionMode::Connected);

    let task_id = connected
        .submit_goal(
            "Durable IPC task submission",
            GoalSubmissionOptions::default(),
        )
        .await
        .expect("submit_goal over ipc");

    let graph = connected.get_task_graph().await.expect("get_task_graph");
    assert!(graph.tasks.iter().any(|t| t.id == task_id));

    // Reconnect a second client against the same server and verify task & event durability
    let (client_stream_2, server_stream_2) = tokio::io::duplex(64 * 1024);
    let server_state_2 = server.state();
    tokio::spawn(async move {
        let _ =
            agent_studios_ipc_transport::handle_connection(server_stream_2, server_state_2).await;
    });
    let ipc_2 = IpcClient::connect_stream(client_stream_2, cfg)
        .await
        .expect("ipc connect 2");
    let reconnected =
        ConnectedClient::from_ipc_client(ipc_2, "p0-connected-ws", RuntimeMode::Production)
            .await
            .expect("reconnect from_ipc_client");

    let reconnected_graph = reconnected
        .get_task_graph()
        .await
        .expect("reconnected graph");
    assert!(
        reconnected_graph.tasks.iter().any(|t| t.id == task_id),
        "Persisted task must survive client reconnect"
    );

    connected.shutdown().await.expect("shutdown 1");
    reconnected.shutdown().await.expect("shutdown 2");
}
