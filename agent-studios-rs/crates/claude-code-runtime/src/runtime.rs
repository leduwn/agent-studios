use async_trait::async_trait;
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use agent_studios_external_runtime::{
    AgentRuntime, DEFAULT_EVENT_RETENTION, DiscoveredRuntimeInstance, RuntimeCapabilities,
    RuntimeError, RuntimeEventKind, RuntimeEventSubscription, RuntimeImplementationId,
    RuntimeInput, RuntimeLifecycleState, RuntimeSessionHandle, RuntimeSessionId, RuntimeSessionRef,
    RuntimeStartRequest, SanitizedRuntimeMessage, SessionEventHub,
};

use crate::approval::{ApprovalAuthority, parse_max_budget_usd, validate_max_budget_usd};
use crate::discovery::{
    CLAUDE_CODE_RUNTIME_ID, ClaudeCodeDiscoveryConfig, default_claude_capabilities,
    discover_claude_binary, resolve_binary_path,
};
use crate::mapping::map_claude_message;
use crate::process::{ClaudeCodeProcess, ClaudeCodeProcessConfig, spawn_claude_process};
use crate::protocol::{ClaudeInterruptInput, ClaudeStreamMessage, ClaudeUserPromptInput};
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

/// Default approval timeout duration if unspecified (60 seconds).
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

/// Vendor-neutral Claude Code CLI runtime adapter implementing `AgentRuntime`.
pub struct ClaudeCodeRuntime {
    implementation_id: RuntimeImplementationId,
    discovery_config: ClaudeCodeDiscoveryConfig,
    sandbox_policy: SandboxPolicy,
    permission_mode: Option<String>,
    approval_timeout: Duration,
    active_sessions: Arc<RwLock<BTreeMap<RuntimeSessionId, ClaudeCodeActiveSession>>>,
}

impl ClaudeCodeRuntime {
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            implementation_id: RuntimeImplementationId::new(CLAUDE_CODE_RUNTIME_ID)?,
            discovery_config: ClaudeCodeDiscoveryConfig::default(),
            sandbox_policy: SandboxPolicy::default(),
            permission_mode: Some("manual".to_string()),
            approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
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

    pub fn with_approval_timeout(mut self, timeout: Duration) -> Self {
        self.approval_timeout = timeout;
        self
    }

    pub fn approval_timeout(&self) -> Duration {
        self.approval_timeout
    }

    /// Validates request and builds the process launch configuration.
    /// Strictly validates budget constraints fail-closed before any child process is spawned.
    pub fn build_process_config(
        &self,
        request: &RuntimeStartRequest,
    ) -> Result<ClaudeCodeProcessConfig, RuntimeError> {
        request.validate()?;

        if request.implementation_id() != &self.implementation_id {
            return Err(RuntimeError::invalid_configuration(format!(
                "Implementation mismatch: request specified '{}', but this adapter is '{}'",
                request.implementation_id(),
                self.implementation_id
            )));
        }

        let mut max_budget_usd = None;

        if let Some(val) = request.metadata.get("max_budget_usd") {
            let parsed = parse_max_budget_usd(val.as_str()).map_err(|e| {
                RuntimeError::invalid_configuration(format!("Invalid max_budget_usd: {e}"))
            })?;
            max_budget_usd = Some(parsed);
        }

        if let Some(config_ref) = request.config_ref() {
            let cfg_str = config_ref.as_str();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                if let Some(budget_val) = json_val
                    .get("max_budget_usd")
                    .or_else(|| json_val.get("budget"))
                {
                    if let Some(num) = budget_val.as_f64() {
                        let parsed = validate_max_budget_usd(num).map_err(|e| {
                            RuntimeError::invalid_configuration(format!(
                                "Invalid max_budget_usd in config_ref: {e}"
                            ))
                        })?;
                        if max_budget_usd.is_none() {
                            max_budget_usd = Some(parsed);
                        }
                    } else if let Some(s) = budget_val.as_str() {
                        let parsed = parse_max_budget_usd(s).map_err(|e| {
                            RuntimeError::invalid_configuration(format!(
                                "Invalid max_budget_usd in config_ref: {e}"
                            ))
                        })?;
                        if max_budget_usd.is_none() {
                            max_budget_usd = Some(parsed);
                        }
                    } else {
                        return Err(RuntimeError::invalid_configuration(
                            "Invalid max_budget_usd in config_ref: must be number or string",
                        ));
                    }
                }
            } else if let Some(rest) = cfg_str
                .strip_prefix("budget=")
                .or_else(|| cfg_str.strip_prefix("max_budget_usd="))
                .or_else(|| cfg_str.strip_prefix("budget:"))
                .or_else(|| cfg_str.strip_prefix("max_budget_usd:"))
            {
                let parsed = parse_max_budget_usd(rest).map_err(|e| {
                    RuntimeError::invalid_configuration(format!(
                        "Invalid max_budget_usd in config_ref: {e}"
                    ))
                })?;
                if max_budget_usd.is_none() {
                    max_budget_usd = Some(parsed);
                }
            }
        }

        let working_directory =
            validate_session_workspace(&request.workspace, request.workspace_access_mode)
                .map_err(RuntimeError::from)?;

        let binary_path = resolve_binary_path(&self.discovery_config).ok_or_else(|| {
            RuntimeError::discovery_failed(
                "Claude Code executable binary was not found in PATH or configured directories",
            )
        })?;

        let model = request.model_ref.as_ref().map(|m| m.model_id().to_string());

        let mut env_vars = Vec::new();
        for binding in &request.environment_bindings {
            if let agent_studios_external_runtime::EnvironmentBindingSource::Literal { value } =
                binding.source()
            {
                env_vars.push((binding.name().to_string(), value.as_str().to_string()));
            }
        }

        Ok(ClaudeCodeProcessConfig {
            binary_path,
            working_dir: working_directory,
            session_id: uuid::Uuid::new_v4().to_string(),
            sandbox_policy: self.sandbox_policy.clone(),
            permission_mode: self.permission_mode.clone(),
            max_budget_usd,
            model,
            bare: false,
            env_vars,
        })
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
        let mut process_config = self.build_process_config(&request)?;

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

        process_config.session_id = raw_session_uuid.to_string();
        let working_directory = process_config.working_dir.clone();

        let approval_timeout = request
            .metadata
            .get("approval_timeout_secs")
            .and_then(|v| v.as_str().parse::<u64>().ok())
            .map(Duration::from_secs)
            .or_else(|| {
                request
                    .metadata
                    .get("approval_timeout_ms")
                    .and_then(|v| v.as_str().parse::<u64>().ok())
                    .map(Duration::from_millis)
            })
            .unwrap_or(self.approval_timeout);

        let (process, stdout) = spawn_claude_process(process_config)
            .await
            .map_err(RuntimeError::from)?;

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
        let approval_authority = Arc::new(ApprovalAuthority::with_session_id(
            raw_session_uuid.to_string(),
        ));

        let hub_clone = Arc::clone(&hub);
        let state_clone = Arc::clone(&state);
        let subagents_clone = Arc::clone(&subagents);
        let approval_clone = Arc::clone(&approval_authority);
        let process_clone = Arc::clone(&process);
        let session_ref_clone = session_ref.clone();

        let task = tokio::spawn(async move {
            let mut reader = NdjsonStreamReader::new(stdout);
            let mut stream_error = None;

            loop {
                match reader.read_message().await {
                    Ok(Some(msg)) => {
                        // If control request for tool approval, register with approval authority
                        if let ClaudeStreamMessage::ControlRequest(ref req) = msg
                            && let Some(approval_id) = approval_clone.register_request(req)
                            && !approval_clone.is_watchdog_active(&approval_id)
                        {
                            let process_wd = Arc::clone(&process_clone);
                            let hub_wd = Arc::clone(&hub_clone);
                            let timeout = approval_timeout;
                            approval_clone.spawn_watchdog(
                                approval_id,
                                timeout,
                                move |deny_resp| async move {
                                    let request_id = deny_resp.request_id().to_string();
                                    warn!(
                                        approval_id = %approval_id,
                                        request_id = %request_id,
                                        "Tool approval timed out; failing closed to Deny"
                                    );
                                    if let Ok(json) = serde_json::to_string(&deny_resp) {
                                        let _ = process_wd.write_stdin_line(&json).await;
                                    }
                                    let timeout_desc = if timeout.as_secs() > 0 {
                                        format!("{}s", timeout.as_secs())
                                    } else {
                                        format!("{}ms", timeout.as_millis())
                                    };
                                    let _ = hub_wd.emit(RuntimeEventKind::Diagnostic {
                                        level: "warn".to_string(),
                                        message: SanitizedRuntimeMessage::new(format!(
                                            "Approval timed out after {timeout_desc} (fail-closed)"
                                        )),
                                    }).await;
                                },
                            );
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
                    Ok(None) => break,
                    Err(e) => {
                        stream_error = Some(e);
                        break;
                    }
                }
            }

            approval_clone.cancel_all_pending();

            if let Some(err) = stream_error {
                let _ = process_clone.terminate().await;
                let _ = process_clone.wait_for_exit().await;
                let mut st = state_clone.write().await;
                if !st.is_terminal() {
                    *st = RuntimeLifecycleState::Failed;
                    let _ = hub_clone
                        .emit(RuntimeEventKind::Failed {
                            safe_error_summary: SanitizedRuntimeMessage::new(format!(
                                "Stream read error: {err}"
                            )),
                        })
                        .await;
                }
                return;
            }

            // Stream closed at EOF. Reap child and obtain true exit status.
            let status = process_clone.wait_for_exit().await;
            let mut st = state_clone.write().await;
            if !st.is_terminal() {
                let stderr = process_clone.stderr_snapshot().await;
                let (target_state, event) = classify_exit_status(status, &stderr);
                *st = target_state;
                let _ = hub_clone.emit(event).await;
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
        let active = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
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
                let prompt =
                    ClaudeUserPromptInput::new(session.session_id.as_uuid().to_string(), content);
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
                let (control_response, token) = active
                    .approval_authority
                    .prepare_resolution(approval_id, approved, reason)
                    .map_err(|e| RuntimeError::send_failed(session.session_id, e.to_string()))?;

                let json = match serde_json::to_string(&control_response) {
                    Ok(j) => j,
                    Err(e) => {
                        active.approval_authority.fail_delivery(token);
                        return Err(RuntimeError::send_failed(
                            session.session_id,
                            format!("Serialization of control response failed: {e}"),
                        ));
                    }
                };

                if let Err(e) = active.process.write_stdin_line(&json).await {
                    active.approval_authority.fail_delivery(token);
                    return Err(RuntimeError::send_failed(session.session_id, e.to_string()));
                }

                if let Err(e) = active.approval_authority.confirm_delivery(token) {
                    return Err(RuntimeError::send_failed(session.session_id, e.to_string()));
                }
                *active.state.write().await = RuntimeLifecycleState::Running;
            }
        }

        Ok(())
    }

    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        self.verify_session_ref(session)?;

        let sessions = self.active_sessions.read().await;
        let active = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
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

        // Cancel all pending approvals fail-closed upon interrupt so watchdogs
        // are aborted and pending approvals cannot accidentally execute.
        active.approval_authority.cancel_all_pending();

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
        let active = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
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
            // Cancel all pending approvals and abort watchdog tasks immediately
            active.approval_authority.cancel_all_pending();

            let mut st = active.state.write().await;
            if st.is_terminal() {
                return Ok(());
            }
            *st = RuntimeLifecycleState::Stopped;
            drop(st);

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

/// Classifies process exit into corresponding terminal runtime lifecycle state and event.
pub fn classify_exit_status(
    status: Option<std::process::ExitStatus>,
    stderr: &str,
) -> (RuntimeLifecycleState, RuntimeEventKind) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(ref s) = status {
            if let Some(sig) = s.signal() {
                if sig == 2 || sig == 15 {
                    return (RuntimeLifecycleState::Stopped, RuntimeEventKind::Stopped);
                }
            }
        }
    }
    classify_exit_code(status.and_then(|s| s.code()), stderr)
}

/// Classifies process exit code into corresponding terminal runtime lifecycle state and event.
pub fn classify_exit_code(
    code: Option<i32>,
    stderr: &str,
) -> (RuntimeLifecycleState, RuntimeEventKind) {
    match code {
        Some(0) => (
            RuntimeLifecycleState::Completed,
            RuntimeEventKind::Completed { summary: None },
        ),
        Some(2) => (
            RuntimeLifecycleState::Failed,
            RuntimeEventKind::Failed {
                safe_error_summary: SanitizedRuntimeMessage::new(
                    "Claude Code terminated: budget limit reached or authentication error",
                ),
            },
        ),
        Some(130) | Some(143) => (RuntimeLifecycleState::Stopped, RuntimeEventKind::Stopped),
        Some(code) => {
            let stderr_trimmed = stderr.trim();
            let summary = if stderr_trimmed.is_empty() {
                format!("Claude Code process exited with status code {code}")
            } else {
                format!("Claude Code process exited with code {code}: {stderr_trimmed}")
            };
            (
                RuntimeLifecycleState::Failed,
                RuntimeEventKind::Failed {
                    safe_error_summary: SanitizedRuntimeMessage::new(summary),
                },
            )
        }
        None => (RuntimeLifecycleState::Stopped, RuntimeEventKind::Stopped),
    }
}
