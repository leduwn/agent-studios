//! Deterministic Local Runtime and Multi-Runtime Discovery Dispatch
//!
//! Provides a deterministic in-process `AgentRuntime` implementation adhering strictly to
//! external runtime contracts, alongside runtime discovery across OpenCode and local adapters.

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use agent_studios_external_runtime::{
    AgentRuntime, DiscoveredRuntimeInstance, RuntimeAvailability, RuntimeCapabilities,
    RuntimeConfigRef, RuntimeError, RuntimeEventKind, RuntimeEventSubscription,
    RuntimeImplementationId, RuntimeInput, RuntimeInstanceId, RuntimeLifecycleState,
    RuntimeSessionHandle, RuntimeSessionId, RuntimeSessionRef, RuntimeStartRequest,
    SanitizedRuntimeMessage, SessionEventHub,
};

use crate::client::{ClientError, RuntimeMode};

pub const DETERMINISTIC_LOCAL_IMPL_ID: &str = "deterministic-local";

struct LocalSession {
    #[allow(dead_code)]
    handle: RuntimeSessionHandle,
    state: Arc<RwLock<RuntimeLifecycleState>>,
    hub: Arc<SessionEventHub>,
}

/// In-process deterministic runtime providing truthful local execution
/// adhering strictly to `AgentRuntime` lifecycle contracts.
#[derive(Clone)]
pub struct DeterministicLocalRuntime {
    implementation_id: RuntimeImplementationId,
    instance_id: RuntimeInstanceId,
    capabilities: RuntimeCapabilities,
    sessions: Arc<RwLock<HashMap<RuntimeSessionId, LocalSession>>>,
    mode: RuntimeMode,
}

impl DeterministicLocalRuntime {
    pub fn new() -> Self {
        Self::with_mode(RuntimeMode::Demo)
    }

    pub fn with_mode(mode: RuntimeMode) -> Self {
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_tools(false)
            .with_stop(true)
            .with_interrupt(true)
            .with_workspace_binding(true);

        let implementation_id = RuntimeImplementationId::new(DETERMINISTIC_LOCAL_IMPL_ID)
            .expect("valid implementation id");
        let instance_id = RuntimeInstanceId::generate();

        Self {
            implementation_id,
            instance_id,
            capabilities: caps,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            mode,
        }
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }
}

impl Default for DeterministicLocalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentRuntime for DeterministicLocalRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.implementation_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let instance = DiscoveredRuntimeInstance::available(
            self.instance_id,
            self.implementation_id.clone(),
            "Deterministic Local Engine",
            self.capabilities.clone(),
        )
        .with_version("0.1.0")
        .with_supported_config(RuntimeConfigRef::new("default")?);

        Ok(vec![instance])
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        self.capabilities.clone()
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        request.validate()?;

        let session_id = RuntimeSessionId::generate();
        let hub = Arc::new(SessionEventHub::new(session_id, self.instance_id, 1024));
        let state = Arc::new(RwLock::new(RuntimeLifecycleState::Running));

        let handle = RuntimeSessionHandle::new(
            session_id,
            self.instance_id,
            self.implementation_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace.clone(),
            request.workspace_access_mode,
            self.capabilities.clone(),
            request.correlation.clone(),
            Utc::now(),
        );

        // Sequence 1 must be SessionStarted
        hub.emit(RuntimeEventKind::SessionStarted {
            session_id,
            instance_id: self.instance_id,
        })
        .await?;

        // If initial prompt is supplied, run deterministic execution loop
        if let Some(prompt) = request.initial_prompt.clone() {
            let hub_clone = hub.clone();
            let state_clone = state.clone();

            let mode = self.mode;
            tokio::spawn(async move {
                // Truthful notice: deterministic demo/preview mode (no fake remote AI claim)
                let mode_label = match mode {
                    RuntimeMode::Demo => "DEMO",
                    RuntimeMode::Preview => "PREVIEW",
                    RuntimeMode::Production => "SIMULATED",
                };
                let diag_msg = SanitizedRuntimeMessage::new(format!(
                    "[{mode_label} MODE] Executing in local deterministic mode. No live remote model credentials configured."
                ));

                let _ = hub_clone
                    .emit(RuntimeEventKind::Diagnostic {
                        level: "info".to_string(),
                        message: diag_msg,
                    })
                    .await;

                // Streaming tokens / chunks
                let _ = hub_clone
                    .emit(RuntimeEventKind::OutputDelta {
                        text: format!("[{mode_label}] Goal received: \"{prompt}\"\n"),
                    })
                    .await;

                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;

                let _ = hub_clone
                    .emit(RuntimeEventKind::OutputDelta {
                        text: format!(
                            "[{mode_label}] Simulated scenario execution. In live production mode, real models stream token deltas.\n"
                        ),
                    })
                    .await;

                tokio::time::sleep(tokio::time::Duration::from_millis(15)).await;

                // Successful completion (no fake tool executions emitted)
                let _ = hub_clone
                    .emit(RuntimeEventKind::Completed {
                        summary: Some(format!(
                            "[{mode_label}] Goal finished in deterministic mode."
                        )),
                    })
                    .await;

                let mut s = state_clone.write().await;
                *s = RuntimeLifecycleState::Completed;
            });
        }

        let session = LocalSession {
            handle: handle.clone(),
            state,
            hub,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session);

        Ok(handle)
    }

    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        let hub = s.hub.clone();
        let prompt_text = match input {
            RuntimeInput::Text { content } => content,
            RuntimeInput::Continuation { .. } => "continue".to_string(),
            RuntimeInput::ApprovalResponse { approved, .. } => {
                format!("approval: {approved}")
            }
        };

        tokio::spawn(async move {
            let _ = hub
                .emit(RuntimeEventKind::OutputDelta {
                    text: format!("[Deterministic Local] Received input: {prompt_text}\n"),
                })
                .await;
            let _ = hub
                .emit(RuntimeEventKind::Completed {
                    summary: Some("[Deterministic Local] Input processed.".to_string()),
                })
                .await;
        });

        Ok(())
    }

    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        let mut state = s.state.write().await;
        *state = RuntimeLifecycleState::Interrupted;
        let reason = SanitizedRuntimeMessage::new("Interrupted by client");
        let _ = s.hub.emit(RuntimeEventKind::Interrupted { reason }).await;

        Ok(())
    }

    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        let mut state = s.state.write().await;
        *state = RuntimeLifecycleState::Running;

        if let Some(inp) = input {
            drop(state);
            drop(sessions);
            return self.send(session, inp).await;
        }

        Ok(())
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        let mut state = s.state.write().await;
        *state = RuntimeLifecycleState::Stopped;
        let _ = s.hub.emit(RuntimeEventKind::Stopped).await;

        Ok(())
    }

    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        let state = s.state.read().await;
        Ok(*state)
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
        let sessions = self.sessions.read().await;
        let s = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;

        s.hub.subscribe(after_sequence).await
    }
}

/// Resolves an `AgentRuntime` instance according to explicit `RuntimeMode`.
/// In `Production` mode:
/// Discovers available model-backed external runtimes (such as OpenCode).
/// Fails closed with `ClientError::Configuration` if no real runtime is available.
/// In `Demo` or `Preview` mode:
/// Returns `DeterministicLocalRuntime` explicitly labeled as simulated execution.
pub async fn resolve_runtime(mode: RuntimeMode) -> Result<Arc<dyn AgentRuntime>, ClientError> {
    match mode {
        RuntimeMode::Production => {
            if let Ok(mut opencode) = agent_studios_opencode_runtime::OpenCodeRuntime::new() {
                if let Some(catalog) = crate::embedded::load_catalog_snapshot_safe() {
                    opencode = opencode.with_catalog(Arc::new(catalog));
                }
                let instances = opencode.discover().await.unwrap_or_default();
                if instances
                    .iter()
                    .any(|i| i.availability == RuntimeAvailability::Available)
                {
                    tracing::info!(
                        "Discovered live OpenCode runtime instance; using OpenCodeRuntime"
                    );
                    return Ok(Arc::new(opencode));
                }
            }

            Err(ClientError::Configuration(
                "No AI runtime configured or discovered. Start an OpenCode daemon or CLI in PATH, or launch with `ags --demo` / `ags --preview` for simulated execution."
                    .to_string(),
            ))
        }
        RuntimeMode::Demo | RuntimeMode::Preview => {
            tracing::info!(mode = ?mode, "Using deterministic mock runtime for simulated execution");
            Ok(Arc::new(DeterministicLocalRuntime::with_mode(mode)))
        }
    }
}

/// Discovers all available runtime instances across configured adapters.
#[allow(clippy::collapsible_if)]
pub async fn discover_available_runtimes() -> Vec<DiscoveredRuntimeInstance> {
    let mut instances = Vec::new();
    if let Ok(mut opencode) = agent_studios_opencode_runtime::OpenCodeRuntime::new() {
        if let Some(catalog) = crate::embedded::load_catalog_snapshot_safe() {
            opencode = opencode.with_catalog(Arc::new(catalog));
        }
        if let Ok(found) = opencode.discover().await {
            instances.extend(found);
        }
    }
    let local = DeterministicLocalRuntime::new();
    if let Ok(found) = local.discover().await {
        instances.extend(found);
    }
    instances
}

/// Resolves the default runtime instance (for backwards compatibility, defaults to Demo).
pub async fn resolve_default_runtime() -> Arc<dyn AgentRuntime> {
    resolve_runtime(RuntimeMode::Demo)
        .await
        .expect("DeterministicLocalRuntime is infallible")
}
