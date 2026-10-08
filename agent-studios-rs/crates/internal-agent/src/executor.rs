use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_studios_protocol::agent::AgentState;
use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_protocol::worktree::ExecutionWorkspace;
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use agent_studios_workspace::WorkspaceOrchestrator;
use async_trait::async_trait;
use codex_config::Constrained;
use codex_core::config::{Config, Permissions};
use codex_core::{
    AgentControl, AgentInput, AgentTarget, CodexThread, SendRequest, SpawnAgentOptions,
    SpawnRequest, StartThreadOptions, ThreadManager, TurnInputRequest, TurnStartOptions,
};
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::{
    AskForApproval, EventMsg, Op, SandboxPolicy, SessionSource, SubAgentSource,
};
use codex_protocol::user_input::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use tokio::sync::RwLock;

use crate::budget::{AgentBudgetTracker, BudgetScopeId};
use crate::contributor::AgentRuntimeExtensionContext;
use crate::control_plane_actor::ControlPlaneHandle;
use crate::error::InternalAgentError;
use crate::profile::{AgentExecutionBudget, InternalAgentSpec};

#[derive(Clone, Debug)]
pub struct AgentExecutionContext {
    pub studio_id: StudioId,
    pub task_id: Option<TaskId>,
    pub run_id: Option<RunId>,
    pub parent_agent_id: Option<AgentId>,
    pub agent_spec: InternalAgentSpec,
    pub prompt: String,
    pub budget: AgentExecutionBudget,
    pub output_schema: Option<serde_json::Value>,
    pub execution_workspace: ExecutionWorkspace,
}

impl AgentExecutionContext {
    pub fn new(
        studio_id: StudioId,
        agent_spec: InternalAgentSpec,
        prompt: impl Into<String>,
    ) -> Self {
        let budget = agent_spec.budget.clone();
        Self {
            studio_id,
            task_id: None,
            run_id: None,
            parent_agent_id: None,
            agent_spec,
            prompt: prompt.into(),
            budget,
            output_schema: None,
            execution_workspace: ExecutionWorkspace::default(),
        }
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_run_id(mut self, run_id: RunId) -> Self {
        self.run_id = Some(run_id);
        self
    }

    pub fn with_parent_agent_id(mut self, parent_id: AgentId) -> Self {
        self.parent_agent_id = Some(parent_id);
        self
    }

    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    pub fn with_execution_workspace(mut self, workspace: ExecutionWorkspace) -> Self {
        self.execution_workspace = workspace;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExecutionResult {
    pub output: String,
    pub turns_used: u32,
    pub tool_calls_used: u32,
    pub duration_secs: u64,
    pub success: bool,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError>;

    async fn cancel_agent(&self, _agent_id: AgentId) -> Result<(), InternalAgentError> {
        Ok(())
    }

    fn validate_agent_spec(&self, _spec: &InternalAgentSpec) -> Result<(), InternalAgentError> {
        Ok(())
    }
}

type MockHandler = Arc<
    dyn Fn(AgentExecutionContext) -> Result<AgentExecutionResult, InternalAgentError> + Send + Sync,
>;

#[derive(Clone, Default)]
pub struct MockAgentExecutor {
    results_by_role: Arc<Mutex<HashMap<String, AgentExecutionResult>>>,
    results_by_alias: Arc<Mutex<HashMap<String, AgentExecutionResult>>>,
    custom_handler: Arc<Mutex<Option<MockHandler>>>,
    recorded_executions: Arc<Mutex<Vec<AgentExecutionContext>>>,
    default_result: Arc<Mutex<Option<AgentExecutionResult>>>,
    cancelled_agents: Arc<Mutex<Vec<AgentId>>>,
}

impl MockAgentExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_result_for_role(&self, role: impl Into<String>, result: AgentExecutionResult) {
        self.results_by_role
            .lock()
            .unwrap()
            .insert(role.into(), result);
    }

    pub fn set_result_for_alias(&self, alias: impl Into<String>, result: AgentExecutionResult) {
        self.results_by_alias
            .lock()
            .unwrap()
            .insert(alias.into(), result);
    }

    pub fn set_default_result(&self, result: AgentExecutionResult) {
        *self.default_result.lock().unwrap() = Some(result);
    }

    pub fn set_handler<F>(&self, handler: F)
    where
        F: Fn(AgentExecutionContext) -> Result<AgentExecutionResult, InternalAgentError>
            + Send
            + Sync
            + 'static,
    {
        *self.custom_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    pub fn recorded_executions(&self) -> Vec<AgentExecutionContext> {
        self.recorded_executions.lock().unwrap().clone()
    }

    pub fn cancelled_agents(&self) -> Vec<AgentId> {
        self.cancelled_agents.lock().unwrap().clone()
    }
}

#[async_trait]
impl AgentExecutor for MockAgentExecutor {
    async fn cancel_agent(&self, agent_id: AgentId) -> Result<(), InternalAgentError> {
        self.cancelled_agents.lock().unwrap().push(agent_id);
        Ok(())
    }
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError> {
        self.recorded_executions
            .lock()
            .unwrap()
            .push(context.clone());

        let maybe_handler = self.custom_handler.lock().unwrap().clone();
        if let Some(handler) = maybe_handler {
            return handler(context);
        }

        if let Some(res) = self
            .results_by_alias
            .lock()
            .unwrap()
            .get(&context.agent_spec.display_name)
        {
            return Ok(res.clone());
        }

        if let Some(res) = self
            .results_by_role
            .lock()
            .unwrap()
            .get(&context.agent_spec.role)
        {
            return Ok(res.clone());
        }

        if let Some(ref res) = *self.default_result.lock().unwrap() {
            return Ok(res.clone());
        }

        Ok(AgentExecutionResult {
            output: format!(
                "Execution completed for agent {}",
                context.agent_spec.display_name
            ),
            turns_used: 1,
            tool_calls_used: 0,
            duration_secs: 0,
            success: true,
        })
    }
}

#[derive(Clone)]
pub struct RunningAgentState {
    pub thread_id: ThreadId,
    pub thread: Arc<CodexThread>,
    pub agent_control: Arc<dyn AgentControl>,
    pub active_turn: Arc<AtomicBool>,
    pub current_run_id: Option<RunId>,
    pub parent_agent_id: Option<AgentId>,
    pub extension_context: Arc<AgentRuntimeExtensionContext>,
    pub budget_tracker: Arc<AgentBudgetTracker>,
    pub bound_workspace: ExecutionWorkspace,
}

impl std::fmt::Debug for RunningAgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningAgentState")
            .field("thread_id", &self.thread_id)
            .field("active_turn", &self.active_turn.load(Ordering::Relaxed))
            .field("current_run_id", &self.current_run_id)
            .field("parent_agent_id", &self.parent_agent_id)
            .field("bound_workspace", &self.bound_workspace)
            .finish()
    }
}

/// RAII guard ensuring active_turn atomic boolean is reset to false on drop.
pub struct ActiveTurnGuard {
    active_turn: Arc<AtomicBool>,
}

impl ActiveTurnGuard {
    pub fn new(active_turn: Arc<AtomicBool>) -> Self {
        active_turn.store(true, Ordering::SeqCst);
        Self { active_turn }
    }
}

impl Drop for ActiveTurnGuard {
    fn drop(&mut self) {
        self.active_turn.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub struct CodexAgentExecutor {
    thread_manager: Arc<ThreadManager>,
    session_factory: Arc<AgentStudiosRuntimeSessionFactory>,
    config: Arc<Config>,
    control_plane: ControlPlaneHandle,
    running_agents: Arc<RwLock<HashMap<AgentId, RunningAgentState>>>,
    coordinator_agent_id: Arc<RwLock<Option<AgentId>>>,
    workspace_orchestrator: Option<Arc<WorkspaceOrchestrator>>,
}

impl CodexAgentExecutor {
    async fn shutdown_and_wait_thread(
        agent_id: AgentId,
        thread: &Arc<CodexThread>,
        timeout: Duration,
    ) -> Result<(), InternalAgentError> {
        thread.submit(Op::Shutdown).await.map_err(|e| {
            InternalAgentError::ThreadShutdownFailed {
                agent_id,
                error: e.to_string(),
            }
        })?;
        tokio::time::timeout(timeout, thread.wait_until_terminated())
            .await
            .map_err(|_| InternalAgentError::ThreadShutdownTimeout { agent_id })
    }
    pub fn try_new(
        session_factory: Arc<AgentStudiosRuntimeSessionFactory>,
        thread_manager: Arc<ThreadManager>,
        config: Arc<Config>,
        control_plane: ControlPlaneHandle,
    ) -> Result<Self, InternalAgentError> {
        Ok(Self {
            thread_manager,
            session_factory,
            config,
            control_plane,
            running_agents: Arc::new(RwLock::new(HashMap::new())),
            coordinator_agent_id: Arc::new(RwLock::new(None)),
            workspace_orchestrator: None,
        })
    }

    pub fn with_workspace_orchestrator(mut self, orchestrator: Arc<WorkspaceOrchestrator>) -> Self {
        self.workspace_orchestrator = Some(orchestrator);
        self
    }

    pub fn workspace_orchestrator(&self) -> Option<&Arc<WorkspaceOrchestrator>> {
        self.workspace_orchestrator.as_ref()
    }

    pub fn validate_agent_spec(&self, spec: &InternalAgentSpec) -> Result<(), InternalAgentError> {
        self.session_factory
            .prepare_runtime_session(&spec.model_ref)
            .map_err(InternalAgentError::RuntimeSession)?;
        Ok(())
    }

    pub fn running_agents(&self) -> Arc<RwLock<HashMap<AgentId, RunningAgentState>>> {
        Arc::clone(&self.running_agents)
    }

    pub fn factory(&self) -> &AgentStudiosRuntimeSessionFactory {
        &self.session_factory
    }

    pub async fn cancel_agent(&self, agent_id: AgentId) -> Result<(), InternalAgentError> {
        let state = {
            let guard = self.running_agents.read().await;
            guard.get(&agent_id).cloned()
        };

        if let Some(state) = state {
            state.active_turn.store(false, Ordering::SeqCst);
            let _ = state.thread.submit(Op::Interrupt).await;

            let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
            while tokio::time::Instant::now() < deadline {
                let status = state.thread.agent_status().await;
                if !matches!(status, codex_protocol::protocol::AgentStatus::Running) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        Ok(())
    }

    pub async fn retire_worker(
        &self,
        agent_id: AgentId,
        state: &RunningAgentState,
    ) -> Result<(), InternalAgentError> {
        if state.active_turn.load(Ordering::SeqCst) {
            return Err(InternalAgentError::WorkspaceAffinityConflict { agent_id });
        }
        let shutdown_timeout = Duration::from_secs(5);
        Self::shutdown_and_wait_thread(agent_id, &state.thread, shutdown_timeout)
            .await
            .map_err(|e| match e {
                InternalAgentError::ThreadShutdownTimeout { agent_id } => {
                    InternalAgentError::WorkerRetirementTimeout { agent_id }
                }
                other => other,
            })?;
        let mut guard = self.running_agents.write().await;
        guard.remove(&agent_id);
        self.thread_manager
            .remove_thread_if_matches(&state.thread_id, &state.thread)
            .await;
        Ok(())
    }

    async fn drain_events(
        &self,
        thread: &Arc<CodexThread>,
        active_turn: &Arc<AtomicBool>,
        tracker: &AgentBudgetTracker,
        run_id: Option<RunId>,
        agent_id: AgentId,
        studio_id: StudioId,
    ) -> Result<(String, u32, u32, bool), InternalAgentError> {
        let mut output = String::new();
        let mut success = false;
        let max_wall_clock = tracker.budget().max_wall_clock_secs;

        while active_turn.load(Ordering::SeqCst) {
            if let Some(limit_secs) = max_wall_clock {
                let total_elapsed = tracker.elapsed_secs();
                if total_elapsed >= limit_secs {
                    active_turn.store(false, Ordering::SeqCst);
                    let _ = thread.submit(Op::Interrupt).await;
                    let _ = self
                        .control_plane
                        .record_budget_exceeded(
                            studio_id,
                            agent_id,
                            run_id,
                            "wall_clock".to_string(),
                            limit_secs,
                            total_elapsed,
                        )
                        .await;
                    return Err(InternalAgentError::BudgetExceeded {
                        agent_id,
                        reason: format!(
                            "Wall-clock duration exceeded: limit {limit_secs}s, actual {total_elapsed}s"
                        ),
                    });
                }
            }

            let event_res =
                match tokio::time::timeout(Duration::from_millis(500), thread.next_event()).await {
                    Ok(res) => res,
                    Err(_) => continue,
                };

            let event = match event_res {
                Ok(ev) => ev,
                Err(e) => {
                    tracing::warn!(agent = %agent_id, error = %e, "Error reading next event from thread");
                    break;
                }
            };

            match event.msg {
                EventMsg::TurnStarted(_) => {}
                EventMsg::AgentMessage(msg) => {
                    output.push_str(&msg.message);
                }
                EventMsg::TurnComplete(_) => {
                    success = true;
                    break;
                }
                EventMsg::TurnAborted(_) => {
                    success = false;
                    break;
                }
                EventMsg::Error(err) => {
                    tracing::error!(agent = %agent_id, error = %err.message, "Turn failed with error");
                    output.push_str(&format!("\nError: {}", err.message));
                    success = false;
                    break;
                }
                _ => {}
            }
        }

        active_turn.store(false, Ordering::SeqCst);

        let _ = self
            .control_plane
            .record_budget_usage_updated(
                studio_id,
                agent_id,
                run_id,
                tracker.turns_used(),
                tracker.tool_calls_used(),
                tracker.wall_clock_secs_used(),
                tracker.child_agents_used(),
            )
            .await;

        Ok((
            output,
            tracker.turns_used(),
            tracker.tool_calls_used(),
            success,
        ))
    }
}

#[async_trait]
impl AgentExecutor for CodexAgentExecutor {
    fn validate_agent_spec(&self, spec: &InternalAgentSpec) -> Result<(), InternalAgentError> {
        self.session_factory
            .prepare_runtime_session(&spec.model_ref)
            .map_err(InternalAgentError::RuntimeSession)?;
        Ok(())
    }

    #[allow(clippy::collapsible_if)]
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError> {
        let start = Instant::now();

        let prepared_session = self
            .session_factory
            .prepare_runtime_session(&context.agent_spec.model_ref)
            .map_err(InternalAgentError::RuntimeSession)?;

        let _ = self
            .control_plane
            .record_runtime_bound(
                context.studio_id,
                context.agent_spec.agent_id,
                "codex",
                prepared_session.model_provider_id(),
                prepared_session.selected_model(),
                prepared_session.protocol().to_string(),
            )
            .await;

        let existing = {
            let guard = self.running_agents.read().await;
            guard.get(&context.agent_spec.agent_id).cloned()
        };

        if let Some(mut state) = existing {
            if state.bound_workspace == context.execution_workspace {
                // Revalidate upstream worktree ownership before turn reservation and dispatch
                if let Some(root) = context.execution_workspace.root() {
                    let orchestrator = self.workspace_orchestrator.as_ref().ok_or_else(|| {
                        InternalAgentError::Workspace(
                            agent_studios_workspace::WorkspaceError::InvalidOperation(
                                "missing workspace orchestrator for managed worktree reuse"
                                    .to_string(),
                            ),
                        )
                    })?;
                    let expected_thread = state.thread_id.to_string();
                    match orchestrator.get_owner(root) {
                        Ok(Some(ref owner)) if owner == &expected_thread => {
                            // Upstream ownership verified
                        }
                        Ok(Some(foreign_owner)) => {
                            return Err(InternalAgentError::WorktreeOwnershipConflict {
                                agent_id: context.agent_spec.agent_id,
                                worktree_root: root.to_path_buf(),
                                owner_thread_id: foreign_owner,
                            });
                        }
                        Ok(None) => {
                            return Err(InternalAgentError::WorktreeOwnershipConflict {
                                agent_id: context.agent_spec.agent_id,
                                worktree_root: root.to_path_buf(),
                                owner_thread_id: "none".to_string(),
                            });
                        }
                        Err(e) => {
                            return Err(InternalAgentError::Workspace(e));
                        }
                    }
                }

                let tracker = if state.parent_agent_id.is_none() {
                    Arc::clone(&state.budget_tracker)
                } else {
                    let scope = context
                        .run_id
                        .map(BudgetScopeId::Run)
                        .unwrap_or_else(|| BudgetScopeId::Coordinator(context.agent_spec.agent_id));
                    Arc::new(AgentBudgetTracker::new_with_scope(
                        scope,
                        context.agent_spec.agent_id,
                        context.budget.clone(),
                    ))
                };

                state.extension_context.set_task_and_run(
                    context.task_id,
                    context.run_id,
                    Arc::clone(&tracker),
                );
                state.budget_tracker = Arc::clone(&tracker);
                state.current_run_id = context.run_id;
                let _turn_guard = ActiveTurnGuard::new(Arc::clone(&state.active_turn));

                {
                    let mut guard = self.running_agents.write().await;
                    guard.insert(context.agent_spec.agent_id, state.clone());
                }

                tracker.check_wall_clock()?;
                if let Err(e) = tracker.reserve_turn() {
                    let _ = self
                        .control_plane
                        .record_budget_exceeded(
                            context.studio_id,
                            context.agent_spec.agent_id,
                            context.run_id,
                            "turns".to_string(),
                            context.budget.max_turns.unwrap_or(0) as u64,
                            tracker.turns_used() as u64 + 1,
                        )
                        .await;
                    return Err(e);
                }

                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Busy)
                    .await;

                let mut start_options = TurnStartOptions::default();
                if let Some(schema) = context.output_schema.clone() {
                    start_options.final_output_json_schema = Some(schema);
                }

                let resume_cfg = (*self.config).clone();
                let send_request = SendRequest {
                    caller: state.thread_id,
                    target: AgentTarget::Id(state.thread_id),
                    resume_config: resume_cfg,
                    input: AgentInput::UserInput(vec![UserInput::Text {
                        text: context.prompt.clone(),
                        text_elements: vec![],
                    }]),
                    start_options,
                };

                if let Err(e) = state.agent_control.send(send_request).await {
                    tracker.rollback_turn();
                    let _ = self
                        .control_plane
                        .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                        .await;
                    return Err(InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: format!("Failed to send input to agent: {e}"),
                    });
                }
                tracker.commit_turn();

                let drain_res = self
                    .drain_events(
                        &state.thread,
                        &state.active_turn,
                        &tracker,
                        context.run_id,
                        context.agent_spec.agent_id,
                        context.studio_id,
                    )
                    .await;

                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                    .await;

                let (output, turns, tools, success) = drain_res?;
                let elapsed = start.elapsed().as_secs();
                return Ok(AgentExecutionResult {
                    output,
                    turns_used: turns,
                    tool_calls_used: tools,
                    duration_secs: elapsed,
                    success,
                });
            } else {
                tracing::info!(
                    agent_id = %context.agent_spec.agent_id,
                    old_ws = ?state.bound_workspace,
                    new_ws = ?context.execution_workspace,
                    "Retiring worker due to workspace change (non-teleporting invariant)"
                );
                self.retire_worker(context.agent_spec.agent_id, &state)
                    .await?;
            }
        }

        let is_coordinator = context.parent_agent_id.is_none();
        if is_coordinator {
            let tracker = Arc::new(AgentBudgetTracker::new_with_scope(
                BudgetScopeId::Coordinator(context.agent_spec.agent_id),
                context.agent_spec.agent_id,
                context.budget.clone(),
            ));

            tracker.check_wall_clock()?;
            if let Err(e) = tracker.reserve_turn() {
                let _ = self
                    .control_plane
                    .record_budget_exceeded(
                        context.studio_id,
                        context.agent_spec.agent_id,
                        context.run_id,
                        "turns".to_string(),
                        context.budget.max_turns.unwrap_or(0) as u64,
                        tracker.turns_used() as u64 + 1,
                    )
                    .await;
                return Err(e);
            }

            let _ = self
                .control_plane
                .transition_agent_state(context.agent_spec.agent_id, AgentState::Starting)
                .await;

            if let Some(orchestrator) = &self.workspace_orchestrator {
                if let Some(root) = context.execution_workspace.root() {
                    match orchestrator.get_owner(root) {
                        Ok(None) => {}
                        Ok(Some(existing_owner)) => {
                            tracker.rollback_turn();
                            let _ = self
                                .control_plane
                                .transition_agent_state(
                                    context.agent_spec.agent_id,
                                    AgentState::Idle,
                                )
                                .await;
                            return Err(InternalAgentError::WorktreeOwnershipConflict {
                                agent_id: context.agent_spec.agent_id,
                                worktree_root: root.to_path_buf(),
                                owner_thread_id: existing_owner,
                            });
                        }
                        Err(e) => {
                            tracker.rollback_turn();
                            let _ = self
                                .control_plane
                                .transition_agent_state(
                                    context.agent_spec.agent_id,
                                    AgentState::Idle,
                                )
                                .await;
                            return Err(InternalAgentError::Workspace(e));
                        }
                    }
                }
            }

            let mut coordinator_config = (*self.config).clone();
            let cwd_path = context.execution_workspace.cwd();
            if !cwd_path.as_os_str().is_empty() {
                coordinator_config.cwd =
                    AbsolutePathBuf::from_absolute_path(cwd_path).map_err(|e| {
                        InternalAgentError::Other(format!("invalid workspace path: {e}"))
                    })?;
            }
            if context.agent_spec.workspace_access.is_read_only() {
                coordinator_config
                    .set_legacy_sandbox_policy(SandboxPolicy::ReadOnly {
                        network_access: false,
                    })
                    .map_err(|e| InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: e.to_string(),
                    })?;
                coordinator_config.permissions = Permissions::from_approval_and_profile(
                    Constrained::allow_any(AskForApproval::Never),
                    Constrained::allow_any(PermissionProfile::read_only()),
                )
                .map_err(|e| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: e.to_string(),
                })?;
            }

            let ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
                context.studio_id,
                context.agent_spec.agent_id,
                Arc::clone(&tracker),
                self.control_plane.clone(),
            ));
            ext_ctx.set_task_and_run(context.task_id, context.run_id, Arc::clone(&tracker));

            let mut thread_extension_init = codex_extension_api::ExtensionDataInit::new();
            thread_extension_init.insert((*ext_ctx).clone());

            let mut start_options = StartThreadOptions::new(coordinator_config);
            start_options.thread_extension_init = thread_extension_init;
            prepared_session.apply_to_start_thread_options(&mut start_options);

            let new_thread = match self.thread_manager.start_thread(start_options).await {
                Ok(nt) => nt,
                Err(e) => {
                    tracker.rollback_turn();
                    let _ = self
                        .control_plane
                        .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                        .await;
                    return Err(InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: format!("Failed to start root thread: {e}"),
                    });
                }
            };

            let thread_id = new_thread.thread_id;
            let thread = new_thread.thread;
            let agent_control = thread.agent_control();
            let active_turn = Arc::new(AtomicBool::new(false));
            let _turn_guard = ActiveTurnGuard::new(Arc::clone(&active_turn));

            if let Some(orchestrator) = &self.workspace_orchestrator {
                if let Some(root) = context.execution_workspace.root() {
                    if let Err(e) = orchestrator
                        .bind_thread_async(root.to_path_buf(), thread_id.to_string())
                        .await
                    {
                        let shutdown_res = Self::shutdown_and_wait_thread(
                            context.agent_spec.agent_id,
                            &thread,
                            Duration::from_secs(5),
                        )
                        .await;
                        self.thread_manager
                            .remove_thread_if_matches(&thread_id, &thread)
                            .await;
                        tracker.rollback_turn();
                        let _ = self
                            .control_plane
                            .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                            .await;
                        if let Some(wt_id) = context.execution_workspace.worktree_id() {
                            let _ = self
                                .control_plane
                                .record_worktree_retained(
                                    wt_id,
                                    Some(format!("Failed upstream thread binding: {e}")),
                                )
                                .await;
                        }
                        shutdown_res?;
                        return Err(InternalAgentError::ExecutionFailed {
                            agent_id: context.agent_spec.agent_id,
                            error: format!("Failed to bind thread to workspace: {e}"),
                        });
                    }
                }
            }
            if let Some(wt_id) = context.execution_workspace.worktree_id() {
                if let Err(e) = self
                    .control_plane
                    .bind_worktree_thread(wt_id, thread_id.to_string())
                    .await
                {
                    let shutdown_res = Self::shutdown_and_wait_thread(
                        context.agent_spec.agent_id,
                        &thread,
                        Duration::from_secs(5),
                    )
                    .await;
                    self.thread_manager
                        .remove_thread_if_matches(&thread_id, &thread)
                        .await;
                    tracker.rollback_turn();
                    let _ = self
                        .control_plane
                        .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                        .await;
                    let _ = self
                        .control_plane
                        .record_worktree_retained(
                            wt_id,
                            Some(format!("Failed durable thread binding: {e}")),
                        )
                        .await;
                    shutdown_res?;
                    return Err(InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: format!("Failed to record durable thread binding: {e}"),
                    });
                }
            }

            let state = RunningAgentState {
                thread_id,
                thread: Arc::clone(&thread),
                agent_control: Arc::clone(&agent_control),
                active_turn: Arc::clone(&active_turn),
                current_run_id: context.run_id,
                parent_agent_id: None,
                extension_context: Arc::clone(&ext_ctx),
                budget_tracker: Arc::clone(&tracker),
                bound_workspace: context.execution_workspace.clone(),
            };

            {
                let mut guard = self.running_agents.write().await;
                guard.insert(context.agent_spec.agent_id, state);
            }
            {
                let mut coord_guard = self.coordinator_agent_id.write().await;
                *coord_guard = Some(context.agent_spec.agent_id);
            }

            let _ = self
                .control_plane
                .record_agent_spawned(
                    context.studio_id,
                    context.agent_spec.agent_id,
                    thread_id.to_string(),
                    None,
                    None,
                )
                .await;

            let _ = self
                .control_plane
                .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                .await;
            let _ = self
                .control_plane
                .transition_agent_state(context.agent_spec.agent_id, AgentState::Busy)
                .await;

            let mut turn_input = TurnInputRequest::user_input(vec![UserInput::Text {
                text: context.prompt.clone(),
                text_elements: vec![],
            }]);
            if let Some(schema) = context.output_schema.clone() {
                turn_input.start.final_output_json_schema = Some(schema);
            }

            if let Err(e) = thread.start_or_steer_turn(turn_input).await {
                tracker.rollback_turn();
                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                    .await;
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to submit turn input: {e}"),
                });
            }
            tracker.commit_turn();

            let drain_res = self
                .drain_events(
                    &thread,
                    &active_turn,
                    &tracker,
                    context.run_id,
                    context.agent_spec.agent_id,
                    context.studio_id,
                )
                .await;

            let _ = self
                .control_plane
                .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                .await;

            let (output, turns, tools, success) = drain_res?;
            let elapsed = start.elapsed().as_secs();
            return Ok(AgentExecutionResult {
                output,
                turns_used: turns,
                tool_calls_used: tools,
                duration_secs: elapsed,
                success,
            });
        }

        // Child agent
        let parent_id = context.parent_agent_id.unwrap();
        let parent_state = {
            let guard = self.running_agents.read().await;
            guard
                .get(&parent_id)
                .cloned()
                .ok_or_else(|| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Parent agent {parent_id} not found in running agents"),
                })?
        };

        if let Err(err) = parent_state.budget_tracker.reserve_child_agent() {
            let limit = parent_state
                .budget_tracker
                .budget()
                .max_child_agents
                .unwrap_or(0);
            let actual = parent_state.budget_tracker.child_agents_used() + 1;
            let _ = self
                .control_plane
                .record_budget_exceeded(
                    context.studio_id,
                    parent_id,
                    context.run_id,
                    "child_agents".to_string(),
                    limit as u64,
                    actual as u64,
                )
                .await;
            return Err(err);
        }

        let child_scope = context
            .run_id
            .map(BudgetScopeId::Run)
            .unwrap_or_else(|| BudgetScopeId::Coordinator(context.agent_spec.agent_id));
        let child_tracker = Arc::new(AgentBudgetTracker::new_with_scope(
            child_scope,
            context.agent_spec.agent_id,
            context.budget.clone(),
        ));

        child_tracker.check_wall_clock()?;
        if let Err(e) = child_tracker.reserve_turn() {
            parent_state.budget_tracker.rollback_child_agent();
            return Err(e);
        }

        let _ = self
            .control_plane
            .transition_agent_state(context.agent_spec.agent_id, AgentState::Starting)
            .await;

        let mut child_config = (*self.config).clone();
        child_config.model_provider_id = prepared_session.model_provider_id().to_string();
        child_config.model = Some(prepared_session.selected_model().to_string());
        let child_cwd = context.execution_workspace.cwd();
        if !child_cwd.as_os_str().is_empty() {
            child_config.cwd = AbsolutePathBuf::from_absolute_path(child_cwd)
                .map_err(|e| InternalAgentError::Other(format!("invalid workspace path: {e}")))?;
        }
        if context.agent_spec.workspace_access.is_read_only() {
            child_config
                .set_legacy_sandbox_policy(SandboxPolicy::ReadOnly {
                    network_access: false,
                })
                .map_err(|e| {
                    parent_state.budget_tracker.rollback_child_agent();
                    child_tracker.rollback_turn();
                    InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: e.to_string(),
                    }
                })?;
            child_config.permissions = Permissions::from_approval_and_profile(
                Constrained::allow_any(AskForApproval::Never),
                Constrained::allow_any(PermissionProfile::read_only()),
            )
            .map_err(|e| {
                parent_state.budget_tracker.rollback_child_agent();
                child_tracker.rollback_turn();
                InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: e.to_string(),
                }
            })?;
        }

        let child_ext_ctx = Arc::new(AgentRuntimeExtensionContext::new(
            context.studio_id,
            context.agent_spec.agent_id,
            Arc::clone(&child_tracker),
            self.control_plane.clone(),
        ));
        child_ext_ctx.set_task_and_run(context.task_id, context.run_id, Arc::clone(&child_tracker));

        let mut child_init = codex_extension_api::ExtensionDataInit::new();
        child_init.insert((*child_ext_ctx).clone());

        let worker_override = prepared_session.into_runtime_override();

        if let Some(orchestrator) = &self.workspace_orchestrator {
            if let Some(root) = context.execution_workspace.root() {
                match orchestrator.get_owner(root) {
                    Ok(None) => {}
                    Ok(Some(existing_owner)) => {
                        parent_state.budget_tracker.rollback_child_agent();
                        child_tracker.rollback_turn();
                        let _ = self
                            .control_plane
                            .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                            .await;
                        return Err(InternalAgentError::WorktreeOwnershipConflict {
                            agent_id: context.agent_spec.agent_id,
                            worktree_root: root.to_path_buf(),
                            owner_thread_id: existing_owner,
                        });
                    }
                    Err(e) => {
                        parent_state.budget_tracker.rollback_child_agent();
                        child_tracker.rollback_turn();
                        let _ = self
                            .control_plane
                            .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                            .await;
                        return Err(InternalAgentError::Workspace(e));
                    }
                }
            }
        }

        let spawn_req = SpawnRequest {
            caller: parent_state.thread_id,
            config: child_config,
            input: AgentInput::UserInput(vec![UserInput::Text {
                text: context.prompt.clone(),
                text_elements: vec![],
            }]),
            source: SessionSource::SubAgent(SubAgentSource::Other(
                context.agent_spec.display_name.clone(),
            )),
            options: SpawnAgentOptions {
                parent_thread_id: Some(parent_state.thread_id),
                ..Default::default()
            },
            model_runtime_override: Some(worker_override),
            thread_extension_init: child_init,
        };

        let (live_agent, _snapshot) = match parent_state.agent_control.spawn(spawn_req).await {
            Ok(res) => {
                parent_state.budget_tracker.commit_child_agent();
                child_tracker.commit_turn();
                let _ = self
                    .control_plane
                    .record_budget_usage_updated(
                        context.studio_id,
                        parent_id,
                        None,
                        parent_state.budget_tracker.turns_used(),
                        parent_state.budget_tracker.tool_calls_used(),
                        parent_state.budget_tracker.wall_clock_secs_used(),
                        parent_state.budget_tracker.child_agents_used(),
                    )
                    .await;
                res
            }
            Err(e) => {
                parent_state.budget_tracker.rollback_child_agent();
                child_tracker.rollback_turn();
                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                    .await;
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to spawn child worker: {e}"),
                });
            }
        };

        let child_thread = match self.thread_manager.get_thread(live_agent.thread_id).await {
            Ok(th) => th,
            Err(e) => {
                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                    .await;
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to get child thread from thread manager: {e}"),
                });
            }
        };

        let child_control = child_thread.agent_control();
        let active_turn = Arc::new(AtomicBool::new(false));
        let _turn_guard = ActiveTurnGuard::new(Arc::clone(&active_turn));

        if let Some(orchestrator) = &self.workspace_orchestrator {
            if let Some(root) = context.execution_workspace.root() {
                if let Err(e) = orchestrator
                    .bind_thread_async(root.to_path_buf(), live_agent.thread_id.to_string())
                    .await
                {
                    let shutdown_res = Self::shutdown_and_wait_thread(
                        context.agent_spec.agent_id,
                        &child_thread,
                        Duration::from_secs(5),
                    )
                    .await;
                    self.thread_manager
                        .remove_thread_if_matches(&live_agent.thread_id, &child_thread)
                        .await;
                    parent_state.budget_tracker.rollback_child_agent();
                    child_tracker.rollback_turn();
                    let _ = self
                        .control_plane
                        .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                        .await;
                    if let Some(wt_id) = context.execution_workspace.worktree_id() {
                        let _ = self
                            .control_plane
                            .record_worktree_retained(
                                wt_id,
                                Some(format!("Failed upstream thread binding: {e}")),
                            )
                            .await;
                    }
                    shutdown_res?;
                    return Err(InternalAgentError::ExecutionFailed {
                        agent_id: context.agent_spec.agent_id,
                        error: format!("Failed to bind child thread to workspace: {e}"),
                    });
                }
            }
        }
        if let Some(wt_id) = context.execution_workspace.worktree_id() {
            if let Err(e) = self
                .control_plane
                .bind_worktree_thread(wt_id, live_agent.thread_id.to_string())
                .await
            {
                let shutdown_res = Self::shutdown_and_wait_thread(
                    context.agent_spec.agent_id,
                    &child_thread,
                    Duration::from_secs(5),
                )
                .await;
                self.thread_manager
                    .remove_thread_if_matches(&live_agent.thread_id, &child_thread)
                    .await;
                parent_state.budget_tracker.rollback_child_agent();
                child_tracker.rollback_turn();
                let _ = self
                    .control_plane
                    .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
                    .await;
                let _ = self
                    .control_plane
                    .record_worktree_retained(
                        wt_id,
                        Some(format!("Failed durable thread binding: {e}")),
                    )
                    .await;
                shutdown_res?;
                return Err(InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to record durable thread binding: {e}"),
                });
            }
        }

        let state = RunningAgentState {
            thread_id: live_agent.thread_id,
            thread: Arc::clone(&child_thread),
            agent_control: Arc::clone(&child_control),
            active_turn: Arc::clone(&active_turn),
            current_run_id: context.run_id,
            parent_agent_id: Some(parent_id),
            extension_context: Arc::clone(&child_ext_ctx),
            budget_tracker: Arc::clone(&child_tracker),
            bound_workspace: context.execution_workspace.clone(),
        };

        {
            let mut guard = self.running_agents.write().await;
            guard.insert(context.agent_spec.agent_id, state);
        }

        let _ = self
            .control_plane
            .record_agent_spawned(
                context.studio_id,
                context.agent_spec.agent_id,
                live_agent.thread_id.to_string(),
                Some(parent_id),
                Some(parent_state.thread_id.to_string()),
            )
            .await;

        let _ = self
            .control_plane
            .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
            .await;
        let _ = self
            .control_plane
            .transition_agent_state(context.agent_spec.agent_id, AgentState::Busy)
            .await;

        let drain_res = self
            .drain_events(
                &child_thread,
                &active_turn,
                &child_tracker,
                context.run_id,
                context.agent_spec.agent_id,
                context.studio_id,
            )
            .await;

        let _ = self
            .control_plane
            .transition_agent_state(context.agent_spec.agent_id, AgentState::Idle)
            .await;

        let (output, turns, tools, success) = drain_res?;
        let elapsed = start.elapsed().as_secs();
        Ok(AgentExecutionResult {
            output,
            turns_used: turns,
            tool_calls_used: tools,
            duration_secs: elapsed,
            success,
        })
    }

    async fn cancel_agent(&self, agent_id: AgentId) -> Result<(), InternalAgentError> {
        Self::cancel_agent(self, agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_turn_guard_sets_and_clears_on_drop() {
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::SeqCst));
        {
            let _guard = ActiveTurnGuard::new(Arc::clone(&flag));
            assert!(flag.load(Ordering::SeqCst));
        }
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_active_turn_guard_clears_on_panic_unwind() {
        let flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&flag);
        let _ = std::panic::catch_unwind(move || {
            let _guard = ActiveTurnGuard::new(flag_clone);
            panic!("simulated turn panic");
        });
        assert!(!flag.load(Ordering::SeqCst));
    }
}
