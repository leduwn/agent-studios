use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use agent_studios_protocol::id::{StudioId, TaskId};
use agent_studios_runtime_session::AgentStudiosRuntimeSessionFactory;
use async_trait::async_trait;

use crate::budget::AgentBudgetTracker;
use crate::error::InternalAgentError;
use crate::profile::{AgentExecutionBudget, InternalAgentSpec};

#[derive(Clone, Debug)]
pub struct AgentExecutionContext {
    pub studio_id: StudioId,
    pub task_id: Option<TaskId>,
    pub agent_spec: InternalAgentSpec,
    pub prompt: String,
    pub budget: AgentExecutionBudget,
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
}

#[async_trait]
impl AgentExecutor for MockAgentExecutor {
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

pub struct CodexAgentExecutor {
    factory: AgentStudiosRuntimeSessionFactory,
}

impl CodexAgentExecutor {
    pub fn new(factory: AgentStudiosRuntimeSessionFactory) -> Self {
        Self { factory }
    }

    pub fn factory(&self) -> &AgentStudiosRuntimeSessionFactory {
        &self.factory
    }
}

#[async_trait]
impl AgentExecutor for CodexAgentExecutor {
    async fn execute_agent(
        &self,
        context: AgentExecutionContext,
    ) -> Result<AgentExecutionResult, InternalAgentError> {
        let mut tracker = AgentBudgetTracker::new(context.agent_spec.agent_id, context.budget);
        let start = Instant::now();

        // Prepare the session override for the agent's authoritative model_ref
        let prepared_session = self
            .factory
            .prepare_runtime_session(&context.agent_spec.model_ref)
            .map_err(InternalAgentError::RuntimeSession)?;

        tracing::info!(
            agent = %context.agent_spec.display_name,
            provider_id = %prepared_session.model_provider_id(),
            model = %prepared_session.selected_model(),
            "Prepared runtime session override for agent"
        );

        // Record single turn execution
        tracker.record_turn()?;

        // Verify budget constraints
        tracker.check_all()?;

        let elapsed = start.elapsed().as_secs();

        Ok(AgentExecutionResult {
            output: format!(
                "Prepared session for agent {} using model {}",
                context.agent_spec.display_name,
                prepared_session.selected_model()
            ),
            turns_used: tracker.turns_used(),
            tool_calls_used: tracker.tool_calls_used(),
            duration_secs: elapsed,
            success: true,
        })
    }
}
