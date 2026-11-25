//! Connected Daemon Client (`ConnectedClient`) for Agent Studios CLI
//!
//! Connects to a live `agsd` background daemon via `agent_studios_ipc_transport::IpcClient`
//! using Windows Named Pipes or Unix Domain Sockets with HMAC authentication,
//! and provides `connect_or_fallback` for seamless offline EmbeddedClient degradation.

use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

use agent_studios_ipc_protocol::{ApprovalAction, ApprovalDecision, ApprovalStatus};
use agent_studios_ipc_transport::IpcClient;
use agent_studios_ipc_transport::daemon::DaemonBootstrapInfo;
use agent_studios_orchestration::read_models::{
    AgentOperationalState, AgentSummary, BudgetUsage, TaskGraphSnapshot, TaskTimelineProjection,
    TimelineItem, TimelineItemKind,
};
use agent_studios_protocol::agent::{AgentExecutionBudget, AgentState};
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{
    ApprovalId, EventId, ReconciliationId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::task::{BatchTaskSpec, TaskExecutionSpec, TaskState};
use agent_studios_workspace::capture::{CapturedChanges, parse_git_diff_summary};
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

use crate::client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, ExecutionMode,
    GoalSubmissionOptions, RuntimeMode, StreamSyncState, WorkspaceInfo, WorktreeRecordSummary,
};
use crate::embedded::EmbeddedClient;

/// Connected client delegating orchestration commands and event subscriptions
/// to an external `agsd` daemon over authenticated native IPC.
pub struct ConnectedClient {
    ipc: Arc<Mutex<IpcClient>>,
    studio_id: StudioId,
    workspace_name: String,
    runtime_mode: RuntimeMode,
    working_dir: Option<std::path::PathBuf>,
}

impl ConnectedClient {
    /// Connects to a running `agsd` daemon via a specific bootstrap file (`agsd.json`).
    pub async fn connect_from_bootstrap(
        bootstrap_path: impl AsRef<Path>,
        workspace_name: impl Into<String>,
    ) -> Result<Self, ClientError> {
        let mut ipc = IpcClient::connect_from_bootstrap_file(bootstrap_path)
            .await
            .map_err(|e| {
                ClientError::Transport(format!("Failed to connect to agsd daemon: {e}"))
            })?;

        let workspace_name = workspace_name.into();
        let existing_studios = ipc
            .list_studios()
            .await
            .map_err(|e| ClientError::Transport(format!("Failed to list studios: {e}")))?;

        let studio = if let Some(found) = existing_studios
            .into_iter()
            .find(|s| s.name == workspace_name)
        {
            found
        } else {
            ipc.create_studio(&workspace_name)
                .await
                .map_err(|e| ClientError::Transport(format!("Failed to create studio: {e}")))?
        };

        Ok(Self {
            ipc: Arc::new(Mutex::new(ipc)),
            studio_id: studio.id,
            workspace_name,
            runtime_mode: RuntimeMode::Production,
            working_dir: None,
        })
    }

    /// Connects to the default `agsd` daemon using `DaemonBootstrapInfo::default_runtime_path()`.
    pub async fn connect_default(workspace_name: impl Into<String>) -> Result<Self, ClientError> {
        let path = DaemonBootstrapInfo::default_runtime_path();
        Self::connect_from_bootstrap(path, workspace_name).await
    }

    /// Wraps an already-authenticated `IpcClient` (useful for duplex/custom transport tests).
    pub async fn from_ipc_client(
        mut ipc: IpcClient,
        workspace_name: impl Into<String>,
        runtime_mode: RuntimeMode,
    ) -> Result<Self, ClientError> {
        let workspace_name = workspace_name.into();
        let existing_studios = ipc
            .list_studios()
            .await
            .map_err(|e| ClientError::Transport(format!("Failed to list studios: {e}")))?;

        let studio = if let Some(found) = existing_studios
            .into_iter()
            .find(|s| s.name == workspace_name)
        {
            found
        } else {
            ipc.create_studio(&workspace_name)
                .await
                .map_err(|e| ClientError::Transport(format!("Failed to create studio: {e}")))?
        };

        Ok(Self {
            ipc: Arc::new(Mutex::new(ipc)),
            studio_id: studio.id,
            workspace_name,
            runtime_mode,
            working_dir: None,
        })
    }

    /// Attaches an explicit working directory path for task submission metadata.
    pub fn with_working_dir(mut self, working_dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    /// Accesses the underlying IPC client handle.
    pub fn ipc_client(&self) -> &Arc<Mutex<IpcClient>> {
        &self.ipc
    }
}

/// Attempts to connect to a live `agsd` daemon, gracefully falling back to
/// an in-process `EmbeddedClient` when `agsd` is offline.
pub async fn connect_or_fallback(
    workspace_name: &str,
    mode: RuntimeMode,
) -> Result<crate::client::DynAgentStudiosClient, ClientError> {
    if let Ok(connected) = ConnectedClient::connect_default(workspace_name).await {
        tracing::info!(
            workspace = workspace_name,
            "Connected to live agsd daemon over native IPC"
        );
        return Ok(Arc::new(connected));
    }

    tracing::info!(
        workspace = workspace_name,
        "Daemon (agsd) offline; falling back to in-process EmbeddedClient"
    );
    let embedded = EmbeddedClient::bootstrap_with_mode(workspace_name, mode).await?;
    Ok(Arc::new(embedded))
}

fn decode_ipc_event(ipc_ev: &agent_studios_ipc_protocol::IpcEvent) -> Option<EventEnvelope> {
    if let Ok(mut envelope) = serde_json::from_value::<EventEnvelope>(ipc_ev.payload.clone()) {
        if envelope.sequence == 0 {
            envelope.sequence = ipc_ev.sequence;
        }
        return Some(envelope);
    }

    // Map daemon-synthesized IpcEvent variants (e.g. approval_requested) if payload is not an EventEnvelope
    if ipc_ev.event_type == "approval_requested"
        && let Ok(contract) = serde_json::from_value::<agent_studios_ipc_protocol::ApprovalContract>(
            ipc_ev.payload.clone(),
        )
    {
        let task_id = contract.task_id.parse().unwrap_or_else(|_| TaskId::new());
        let approval_id = uuid::Uuid::parse_str(contract.approval_id.as_str())
            .map(ApprovalId::from_uuid)
            .unwrap_or_else(|_| ApprovalId::new());
        let summary = match &contract.action {
            ApprovalAction::CommandExecution { command, .. } => {
                format!("{} (cmd: {})", contract.description, command)
            }
            _ => contract.description.clone(),
        };
        let approval = agent_studios_protocol::approval::ApprovalRequest {
            id: approval_id,
            studio_id: StudioId::new(),
            task_id,
            agent_id: agent_studios_protocol::id::AgentId::new(),
            kind: agent_studios_protocol::approval::ApprovalKind::CommandExecution,
            summary,
            state: match contract.status {
                ApprovalStatus::Authorized
                | ApprovalStatus::Delivered
                | ApprovalStatus::ExecutionCompleted => {
                    agent_studios_protocol::approval::ApprovalState::Approved
                }
                ApprovalStatus::Rejected | ApprovalStatus::Cancelled | ApprovalStatus::TimedOut => {
                    agent_studios_protocol::approval::ApprovalState::Denied
                }
                _ => agent_studios_protocol::approval::ApprovalState::Pending,
            },
            created_at: contract.created_at,
            resolved_at: None,
        };
        return Some(EventEnvelope::new(
            StudioId::new(),
            ipc_ev.sequence,
            ipc_ev.timestamp,
            ControlPlaneEvent::ApprovalRequested { approval },
        ));
    }

    None
}

#[async_trait]
impl AgentStudiosClient for ConnectedClient {
    fn studio_id(&self) -> StudioId {
        self.studio_id
    }

    fn workspace_name(&self) -> &str {
        &self.workspace_name
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Connected
    }

    fn runtime_mode(&self) -> RuntimeMode {
        self.runtime_mode
    }

    async fn get_workspace_info(&self) -> Result<WorkspaceInfo, ClientError> {
        let cwd_path = self.working_dir.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
        let cwd = cwd_path.to_string_lossy().to_string();

        let branch_name = tokio::process::Command::new("git")
            .current_dir(&cwd_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        let is_dirty = tokio::process::Command::new("git")
            .current_dir(&cwd_path)
            .args(["status", "--porcelain"])
            .output()
            .await
            .ok()
            .filter(|out| out.status.success())
            .map(|out| !String::from_utf8_lossy(&out.stdout).trim().is_empty())
            .unwrap_or(false);

        Ok(WorkspaceInfo {
            project_name: self.workspace_name.clone(),
            branch_name,
            is_dirty,
            cwd,
        })
    }

    async fn shutdown(&self) -> Result<(), ClientError> {
        let mut ipc = self.ipc.lock().await;
        ipc.disconnect().await;
        Ok(())
    }

    async fn submit_goal(
        &self,
        prompt: &str,
        options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError> {
        let mut exec = TaskExecutionSpec::new().with_initial_prompt(prompt);
        if let Some(ref cwd) = self.working_dir {
            exec = exec.with_repository_path(cwd.to_string_lossy().to_string());
        }
        if let Some(ref rt) = options.runtime {
            exec = exec.with_runtime_preference(rt);
        }
        if let Some(ref prov) = options.provider {
            exec = exec.with_provider_instance_id(prov);
        }
        if let Some(ref model) = options.model {
            exec = exec.with_model_id(model);
        }

        let spec = BatchTaskSpec {
            key: format!("goal-{}", uuid::Uuid::new_v4().simple()),
            title: prompt.chars().take(80).collect(),
            description: prompt.to_string(),
            assigned_agent_id: None,
            parent_task_key: None,
            parent_task_id: None,
            dependency_keys: Vec::new(),
            dependency_task_ids: Vec::new(),
            dependency_output_policy: Default::default(),
            execution: Some(exec),
        };

        let mut ipc = self.ipc.lock().await;
        let ids = ipc
            .submit_task(self.studio_id, spec)
            .await
            .map_err(|e| ClientError::Transport(format!("submit_task failed: {e}")))?;

        ids.into_iter().next().ok_or_else(|| {
            ClientError::OperationFailed(
                "Daemon returned empty task list on submit_task".to_string(),
            )
        })
    }

    async fn cancel_active_run(&self, reason: Option<String>) -> Result<(), ClientError> {
        let mut ipc = self.ipc.lock().await;
        let tasks = ipc
            .list_tasks(self.studio_id)
            .await
            .map_err(|e| ClientError::Transport(format!("list_tasks failed: {e}")))?;

        let cancel_reason = reason.unwrap_or_else(|| "Cancelled by CLI client".to_string());
        for task in tasks {
            if !task.state.is_terminal() {
                ipc.cancel_task(self.studio_id, task.id, &cancel_reason)
                    .await
                    .map_err(|e| {
                        ClientError::Transport(format!("cancel_task {} failed: {e}", task.id))
                    })?;
            }
        }
        Ok(())
    }

    async fn subscribe_events(
        &self,
        from_sequence: u64,
    ) -> Result<ClientSubscription, ClientError> {
        let mut sub_stream = {
            let mut ipc = self.ipc.lock().await;
            ipc.subscribe_events(from_sequence)
                .await
                .map_err(|e| ClientError::Transport(format!("subscribe_events failed: {e}")))?
        };

        let (tx, rx) = mpsc::channel(512);
        let (sync_tx, sync_rx) = tokio::sync::watch::channel(StreamSyncState::Healthy);

        tokio::spawn(async move {
            let mut seen_ids: HashSet<EventId> = HashSet::new();
            let mut expected_seq = from_sequence.max(1);

            while let Ok(Some(ipc_ev)) = sub_stream.recv().await {
                if let Some(env) = decode_ipc_event(&ipc_ev) {
                    if !seen_ids.insert(env.event_id) {
                        continue;
                    }
                    if env.sequence > expected_seq && expected_seq > 1 {
                        let _ = sync_tx.send(StreamSyncState::GapUnrecoverable {
                            expected: expected_seq,
                            found: env.sequence,
                        });
                    } else {
                        let _ = sync_tx.send(StreamSyncState::Healthy);
                    }
                    expected_seq = env.sequence + 1;

                    if tx.send(env).await.is_err() {
                        break;
                    }
                }
            }
            let _ = sync_tx.send(StreamSyncState::Closed);
        });

        Ok(ClientSubscription {
            backfill: Vec::new(),
            stream: rx,
            sync_state: sync_rx,
        })
    }

    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError> {
        let mut ipc = self.ipc.lock().await;
        let tasks = ipc
            .list_tasks(self.studio_id)
            .await
            .map_err(|e| ClientError::Transport(format!("list_tasks failed: {e}")))?;

        let mut ready_tasks = Vec::new();
        let mut running_tasks = Vec::new();
        let mut blocked_tasks = Vec::new();
        let mut retrying_tasks = Vec::new();
        let mut succeeded_tasks = Vec::new();
        let mut failed_tasks = Vec::new();
        let mut cancelled_tasks = Vec::new();
        let mut adjacency = HashMap::new();

        for task in &tasks {
            adjacency.insert(task.id, task.dependencies.clone());
            match task.state {
                TaskState::Ready => ready_tasks.push(task.id),
                TaskState::Running => running_tasks.push(task.id),
                TaskState::Pending | TaskState::Blocked | TaskState::Paused => {
                    blocked_tasks.push(task.id)
                }
                TaskState::Retrying => retrying_tasks.push(task.id),
                TaskState::Succeeded => succeeded_tasks.push(task.id),
                TaskState::Failed => failed_tasks.push(task.id),
                TaskState::Cancelled => cancelled_tasks.push(task.id),
            }
        }

        Ok(TaskGraphSnapshot {
            studio_id: self.studio_id,
            tasks,
            ready_tasks,
            running_tasks,
            blocked_tasks,
            retrying_tasks,
            succeeded_tasks,
            failed_tasks,
            cancelled_tasks,
            adjacency,
            task_worktrees: HashMap::new(),
            task_artifacts: HashMap::new(),
            task_reconciliations: HashMap::new(),
            task_block_reasons: HashMap::new(),
        })
    }

    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError> {
        let mut ipc = self.ipc.lock().await;
        let runs = ipc
            .list_runs(self.studio_id)
            .await
            .map_err(|e| ClientError::Transport(format!("list_runs failed: {e}")))?;

        let mut seen = HashSet::new();
        let mut summaries = Vec::new();
        let now = Utc::now();

        for run in runs {
            if !seen.insert(run.agent_id) {
                continue;
            }
            summaries.push(AgentSummary {
                agent_id: run.agent_id,
                studio_id: self.studio_id,
                agent_name: format!("agent-{}", run.agent_id),
                role: None,
                state: AgentState::Idle,
                operational_state: AgentOperationalState::Idle,
                runtime_bound: false,
                runtime_kind: None,
                provider: None,
                model: None,
                wire_protocol: None,
                codex_thread_id: None,
                parent_thread_id: None,
                parent_agent_id: None,
                spawn_depth: 0,
                current_task_id: Some(run.task_id),
                tool_calls_total: 0,
                tool_calls_active: 0,
                budget: AgentExecutionBudget::default(),
                budget_usage: BudgetUsage::default(),
                budget_exceeded: false,
                budget_exceeded_dimension: None,
                created_at: now,
                updated_at: now,
            });
        }

        Ok(summaries)
    }

    async fn get_task_timeline(
        &self,
        task_id: TaskId,
    ) -> Result<TaskTimelineProjection, ClientError> {
        let mut ipc = self.ipc.lock().await;
        let task = ipc
            .get_task(self.studio_id, task_id)
            .await
            .map_err(|e| ClientError::Transport(format!("get_task failed: {e}")))?
            .ok_or_else(|| ClientError::NotFound(format!("Task {task_id} not found")))?;

        let items = vec![TimelineItem {
            timestamp: task.created_at,
            sequence: 1,
            kind: TimelineItemKind::TaskCreated,
            description: format!("Task created: {}", task.title),
            related_id: Some(task.id.to_string()),
        }];

        Ok(TaskTimelineProjection {
            task_id,
            studio_id: self.studio_id,
            current_state: task.state,
            assigned_agent_id: task.assigned_agent_id,
            assigned_worktree_id: None,
            bound_thread_id: None,
            artifact_ids: Vec::new(),
            reconciliation_ids: Vec::new(),
            items,
        })
    }

    async fn resolve_approval(
        &self,
        approval_id: ApprovalId,
        decision: ClientApprovalDecision,
    ) -> Result<(), ClientError> {
        let ipc_decision = match decision {
            ClientApprovalDecision::Allow | ClientApprovalDecision::AllowForSession => {
                ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: None,
                }
            }
            ClientApprovalDecision::Deny => ApprovalDecision::Rejected {
                reason: "Denied via CLI client".to_string(),
            },
        };

        let mut ipc = self.ipc.lock().await;
        ipc.resolve_approval(approval_id.to_string(), ipc_decision)
            .await
            .map_err(|e| ClientError::OperationFailed(format!("resolve_approval failed: {e}")))?;
        Ok(())
    }

    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecordSummary>, ClientError> {
        Ok(Vec::new())
    }

    async fn get_worktree_diff(
        &self,
        _worktree_id: WorktreeId,
    ) -> Result<CapturedChanges, ClientError> {
        // Connected mode diffs are queried by task ID via IpcClient::get_task_diff.
        // Fall back to task lookup across studio tasks if needed.
        let mut ipc = self.ipc.lock().await;
        let tasks = ipc
            .list_tasks(self.studio_id)
            .await
            .map_err(|e| ClientError::Transport(format!("list_tasks failed: {e}")))?;

        for task in tasks {
            if let Ok(diff_resp) = ipc.get_task_diff(self.studio_id, task.id, None, None).await
                && diff_resp.total_bytes > 0
            {
                let bytes = diff_resp.patch.into_bytes();
                let summary = parse_git_diff_summary(
                    &bytes,
                    diff_resp.base_commit.as_deref().unwrap_or("HEAD"),
                );
                return Ok(CapturedChanges {
                    patch_bytes: bytes,
                    files_changed: summary.files_changed,
                    changed_files: summary
                        .added_files
                        .into_iter()
                        .chain(summary.modified_files)
                        .chain(summary.deleted_files)
                        .collect(),
                    stats_summary: String::new(),
                    base_commit: diff_resp.base_commit.unwrap_or_else(|| "HEAD".to_string()),
                    head_commit: diff_resp.head_commit,
                });
            }
        }

        Err(ClientError::NotFound(
            "No worktree diff found for studio tasks".to_string(),
        ))
    }

    async fn reconcile_patch(
        &self,
        _reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError> {
        Err(ClientError::Unsupported(
            "Patch reconciliation over IPC is initiated by daemon completion".to_string(),
        ))
    }
}
