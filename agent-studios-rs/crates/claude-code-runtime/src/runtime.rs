use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info};

use agent_studios_external_runtime::{
    AgentRuntime, DEFAULT_EVENT_RETENTION, DiscoveredRuntimeInstance, RuntimeCapabilities,
    RuntimeError, RuntimeEventKind, RuntimeEventSubscription, RuntimeImplementationId,
    RuntimeInput, RuntimeLifecycleState, RuntimeSessionHandle, RuntimeSessionId,
    RuntimeSessionRef, RuntimeStartRequest, SanitizedRuntimeMessage, SessionEventHub,
};

use crate::approval::ApprovalAuthority;
use crate::discovery::{
    CLAUDE_CODE_RUNTIME_ID, ClaudeCodeDiscoveryConfig, default_claude_capabilities,
    discover_claude_binary, resolve_binary_path,
};
use crate::mapping::map_claude_message;
use crate::process::{ClaudeCodeProcess, ClaudeCodeProcessConfig, spawn_claude_process};
use crate::protocol::{
    ClaudeInterruptInput, ClaudeStreamMessage, ClaudeUserPromptInput,
};
use crate::sandbox::{SandboxPolicy, validate_session_workspace};
use crate::stream::NdjsonStreamReader;
use crate::subagent::SubagentCorrelationTracker;

/// Active supervised session running Claude Code.
pub struct ClaudeCodeActiveSession {
    pub session_ref: RuntimeSessionRef,
    pub process: Arc<ClaudeCodeProcess>,
    pub hub: Arc<SessionEventHub>,
    pub state: Arc<RwLock<RuntimeLifecycleState>>,
    pub subagents: Arc<RwLock<SubagentCorrelationTracker>>,
    pub approval_authority: Arc<ApprovalAuthority>,
    pub working_directory: PathBuf,
    pub task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

/// Vendor-neutral Claude Code CLI runtime adapter implementing `AgentRuntime`.
pub struct ClaudeCodeRuntime {
    implementation_id: RuntimeImplementationId,
    discovery_config: ClaudeCodeDiscoveryConfig,
    sandbox_policy: SandboxPolicy,
    permission_mode: Option<String>,
    active_sessions: Arc<RwLock<BTreeMap<RuntimeSessionId, ClaudeCodeActiveSession>>>,
}

impl ClaudeCodeRuntime {
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            implementation_id: RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID)?,
            discovery_config: ClaudeCodeDiscoveryConfig::default(),
            sandbox_policy: SandboxPolicy::default(),
            permission_mode: Some("manual".to_string()),
            active_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    pub fn with_discovery_config(mut self, config: ClaudeCodeDiscoveryConfig) -> Self {
        self.discovery_config = config;
        self
    }

    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    pub fn with_permission_mode(mut self, mode: impl Into<String>) -> Self {
        self.permission_mode = Some(mode.into());
        self
    }

    pub async fn active_session_count(&self) -> usize {
        let lock = self.active_sessions.read().await;
        lock.len()
    }

    fn verify_session_ref(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        if session.implementation_id != self.implementation_id {
            return Err(RuntimeError::SessionImplementationMismatch {
                expected: self.implementation_id.clone(),
                actual: session.implementation_id.clone(),
                session_id: session.session_id,
            });
        }
        Ok(())
    }
}

#[async_trait]
impl AgentRuntime for ClaudeCodeRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.implementation_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        if let Some(inst) = discover_claude_binary(&self.discovery_config).await? {
            Ok(vec![inst])
        } else {
            Ok(Vec::new())
        }
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        default_claude_capabilities()
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        request.validate()?;

        if request.implementation_id() != &self.implementation_id {
            return Err(RuntimeError::invalid_configuration(format!(
                "Implementation mismatch: request specified '{}', but this adapter is '{}'",
                request.implementation_id(),
                self.implementation_id
            )));
        }

        let working_directory =
            validate_session_workspace(&request.workspace, request.workspace_access_mode)
                .map_err(RuntimeError::from)?;

        let binary_path = resolve_binary_path(&self.discovery_config).ok_or_else(|| {
            RuntimeError::discovery_failed(
                "Claude Code executable binary was not found in PATH or configured directories",
            )
        })?;

        let session_id = RuntimeSessionId::new();
        let raw_session_uuid = *session_id.as_uuid();
        let session_ref = RuntimeSessionRef::new(
            self.implementation_id.clone(),
            request.instance_ref.instance_id,
            session_id,
        );

        {
            let lock = self.active_sessions.read().await;
            if lock.contains_key(&session_id) {
                return Err(RuntimeError::invalid_configuration(format!(
                    "Session {session_id} is already active"
                )));
            }
        }

        let model = request.model_ref.as_ref().map(|m| m.model_id().to_string());
        let max_budget_usd = request
            .metadata
            .get("max_budget_usd")
            .and_then(|v| v.as_str().parse::<f64>().ok());

        let mut env_vars = Vec::new();
        for binding in &request.environment_bindings {
            if let agent_studios_external_runtime::EnvironmentBindingSource::Literal { value } =
                binding.source()
            {
                env_vars.push((binding.name().to_string(), value.as_str().to_string()));
            }
        }

        let process_config = ClaudeCodeProcessConfig {
            binary_path,
            working_dir: working_directory.clone(),
            session_id: raw_session_uuid.to_string(),
            sandbox_policy: self.sandbox_policy.clone(),
            permission_mode: self.permission_mode.clone(),
            max_budget_usd,
            model,
            bare: false,
            env_vars,
        };

        let (process, stdout) =
            spawn_claude_process(process_config).await.map_err(RuntimeError::from)?;

        let hub = Arc::new(SessionEventHub::new(
            session_id,
            request.instance_ref.instance_id,
            DEFAULT_EVENT_RETENTION,
        ));

        // Invariant: Sequence 1 must be SessionStarted
        hub.emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: request.instance_ref.instance_id,
        })
        .await?;

        let state = Arc::new(RwLock::new(RuntimeLifecycleState::Running));
        let subagents = Arc::new(RwLock::new(SubagentCorrelationTracker::new()));
        let approval_authority = Arc::new(ApprovalAuthority::new());

        let hub_clone = Arc::clone(&hub);
        let state_clone = Arc::clone(&state);
        let subagents_clone = Arc::clone(&subagents);
        let approval_clone = Arc::clone(&approval_authority);
        let process_clone = Arc::clone(&process);
        let session_ref_clone = session_ref.clone();

        let task = tokio::spawn(async move {
            let mut reader = NdjsonStreamReader::new(stdout);

            while let Ok(Some(msg)) = reader.read_message().await {
                // If control request for tool approval, register with approval authority
                if let ClaudeStreamMessage::ControlRequest(ref req) = msg {
                    approval_clone.register_request(req);
                }

                // Map message to normalized runtime events
                let events = {
                    let mut sub_lock = subagents_clone.write().await;
                    map_claude_message(&msg, &session_ref_clone, &mut sub_lock)
                };

                // Emit to SessionEventHub and advance lifecycle state
                for event_kind in events {
                    if let Some(target_state) = event_kind.to_lifecycle_state() {
                        let mut st = state_clone.write().await;
                        if !st.is_terminal() {
                            *st = target_state;
                        }
                    }

                    if let Err(e) = hub_clone.emit(event_kind).await {
                        debug!(error = %e, "Event publication to hub failed or ignored");
                    }
                }
            }

            // Stream closed. Check process exit status.
            let status = process_clone.exit_status().await;
            let mut st = state_clone.write().await;
            if !st.is_terminal() {
                match status.and_then(|s| s.code()) {
                    Some(0) => {
                        *st = RuntimeLifecycleState::Completed;
                        let _ = hub_clone.emit(RuntimeEventKind::Completed { summary: None }).await;
                    }
                    Some(2) => {
                        *st = RuntimeLifecycleState::Failed;
                        let _ = hub_clone
                            .emit(RuntimeEventKind::Failed {
                                safe_error_summary: SanitizedRuntimeMessage::new(
                                    "Claude Code terminated: budget limit reached or authentication error",
                                ),
                            })
                            .await;
                    }
                    Some(130) | Some(143) => {
                        *st = RuntimeLifecycleState::Stopped;
                        let _ = hub_clone.emit(RuntimeEventKind::Stopped).await;
                    }
                    Some(code) => {
                        *st = RuntimeLifecycleState::Failed;
                        let stderr = process_clone.stderr_snapshot().await;
                        let summary = if stderr.is_empty() {
                            format!("Claude Code process exited with status code {code}")
                        } else {
                            format!("Claude Code process exited with code {code}: {stderr}")
                        };
                        let _ = hub_clone
                            .emit(RuntimeEventKind::Failed {
                                safe_error_summary: SanitizedRuntimeMessage::new(summary),
                            })
                            .await;
                    }
                    None => {
                        *st = RuntimeLifecycleState::Stopped;
                        let _ = hub_clone.emit(RuntimeEventKind::Stopped).await;
                    }
                }
            }
        });

        // Submit initial prompt if specified in start request
        if let Some(ref prompt) = request.initial_prompt {
            let prompt_input = ClaudeUserPromptInput::new(raw_session_uuid.to_string(), prompt);
            let json = serde_json::to_string(&prompt_input).map_err(|e| {
                RuntimeError::startup_failed(format!("Serialization of prompt input failed: {e}"))
            })?;
            process
                .write_stdin_line(&json)
                .await
                .map_err(RuntimeError::from)?;
        }

        let active_session = ClaudeCodeActiveSession {
            session_ref: session_ref.clone(),
            process,
            hub,
            state,
            subagents,
            approval_authority,
            working_directory,
            task_handle: Arc::new(Mutex::new(Some(task))),
        };

        {
            let mut lock = self.active_sessions.write().await;
            lock.insert(session_id, active_session);
        }

        info!(session_id = %session_id, "Claude Code runtime session started successfully");

        let capabilities = self.capabilities().await;
        Ok(RuntimeSessionHandle::new(
            session_id,
            request.instance_ref.instance_id,
            self.implementation_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace.clone(),
            request.workspace_access_mode,
            capabilities,
            request.correlation.clone(),
            Utc::now(),
        ))
    }

    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        let active = sessions.get(&session.session_id).ok_or(RuntimeError::SessionNotFound {
            session_id: session.session_id,
        })?;

        if session.instance_id != active.session_ref.instance_id {
            return Err(RuntimeError::SessionInstanceMismatch {
                expected: active.session_ref.instance_id,
                actual: session.instance_id,
                session_id: session.session_id,
            });
        }

        let current_state = *active.state.read().await;
        if current_state.is_terminal() {
            return Err(RuntimeError::terminal_state_error(
                session.session_id,
                current_state,
                "send",
            ));
        }

        match input {
            RuntimeInput::Text { content } => {
                let prompt = ClaudeUserPromptInput::new(
                    session.session_id.as_uuid().to_string(),
                    content,
                );
                let json = serde_json::to_string(&prompt).map_err(|e| {
                    RuntimeError::send_failed(
                        session.session_id,
                        format!("Serialization of prompt failed: {e}"),
                    )
                })?;
                active
                    .process
                    .write_stdin_line(&json)
                    .await
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;
                *active.state.write().await = RuntimeLifecycleState::Running;
            }
            RuntimeInput::Continuation { context } => {
                let prompt = ClaudeUserPromptInput::new(
                    session.session_id.as_uuid().to_string(),
                    context.unwrap_or_else(|| "continue".to_string()),
                );
                let json = serde_json::to_string(&prompt).map_err(|e| {
                    RuntimeError::send_failed(
                        session.session_id,
                        format!("Serialization of continuation failed: {e}"),
                    )
                })?;
                active
                    .process
                    .write_stdin_line(&json)
                    .await
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;
                *active.state.write().await = RuntimeLifecycleState::Running;
            }
            RuntimeInput::ApprovalResponse {
                approval_id,
                approved,
                reason,
            } => {
                let control_response = active
                    .approval_authority
                    .resolve_approval_by_id(approval_id, approved, reason)
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;

                let json = serde_json::to_string(&control_response).map_err(|e| {
                    RuntimeError::send_failed(
                        session.session_id,
                        format!("Serialization of control response failed: {e}"),
                    )
                })?;
                active
                    .process
                    .write_stdin_line(&json)
                    .await
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;
                *active.state.write().await = RuntimeLifecycleState::Running;
            }
        }

        Ok(())
    }

    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        let active = sessions.get(&session.session_id).ok_or(RuntimeError::SessionNotFound {
            session_id: session.session_id,
        })?;

        if session.instance_id != active.session_ref.instance_id {
            return Err(RuntimeError::SessionInstanceMismatch {
                expected: active.session_ref.instance_id,
                actual: session.instance_id,
                session_id: session.session_id,
            });
        }

        let current_state = *active.state.read().await;
        if current_state.is_terminal() {
            return Err(RuntimeError::terminal_state_error(
                session.session_id,
                current_state,
                "interrupt",
            ));
        }

        let int_msg = ClaudeInterruptInput::new(uuid::Uuid::new_v4().to_string());
        let json = serde_json::to_string(&int_msg).map_err(|e| {
            RuntimeError::interruption_failed(
                session.session_id,
                format!("Serialization of interrupt request failed: {e}"),
            )
        })?;

        active
            .process
            .write_stdin_line(&json)
            .await
            .map_err(|e| RuntimeError::interruption_failed(session.session_id, e.to_string()))?;
        *active.state.write().await = RuntimeLifecycleState::Interrupted;
        let _ = active
            .hub
            .emit(RuntimeEventKind::Interrupted {
                reason: SanitizedRuntimeMessage::new("Execution interrupted by host"),
            })
            .await;

        Ok(())
    }

    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        let active = sessions.get(&session.session_id).ok_or(RuntimeError::SessionNotFound {
            session_id: session.session_id,
        })?;

        if session.instance_id != active.session_ref.instance_id {
            return Err(RuntimeError::SessionInstanceMismatch {
                expected: active.session_ref.instance_id,
                actual: session.instance_id,
                session_id: session.session_id,
            });
        }

        let current_state = *active.state.read().await;
        if current_state != RuntimeLifecycleState::Interrupted {
            return Err(RuntimeError::resume_failed(
                session.session_id,
                format!(
                    "Session {} is in state {:?}; expected Interrupted to resume",
                    session.session_id, current_state
                ),
            ));
        }

        drop(sessions);

        if let Some(inp) = input {
            self.send(session, inp).await?;
        } else {
            self.send(session, RuntimeInput::continuation(None)).await?;
        }

        Ok(())
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        self.verify_session_ref(session)?;

        let mut sessions = self.active_sessions.write().await;
        if let Some(active) = sessions.remove(&session.session_id) {
            let mut st = active.state.write().await;
            if st.is_terminal() {
                return Ok(());
            }
            *st = RuntimeLifecycleState::Stopped;
            drop(st);

            active.approval_authority.cancel_all_pending();
            let _ = active.process.terminate().await;
            let _ = active.hub.emit(RuntimeEventKind::Stopped).await;

            let mut handle_lock = active.task_handle.lock().await;
            if let Some(handle) = handle_lock.take() {
                handle.abort();
            }
        }

        // Idempotent completion
        Ok(())
    }

    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        if let Some(active) = sessions.get(&session.session_id) {
            if session.instance_id != active.session_ref.instance_id {
                return Err(RuntimeError::SessionInstanceMismatch {
                    expected: active.session_ref.instance_id,
                    actual: session.instance_id,
                    session_id: session.session_id,
                });
            }
            Ok(*active.state.read().await)
        } else {
            Err(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })
        }
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        if let Some(active) = sessions.get(&session.session_id) {
            if session.instance_id != active.session_ref.instance_id {
                return Err(RuntimeError::SessionInstanceMismatch {
                    expected: active.session_ref.instance_id,
                    actual: session.instance_id,
                    session_id: session.session_id,
                });
            }
            active.hub.subscribe(after_sequence).await
        } else {
            Err(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })
        }
    }
}
