use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use agent_studios_protocol::id::{AgentId, RunId, StudioId, TaskId};
use chrono::Utc;
use codex_extension_api::{
    ExtensionRegistry, ExtensionRegistryBuilder, ToolCallOutcome, ToolFinishInput,
    ToolLifecycleContributor, ToolLifecycleFuture, ToolStartInput,
};

use crate::budget::AgentBudgetTracker;
use crate::control_plane_actor::ControlPlaneHandle;
use crate::error::InternalAgentError;

/// Host thread extension context attached to the thread store via `ExtensionDataInit`.
#[derive(Clone)]
pub struct AgentRuntimeExtensionContext {
    pub studio_id: StudioId,
    pub agent_id: AgentId,
    pub current_task_id: Arc<RwLock<Option<TaskId>>>,
    pub current_run_id: Arc<RwLock<Option<RunId>>>,
    pub budget_tracker: Arc<RwLock<Arc<AgentBudgetTracker>>>,
    pub control_plane: ControlPlaneHandle,
    pub active_reservations: Arc<Mutex<HashSet<String>>>,
    pub tool_start_times: Arc<Mutex<HashMap<String, Instant>>>,
}

impl AgentRuntimeExtensionContext {
    pub fn new(
        studio_id: StudioId,
        agent_id: AgentId,
        budget_tracker: Arc<AgentBudgetTracker>,
        control_plane: ControlPlaneHandle,
    ) -> Self {
        Self {
            studio_id,
            agent_id,
            current_task_id: Arc::new(RwLock::new(None)),
            current_run_id: Arc::new(RwLock::new(None)),
            budget_tracker: Arc::new(RwLock::new(budget_tracker)),
            control_plane,
            active_reservations: Arc::new(Mutex::new(HashSet::new())),
            tool_start_times: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_task_and_run(
        &self,
        task_id: Option<TaskId>,
        run_id: Option<RunId>,
        tracker: Arc<AgentBudgetTracker>,
    ) {
        *self.current_task_id.write().unwrap() = task_id;
        *self.current_run_id.write().unwrap() = run_id;
        *self.budget_tracker.write().unwrap() = tracker;
    }
}

/// Agent Studios ToolLifecycleContributor enforcing pre-execution tool budget limits
/// and recording durable lifecycle events.
#[derive(Clone, Default)]
pub struct AgentStudiosToolLifecycleContributor;

impl AgentStudiosToolLifecycleContributor {
    pub fn new() -> Self {
        Self
    }
}

impl ToolLifecycleContributor for AgentStudiosToolLifecycleContributor {
    fn authorize_tool_call(&self, input: &ToolStartInput<'_>) -> Result<(), String> {
        let Some(ctx) = input.thread_store.get::<AgentRuntimeExtensionContext>() else {
            return Ok(());
        };

        let tracker = ctx.budget_tracker.read().unwrap().clone();
        let call_id = input.call_id.to_string();

        if let Err(err) = tracker.reserve_tool_call() {
            let (limit, actual) = match err {
                InternalAgentError::ToolCallBudgetExceeded { limit, actual, .. } => {
                    (limit as u64, actual as u64)
                }
                _ => (0, 0),
            };
            let run_id = *ctx.current_run_id.read().unwrap();
            let cp = ctx.control_plane.clone();
            let studio_id = ctx.studio_id;
            let agent_id = ctx.agent_id;

            tokio::spawn(async move {
                let _ = cp
                    .record_budget_exceeded(
                        studio_id,
                        agent_id,
                        run_id,
                        "tool_calls".to_string(),
                        limit,
                        actual,
                    )
                    .await;
            });

            return Err(format!(
                "Tool call budget exceeded for agent {agent_id}: limit {limit}, actual {actual}"
            ));
        }

        ctx.active_reservations.lock().unwrap().insert(call_id);
        Ok(())
    }

    fn on_tool_start<'a>(&'a self, input: ToolStartInput<'a>) -> ToolLifecycleFuture<'a> {
        let Some(ctx) = input.thread_store.get::<AgentRuntimeExtensionContext>() else {
            return Box::pin(std::future::ready(()));
        };

        let tool_name = input.tool_name.name.clone();
        let call_id = input.call_id.to_string();
        let timestamp = Utc::now();

        ctx.tool_start_times
            .lock()
            .unwrap()
            .insert(call_id.clone(), Instant::now());

        let task_id = *ctx.current_task_id.read().unwrap();
        let run_id = *ctx.current_run_id.read().unwrap();
        let cp = ctx.control_plane.clone();
        let studio_id = ctx.studio_id;
        let agent_id = ctx.agent_id;

        Box::pin(async move {
            let _ = cp
                .record_tool_started(
                    studio_id, agent_id, task_id, run_id, tool_name, call_id, timestamp,
                )
                .await;
        })
    }

    fn on_tool_finish<'a>(&'a self, input: ToolFinishInput<'a>) -> ToolLifecycleFuture<'a> {
        let Some(ctx) = input.thread_store.get::<AgentRuntimeExtensionContext>() else {
            return Box::pin(std::future::ready(()));
        };

        let tool_name = input.tool_name.name.clone();
        let call_id = input.call_id.to_string();
        let duration_ms = ctx
            .tool_start_times
            .lock()
            .unwrap()
            .remove(&call_id)
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);
        let timestamp = Utc::now();

        let had_reservation = ctx.active_reservations.lock().unwrap().remove(&call_id);
        let tracker = ctx.budget_tracker.read().unwrap().clone();
        let task_id = *ctx.current_task_id.read().unwrap();
        let run_id = *ctx.current_run_id.read().unwrap();
        let cp = ctx.control_plane.clone();
        let studio_id = ctx.studio_id;
        let agent_id = ctx.agent_id;
        let outcome = input.outcome;

        match outcome {
            ToolCallOutcome::Blocked => {
                if had_reservation {
                    tracker.rollback_tool_call();
                }
                Box::pin(std::future::ready(()))
            }
            ToolCallOutcome::Completed { success } => {
                if had_reservation {
                    tracker.commit_tool_call();
                }
                let turns = tracker.turns_used();
                let tools = tracker.tool_calls_used();
                let wall_clock = tracker.wall_clock_secs_used();
                let child_agents = tracker.child_agents_used();

                Box::pin(async move {
                    if success {
                        let _ = cp
                            .record_tool_completed(
                                studio_id,
                                agent_id,
                                task_id,
                                run_id,
                                tool_name,
                                call_id,
                                duration_ms,
                                timestamp,
                            )
                            .await;
                    } else {
                        let _ = cp
                            .record_tool_failed(
                                studio_id,
                                agent_id,
                                task_id,
                                run_id,
                                tool_name,
                                call_id,
                                "tool_reported_failure".to_string(),
                                duration_ms,
                                timestamp,
                            )
                            .await;
                    }

                    let _ = cp
                        .record_budget_usage_updated(
                            studio_id,
                            agent_id,
                            run_id,
                            turns,
                            tools,
                            wall_clock,
                            child_agents,
                        )
                        .await;
                })
            }
            ToolCallOutcome::Failed { .. } | ToolCallOutcome::Aborted => {
                if had_reservation {
                    tracker.commit_tool_call();
                }
                let turns = tracker.turns_used();
                let tools = tracker.tool_calls_used();
                let wall_clock = tracker.wall_clock_secs_used();
                let child_agents = tracker.child_agents_used();

                Box::pin(async move {
                    let _ = cp
                        .record_tool_failed(
                            studio_id,
                            agent_id,
                            task_id,
                            run_id,
                            tool_name,
                            call_id,
                            "tool_failed_or_aborted".to_string(),
                            duration_ms,
                            timestamp,
                        )
                        .await;

                    let _ = cp
                        .record_budget_usage_updated(
                            studio_id,
                            agent_id,
                            run_id,
                            turns,
                            tools,
                            wall_clock,
                            child_agents,
                        )
                        .await;
                })
            }
        }
    }
}

/// Helper to build an `ExtensionRegistryBuilder` with `AgentStudiosToolLifecycleContributor` registered.
pub fn build_agent_studios_extension_builder<C: Sync>() -> ExtensionRegistryBuilder<C> {
    let mut builder = ExtensionRegistryBuilder::new();
    builder.tool_lifecycle_contributor(Arc::new(AgentStudiosToolLifecycleContributor::new()));
    builder
}

/// Helper to build an `ExtensionRegistry` with `AgentStudiosToolLifecycleContributor` registered.
pub fn build_agent_studios_extension_registry<C: Sync>() -> ExtensionRegistry<C> {
    build_agent_studios_extension_builder().build()
}
