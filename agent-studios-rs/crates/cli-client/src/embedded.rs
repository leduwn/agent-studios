//! In-process Embedded Implementation of AgentStudiosClient
//!
//! Owns the deterministic in-process ControlPlane, EventStore, and local event broadcast channel.
//! Dispatches submitted goals through the runtime layer (external runtime or local deterministic adapter)
//! to produce streaming assistant events and task execution transitions.

use async_trait::async_trait;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use agent_studios_control_plane::clock::SystemClock;
use agent_studios_control_plane::engine::ControlPlane;
use agent_studios_control_plane::store::{EventStore, InMemoryStore};
use agent_studios_external_runtime::{
    AgentRuntime, RuntimeCorrelation, RuntimeEventKind, RuntimeInstanceRef, RuntimeSessionId,
    RuntimeSessionRef, RuntimeStartRequest, WorkspaceAccessMode,
};
use agent_studios_orchestration::projection::{
    build_task_graph_snapshot, project_agent_summary, project_task_timeline,
};
use agent_studios_orchestration::read_models::{
    AgentSummary, TaskGraphSnapshot, TaskTimelineProjection,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::approval::{ApprovalKind, ApprovalState};
use agent_studios_protocol::cancellation::CancellationScope;
use agent_studios_protocol::event::{ControlPlaneEvent, EventEnvelope};
use agent_studios_protocol::id::{
    AgentId, ApprovalId, EventId, ReconciliationId, RunId, StudioId, TaskId, WorktreeId,
};
use agent_studios_protocol::run::RunState;
use agent_studios_protocol::task::TaskState;
use agent_studios_protocol::worktree::{ExecutionWorkspace, WorktreeState};
use agent_studios_workspace::WorkspaceOrchestrator;
use agent_studios_workspace::capture::CapturedChanges;
use agent_studios_workspace::reconciliation::ReconciliationOutcome;

use crate::client::{
    AgentStudiosClient, ClientApprovalDecision, ClientError, ClientSubscription, ExecutionMode,
    GoalSubmissionOptions, RuntimeMode, StreamSyncState, WorkspaceInfo, WorktreeRecordSummary,
};
use crate::runtime::{resolve_default_runtime, resolve_runtime};

fn default_orchestrator() -> Result<Arc<WorkspaceOrchestrator>, ClientError> {
    let base = match std::env::var_os("AGENT_STUDIOS_HOME") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::env::temp_dir().join("agent-studios"),
    };
    let wt_root = base.join("worktrees");
    let art_root = base.join("artifacts");
    let orchestrator = WorkspaceOrchestrator::new(wt_root, art_root).map_err(|e| {
        ClientError::OperationFailed(format!("Failed to initialize workspace orchestrator: {e}"))
    })?;
    Ok(Arc::new(orchestrator))
}

/// In-process client running the deterministic control plane inside the CLI process.
pub struct EmbeddedClient {
    studio_id: StudioId,
    workspace_name: String,
    control_plane: Arc<Mutex<ControlPlane<SystemClock, InMemoryStore>>>,
    events_tx: broadcast::Sender<EventEnvelope>,
    events_history: Arc<RwLock<Vec<EventEnvelope>>>,
    runtime: Arc<dyn AgentRuntime>,
    active_sessions: Arc<RwLock<Vec<RuntimeSessionRef>>>,
    approval_sessions: Arc<RwLock<HashMap<ApprovalId, RuntimeSessionId>>>,
    mode: RuntimeMode,
    orchestrator: Arc<WorkspaceOrchestrator>,
    working_dir: Option<std::path::PathBuf>,
}

impl EmbeddedClient {
    /// Bootstraps a fresh in-process EmbeddedClient using default discovered runtime.
    pub async fn bootstrap(workspace_name: &str) -> Result<Self, ClientError> {
        Self::bootstrap_with_mode(workspace_name, RuntimeMode::Demo).await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with explicit runtime execution mode.
    pub async fn bootstrap_with_mode(
        workspace_name: &str,
        mode: RuntimeMode,
    ) -> Result<Self, ClientError> {
        let runtime = resolve_runtime(mode).await?;
        Self::bootstrap_with_runtime_mode_and_capacity(workspace_name, runtime, mode, 512).await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with custom broadcast channel capacity.
    pub async fn bootstrap_with_capacity(
        workspace_name: &str,
        capacity: usize,
    ) -> Result<Self, ClientError> {
        let runtime = resolve_default_runtime().await;
        Self::bootstrap_with_runtime_mode_and_capacity(
            workspace_name,
            runtime,
            RuntimeMode::Demo,
            capacity,
        )
        .await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with an explicit runtime adapter.
    pub async fn bootstrap_with_runtime(
        workspace_name: &str,
        runtime: Arc<dyn AgentRuntime>,
    ) -> Result<Self, ClientError> {
        Self::bootstrap_with_runtime_mode_and_capacity(
            workspace_name,
            runtime,
            RuntimeMode::Demo,
            512,
        )
        .await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with explicit runtime adapter and capacity.
    pub async fn bootstrap_with_runtime_and_capacity(
        workspace_name: &str,
        runtime: Arc<dyn AgentRuntime>,
        capacity: usize,
    ) -> Result<Self, ClientError> {
        Self::bootstrap_with_runtime_mode_and_capacity(
            workspace_name,
            runtime,
            RuntimeMode::Demo,
            capacity,
        )
        .await
    }

    /// Bootstraps a fresh in-process EmbeddedClient with explicit runtime adapter, mode, and capacity.
    pub async fn bootstrap_with_runtime_mode_and_capacity(
        workspace_name: &str,
        runtime: Arc<dyn AgentRuntime>,
        mode: RuntimeMode,
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

        let orchestrator = default_orchestrator()?;

        Ok(Self {
            studio_id,
            workspace_name: workspace_name.to_string(),
            control_plane: Arc::new(Mutex::new(cp)),
            events_tx: tx,
            events_history,
            runtime,
            active_sessions: Arc::new(RwLock::new(Vec::new())),
            approval_sessions: Arc::new(RwLock::new(HashMap::new())),
            mode,
            orchestrator,
            working_dir: None,
        })
    }

    /// Overrides the internal workspace orchestrator (useful for test isolation).
    pub fn with_orchestrator(mut self, orchestrator: Arc<WorkspaceOrchestrator>) -> Self {
        self.orchestrator = orchestrator;
        self
    }

    /// Overrides working directory for workspace root discovery and worktree allocation.
    pub fn with_working_dir(mut self, working_dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    /// Access internal workspace orchestrator.
    pub fn orchestrator(&self) -> &Arc<WorkspaceOrchestrator> {
        &self.orchestrator
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

    #[allow(
        dead_code,
        clippy::too_many_arguments,
        clippy::needless_borrows_for_generic_args
    )]
    async fn fail_task_and_run(
        cp: &mut ControlPlane<SystemClock, InMemoryStore>,
        studio_id: StudioId,
        task_id: TaskId,
        run_id: RunId,
        agent_id: AgentId,
        err_msg: &str,
        events_history: &RwLock<Vec<EventEnvelope>>,
        events_tx: &broadcast::Sender<EventEnvelope>,
    ) {
        if let Err(e) = cp.record_assistant_chunk(
            studio_id,
            agent_id,
            Some(task_id),
            Some(run_id),
            &format!("[Startup Error] {err_msg}\n"),
            false,
        ) {
            tracing::warn!("Failed to record assistant error chunk in Control Plane: {e}");
        }
        if let Err(e) = cp.record_run_outcome(run_id, "startup_failed", Some(err_msg.to_string())) {
            tracing::warn!("Failed to record run outcome in Control Plane: {e}");
        }
        if let Err(e) = cp.transition_run_state(run_id, RunState::Failed) {
            tracing::warn!("Failed to transition run {run_id} to Failed: {e}");
        }
        if let Err(e) = cp.transition_task_state(task_id, TaskState::Failed) {
            tracing::warn!("Failed to transition task {task_id} to Failed: {e}");
        }
        Self::sync_events_internal(cp, studio_id, events_history, events_tx).await;
    }

    #[allow(dead_code)]
    async fn record_startup_failure(
        &self,
        task_id: TaskId,
        run_id: RunId,
        agent_id: AgentId,
        err_msg: &str,
    ) {
        let mut cp = self.control_plane.lock().await;
        Self::fail_task_and_run(
            &mut cp,
            self.studio_id,
            task_id,
            run_id,
            agent_id,
            err_msg,
            &self.events_history,
            &self.events_tx,
        )
        .await;
    }

    /// Access internal active runtime sessions (for inspection/testing).
    pub fn active_sessions(&self) -> &Arc<RwLock<Vec<RuntimeSessionRef>>> {
        &self.active_sessions
    }

    /// Access internal control plane instance (for inspection/testing).
    pub fn control_plane(&self) -> &Arc<Mutex<ControlPlane<SystemClock, InMemoryStore>>> {
        &self.control_plane
    }

    /// Injects an event envelope directly into history and/or broadcast channel.
    /// Used for testing gap recovery, broadcast loss, and fault injection.
    #[doc(hidden)]
    pub async fn inject_raw_event_for_testing(
        &self,
        envelope: EventEnvelope,
        to_history: bool,
        to_broadcast: bool,
    ) {
        if to_history {
            let mut history = self.events_history.write().await;
            history.push(envelope.clone());
        }
        if to_broadcast {
            let _ = self.events_tx.send(envelope);
        }
    }
}

fn is_terminal_event(event: &ControlPlaneEvent) -> bool {
    match event {
        ControlPlaneEvent::TaskStateChanged { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::RunStateChanged { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::RunOutcomeRecorded { .. } => true,
        ControlPlaneEvent::ApprovalResolved { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::AgentStateChanged { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::WorktreeStateChanged { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::WorktreeReleased { .. } => true,
        ControlPlaneEvent::ReconciliationStateChanged { new_state, .. } => new_state.is_terminal(),
        ControlPlaneEvent::ReconciliationApplied { .. }
        | ControlPlaneEvent::ReconciliationFailed { .. } => true,
        ControlPlaneEvent::CancellationRequested { .. } => true,
        _ => false,
    }
}

async fn deliver_events_with_continuity_check(
    candidates: Vec<EventEnvelope>,
    last_delivered: &mut u64,
    sync_state: &mut StreamSyncState,
    sync_state_tx: &tokio::sync::watch::Sender<StreamSyncState>,
    delivered_terminal_events: &mut HashSet<EventId>,
    tx: &mpsc::Sender<EventEnvelope>,
) -> bool {
    for candidate in candidates {
        if candidate.sequence <= *last_delivered {
            continue;
        }

        if candidate.sequence == *last_delivered + 1 {
            *last_delivered = candidate.sequence;
            if *sync_state == StreamSyncState::Recovering {
                *sync_state = StreamSyncState::Healthy;
                sync_state_tx.send_replace(StreamSyncState::Healthy);
            }
            if tx.send(candidate).await.is_err() {
                return false;
            }
        } else {
            let expected = *last_delivered + 1;
            let found = candidate.sequence;
            let gap_state = StreamSyncState::GapUnrecoverable { expected, found };
            if *sync_state != gap_state {
                *sync_state = gap_state;
                sync_state_tx.send_replace(gap_state);
                tracing::warn!(
                    studio_id = %candidate.studio_id,
                    last_delivered = *last_delivered,
                    expected,
                    found,
                    "Unrecoverable sequence gap detected: missing events in history; projection degraded"
                );
            }

            if is_terminal_event(&candidate.event)
                && delivered_terminal_events.insert(candidate.event_id)
                && tx.send(candidate).await.is_err()
            {
                return false;
            }
        }
    }

    true
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

    fn runtime_mode(&self) -> RuntimeMode {
        self.mode
    }

    async fn get_workspace_info(&self) -> Result<WorkspaceInfo, ClientError> {
        let cwd_path = self.working_dir.clone().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        });
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
            let mut active = self.active_sessions.write().await;
            std::mem::take(&mut *active)
        };
        let mut stop_errors = Vec::new();
        for session in active_sessions {
            if let Err(e) = self.runtime.stop(&session).await {
                stop_errors.push(format!(
                    "Failed to stop session {}: {e}",
                    session.session_id
                ));
            }
        }
        self.approval_sessions.write().await.clear();
        if !stop_errors.is_empty() {
            return Err(ClientError::OperationFailed(stop_errors.join("; ")));
        }
        Ok(())
    }

    async fn submit_goal(
        &self,
        prompt: &str,
        options: GoalSubmissionOptions,
    ) -> Result<TaskId, ClientError> {
        // 1. Create Control Plane Task and Run, and transition to Running
        let (task_id, run_id, agent_id, cwd_path) = {
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

            let agent = match cp
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
            {
                Some(a) => a,
                None => {
                    let _ = cp.transition_task_state(task.id, TaskState::Failed);
                    Self::sync_events_internal(
                        &cp,
                        self.studio_id,
                        &self.events_history,
                        &self.events_tx,
                    )
                    .await;
                    return Err(ClientError::OperationFailed(
                        "No registered agent found for studio".to_string(),
                    ));
                }
            };

            let run = match cp.create_run(task.id, agent.id) {
                Ok(r) => r,
                Err(e) => {
                    let _ = cp.transition_task_state(task.id, TaskState::Failed);
                    Self::sync_events_internal(
                        &cp,
                        self.studio_id,
                        &self.events_history,
                        &self.events_tx,
                    )
                    .await;
                    return Err(ClientError::OperationFailed(format!(
                        "Failed to create run: {e}"
                    )));
                }
            };

            if let Err(e) = cp.transition_task_state(task.id, TaskState::Running) {
                let _ = cp.transition_run_state(run.id, RunState::Failed);
                let _ = cp.transition_task_state(task.id, TaskState::Failed);
                Self::sync_events_internal(
                    &cp,
                    self.studio_id,
                    &self.events_history,
                    &self.events_tx,
                )
                .await;
                return Err(ClientError::OperationFailed(format!(
                    "Failed to transition task to Running: {e}"
                )));
            }

            if let Err(e) = cp.transition_run_state(run.id, RunState::Starting) {
                Self::fail_task_and_run(
                    &mut cp,
                    self.studio_id,
                    task.id,
                    run.id,
                    agent.id,
                    &format!("Failed to transition run to Starting: {e}"),
                    &self.events_history,
                    &self.events_tx,
                )
                .await;
                return Err(ClientError::OperationFailed(format!(
                    "Failed to transition run to Starting: {e}"
                )));
            }

            if let Err(e) = cp.transition_run_state(run.id, RunState::Running) {
                Self::fail_task_and_run(
                    &mut cp,
                    self.studio_id,
                    task.id,
                    run.id,
                    agent.id,
                    &format!("Failed to transition run to Running: {e}"),
                    &self.events_history,
                    &self.events_tx,
                )
                .await;
                return Err(ClientError::OperationFailed(format!(
                    "Failed to transition run to Running: {e}"
                )));
            }

            let cwd_path = self.working_dir.clone().unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            });

            Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
                .await;

            (task.id, run.id, agent.id, cwd_path)
        };

        // Determine workspace access policy (ReadOnly vs Mutating)
        let access_mode = match options.workspace_access_mode {
            Some(mode) => mode,
            None => {
                if self.mode == RuntimeMode::Demo {
                    WorkspaceAccessMode::ReadOnly
                } else {
                    WorkspaceAccessMode::Mutating
                }
            }
        };

        // Provision isolated physical worktree via WorkspaceOrchestrator
        let managed = match self
            .orchestrator
            .create_worktree_async(cwd_path.clone(), None)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                let err_msg = format!("Failed to allocate physical worktree for execution: {e}");
                self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                    .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

        if managed.root == managed.source_root {
            let err_msg = "Physical worktree root equals source root; worktree isolation violated"
                .to_string();
            self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                .await;
            return Err(ClientError::OperationFailed(err_msg));
        }

        let mut cp = self.control_plane.lock().await;
        let wt = match cp.create_worktree(
            self.studio_id,
            format!("worktree-{}", task_id),
            managed.root.clone(),
            managed.cwd.clone(),
            &managed.head_sha,
        ) {
            Ok(wt) => wt,
            Err(e) => {
                let err_msg = format!("Failed to register worktree in Control Plane: {e}");
                Self::fail_task_and_run(
                    &mut cp,
                    self.studio_id,
                    task_id,
                    run_id,
                    agent_id,
                    &err_msg,
                    &self.events_history,
                    &self.events_tx,
                )
                .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

        if let Err(e) = cp.transition_worktree_state(wt.id, WorktreeState::Ready) {
            let err_msg = format!("Failed to transition worktree to Ready in Control Plane: {e}");
            Self::fail_task_and_run(
                &mut cp,
                self.studio_id,
                task_id,
                run_id,
                agent_id,
                &err_msg,
                &self.events_history,
                &self.events_tx,
            )
            .await;
            return Err(ClientError::OperationFailed(err_msg));
        }

        if let Err(e) = cp.assign_worktree(wt.id, task_id, agent_id, Some(run_id)) {
            let err_msg = format!("Failed to assign worktree in Control Plane: {e}");
            Self::fail_task_and_run(
                &mut cp,
                self.studio_id,
                task_id,
                run_id,
                agent_id,
                &err_msg,
                &self.events_history,
                &self.events_tx,
            )
            .await;
            return Err(ClientError::OperationFailed(err_msg));
        }

        if let Err(e) = cp.transition_worktree_state(wt.id, WorktreeState::InUse) {
            let err_msg = format!("Failed to transition worktree to InUse in Control Plane: {e}");
            Self::fail_task_and_run(
                &mut cp,
                self.studio_id,
                task_id,
                run_id,
                agent_id,
                &err_msg,
                &self.events_history,
                &self.events_tx,
            )
            .await;
            return Err(ClientError::OperationFailed(err_msg));
        }

        Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
            .await;

        let workspace = ExecutionWorkspace::managed(
            wt.id,
            managed.root,
            managed.cwd,
            managed.source_root,
            managed.source_cwd,
            managed.head_sha,
        );

        // 2. Discover runtime instance
        let instances = match self.runtime.discover().await {
            Ok(inst) => inst,
            Err(e) => {
                let err_msg = format!("Runtime discovery error: {e}");
                self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                    .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

        let instance = match instances
            .iter()
            .find(|i| i.is_available())
            .or_else(|| instances.first())
        {
            Some(inst) => inst,
            None => {
                let err_msg = "No runtime instances discovered".to_string();
                self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                    .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

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

        let start_req = RuntimeStartRequest::new(instance_ref, workspace)
            .with_workspace_access_mode(access_mode)
            .with_initial_prompt(prompt.to_string())
            .with_correlation(correlation);

        let handle = match self.runtime.start(start_req).await {
            Ok(h) => h,
            Err(e) => {
                let err_msg = format!("Failed to start runtime session: {e}");
                self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                    .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

        let session_ref = handle.session_ref();
        let session_id = session_ref.session_id;
        {
            let mut active = self.active_sessions.write().await;
            active.push(session_ref.clone());
        }

        // 4. Subscribe to runtime events
        let mut runtime_sub = match self.runtime.events_after(&session_ref, None).await {
            Ok(sub) => sub,
            Err(e) => {
                let err_msg = format!("Failed to subscribe to runtime events: {e}");
                if let Err(stop_err) = self.runtime.stop(&session_ref).await {
                    tracing::warn!(
                        "Failed to stop runtime session after subscription failure: {stop_err}"
                    );
                }
                {
                    let mut active = self.active_sessions.write().await;
                    active.retain(|s| s != &session_ref);
                }
                self.record_startup_failure(task_id, run_id, agent_id, &err_msg)
                    .await;
                return Err(ClientError::OperationFailed(err_msg));
            }
        };

        // 5. Ingestion loop in background: routes RuntimeEvents into Control Plane & event stream
        let cp_arc = self.control_plane.clone();
        let studio_id = self.studio_id;
        let events_tx = self.events_tx.clone();
        let events_history = self.events_history.clone();
        let active_sessions = self.active_sessions.clone();
        let approval_sessions = self.approval_sessions.clone();

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
                    RuntimeEventKind::ApprovalRequested {
                        approval_id,
                        description,
                        command,
                    } => {
                        // Authoritative typed session correlation: map approval_id -> session_id
                        {
                            let mut mapping = approval_sessions.write().await;
                            mapping.insert(*approval_id, session_id);
                        }

                        let kind = if command.is_some() {
                            ApprovalKind::CommandExecution
                        } else {
                            ApprovalKind::Other
                        };

                        let _ = cp.record_approval_request(
                            *approval_id,
                            studio_id,
                            task_id,
                            agent_id,
                            kind,
                            description.as_str(),
                        );
                    }
                    RuntimeEventKind::Completed { summary } => {
                        let _ = cp.record_run_outcome(run_id, "succeeded", summary.clone());
                        let _ = cp.transition_run_state(run_id, RunState::Succeeded);
                        let _ = cp.transition_task_state(task_id, TaskState::Succeeded);
                        active_sessions
                            .write()
                            .await
                            .retain(|s| s.session_id != session_id);
                        approval_sessions
                            .write()
                            .await
                            .retain(|_, sid| *sid != session_id);
                    }
                    RuntimeEventKind::Failed { safe_error_summary } => {
                        let _ = cp.record_run_outcome(
                            run_id,
                            "failed",
                            Some(safe_error_summary.as_str().to_string()),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Failed);
                        let _ = cp.transition_task_state(task_id, TaskState::Failed);
                        active_sessions
                            .write()
                            .await
                            .retain(|s| s.session_id != session_id);
                        approval_sessions
                            .write()
                            .await
                            .retain(|_, sid| *sid != session_id);
                    }
                    RuntimeEventKind::Stopped => {
                        let _ = cp.record_run_outcome(
                            run_id,
                            "stopped",
                            Some("Runtime session stopped".to_string()),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Cancelled);
                        let _ = cp.transition_task_state(task_id, TaskState::Cancelled);
                        active_sessions
                            .write()
                            .await
                            .retain(|s| s.session_id != session_id);
                        approval_sessions
                            .write()
                            .await
                            .retain(|_, sid| *sid != session_id);
                    }
                    RuntimeEventKind::Interrupted { reason } => {
                        let _ = cp.record_run_outcome(
                            run_id,
                            "interrupted",
                            Some(reason.as_str().to_string()),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Cancelled);
                        let _ = cp.transition_task_state(task_id, TaskState::Cancelled);
                        active_sessions
                            .write()
                            .await
                            .retain(|s| s.session_id != session_id);
                        approval_sessions
                            .write()
                            .await
                            .retain(|_, sid| *sid != session_id);
                    }
                    _ => {}
                }

                Self::sync_events_internal(&cp, studio_id, &events_history, &events_tx).await;
            }

            // Terminal lifecycle safety: reap exit status on stream termination so no run remains permanently in Running
            let mut cp = cp_arc.lock().await;
            if let Some(_run) = cp
                .state()
                .runs
                .get(&run_id)
                .filter(|r| !r.state.is_terminal())
            {
                let _ = cp.record_run_outcome(
                    run_id,
                    "aborted",
                    Some(
                        "Runtime event stream terminated without explicit terminal event"
                            .to_string(),
                    ),
                );
                let _ = cp.transition_run_state(run_id, RunState::Cancelled);
                let _ = cp.transition_task_state(task_id, TaskState::Cancelled);
                Self::sync_events_internal(&cp, studio_id, &events_history, &events_tx).await;
            }

            // Clean up active session and correlated approvals on session termination
            {
                let mut active = active_sessions.write().await;
                active.retain(|s| s.session_id != session_id);
            }
            {
                let mut mapping = approval_sessions.write().await;
                mapping.retain(|_, sid| *sid != session_id);
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

        let sessions = {
            let mut active = self.active_sessions.write().await;
            std::mem::take(&mut *active)
        };

        let mut stop_errors = Vec::new();
        let mut failed_to_stop = Vec::new();

        for session in sessions {
            if let Err(e) = self.runtime.stop(&session).await {
                stop_errors.push(format!(
                    "Failed to stop runtime session {}: {e}",
                    session.session_id
                ));
                failed_to_stop.push(session);
            }
        }

        if !failed_to_stop.is_empty() {
            let mut active = self.active_sessions.write().await;
            active.extend(failed_to_stop);
        } else {
            let mut mapping = self.approval_sessions.write().await;
            mapping.clear();
        }

        if !stop_errors.is_empty() {
            return Err(ClientError::OperationFailed(stop_errors.join("; ")));
        }

        Ok(())
    }

    async fn subscribe_events(
        &self,
        from_sequence: u64,
    ) -> Result<ClientSubscription, ClientError> {
        // Linearizable handoff: subscribe to broadcast channel BEFORE taking snapshot
        // so no events emitted during backfill extraction are lost.
        let mut bcast_rx = self.events_tx.subscribe();

        let history = self.events_history.read().await;
        let backfill: Vec<EventEnvelope> = history
            .iter()
            .filter(|env| env.sequence >= from_sequence)
            .cloned()
            .collect();

        // Check for any unrecoverable gap in backfill history
        if !backfill.is_empty() {
            let mut prev_seq = backfill[0].sequence;
            for env in &backfill[1..] {
                if env.sequence > prev_seq + 1 {
                    tracing::warn!(
                        studio_id = %env.studio_id,
                        expected = prev_seq + 1,
                        found = env.sequence,
                        "Unrecoverable sequence gap detected in backfill history"
                    );
                }
                prev_seq = env.sequence;
            }
        }

        let mut last_delivered = backfill
            .last()
            .map(|e| e.sequence)
            .unwrap_or(from_sequence.saturating_sub(1));

        drop(history);

        let (tx, rx) = mpsc::channel(512);
        let history_arc = self.events_history.clone();
        let (state_tx, state_rx) = tokio::sync::watch::channel(StreamSyncState::Healthy);

        tokio::spawn(async move {
            let mut delivered_terminal_events = HashSet::new();
            let mut sync_state = StreamSyncState::Healthy;

            loop {
                match bcast_rx.recv().await {
                    Ok(env) => {
                        // Deduplicate events already included in backfill or delivered
                        if env.sequence <= last_delivered {
                            continue;
                        }

                        let mut candidates = Vec::new();

                        // Monotonic gap recovery: if sequence jumped ahead, catch up from history
                        if env.sequence > last_delivered + 1 {
                            sync_state = StreamSyncState::Recovering;
                            state_tx.send_replace(StreamSyncState::Recovering);

                            let history = history_arc.read().await;
                            let mut missed: Vec<EventEnvelope> = history
                                .iter()
                                .filter(|e| {
                                    e.sequence > last_delivered && e.sequence < env.sequence
                                })
                                .cloned()
                                .collect();
                            drop(history);
                            missed.sort_by_key(|e| e.sequence);
                            candidates.extend(missed);
                        }

                        candidates.push(env);

                        if !deliver_events_with_continuity_check(
                            candidates,
                            &mut last_delivered,
                            &mut sync_state,
                            &state_tx,
                            &mut delivered_terminal_events,
                            &tx,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped_count)) => {
                        sync_state = StreamSyncState::Recovering;
                        state_tx.send_replace(StreamSyncState::Recovering);

                        tracing::warn!(
                            skipped_count = skipped_count,
                            last_delivered = last_delivered,
                            "Broadcast receiver lagged; recovering missing events from history"
                        );
                        let history = history_arc.read().await;
                        let mut missed: Vec<EventEnvelope> = history
                            .iter()
                            .filter(|e| e.sequence > last_delivered)
                            .cloned()
                            .collect();
                        drop(history);
                        missed.sort_by_key(|e| e.sequence);

                        if !deliver_events_with_continuity_check(
                            missed,
                            &mut last_delivered,
                            &mut sync_state,
                            &state_tx,
                            &mut delivered_terminal_events,
                            &tx,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        sync_state = StreamSyncState::Closed;
                        state_tx.send_replace(StreamSyncState::Closed);

                        // Final drain on channel close to deliver any pending events recorded in history
                        let history = history_arc.read().await;
                        let mut missed: Vec<EventEnvelope> = history
                            .iter()
                            .filter(|e| e.sequence > last_delivered)
                            .cloned()
                            .collect();
                        drop(history);
                        missed.sort_by_key(|e| e.sequence);

                        let _ = deliver_events_with_continuity_check(
                            missed,
                            &mut last_delivered,
                            &mut sync_state,
                            &state_tx,
                            &mut delivered_terminal_events,
                            &tx,
                        )
                        .await;

                        break;
                    }
                }
            }
        });

        Ok(ClientSubscription {
            backfill,
            stream: rx,
            sync_state: state_rx,
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
        // Enforce fail-closed approval bridge: require exact correlated active runtime session
        let session_id = {
            let mapping = self.approval_sessions.read().await;
            mapping.get(&approval_id).copied()
        };

        let session_id = session_id.ok_or_else(|| {
            ClientError::OperationFailed(format!(
                "No active runtime session correlated with approval ID {approval_id}"
            ))
        })?;

        let active_session = {
            let active = self.active_sessions.read().await;
            active.iter().find(|s| s.session_id == session_id).cloned()
        };

        let session = active_session.ok_or_else(|| {
            ClientError::OperationFailed(format!(
                "No active runtime session correlated with approval ID {approval_id}"
            ))
        })?;

        // Authoritative Control Plane verification before transport delivery
        {
            let cp = self.control_plane.lock().await;
            let approval = cp.get_approval(approval_id).ok_or_else(|| {
                ClientError::NotFound(format!(
                    "Control Plane approval record {approval_id} not found"
                ))
            })?;
            if approval.state != ApprovalState::Pending {
                return Err(ClientError::OperationFailed(format!(
                    "Approval {approval_id} is already in terminal state {:?}, cannot resolve",
                    approval.state
                )));
            }
        }

        let approved = matches!(
            decision,
            ClientApprovalDecision::Allow | ClientApprovalDecision::AllowForSession
        );

        let input = agent_studios_external_runtime::RuntimeInput::ApprovalResponse {
            approval_id,
            approved,
            reason: None,
        };

        // Deliver resolution frame to runtime first; fail closed if delivery fails
        self.runtime.send(&session, input).await.map_err(|e| {
            ClientError::OperationFailed(format!(
                "Failed to deliver approval resolution frame to runtime: {e}"
            ))
        })?;

        let mut cp = self.control_plane.lock().await;

        let target_state = if approved {
            ApprovalState::Approved
        } else {
            ApprovalState::Denied
        };

        let approval = cp.get_approval(approval_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "Control Plane approval record {approval_id} disappeared during resolution"
            ))
        })?;

        if approval.state != ApprovalState::Pending {
            return Err(ClientError::OperationFailed(format!(
                "Approval {approval_id} transitioned to terminal state {:?} during resolution",
                approval.state
            )));
        }

        cp.resolve_approval(approval_id, target_state)
            .map_err(|e| {
                ClientError::OperationFailed(format!(
                    "Failed to resolve approval in Control Plane: {e}"
                ))
            })?;

        Self::sync_events_internal(&cp, self.studio_id, &self.events_history, &self.events_tx)
            .await;

        // Clean up correlation mapping on successful resolution
        {
            let mut mapping = self.approval_sessions.write().await;
            mapping.remove(&approval_id);
        }

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
