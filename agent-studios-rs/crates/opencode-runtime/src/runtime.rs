use std::collections::BTreeMap;
use std::sync::Arc;

use agent_studios_external_runtime::{
    AgentRuntime, DiscoveredRuntimeInstance, RuntimeAvailability, RuntimeCapabilities,
    RuntimeConfigRef, RuntimeError, RuntimeEventKind, RuntimeEventSubscription,
    RuntimeImplementationId, RuntimeInput, RuntimeInstanceId, RuntimeLifecycleState,
    RuntimeSessionHandle, RuntimeSessionId, RuntimeSessionRef, RuntimeStartRequest,
    SanitizedRuntimeMessage, SessionEventHub,
};
use agent_studios_provider::ProviderCatalog;
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use url::Url;

use crate::client::{CreateSessionRequest, OpenCodeHttpClient, OpenCodeMessageRequest};
use crate::control_plane::OpenCodeSessionContext;
use crate::discovery::{
    OPENCODE_IMPLEMENTATION_ID, OpenCodeDiscoveryConfig, default_opencode_capabilities,
    discover_opencode_binary,
};
use crate::provider_mapping::map_model_ref_to_opencode;
use crate::sandbox::SandboxPolicy;
use crate::server::{OpenCodeServerConfig, OpenCodeServerHandle, spawn_opencode_server};
use crate::sse::run_sse_ingestion_loop;

/// Retention limit for replay events held by the SessionEventHub.
const DEFAULT_EVENT_RETENTION: usize = 1000;

/// Representation of an active OpenCode session managed by OpenCodeRuntime.
pub struct OpenCodeActiveSession {
    pub session_ref: RuntimeSessionRef,
    pub opencode_session_id: String,
    pub hub: Arc<SessionEventHub>,
    pub client: OpenCodeHttpClient,
    pub server_handle: Option<Arc<OpenCodeServerHandle>>,
    pub context: OpenCodeSessionContext,
    pub state: Arc<RwLock<RuntimeLifecycleState>>,
}

/// Vendor-neutral OpenCode runtime adapter implementing `AgentRuntime`.
pub struct OpenCodeRuntime {
    implementation_id: RuntimeImplementationId,
    discovery_config: OpenCodeDiscoveryConfig,
    catalog: Option<Arc<ProviderCatalog>>,
    default_sandbox_policy: SandboxPolicy,
    mock_server_url: Option<Url>,
    mock_server_token: Option<String>,
    active_sessions: Arc<RwLock<BTreeMap<RuntimeSessionId, OpenCodeActiveSession>>>,
}

impl OpenCodeRuntime {
    pub fn new() -> Result<Self, RuntimeError> {
        Ok(Self {
            implementation_id: RuntimeImplementationId::new(OPENCODE_IMPLEMENTATION_ID)?,
            discovery_config: OpenCodeDiscoveryConfig::default(),
            catalog: None,
            default_sandbox_policy: SandboxPolicy::default(),
            mock_server_url: None,
            mock_server_token: None,
            active_sessions: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }

    pub fn with_discovery_config(mut self, config: OpenCodeDiscoveryConfig) -> Self {
        self.discovery_config = config;
        self
    }

    pub fn with_catalog(mut self, catalog: Arc<ProviderCatalog>) -> Self {
        self.catalog = Some(catalog);
        self
    }

    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.default_sandbox_policy = policy;
        self
    }

    /// Configures this runtime to target an existing or mock OpenCode HTTP server.
    pub fn with_mock_server(mut self, url: Url, token: impl Into<String>) -> Self {
        self.mock_server_url = Some(url);
        self.mock_server_token = Some(token.into());
        self
    }
}

#[async_trait]
impl AgentRuntime for OpenCodeRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.implementation_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        if let Some(ref mock_url) = self.mock_server_url {
            let mut metadata = BTreeMap::new();
            metadata.insert("mock_server_url".to_string(), mock_url.to_string());

            return Ok(vec![DiscoveredRuntimeInstance {
                implementation_id: self.implementation_id.clone(),
                instance_id: RuntimeInstanceId::from_uuid(uuid::Uuid::from_u128(1)),
                display_name: "OpenCode Mock Server".to_string(),
                version: Some("mock-server-1.0.0".to_string()),
                binary_path: None,
                capabilities: default_opencode_capabilities(),
                supported_configs: vec![
                    RuntimeConfigRef::new("default")?,
                    RuntimeConfigRef::new("sandbox-strict")?,
                    RuntimeConfigRef::new("sandbox-standard")?,
                ],
                availability: RuntimeAvailability::Available,
                metadata,
            }]);
        }

        match discover_opencode_binary(&self.discovery_config).await? {
            Some(instance) => Ok(vec![instance]),
            None => Ok(Vec::new()),
        }
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        default_opencode_capabilities()
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        // 1. Authoritative pre-spawn validation
        request.validate()?;

        if request.implementation_id() != &self.implementation_id {
            return Err(RuntimeError::invalid_configuration(format!(
                "Implementation mismatch: request specified '{}', but this adapter is '{}'",
                request.implementation_id(),
                self.implementation_id
            )));
        }

        // Validate model ref upfront before spawning process or creating session (M11-R05, M11-R07)
        let model_binding = if let Some(ref model_ref) = request.model_ref {
            let catalog = self.catalog.as_ref().ok_or_else(|| {
                RuntimeError::invalid_configuration(
                    "ModelRef provided but no ProviderCatalog configured on OpenCodeRuntime",
                )
            })?;
            let binding =
                map_model_ref_to_opencode(catalog, model_ref).map_err(RuntimeError::from)?;
            Some(binding)
        } else {
            None
        };

        // 2. Extract and validate session context and worktree lease
        let context =
            OpenCodeSessionContext::from_start_request(&request).map_err(RuntimeError::from)?;

        let session_id = RuntimeSessionId::new();
        let session_ref = RuntimeSessionRef::new(
            self.implementation_id.clone(),
            *request.instance_id(),
            session_id,
        );

        let hub = Arc::new(SessionEventHub::new(
            session_id,
            *request.instance_id(),
            DEFAULT_EVENT_RETENTION,
        ));

        // 3. Connect to server or spawn process
        let (client, server_handle) = if let Some(ref mock_url) = self.mock_server_url {
            let token = self
                .mock_server_token
                .clone()
                .unwrap_or_else(|| "mock_token".to_string());
            let client = OpenCodeHttpClient::with_credentials(mock_url.clone(), "opencode", token)
                .map_err(RuntimeError::from)?;
            (client, None)
        } else {
            // Resolve binary
            let binary_path = self
                .discovery_config
                .custom_binary_path
                .clone()
                .or_else(|| {
                    std::env::var("OPENCODE_BIN")
                        .ok()
                        .map(std::path::PathBuf::from)
                })
                .unwrap_or_else(|| std::path::PathBuf::from("opencode"));

            let mut server_cfg = OpenCodeServerConfig::new(binary_path, &context.working_directory);
            server_cfg = server_cfg.with_sandbox_policy(self.default_sandbox_policy.clone());

            // Bind environment variables
            for binding in &request.environment_bindings {
                if let agent_studios_external_runtime::EnvironmentBindingSource::Literal { value } =
                    binding.source()
                {
                    server_cfg = server_cfg.with_env_var(binding.name(), value.as_str());
                }
            }

            let handle = spawn_opencode_server(server_cfg)
                .await
                .map_err(RuntimeError::from)?;
            let client = OpenCodeHttpClient::with_credentials(
                handle.base_url().clone(),
                handle.username(),
                handle.password(),
            )
            .map_err(RuntimeError::from)?;
            (client, Some(Arc::new(handle)))
        };

        // 4. Create OpenCode session with transactional cleanup on failure (M11-R05)
        let opencode_session = match client
            .create_session(CreateSessionRequest {
                title: Some(format!("AgentStudios-{}", session_id)),
                directory: Some(context.working_directory.display().to_string()),
            })
            .await
        {
            Ok(s) => s,
            Err(e) => {
                if let Some(ref srv) = server_handle {
                    let _ = srv.stop().await;
                }
                return Err(RuntimeError::from(e));
            }
        };

        // 5. Open SSE event stream and launch ingestion reader
        if let Ok(sse_resp) = client.open_event_stream().await {
            tokio::spawn(run_sse_ingestion_loop(
                sse_resp,
                hub.clone(),
                session_id,
                *request.instance_id(),
                Some(opencode_session.id.clone()),
            ));
        } else {
            // Emit authoritative initial SessionStarted if SSE stream delayed
            hub.emit(RuntimeEventKind::SessionStarted {
                session_id,
                instance_id: *request.instance_id(),
            })
            .await?;
        }

        // 6. Send initial prompt if present with transactional rollback on failure (M11-R05)
        if let Some(ref prompt) = request.initial_prompt {
            let mut msg_req = OpenCodeMessageRequest::from_content(prompt);
            if let Some(ref binding) = model_binding {
                msg_req =
                    msg_req.with_model(binding.provider_name.clone(), binding.model_name.clone());
            }

            if let Err(e) = client.send_message(&opencode_session.id, msg_req).await {
                if let Some(ref srv) = server_handle {
                    let _ = srv.stop().await;
                }
                return Err(RuntimeError::from(e));
            }
        }

        let state = Arc::new(RwLock::new(RuntimeLifecycleState::Running));

        let active = OpenCodeActiveSession {
            session_ref: session_ref.clone(),
            opencode_session_id: opencode_session.id,
            hub,
            client,
            server_handle,
            context,
            state: state.clone(),
        };

        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id, active);
        }

        let capabilities = self.capabilities().await;
        Ok(RuntimeSessionHandle::new(
            session_id,
            *request.instance_id(),
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

        // Synchronize state with hub events (M11-R05)
        let mut current_state = *active.state.read().await;
        if !current_state.is_terminal() {
            let events = active.hub.recorded_events().await;
            for ev in events.iter().rev() {
                if let Some(st) = ev
                    .kind
                    .to_lifecycle_state()
                    .filter(|&st| st != current_state)
                {
                    *active.state.write().await = st;
                    current_state = st;
                    break;
                }
            }
        }

        if current_state.is_terminal() {
            return Err(RuntimeError::TerminalStateError {
                session_id: session.session_id,
                current_state,
                attempted_action: "send".to_string(),
            });
        }

        match input {
            RuntimeInput::Text { content } => {
                active
                    .client
                    .send_message(
                        &active.opencode_session_id,
                        OpenCodeMessageRequest::from_content(content),
                    )
                    .await
                    .map_err(RuntimeError::from)?;
            }
            RuntimeInput::ApprovalResponse {
                approval_id,
                approved,
                reason,
            } => {
                // Dispatch decision via dedicated permission response endpoint rather than chat message (M11-R08)
                let req = match reason {
                    Some(r) => crate::client::PermissionDecisionRequest::with_reason(approved, r),
                    None => crate::client::PermissionDecisionRequest::new(approved),
                };
                active
                    .client
                    .respond_permission(&active.opencode_session_id, &approval_id.to_string(), req)
                    .await
                    .map_err(RuntimeError::from)?;
            }
            RuntimeInput::Continuation { context } => {
                let text = context.unwrap_or_else(|| "continue".to_string());
                active
                    .client
                    .send_message(
                        &active.opencode_session_id,
                        OpenCodeMessageRequest::from_content(text),
                    )
                    .await
                    .map_err(RuntimeError::from)?;
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

        // Synchronize state with hub events (M11-R05)
        let mut current_state = *active.state.read().await;
        if !current_state.is_terminal() {
            let events = active.hub.recorded_events().await;
            for ev in events.iter().rev() {
                if let Some(st) = ev
                    .kind
                    .to_lifecycle_state()
                    .filter(|&st| st != current_state)
                {
                    *active.state.write().await = st;
                    current_state = st;
                    break;
                }
            }
        }

        if current_state.is_terminal() {
            return Err(RuntimeError::TerminalStateError {
                session_id: session.session_id,
                current_state,
                attempted_action: "interrupt".to_string(),
            });
        }

        active
            .client
            .abort_session(&active.opencode_session_id)
            .await
            .map_err(RuntimeError::from)?;

        *active.state.write().await = RuntimeLifecycleState::Interrupted;
        let _ = active
            .hub
            .emit(RuntimeEventKind::Interrupted {
                reason: SanitizedRuntimeMessage::new("Session interrupted by user"),
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
            return Err(RuntimeError::InvalidLifecycleTransition {
                from: current_state,
                to: RuntimeLifecycleState::Running,
                reason: format!("Cannot resume session in state {current_state:?}").into(),
            });
        }

        *active.state.write().await = RuntimeLifecycleState::Running;

        if let Some(inp) = input {
            drop(sessions);
            if let Err(e) = self.send(session, inp).await {
                // Atomic state rollback on error (M11-R05)
                let sessions = self.active_sessions.read().await;
                if let Some(active) = sessions.get(&session.session_id) {
                    *active.state.write().await = RuntimeLifecycleState::Interrupted;
                }
                return Err(e);
            }
        }

        Ok(())
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
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

        // Synchronize state with hub events if not yet terminal
        {
            let current = *active.state.read().await;
            if !current.is_terminal() {
                let events = active.hub.recorded_events().await;
                for ev in events.iter().rev() {
                    if let Some(st) = ev.kind.to_lifecycle_state().filter(|&st| st != current) {
                        *active.state.write().await = st;
                        break;
                    }
                }
            }
        }

        let mut state_guard = active.state.write().await;
        if state_guard.is_terminal() {
            // Idempotent stop on terminal state (M11-R05)
            if let Some(ref srv) = active.server_handle {
                let _ = srv.stop().await;
            }
            return Ok(());
        }

        *state_guard = RuntimeLifecycleState::Stopped;

        if let Some(ref srv) = active.server_handle {
            let _ = srv.stop().await;
        }

        let _ = active.hub.emit(RuntimeEventKind::Stopped).await;
        Ok(())
    }

    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
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

        let mut state = *active.state.read().await;
        if !state.is_terminal() {
            let events = active.hub.recorded_events().await;
            for ev in events.iter().rev() {
                if let Some(st) = ev.kind.to_lifecycle_state().filter(|&st| st != state) {
                    *active.state.write().await = st;
                    state = st;
                    break;
                }
            }
        }
        Ok(state)
    }

    async fn events_after(
        &self,
        session: &RuntimeSessionRef,
        after_sequence: Option<u64>,
    ) -> Result<RuntimeEventSubscription, RuntimeError> {
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

        active.hub.subscribe(after_sequence).await
    }
}

impl OpenCodeRuntime {
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
