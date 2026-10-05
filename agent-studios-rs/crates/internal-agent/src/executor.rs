use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use async_trait::async_trait;
use chrono::Utc;
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
use tokio::sync::RwLock;

use crate::budget::AgentBudgetTracker;
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

        if let Some(ref handler) = *self.custom_handler.lock().unwrap() {
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
}

impl std::fmt::Debug for RunningAgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningAgentState")
            .field("thread_id", &self.thread_id)
            .field("active_turn", &self.active_turn.load(Ordering::Relaxed))
            .field("current_run_id", &self.current_run_id)
            .field("parent_agent_id", &self.parent_agent_id)
            .finish()
    }
}

pub struct CodexAgentExecutor {
    thread_manager: Option<Arc<ThreadManager>>,
    session_factory: Arc<AgentStudiosRuntimeSessionFactory>,
    config: Option<Arc<Config>>,
    control_plane: Option<ControlPlaneHandle>,
    running_agents: Arc<RwLock<HashMap<AgentId, RunningAgentState>>>,
    coordinator_agent_id: Arc<RwLock<Option<AgentId>>>,
}

impl CodexAgentExecutor {
    pub fn new(session_factory: AgentStudiosRuntimeSessionFactory) -> Self {
        Self {
            thread_manager: None,
            session_factory: Arc::new(session_factory),
            config: None,
            control_plane: None,
            running_agents: Arc::new(RwLock::new(HashMap::new())),
            coordinator_agent_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_thread_manager(mut self, thread_manager: Arc<ThreadManager>) -> Self {
        self.thread_manager = Some(thread_manager);
        self
    }

    pub fn with_config(mut self, config: Arc<Config>) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_control_plane(mut self, control_plane: ControlPlaneHandle) -> Self {
        self.control_plane = Some(control_plane);
        self
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

    #[allow(clippy::too_many_arguments)]
    async fn drain_events(
        &self,
        thread: &Arc<CodexThread>,
        active_turn: &Arc<AtomicBool>,
        tracker: &mut AgentBudgetTracker,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        agent_id: AgentId,
        studio_id: StudioId,
    ) -> Result<(String, u32, u32, bool), InternalAgentError> {
        let mut output = String::new();
        let mut success = false;
        let mut turns_used: u32 = 0;
        let mut tool_calls_used: u32 = 0;

        let timeout_duration = Duration::from_secs(60);
        let deadline = tokio::time::Instant::now() + timeout_duration;

        while active_turn.load(Ordering::SeqCst) && tokio::time::Instant::now() < deadline {
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
                EventMsg::TurnStarted(_) => {
                    if let Err(e) = tracker.record_turn() {
                        if let Some(ref cp) = self.control_plane {
                            match e {
                                InternalAgentError::TurnBudgetExceeded {
                                    agent_id: aid,
                                    limit,
                                    actual,
                                } => {
                                    let _ = cp
                                        .record_budget_exceeded(
                                            studio_id,
                                            aid,
                                            run_id,
                                            "turns".to_string(),
                                            limit as u64,
                                            actual as u64,
                                        )
                                        .await;
                                }
                                InternalAgentError::BudgetExceeded { agent_id: aid, .. } => {
                                    let _ = cp
                                        .record_budget_exceeded(
                                            studio_id,
                                            aid,
                                            run_id,
                                            "wall_clock".to_string(),
                                            tracker.elapsed_secs(),
                                            tracker.elapsed_secs(),
                                        )
                                        .await;
                                }
                                _ => {}
                            }
                        }
                        return Err(e);
                    }
                    turns_used += 1;
                }
                EventMsg::AgentMessage(msg) => {
                    output.push_str(&msg.message);
                }
                EventMsg::ExecCommandBegin(_) | EventMsg::McpToolCallBegin(_) => {
                    if let Err(e) = tracker.record_tool_call() {
                        if let Some(ref cp) = self.control_plane {
                            match e {
                                InternalAgentError::ToolCallBudgetExceeded {
                                    agent_id: aid,
                                    limit,
                                    actual,
                                } => {
                                    let _ = cp
                                        .record_budget_exceeded(
                                            studio_id,
                                            aid,
                                            run_id,
                                            "tool_calls".to_string(),
                                            limit as u64,
                                            actual as u64,
                                        )
                                        .await;
                                }
                                InternalAgentError::BudgetExceeded { agent_id: aid, .. } => {
                                    let _ = cp
                                        .record_budget_exceeded(
                                            studio_id,
                                            aid,
                                            run_id,
                                            "wall_clock".to_string(),
                                            tracker.elapsed_secs(),
                                            tracker.elapsed_secs(),
                                        )
                                        .await;
                                }
                                _ => {}
                            }
                        }
                        return Err(e);
                    }
                    tool_calls_used += 1;
                    if let Some(ref cp) = self.control_plane {
                        let _ = cp
                            .record_tool_started(
                                studio_id,
                                agent_id,
                                task_id,
                                run_id,
                                "tool",
                                event.id.clone(),
                                Utc::now(),
                            )
                            .await;
                    }
                }
                EventMsg::ExecCommandOutputDelta(_) => {}
                EventMsg::ExecCommandEnd(_) | EventMsg::McpToolCallEnd(_) => {
                    if let Some(ref cp) = self.control_plane {
                        let _ = cp
                            .record_tool_completed(
                                studio_id,
                                agent_id,
                                task_id,
                                run_id,
                                "tool",
                                event.id.clone(),
                                10,
                                Utc::now(),
                            )
                            .await;
                    }
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

        if let Some(ref cp) = self.control_plane {
            let _ = cp
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
        }

        Ok((output, turns_used, tool_calls_used, success))
    }
}

#[async_trait]
impl AgentExecutor for CodexAgentExecutor {
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError> {
        let mut tracker =
            AgentBudgetTracker::new(context.agent_spec.agent_id, context.budget.clone());
        let start = Instant::now();

        let prepared_session = self
            .session_factory
            .prepare_runtime_session(&context.agent_spec.model_ref)
            .map_err(InternalAgentError::RuntimeSession)?;

        // If no ThreadManager configured, fall back to lightweight session preparation
        let tm = match self.thread_manager {
            Some(ref tm) => Arc::clone(tm),
            None => {
                tracing::info!(
                    agent = %context.agent_spec.display_name,
                    provider_id = %prepared_session.model_provider_id(),
                    model = %prepared_session.selected_model(),
                    "Prepared runtime session override for agent (simulated)"
                );

                tracker.record_turn()?;
                tracker.check_all()?;
                let elapsed = start.elapsed().as_secs();

                return Ok(AgentExecutionResult {
                    output: format!(
                        "Prepared session for agent {} using model {}",
                        context.agent_spec.display_name,
                        prepared_session.selected_model()
                    ),
                    turns_used: tracker.turns_used(),
                    tool_calls_used: tracker.tool_calls_used(),
                    duration_secs: elapsed,
                    success: true,
                });
            }
        };

        let base_config =
            self.config
                .as_ref()
                .ok_or_else(|| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: "Config is required when running with ThreadManager".to_string(),
                })?;

        if let Some(ref cp) = self.control_plane {
            let _ = cp
                .record_runtime_bound(
                    context.studio_id,
                    context.agent_spec.agent_id,
                    "codex",
                    prepared_session.model_provider_id(),
                    prepared_session.selected_model(),
                    prepared_session.protocol().to_string(),
                )
                .await;
        }

        let existing = {
            let guard = self.running_agents.read().await;
            guard.get(&context.agent_spec.agent_id).cloned()
        };

        if let Some(state) = existing {
            state.active_turn.store(true, Ordering::SeqCst);

            let resume_cfg = (**base_config).clone();
            let send_request = SendRequest {
                caller: state.thread_id,
                target: AgentTarget::Id(state.thread_id),
                resume_config: resume_cfg,
                input: AgentInput::UserInput(vec![UserInput::Text {
                    text: context.prompt.clone(),
                    text_elements: vec![],
                }]),
                start_options: TurnStartOptions::default(),
            };

            state.agent_control.send(send_request).await.map_err(|e| {
                InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to send input to agent: {e}"),
                }
            })?;

            let (output, turns, tools, success) = self
                .drain_events(
                    &state.thread,
                    &state.active_turn,
                    &mut tracker,
                    context.task_id,
                    context.run_id,
                    context.agent_spec.agent_id,
                    context.studio_id,
                )
                .await?;

            let elapsed = start.elapsed().as_secs();
            return Ok(AgentExecutionResult {
                output,
                turns_used: turns,
                tool_calls_used: tools,
                duration_secs: elapsed,
                success,
            });
        }

        let is_coordinator = context.parent_agent_id.is_none();
        if is_coordinator {
            let mut coordinator_config = (**base_config).clone();
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

            let mut start_options = StartThreadOptions::new(coordinator_config);
            prepared_session.apply_to_start_thread_options(&mut start_options);

            let new_thread = tm.start_thread(start_options).await.map_err(|e| {
                InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to start root thread: {e}"),
                }
            })?;

            let thread_id = new_thread.thread_id;
            let thread = new_thread.thread;
            let agent_control = thread.agent_control();
            let active_turn = Arc::new(AtomicBool::new(true));

            let state = RunningAgentState {
                thread_id,
                thread: Arc::clone(&thread),
                agent_control: Arc::clone(&agent_control),
                active_turn: Arc::clone(&active_turn),
                current_run_id: context.run_id,
                parent_agent_id: None,
            };

            {
                let mut guard = self.running_agents.write().await;
                guard.insert(context.agent_spec.agent_id, state);
            }
            {
                let mut coord_guard = self.coordinator_agent_id.write().await;
                *coord_guard = Some(context.agent_spec.agent_id);
            }

            if let Some(ref cp) = self.control_plane {
                let _ = cp
                    .record_agent_spawned(
                        context.studio_id,
                        context.agent_spec.agent_id,
                        thread_id.to_string(),
                        None,
                        None,
                    )
                    .await;
            }

            thread
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: context.prompt.clone(),
                    text_elements: vec![],
                }]))
                .await
                .map_err(|e| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to submit turn input: {e}"),
                })?;

            let (output, turns, tools, success) = self
                .drain_events(
                    &thread,
                    &active_turn,
                    &mut tracker,
                    context.task_id,
                    context.run_id,
                    context.agent_spec.agent_id,
                    context.studio_id,
                )
                .await?;

            let elapsed = start.elapsed().as_secs();
            return Ok(AgentExecutionResult {
                output,
                turns_used: turns,
                tool_calls_used: tools,
                duration_secs: elapsed,
                success,
            });
        }

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

        let mut child_config = (**base_config).clone();
        child_config.model_provider_id = prepared_session.model_provider_id().to_string();
        child_config.model = Some(prepared_session.selected_model().to_string());
        if context.agent_spec.workspace_access.is_read_only() {
            child_config
                .set_legacy_sandbox_policy(SandboxPolicy::ReadOnly {
                    network_access: false,
                })
                .map_err(|e| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: e.to_string(),
                })?;
            child_config.permissions = Permissions::from_approval_and_profile(
                Constrained::allow_any(AskForApproval::Never),
                Constrained::allow_any(PermissionProfile::read_only()),
            )
            .map_err(|e| InternalAgentError::ExecutionFailed {
                agent_id: context.agent_spec.agent_id,
                error: e.to_string(),
            })?;
        }

        let worker_override = prepared_session.into_runtime_override();
        let spawn_req = SpawnRequest {
            caller: parent_state.thread_id,
            config: child_config,
            input: AgentInput::UserInput(vec![UserInput::Text {
                text: context.prompt.clone(),
                text_elements: vec![],
            }]),
            source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: parent_state.thread_id,
                depth: 1,
                agent_path: None,
                agent_nickname: Some(context.agent_spec.display_name.clone()),
                agent_role: Some(context.agent_spec.role.clone()),
            }),
            options: SpawnAgentOptions {
                parent_thread_id: Some(parent_state.thread_id),
                ..Default::default()
            },
            model_runtime_override: Some(worker_override),
            thread_extension_init: codex_extension_api::ExtensionDataInit::default(),
        };

        let (live_agent, _snapshot) =
            parent_state
                .agent_control
                .spawn(spawn_req)
                .await
                .map_err(|e| InternalAgentError::ExecutionFailed {
                    agent_id: context.agent_spec.agent_id,
                    error: format!("Failed to spawn child worker: {e}"),
                })?;

        let child_thread = tm.get_thread(live_agent.thread_id).await.map_err(|e| {
            InternalAgentError::ExecutionFailed {
                agent_id: context.agent_spec.agent_id,
                error: format!("Failed to get child thread from thread manager: {e}"),
            }
        })?;

        let child_control = child_thread.agent_control();
        let active_turn = Arc::new(AtomicBool::new(true));

        let state = RunningAgentState {
            thread_id: live_agent.thread_id,
            thread: Arc::clone(&child_thread),
            agent_control: Arc::clone(&child_control),
            active_turn: Arc::clone(&active_turn),
            current_run_id: context.run_id,
            parent_agent_id: Some(parent_id),
        };

        {
            let mut guard = self.running_agents.write().await;
            guard.insert(context.agent_spec.agent_id, state);
        }

        if let Some(ref cp) = self.control_plane {
            let _ = cp
                .record_agent_spawned(
                    context.studio_id,
                    context.agent_spec.agent_id,
                    live_agent.thread_id.to_string(),
                    Some(parent_id),
                    Some(parent_state.thread_id.to_string()),
                )
                .await;
        }

        let (output, turns, tools, success) = self
            .drain_events(
                &child_thread,
                &active_turn,
                &mut tracker,
                context.task_id,
                context.run_id,
                context.agent_spec.agent_id,
                context.studio_id,
            )
            .await?;

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
