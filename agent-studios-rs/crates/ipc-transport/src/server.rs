use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use agent_studios_ipc_protocol::{
    ApprovalContract, ApprovalId, HmacSigner, IpcEvent, NonceTracker, ReplayBuffer,
};

use crate::error::TransportError;

pub struct ServerState {
    pub auth_signer: Arc<HmacSigner>,
    pub nonce_tracker: Arc<Mutex<NonceTracker>>,
    pub replay_buffer: Arc<Mutex<ReplayBuffer>>,
    pub broadcast_tx: tokio::sync::broadcast::Sender<IpcEvent>,
    pub active_connections: Arc<AtomicUsize>,
    pub shutdown_token: CancellationToken,
    pub start_time: i64,
    pub approvals: Arc<Mutex<HashMap<ApprovalId, ApprovalContract>>>,
    pub auth_keys: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    pub supported_capabilities: Vec<String>,
}

impl ServerState {
    pub fn publish_event(&self, event: IpcEvent) -> Result<(), TransportError> {
        self.replay_buffer.lock().unwrap().push(event.clone())?;
        let _ = self.broadcast_tx.send(event);
        Ok(())
    }
}

pub struct IpcServerBuilder {
    auth_keys: HashMap<String, Vec<u8>>,
    replay_capacity: usize,
    supported_capabilities: Vec<String>,
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
                "admin".to_string(),
                "shutdown".to_string(),
            ],
        }
    }

    pub fn with_auth_key(mut self, id: impl Into<String>, key: impl Into<Vec<u8>>) -> Self {
        self.auth_keys.insert(id.into(), key.into());
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

    pub fn build(self) -> Result<IpcServer, TransportError> {
        let replay_buffer = ReplayBuffer::new(self.replay_capacity.max(1))?;
        let (broadcast_tx, _) = tokio::sync::broadcast::channel(1024);
        let nonce_tracker = NonceTracker::new(60_000, 10_000);

        let state = Arc::new(ServerState {
            auth_signer: Arc::new(HmacSigner),
            nonce_tracker: Arc::new(Mutex::new(nonce_tracker)),
            replay_buffer: Arc::new(Mutex::new(replay_buffer)),
            broadcast_tx,
            active_connections: Arc::new(AtomicUsize::new(0)),
            shutdown_token: CancellationToken::new(),
            start_time: chrono::Utc::now().timestamp(),
            approvals: Arc::new(Mutex::new(HashMap::new())),
            auth_keys: Arc::new(Mutex::new(self.auth_keys)),
            supported_capabilities: self.supported_capabilities,
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
        use tokio::net::windows::named_pipe::ServerOptions;

        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(pipe_name)?;

        tracing::info!("IpcServer listening on Windows named pipe: {}", pipe_name);

        loop {
            tokio::select! {
                _ = self.state.shutdown_token.cancelled() => {
                    tracing::info!("Shutdown token cancelled; closing accept loop");
                    break;
                }
                res = server.connect() => {
                    match res {
                        Ok(()) => {
                            let client = server;
                            server = ServerOptions::new()
                                .first_pipe_instance(false)
                                .create(pipe_name)?;

                            let state = self.state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = crate::connection::handle_connection(client, state).await {
                                    tracing::debug!("Connection terminated: {:?}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Named pipe accept error: {:?}", e);
                            server = ServerOptions::new()
                                .first_pipe_instance(false)
                                .create(pipe_name)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    pub async fn run_uds(&self, socket_path: &std::path::Path) -> Result<(), TransportError> {
        use tokio::net::UnixListener;

        let listener = UnixListener::bind(socket_path)?;
        tracing::info!("IpcServer listening on Unix socket: {:?}", socket_path);

        loop {
            tokio::select! {
                _ = self.state.shutdown_token.cancelled() => {
                    tracing::info!("Shutdown token cancelled; closing accept loop");
                    break;
                }
                res = listener.accept() => {
                    match res {
                        Ok((stream, _)) => {
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
