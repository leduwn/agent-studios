#[allow(unused_imports)]
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::Mutex;
use tracing::debug;

use agent_studios_ipc_protocol::{
    AuthBinding, HmacSigner, IpcEvent, ProtocolError, ProtocolVersion, SessionId,
};

use crate::connection::{IpcCommand, IpcResponse};
use crate::error::TransportError;
use crate::framing::{DEFAULT_MAX_FRAME_SIZE, recv_msg, send_msg};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PongResponse {
    pub payload: String,
    pub client_timestamp: i64,
    pub server_timestamp: i64,
    pub roundtrip_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub active_clients: usize,
    pub uptime_secs: u64,
    pub memory_events_count: usize,
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub client_id: String,
    pub key_id: String,
    pub secret: Vec<u8>,
    pub capabilities: Vec<String>,
    pub protocol_version: ProtocolVersion,
    pub max_frame_size: usize,
}

impl ClientConfig {
    pub fn new(key_id: impl Into<String>, secret: impl Into<Vec<u8>>) -> Self {
        Self {
            client_id: format!("client_{}", uuid::Uuid::new_v4().simple()),
            key_id: key_id.into(),
            secret: secret.into(),
            capabilities: vec!["admin".to_string(), "shutdown".to_string()],
            protocol_version: ProtocolVersion::CURRENT,
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_client_id(mut self, client_id: impl Into<String>) -> Self {
        self.client_id = client_id.into();
        self
    }

    pub fn with_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.protocol_version = protocol_version;
        self
    }

    pub fn with_max_frame_size(mut self, max_frame_size: usize) -> Self {
        self.max_frame_size = max_frame_size;
        self
    }
}

#[derive(Debug, Clone)]
pub enum Endpoint {
    #[cfg(windows)]
    NamedPipe(String),
    #[cfg(unix)]
    UnixSocket(PathBuf),
    Duplex,
}

struct ClientInner {
    reader: Box<dyn AsyncRead + Unpin + Send>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
    session_id: Option<SessionId>,
    negotiated_capabilities: Vec<String>,
    was_subscribed: bool,
}

pub struct IpcClient {
    inner: Arc<Mutex<ClientInner>>,
    config: ClientConfig,
    endpoint: Option<Endpoint>,
    last_received_sequence: Arc<AtomicU64>,
}

pub struct EventSubscriptionStream {
    inner: Arc<Mutex<ClientInner>>,
    last_received_sequence: Arc<AtomicU64>,
    max_frame_size: usize,
}

impl EventSubscriptionStream {
    /// Receives next `IpcEvent` from server stream.
    /// Updates sequence watermark automatically.
    pub async fn next_event(&mut self) -> Result<IpcEvent, TransportError> {
        let mut inner = self.inner.lock().await;
        let resp: IpcResponse = recv_msg(&mut inner.reader, self.max_frame_size).await?;
        match resp {
            IpcResponse::Event { event } => {
                self.last_received_sequence
                    .store(event.sequence, Ordering::SeqCst);
                Ok(event)
            }
            IpcResponse::Error { message } => Err(TransportError::Protocol(
                ProtocolError::InvalidState(message),
            )),
            other => Err(TransportError::InvalidState {
                expected: "IpcResponse::Event".into(),
                actual: format!("{:?}", other),
            }),
        }
    }

    /// Receives next `IpcEvent` as an Option, returning `None` if the stream closed cleanly.
    pub async fn recv(&mut self) -> Result<Option<IpcEvent>, TransportError> {
        match self.next_event().await {
            Ok(event) => Ok(Some(event)),
            Err(TransportError::ConnectionClosed) => Ok(None),
            Err(TransportError::Io(ref e))
                if e.kind() == std::io::ErrorKind::ConnectionReset
                    || e.kind() == std::io::ErrorKind::UnexpectedEof
                    || e.kind() == std::io::ErrorKind::BrokenPipe =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Current sequence watermark recorded by this stream.
    pub fn last_sequence(&self) -> Option<u64> {
        let seq = self.last_received_sequence.load(Ordering::SeqCst);
        if seq == 0 { None } else { Some(seq) }
    }
}

impl IpcClient {
    /// Connects to server over an arbitrary in-memory duplex stream and runs handshake.
    pub async fn connect_stream<S>(stream: S, config: ClientConfig) -> Result<Self, TransportError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let mut client = Self::from_stream_with_endpoint(stream, config, Some(Endpoint::Duplex));
        client.handshake().await?;
        Ok(client)
    }

    /// Creates client without performing handshake (used for testing rejection of unauthenticated commands).
    pub fn connect_stream_unauthenticated<S>(stream: S, config: ClientConfig) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Self::from_stream_with_endpoint(stream, config, Some(Endpoint::Duplex))
    }

    /// Connects to a Windows Named Pipe and runs handshake.
    #[cfg(windows)]
    pub async fn connect_named_pipe(
        pipe_name: impl AsRef<str>,
        config: ClientConfig,
    ) -> Result<Self, TransportError> {
        let pipe_name = pipe_name.as_ref();
        let stream = crate::windows::NamedPipeStream::connect(pipe_name).await?;
        let mut client = Self::from_stream_with_endpoint(
            stream,
            config,
            Some(Endpoint::NamedPipe(pipe_name.to_string())),
        );
        client.handshake().await?;
        Ok(client)
    }

    /// Connects to a Unix Domain Socket and runs handshake.
    #[cfg(unix)]
    pub async fn connect_unix(
        socket_path: impl AsRef<Path>,
        config: ClientConfig,
    ) -> Result<Self, TransportError> {
        let socket_path = socket_path.as_ref().to_path_buf();
        let stream = crate::unix::UnixSocketStream::connect(&socket_path).await?;
        let mut client = Self::from_stream_with_endpoint(
            stream,
            config,
            Some(Endpoint::UnixSocket(socket_path)),
        );
        client.handshake().await?;
        Ok(client)
    }

    fn from_stream_with_endpoint<S>(
        stream: S,
        config: ClientConfig,
        endpoint: Option<Endpoint>,
    ) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        let inner = ClientInner {
            reader: Box::new(reader),
            writer: Box::new(writer),
            session_id: None,
            negotiated_capabilities: Vec::new(),
            was_subscribed: false,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            config,
            endpoint,
            last_received_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Performs handshake:
    /// 1. Sends `NegotiateCapabilities`.
    /// 2. Generates HMAC signature using `HmacSigner` and sends `Authenticate`.
    pub async fn handshake(&mut self) -> Result<(), TransportError> {
        let mut inner = self.inner.lock().await;

        // 1. NegotiateCapabilities
        let neg_cmd = IpcCommand::NegotiateCapabilities {
            protocol_version: self.config.protocol_version,
            capabilities: self.config.capabilities.clone(),
        };
        send_msg(&mut inner.writer, &neg_cmd).await?;

        let neg_resp: IpcResponse = recv_msg(&mut inner.reader, self.config.max_frame_size).await?;
        let (selected_version, agreed_capabilities) = match neg_resp {
            IpcResponse::CapabilitiesNegotiated {
                selected_version,
                agreed_capabilities,
            } => (selected_version, agreed_capabilities),
            IpcResponse::Error { message } => {
                return Err(TransportError::AuthenticationFailed(message));
            }
            other => {
                return Err(TransportError::InvalidState {
                    expected: "CapabilitiesNegotiated".into(),
                    actual: format!("{:?}", other),
                });
            }
        };

        inner.negotiated_capabilities = agreed_capabilities.clone();

        // 2. Generate HMAC signature and send Authenticate
        let session_id = SessionId::generate();
        let nonce = format!("nonce_{}", uuid::Uuid::new_v4().simple());
        let timestamp_millis = chrono::Utc::now().timestamp_millis();
        let payload_bytes = b"";
        let caps_slice = if agreed_capabilities.is_empty() {
            None
        } else {
            Some(agreed_capabilities)
        };

        let binding = AuthBinding::new(
            &self.config.key_id,
            session_id.clone(),
            &nonce,
            timestamp_millis,
            payload_bytes,
            selected_version,
            caps_slice,
        );

        let header = HmacSigner::sign(&self.config.secret, &binding)
            .map_err(|e| TransportError::AuthenticationFailed(e.to_string()))?;

        let auth_cmd = IpcCommand::Authenticate {
            header,
            payload: None,
        };
        send_msg(&mut inner.writer, &auth_cmd).await?;

        let auth_resp: IpcResponse =
            recv_msg(&mut inner.reader, self.config.max_frame_size).await?;
        match auth_resp {
            IpcResponse::Authenticated { session_id } => {
                inner.session_id = Some(session_id);
                debug!("Client authenticated successfully");
                Ok(())
            }
            IpcResponse::Error { message } => Err(TransportError::AuthenticationFailed(message)),
            other => Err(TransportError::InvalidState {
                expected: "Authenticated".into(),
                actual: format!("{:?}", other),
            }),
        }
    }

    /// Sends Ping, verifies Pong, measures roundtrip latency.
    pub async fn ping(&mut self, payload: &str) -> Result<PongResponse, TransportError> {
        let mut inner = self.inner.lock().await;
        let client_timestamp = chrono::Utc::now().timestamp_millis();

        let ping_cmd = IpcCommand::Ping {
            payload: payload.to_string(),
            timestamp: client_timestamp,
        };
        send_msg(&mut inner.writer, &ping_cmd).await?;

        let resp: IpcResponse = recv_msg(&mut inner.reader, self.config.max_frame_size).await?;
        match resp {
            IpcResponse::Pong {
                payload: pong_payload,
                client_timestamp: c_ts,
                server_timestamp: s_ts,
            } => {
                if pong_payload != payload {
                    return Err(TransportError::Protocol(ProtocolError::InvalidState(
                        format!(
                            "Pong payload mismatch: expected '{}', received '{}'",
                            payload, pong_payload
                        ),
                    )));
                }
                let now = chrono::Utc::now().timestamp_millis();
                let roundtrip_latency_ms = (now - client_timestamp).max(0) as u64;

                Ok(PongResponse {
                    payload: pong_payload,
                    client_timestamp: c_ts,
                    server_timestamp: s_ts,
                    roundtrip_latency_ms,
                })
            }
            IpcResponse::Error { message } => Err(TransportError::AuthenticationFailed(message)),
            other => Err(TransportError::InvalidState {
                expected: "Pong".into(),
                actual: format!("{:?}", other),
            }),
        }
    }

    /// Sends `GetHealth` command and returns `HealthResponse`.
    pub async fn get_health(&mut self) -> Result<HealthResponse, TransportError> {
        let mut inner = self.inner.lock().await;
        send_msg(&mut inner.writer, &IpcCommand::GetHealth).await?;

        let resp: IpcResponse = recv_msg(&mut inner.reader, self.config.max_frame_size).await?;
        match resp {
            IpcResponse::Health {
                status,
                active_clients,
                uptime_secs,
                memory_events_count,
            } => Ok(HealthResponse {
                status,
                active_clients,
                uptime_secs,
                memory_events_count,
            }),
            IpcResponse::Error { message } => Err(TransportError::AuthenticationFailed(message)),
            other => Err(TransportError::InvalidState {
                expected: "Health".into(),
                actual: format!("{:?}", other),
            }),
        }
    }

    /// Subscribes to events starting from `from_sequence`.
    /// Returns an `EventSubscriptionStream`.
    pub async fn subscribe_events(
        &mut self,
        from_sequence: u64,
    ) -> Result<EventSubscriptionStream, TransportError> {
        let mut inner = self.inner.lock().await;
        send_msg(
            &mut inner.writer,
            &IpcCommand::SubscribeEvents {
                from_sequence: Some(from_sequence),
            },
        )
        .await?;
        inner.was_subscribed = true;

        Ok(EventSubscriptionStream {
            inner: self.inner.clone(),
            last_received_sequence: self.last_received_sequence.clone(),
            max_frame_size: self.config.max_frame_size,
        })
    }

    /// Sends `Shutdown` command to server.
    pub async fn shutdown(&mut self) -> Result<(), TransportError> {
        let mut inner = self.inner.lock().await;
        send_msg(&mut inner.writer, &IpcCommand::Shutdown).await?;

        let resp: IpcResponse = recv_msg(&mut inner.reader, self.config.max_frame_size).await?;
        match resp {
            IpcResponse::ShutdownAcknowledged => Ok(()),
            IpcResponse::Error { message } => Err(TransportError::PermissionDenied(message)),
            other => Err(TransportError::InvalidState {
                expected: "ShutdownAcknowledged".into(),
                actual: format!("{:?}", other),
            }),
        }
    }

    /// Closes connection from client side.
    pub async fn disconnect(&mut self) {
        let mut inner = self.inner.lock().await;
        let _ = inner.writer.shutdown().await;
    }

    /// Tracks last received event sequence watermark.
    pub fn last_received_sequence(&self) -> Option<u64> {
        let seq = self.last_received_sequence.load(Ordering::SeqCst);
        if seq == 0 { None } else { Some(seq) }
    }

    /// Manually sets or resets the sequence watermark.
    pub fn set_last_received_sequence(&mut self, seq: u64) {
        self.last_received_sequence.store(seq, Ordering::SeqCst);
    }

    /// Active session ID returned by the server on authentication.
    pub async fn session_id(&self) -> Option<SessionId> {
        self.inner.lock().await.session_id.clone()
    }

    /// Agreed capabilities from handshake.
    pub async fn negotiated_capabilities(&self) -> Vec<String> {
        self.inner.lock().await.negotiated_capabilities.clone()
    }

    /// Reconnects to server endpoint (Named Pipe or Unix Socket).
    /// Recovers watermark automatically: if client was subscribed, requests `from_sequence = last_received_sequence + 1`.
    pub async fn reconnect(&mut self) -> Result<(), TransportError> {
        let endpoint = self
            .endpoint
            .clone()
            .ok_or_else(|| TransportError::InvalidState {
                expected: "valid endpoint for reconnect".into(),
                actual: "none".into(),
            })?;

        match endpoint {
            #[cfg(windows)]
            Endpoint::NamedPipe(ref name) => {
                let stream = crate::windows::NamedPipeStream::connect(name).await?;
                self.reconnect_stream(stream).await
            }
            #[cfg(unix)]
            Endpoint::UnixSocket(ref path) => {
                let stream = crate::unix::UnixSocketStream::connect(path).await?;
                self.reconnect_stream(stream).await
            }
            Endpoint::Duplex => Err(TransportError::InvalidState {
                expected: "OS socket endpoint for reconnect()".into(),
                actual: "in-memory DuplexStream: use reconnect_stream() instead".into(),
            }),
        }
    }

    /// Reconnects using an explicit newly provided stream (e.g. for duplex streams).
    /// Re-runs handshake and if previously subscribed, automatically requests `from_sequence = last_received_sequence + 1`.
    pub async fn reconnect_stream<S>(&mut self, stream: S) -> Result<(), TransportError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (reader, writer) = tokio::io::split(stream);
        let was_sub = {
            let mut inner = self.inner.lock().await;
            inner.reader = Box::new(reader);
            inner.writer = Box::new(writer);
            inner.session_id = None;
            inner.was_subscribed
        };

        self.handshake().await?;

        if was_sub {
            let next_seq = self.last_received_sequence.load(Ordering::SeqCst) + 1;
            let mut inner = self.inner.lock().await;
            send_msg(
                &mut inner.writer,
                &IpcCommand::SubscribeEvents {
                    from_sequence: Some(next_seq),
                },
            )
            .await?;
        }

        Ok(())
    }
}
