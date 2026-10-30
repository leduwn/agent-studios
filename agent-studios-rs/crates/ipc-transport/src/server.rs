use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use agent_studios_control_plane::ControlPlane;
use agent_studios_ipc_protocol::{
    ApprovalContract, ApprovalId, HmacSigner, IpcEvent, NonceTracker, ReplayBuffer, SessionId,
};
use agent_studios_protocol::event::EventEnvelope;
use agent_studios_provider::ProviderCatalog;

use crate::error::TransportError;

/// Client authorization role associated with an authenticated key identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientRole {
    Admin,
    Standard,
}

/// Pending runtime approval mapping tracking correlation between Control Plane approvals and external agent runtimes.
#[derive(Clone)]
pub struct PendingRuntimeApproval {
    pub session_ref: agent_studios_external_runtime::RuntimeSessionRef,
    pub runtime_approval_id: agent_studios_protocol::id::ApprovalId,
    pub runtime: Arc<dyn agent_studios_external_runtime::AgentRuntime>,
}

pub type ActiveRuntimeSessions = Arc<
    tokio::sync::RwLock<
        HashMap<
            agent_studios_protocol::id::TaskId,
            (
                agent_studios_external_runtime::RuntimeSessionRef,
                Arc<dyn agent_studios_external_runtime::AgentRuntime>,
            ),
        >,
    >,
>;

pub struct ServerState {
    pub auth_signer: Arc<HmacSigner>,
    pub nonce_tracker: Arc<Mutex<NonceTracker>>,
    pub replay_buffer: Arc<Mutex<ReplayBuffer>>,
    pub broadcast_tx: tokio::sync::broadcast::Sender<IpcEvent>,
    pub active_connections: Arc<AtomicUsize>,
    pub shutdown_token: CancellationToken,
    pub start_time: i64,
    pub approvals: Arc<Mutex<HashMap<ApprovalId, ApprovalContract>>>,
    pub approval_origins: Arc<Mutex<HashMap<ApprovalId, (SessionId, String)>>>,
    pub auth_keys: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub key_roles: Arc<Mutex<HashMap<String, ClientRole>>>,
    pub supported_capabilities: Vec<String>,
    pub control_plane: Arc<tokio::sync::RwLock<ControlPlane>>,
    pub provider_catalog: Arc<tokio::sync::RwLock<ProviderCatalog>>,
    pub auto_execute: bool,
    pub runtime: Option<Arc<dyn agent_studios_external_runtime::AgentRuntime>>,
    pub workspace_orchestrator: Option<Arc<agent_studios_workspace::WorkspaceOrchestrator>>,
    pub active_runtime_sessions: ActiveRuntimeSessions,
    pub pending_runtime_approvals: Arc<
        tokio::sync::RwLock<
            HashMap<agent_studios_protocol::id::ApprovalId, PendingRuntimeApproval>,
        >,
    >,
}

impl ServerState {
    pub fn register_approval_origin(
        &self,
        approval_id: impl Into<String>,
        session_id: SessionId,
        key_id: impl Into<String>,
    ) {
        self.approval_origins.lock().unwrap().insert(
            ApprovalId::new(approval_id.into()),
            (session_id, key_id.into()),
        );
    }

    pub fn publish_event(&self, event: IpcEvent) -> Result<(), TransportError> {
        self.replay_buffer.lock().unwrap().push(event.clone())?;
        let _ = self.broadcast_tx.send(event);
        Ok(())
    }

    pub fn emit_event(
        &self,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<IpcEvent, TransportError> {
        let mut replay = self.replay_buffer.lock().unwrap();
        let next_seq = replay.latest_sequence().unwrap_or(0) + 1;
        let event = IpcEvent::new(next_seq, event_type, payload);
        replay.push(event.clone())?;
        let _ = self.broadcast_tx.send(event.clone());
        Ok(event)
    }

    pub fn publish_control_plane_events(
        &self,
        envelopes: Vec<EventEnvelope>,
    ) -> Result<(), TransportError> {
        let mut replay = self.replay_buffer.lock().unwrap();
        for envelope in envelopes {
            let next_seq = replay.latest_sequence().unwrap_or(0) + 1;
            let kind = serde_json::to_value(&envelope.event)
                .ok()
                .and_then(|v| {
                    v.get("kind")
                        .and_then(|k| k.as_str().map(|s| s.to_string()))
                })
                .unwrap_or_else(|| "control_plane_event".to_string());
            let payload = serde_json::to_value(&envelope).unwrap_or_default();
            let event = IpcEvent {
                sequence: next_seq,
                timestamp: envelope.timestamp,
                event_type: kind,
                payload,
            };
            replay.push(event.clone())?;
            let _ = self.broadcast_tx.send(event);
        }
        Ok(())
    }
}

pub struct IpcServerBuilder {
    auth_keys: HashMap<String, Vec<u8>>,
    key_roles: HashMap<String, ClientRole>,
    replay_capacity: usize,
    supported_capabilities: Vec<String>,
    control_plane: Option<ControlPlane>,
    provider_catalog: Option<ProviderCatalog>,
    auto_execute: bool,
    runtime: Option<Arc<dyn agent_studios_external_runtime::AgentRuntime>>,
    workspace_orchestrator: Option<Arc<agent_studios_workspace::WorkspaceOrchestrator>>,
}

impl Default for IpcServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcServerBuilder {
    pub fn new() -> Self {
        Self {
            auth_keys: HashMap::new(),
            key_roles: HashMap::new(),
            replay_capacity: 1000,
            supported_capabilities: vec![
                "worktree_isolation".to_string(),
                "approval_routing".to_string(),
                "event_streaming".to_string(),
                "event_replay".to_string(),
                "secret_redaction".to_string(),
                "hmac_auth".to_string(),
                "tool_execution".to_string(),
                "diff_inspection".to_string(),
            ],
            control_plane: None,
            provider_catalog: None,
            auto_execute: false,
            runtime: None,
            workspace_orchestrator: None,
        }
    }

    pub fn with_auth_key(mut self, id: impl Into<String>, key: impl Into<Vec<u8>>) -> Self {
        let id_str = id.into();
        self.auth_keys.insert(id_str.clone(), key.into());
        self.key_roles.insert(id_str, ClientRole::Standard);
        self
    }

    pub fn with_admin_auth_key(mut self, id: impl Into<String>, key: impl Into<Vec<u8>>) -> Self {
        let id_str = id.into();
        self.auth_keys.insert(id_str.clone(), key.into());
        self.key_roles.insert(id_str, ClientRole::Admin);
        self
    }

    pub fn with_auth_key_role(
        mut self,
        id: impl Into<String>,
        key: impl Into<Vec<u8>>,
        role: ClientRole,
    ) -> Self {
        let id_str = id.into();
        self.auth_keys.insert(id_str.clone(), key.into());
        self.key_roles.insert(id_str, role);
        self
    }

    pub fn with_replay_capacity(mut self, capacity: usize) -> Self {
        self.replay_capacity = capacity;
        self
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.supported_capabilities = capabilities;
        self
    }

    pub fn with_control_plane(mut self, control_plane: ControlPlane) -> Self {
        self.control_plane = Some(control_plane);
        self
    }

    pub fn with_provider_catalog(mut self, provider_catalog: ProviderCatalog) -> Self {
        self.provider_catalog = Some(provider_catalog);
        self
    }

    pub fn with_auto_execute(mut self, auto_execute: bool) -> Self {
        self.auto_execute = auto_execute;
        self
    }

    pub fn with_runtime(
        mut self,
        runtime: Arc<dyn agent_studios_external_runtime::AgentRuntime>,
    ) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_workspace_orchestrator(
        mut self,
        orchestrator: Arc<agent_studios_workspace::WorkspaceOrchestrator>,
    ) -> Self {
        self.workspace_orchestrator = Some(orchestrator);
        self
    }

    pub fn build(self) -> Result<IpcServer, TransportError> {
        let replay_buffer = ReplayBuffer::new(self.replay_capacity.max(1))?;
        let (broadcast_tx, _) = tokio::sync::broadcast::channel(1024);
        let nonce_tracker = NonceTracker::new(60_000, 10_000);

        let control_plane = Arc::new(tokio::sync::RwLock::new(
            self.control_plane.unwrap_or_else(ControlPlane::new_default),
        ));

        let catalog_arc = Arc::new(
            self.provider_catalog
                .unwrap_or_else(ProviderCatalog::with_builtin_definitions),
        );
        let provider_catalog = Arc::new(tokio::sync::RwLock::new((*catalog_arc).clone()));

        let runtime = self.runtime.or_else(|| {
            agent_studios_opencode_runtime::OpenCodeRuntime::new()
                .map(|rt| rt.with_catalog(catalog_arc.clone()))
                .ok()
                .map(|rt| Arc::new(rt) as Arc<dyn agent_studios_external_runtime::AgentRuntime>)
        });

        let workspace_orchestrator = self.workspace_orchestrator.or_else(|| {
            let base_dir = std::env::temp_dir().join("agent-studios-workspaces");
            let wt_root = base_dir.join("worktrees");
            let art_root = base_dir.join("artifacts");
            agent_studios_workspace::WorkspaceOrchestrator::new(wt_root, art_root)
                .ok()
                .map(Arc::new)
        });

        let state = Arc::new(ServerState {
            auth_signer: Arc::new(HmacSigner),
            nonce_tracker: Arc::new(Mutex::new(nonce_tracker)),
            replay_buffer: Arc::new(Mutex::new(replay_buffer)),
            broadcast_tx,
            active_connections: Arc::new(AtomicUsize::new(0)),
            shutdown_token: CancellationToken::new(),
            start_time: chrono::Utc::now().timestamp(),
            approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_origins: Arc::new(Mutex::new(HashMap::new())),
            auth_keys: Arc::new(Mutex::new(self.auth_keys)),
            key_roles: Arc::new(Mutex::new(self.key_roles)),
            supported_capabilities: self.supported_capabilities,
            control_plane,
            provider_catalog,
            auto_execute: self.auto_execute,
            runtime,
            workspace_orchestrator,
            active_runtime_sessions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            pending_runtime_approvals: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        });

        Ok(IpcServer { state })
    }
}

#[derive(Clone)]
pub struct IpcServer {
    pub state: Arc<ServerState>,
}

impl IpcServer {
    pub fn builder() -> IpcServerBuilder {
        IpcServerBuilder::new()
    }

    pub fn state(&self) -> Arc<ServerState> {
        self.state.clone()
    }

    pub fn control_plane(&self) -> Arc<tokio::sync::RwLock<ControlPlane>> {
        self.state.control_plane.clone()
    }

    pub fn provider_catalog(&self) -> Arc<tokio::sync::RwLock<ProviderCatalog>> {
        self.state.provider_catalog.clone()
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.state.shutdown_token.clone()
    }

    pub fn publish_event(&self, event: IpcEvent) -> Result<(), TransportError> {
        self.state.publish_event(event)
    }

    pub fn handle_connection<S>(
        &self,
        stream: S,
    ) -> tokio::task::JoinHandle<Result<(), TransportError>>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let state = self.state.clone();
        tokio::spawn(async move { crate::connection::handle_connection(stream, state).await })
    }

    #[cfg(windows)]
    pub async fn run_named_pipe(&self, pipe_name: &str) -> Result<(), TransportError> {
        let mut listener = crate::windows::NamedPipeListener::bind(pipe_name)?;
        tracing::info!("IpcServer listening on Windows named pipe: {}", pipe_name);

        loop {
            tokio::select! {
                _ = self.state.shutdown_token.cancelled() => {
                    tracing::info!("Shutdown token cancelled; closing accept loop");
                    break;
                }
                res = listener.accept() => {
                    match res {
                        Ok(client) => {
                            let state = self.state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = crate::connection::handle_connection(client, state).await {
                                    tracing::debug!("Connection terminated: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Named pipe accept error: {:?}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    pub async fn run_uds(&self, socket_path: &std::path::Path) -> Result<(), TransportError> {
        let listener = crate::unix::UnixSocketListener::bind(socket_path)?;
        tracing::info!("IpcServer listening on Unix socket: {:?}", socket_path);

        loop {
            tokio::select! {
                _ = self.state.shutdown_token.cancelled() => {
                    tracing::info!("Shutdown token cancelled; closing accept loop");
                    break;
                }
                res = listener.accept() => {
                    match res {
                        Ok(stream) => {
                            let state = self.state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = crate::connection::handle_connection(stream, state).await {
                                    tracing::debug!("Connection terminated: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Unix socket accept error: {:?}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
