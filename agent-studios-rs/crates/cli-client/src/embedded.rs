//! In-process Embedded Implementation of AgentStudiosClient
//!
//! Owns the deterministic in-process ControlPlane, EventStore, and local event broadcast channel.
//! Dispatches submitted goals through the runtime layer (external runtime or local deterministic adapter)
//! to produce streaming assistant events and task execution transitions.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use agent_studios_control_plane::clock::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::{EventStore, InMemoryStore};
use agent_studios_external_runtime::{
    AgentRuntime, RuntimeCorrelation, RuntimeEventKind, RuntimeInstanceRef, RuntimeSessionRef,
    RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_orchestration::projection::{
    build_task_graph_snapshot, project_agent_summary, project_task_timeline,
};
use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::ApprovalState;
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_protocol::id::{ApprovalId, ReconciliationId, StudioId, TaskId, WorktreeId};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

use crate::client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, ExecutionMode,
    GoalSubmissionOptions, WorkspaceInfo, WorktreeRecordSummary,
};
use crate::runtime::resolve_default_runtime;

/// In-process client running the deterministic control plane inside the CLI process.
pub struct EmbeddedClient {
    studio_id: StudioId,
    workspace_name: String,
    control_plane: Arc<Mutex<ControlPlane<SystemClock, InMemoryStore>>>,
    events_tx: broadcast::Sender<EventEnvelope>,
    events_history: Arc<RwLock<Vec<EventEnvelope>>>,
    runtime: Arc<dyn AgentRuntime>,
    active_sessions: Arc<RwLock<Vec<RuntimeSessionRef>>>,
}

impl EmbeddedClient {
    /// Bootstraps a fresh in-process EmbeddedClient using default discovered runtime.
    pub async fn bootstrap(workspace_name: &str) -> Result<Self, ClientError> {
        let runtime = resolve_default_runtime().await;
        Self::bootstrap_with_runtime_and_capacity(workspace_name, runtime, 512).await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with custom broadcast channel capacity.
    pub async fn bootstrap_with_capacity(
        workspace_name: &str,
        capacity: usize,
    ) -> Result<Self, ClientError> {
        let runtime = resolve_default_runtime().await;
        Self::bootstrap_with_runtime_and_capacity(workspace_name, runtime, capacity).await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with an explicit runtime adapter.
    pub async fn bootstrap_with_runtime(
        workspace_name: &str,
        runtime: Arc<dyn AgentRuntime>,
    ) -> Result<Self, ClientError> {
        Self::bootstrap_with_runtime_and_capacity(workspace_name, runtime, 512).await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with explicit runtime adapter and capacity.
    pub async fn bootstrap_with_runtime_and_capacity(
        workspace_name: &str,
        runtime: Arc<dyn AgentRuntime>,
        capacity: usize,
    ) -> Result<Self, ClientError> {
        let store = InMemoryStore::new();
        let clock = SystemClock;
        let mut cp = ControlPlane::new(clock, store);

        let (tx, _rx) = broadcast::channel(capacity);
        let events_history = Arc::new(RwLock::new(Vec::new()));

        let studio = cp
            .create_studio(workspace_name)
            .map_err(|e| ClientError::OperationFailed(format!("Failed to create studio: {e}")))?;
        let studio_id = studio.id;

        // Register default multi-agent team roles
        let default_roles = [
            (
                "Coordinator",
                AgentKind::Internal,
                Some("coordinator".to_string()),
            ),
            ("Coder", AgentKind::Internal, Some("coder".to_string())),
            ("Tester", AgentKind::Internal, Some("tester".to_string())),
            (
                "Reviewer",
                AgentKind::Internal,
                Some("reviewer".to_string()),
            ),
        ];

        for (name, kind, role) in default_roles {
            let _ = cp.register_agent(studio_id, name, kind, role);
        }

        // Collect all initial events into history
        if let Ok(events) = cp.store().events_for_studio(studio_id, 0) {
            let mut history = events_history.write().await;
            for ev in events {
                let _ = tx.send(ev.clone());
                history.push(ev);
            }
        }

        Ok(Self {
            studio_id,
            workspace_name: workspace_name.to_string(),
            control_plane: Arc::new(Mutex::new(cp)),
            events_tx: tx,
            events_history,
            runtime,
            active_sessions: Arc::new(RwLock::new(Vec::new())),
        })
    }

    async fn sync_events_internal(
        cp: &ControlPlane<SystemClock, InMemoryStore>,
        studio_id: StudioId,
        history_lock: &RwLock<Vec<EventEnvelope>>,
        tx: &broadcast::Sender<EventEnvelope>,
    ) {
        if let Ok(events) = cp.store().events_for_studio(studio_id, 0) {
            let mut history = history_lock.write().await;
            for ev in events {
                if !history.iter().any(|h| h.event_id == ev.event_id) {
                    let _ = tx.send(ev.clone());
                    history.push(ev);
                }
            }
        }
    }
}

#[async_trait]
impl AgentStudiosClient for EmbeddedClient {
    fn studio_id(&self) -> StudioId {
        self.studio_id
    }

    fn workspace_name(&self) -> &str {
        &self.workspace_name
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Embedded
    }

    async fn get_workspace_info(&self) -> Result<WorkspaceInfo, ClientError> {
        let cwd_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let cwd = cwd_path.to_string_lossy().to_string();

        let dir_name = cwd_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let project_name = if !self.workspace_name.is_empty()
            && self.workspace_name != "agent-studios-workspace"
        {
            self.workspace_name.clone()
        } else if !dir_name.is_empty() {
            dir_name.to_string()
        } else if !self.workspace_name.is_empty() {
            self.workspace_name.clone()
        } else {
            "workspace".to_string()
        };

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
            project_name,
            branch_name,
            is_dirty,
            cwd,
        })
    }

    async fn shutdown(&self) -> Result<(), ClientError> {
        tracing::info!(studio_id = %self.studio_id, "Embedded client shutting down");
        let active_sessions = {
            let active = self.active_sessions.read().await;
            active.clone()
        };
        for session in active_sessions {
            let _ = self.runtime.stop(&session).await;
        }
        Ok(())
    }

    async fn submit_goal(
        &self,
        prompt: &str,
        _options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError> {
        // 1. Create Control Plane Task and Run, and transition to Running
        let (task_id, run_id, agent_id) = {
            let mut cp = self.control_plane.lock().await;
            let task = cp
                .create_task(
                    self.studio_id,
                    prompt.chars().take(80).collect::<String>(),
                    prompt,
                    None,
                    None,
                    Vec::new(),
                )
                .map_err(|e| ClientError::OperationFailed(format!("Failed to submit task: {e}")))?;

            let agent = cp
                .state()
                .agents
                .values()
                .find(|a| a.studio_id == self.studio_id && a.role.as_deref() == Some("coder"))
                .or_else(|| {
                    cp.state()
                        .agents
                        .values()
                        .find(|a| a.studio_id == self.studio_id)
                })
                .cloned()
                .ok_or_else(|| {
                    ClientError::OperationFailed("No registered agent found for studio".to_string())
                })?;

            let run = cp
                .create_run(task.id, agent.id)
                .map_err(|e| ClientError::OperationFailed(format!("Failed to create run: {e}")))?;

            cp.transition_task_state(task.id, TaskState::Running)
                .map_err(|e| {
                    ClientError::OperationFailed(format!(
                        "Failed to transition task to Running: {e}"
                    ))
                })?;

            cp.transition_run_state(run.id, RunState::Starting)
                .map_err(|e| {
                    ClientError::OperationFailed(format!(
                        "Failed to transition run to Starting: {e}"
                    ))
                })?;
            cp.transition_run_state(run.id, RunState::Running)
                .map_err(|e| {
                    ClientError::OperationFailed(format!(
                        "Failed to transition run to Running: {e}"
                    ))
                })?;

            Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
                .await;

            (task.id, run.id, agent.id)
        };

        // 2. Discover runtime instance
        let instances =
            self.runtime.discover().await.map_err(|e| {
                ClientError::OperationFailed(format!("Runtime discovery error: {e}"))
            })?;

        let instance = instances
            .iter()
            .find(|i| i.is_available())
            .or_else(|| instances.first())
            .ok_or_else(|| {
                ClientError::OperationFailed("No runtime instances discovered".to_string())
            })?;

        // 3. Build RuntimeStartRequest with correlation
        let correlation = RuntimeCorrelation::new()
            .with_studio_id(self.studio_id)
            .with_task_id(task_id)
            .with_run_id(run_id)
            .with_agent_id(agent_id);

        let instance_ref = RuntimeInstanceRef::new(
            self.runtime.implementation_id().clone(),
            instance.instance_id,
        );

        let cwd_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let workspace = ExecutionWorkspace::Managed {
            worktree_id: WorktreeId::new(),
            root: cwd_path.clone(),
            cwd: cwd_path.clone(),
            source_root: cwd_path.clone(),
            source_cwd: cwd_path,
            base_sha: "HEAD".to_string(),
        };

        let start_req = RuntimeStartRequest::new(instance_ref, workspace)
            .with_workspace_access_mode(WorkspaceAccessMode::ReadOnly)
            .with_initial_prompt(prompt.to_string())
            .with_correlation(correlation);

        let handle = self.runtime.start(start_req).await.map_err(|e| {
            ClientError::OperationFailed(format!("Failed to start runtime session: {e}"))
        })?;

        let session_ref = handle.session_ref();
        {
            let mut active = self.active_sessions.write().await;
            active.push(session_ref.clone());
        }

        // 4. Subscribe to runtime events
        let mut runtime_sub = self
            .runtime
            .events_after(&session_ref, None)
            .await
            .map_err(|e| {
                ClientError::OperationFailed(format!("Failed to subscribe to runtime events: {e}"))
            })?;

        // 5. Ingestion loop in background: routes RuntimeEvents into Control Plane & event stream
        let cp_arc = self.control_plane.clone();
        let studio_id = self.studio_id;
        let events_tx = self.events_tx.clone();
        let events_history = self.events_history.clone();

        tokio::spawn(async move {
            while let Ok(Some(runtime_event)) = runtime_sub.next_event().await {
                let mut cp = cp_arc.lock().await;
                match &runtime_event.kind {
                    RuntimeEventKind::OutputDelta { text } => {
                        let _ = cp.record_assistant_chunk(
                            studio_id,
                            agent_id,
                            Some(task_id),
                            Some(run_id),
                            text,
                            false,
                        );
                    }
                    RuntimeEventKind::Diagnostic { message, .. } => {
                        let _ = cp.record_assistant_chunk(
                            studio_id,
                            agent_id,
                            Some(task_id),
                            Some(run_id),
                            message.as_str(),
                            true,
                        );
                    }
                    RuntimeEventKind::ToolStarted {
                        tool_name,
                        tool_call_id,
                    } => {
                        let _ = cp.record_tool_started(
                            studio_id,
                            agent_id,
                            Some(task_id),
                            Some(run_id),
                            tool_name,
                            tool_call_id,
                            Utc::now(),
                        );
                    }
                    RuntimeEventKind::ToolCompleted {
                        tool_name,
                        tool_call_id,
                        duration_ms,
                        success,
                    } => {
                        if *success {
                            let _ = cp.record_tool_completed(
                                studio_id,
                                agent_id,
                                Some(task_id),
                                Some(run_id),
                                tool_name,
                                tool_call_id,
                                *duration_ms,
                                Utc::now(),
                            );
                        } else {
                            let _ = cp.record_tool_failed(
                                studio_id,
                                agent_id,
                                Some(task_id),
                                Some(run_id),
                                tool_name,
                                tool_call_id,
                                "Tool execution reported failure",
                                *duration_ms,
                                Utc::now(),
                            );
                        }
                    }
                    RuntimeEventKind::Completed { .. } => {
                        let _ = cp.transition_run_state(run_id, RunState::Succeeded);
                        let _ = cp.transition_task_state(task_id, TaskState::Succeeded);
                    }
                    RuntimeEventKind::Failed { safe_error_summary } => {
                        let _ = cp.record_run_outcome(
                            run_id,
                            "failed",
                            Some(safe_error_summary.as_str().to_string()),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Failed);
                        let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    }
                    _ => {}
                }

                Self::sync_events_internal(&cp, studio_id, &events_history, &events_tx).await;
            }
        });

        Ok(task_id)
    }

    async fn cancel_active_run(&self, reason: Option<String>) -> Result<(), ClientError> {
        let mut cp = self.control_plane.lock().await;
        let _ = cp
            .request_cancellation(CancellationScope::Studio(self.studio_id), reason)
            .map_err(|e| {
                ClientError::OperationFailed(format!("Failed to request cancellation: {e}"))
            })?;

        Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
            .await;

        let active_sessions = {
            let active = self.active_sessions.read().await;
            active.clone()
        };

        for session in active_sessions {
            let _ = self.runtime.stop(&session).await;
        }

        Ok(())
    }

    async fn subscribe_events(
        &self,
        from_sequence: u64,
    ) -> Result<ClientSubscription, ClientError> {
        let history = self.events_history.read().await;
        let backfill: Vec<EventEnvelope> = history
            .iter()
            .filter(|env| env.sequence >= from_sequence)
            .cloned()
            .collect();

        let mut last_delivered = backfill
            .last()
            .map(|e| e.sequence)
            .unwrap_or(from_sequence.saturating_sub(1));

        drop(history);

        let (tx, rx) = mpsc::channel(512);
        let mut bcast_rx = self.events_tx.subscribe();
        let history_arc = self.events_history.clone();

        tokio::spawn(async move {
            loop {
                match bcast_rx.recv().await {
                    Ok(env) => {
                        if env.sequence > last_delivered {
                            last_delivered = env.sequence;
                            if tx.send(env).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let history = history_arc.read().await;
                        let missed: Vec<EventEnvelope> = history
                            .iter()
                            .filter(|e| e.sequence > last_delivered)
                            .cloned()
                            .collect();
                        drop(history);
                        for env in missed {
                            last_delivered = env.sequence;
                            if tx.send(env).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        Ok(ClientSubscription {
            backfill,
            stream: rx,
        })
    }

    async fn get_task_graph(&self) -> Result<TaskGraphSnapshot, ClientError> {
        let cp = self.control_plane.lock().await;
        Ok(build_task_graph_snapshot(self.studio_id, cp.state()))
    }

    async fn get_agent_summaries(&self) -> Result<Vec<AgentSummary>, ClientError> {
        let cp = self.control_plane.lock().await;
        let history = self.events_history.read().await;

        let mut summaries = Vec::new();
        for agent in cp.state().agents.values() {
            if agent.studio_id != self.studio_id {
                continue;
            }
            if let Some(summary) = project_agent_summary(agent.id, &history, None) {
                summaries.push(summary);
            }
        }

        Ok(summaries)
    }

    async fn get_task_timeline(
        &self,
        task_id: TaskId,
    ) -> Result<TaskTimelineProjection, ClientError> {
        let history = self.events_history.read().await;
        project_task_timeline(task_id, &history)
            .ok_or_else(|| ClientError::NotFound(format!("Task {task_id} timeline not found")))
    }

    async fn resolve_approval(
        &self,
        approval_id: ApprovalId,
        decision: ClientApprovalDecision,
    ) -> Result<(), ClientError> {
        let mut cp = self.control_plane.lock().await;

        let target_state = match decision {
            ClientApprovalDecision::Allow | ClientApprovalDecision::AllowForSession => {
                ApprovalState::Approved
            }
            ClientApprovalDecision::Deny => ApprovalState::Denied,
        };

        cp.resolve_approval(approval_id, target_state)
            .map_err(|e| {
                ClientError::OperationFailed(format!("Failed to resolve approval: {e}"))
            })?;

        Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
            .await;

        Ok(())
    }

    async fn list_worktrees(&self) -> Result<Vec<WorktreeRecordSummary>, ClientError> {
        let cp = self.control_plane.lock().await;
        let mut summaries = Vec::new();

        for wt in cp.state().worktrees.values() {
            if wt.studio_id == self.studio_id {
                summaries.push(WorktreeRecordSummary {
                    worktree_id: wt.id,
                    rel_path: wt.name.clone(),
                    state: format!("{:?}", wt.state),
                });
            }
        }

        Ok(summaries)
    }

    async fn get_worktree_diff(
        &self,
        _worktree_id: WorktreeId,
    ) -> Result<CapturedChanges, ClientError> {
        Err(ClientError::Unsupported(
            "Worktree diff inspection is not attached in embedded mode".to_string(),
        ))
    }

    async fn reconcile_patch(
        &self,
        _reconciliation_id: ReconciliationId,
    ) -> Result<ReconciliationOutcome, ClientError> {
        Err(ClientError::Unsupported(
            "Git reconciliation engine is not attached in embedded mode; requires external workspace reconciler".to_string(),
        ))
    }
}
