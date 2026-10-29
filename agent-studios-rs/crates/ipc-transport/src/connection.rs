use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use agent_studios_ipc_protocol::{
    ApprovalContract, ApprovalDecision, ApprovalId, ApprovalStatus, HmacAuthHeader, HmacSigner,
    IpcEvent, ProtocolError, ProtocolVersion, SessionId,
};
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::id::{RunId, StudioId, TaskId};
use agent_studios_protocol::run::RunRecord;
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord};
use agent_studios_provider::{ModelDescriptor, ProviderInstance, ProviderInstanceId};

use crate::error::TransportError;
use crate::framing::{DEFAULT_MAX_FRAME_SIZE, recv_msg, send_msg};
use crate::server::ServerState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Connected,
    Authenticating,
    Ready,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct ConnectionSession {
    pub session_id: SessionId,
    pub key_id: Option<String>,
    pub protocol_version: ProtocolVersion,
    pub negotiated_capabilities: Vec<String>,
    pub state: ConnectionState,
    pub connected_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum IpcCommand {
    NegotiateCapabilities {
        #[serde(alias = "version")]
        protocol_version: ProtocolVersion,
        capabilities: Vec<String>,
    },
    Authenticate {
        header: HmacAuthHeader,
        #[serde(default)]
        payload: Option<Vec<u8>>,
    },
    Ping {
        payload: String,
        timestamp: i64,
    },
    GetHealth,
    SubscribeEvents {
        #[serde(default)]
        from_sequence: Option<u64>,
    },
    RequestApproval {
        contract: ApprovalContract,
    },
    ResolveApproval {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    CreateStudio {
        name: String,
    },
    ListStudios,
    GetStudio {
        studio_id: StudioId,
    },
    SubmitTask {
        studio_id: StudioId,
        task_spec: BatchTaskSpec,
    },
    CancelTask {
        studio_id: StudioId,
        task_id: TaskId,
        #[serde(default)]
        reason: Option<String>,
    },
    ListTasks {
        studio_id: StudioId,
    },
    GetTask {
        studio_id: StudioId,
        task_id: TaskId,
    },
    ListRuns {
        studio_id: StudioId,
    },
    GetRun {
        studio_id: StudioId,
        run_id: RunId,
    },
    ListProviders,
    GetProvider {
        provider_id: ProviderInstanceId,
    },
    ListModels {
        #[serde(default)]
        instance_id: Option<ProviderInstanceId>,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum IpcResponse {
    CapabilitiesNegotiated {
        selected_version: ProtocolVersion,
        agreed_capabilities: Vec<String>,
    },
    Authenticated {
        session_id: SessionId,
    },
    Pong {
        payload: String,
        client_timestamp: i64,
        server_timestamp: i64,
    },
    Health {
        status: String,
        active_clients: usize,
        uptime_secs: u64,
        memory_events_count: usize,
    },
    Event {
        event: IpcEvent,
    },
    ApprovalRequested {
        approval_id: ApprovalId,
    },
    ApprovalResolved {
        approval_id: ApprovalId,
        status: ApprovalStatus,
    },
    StudioCreated {
        studio: Studio,
    },
    Studios {
        studios: Vec<Studio>,
    },
    Studio {
        studio: Option<Studio>,
    },
    TasksSubmitted {
        task_ids: Vec<TaskId>,
    },
    TaskCancelled {
        #[serde(default)]
        summary: CancellationSummary,
    },
    Tasks {
        tasks: Vec<TaskRecord>,
    },
    Task {
        task: Option<TaskRecord>,
    },
    Runs {
        runs: Vec<RunRecord>,
    },
    Run {
        run: Option<RunRecord>,
    },
    Providers {
        providers: Vec<ProviderInstance>,
    },
    Provider {
        provider: Option<ProviderInstance>,
    },
    Models {
        models: Vec<ModelDescriptor>,
    },
    ShutdownAcknowledged,
    Error {
        message: String,
    },
}

struct ConnectionGuard(Arc<AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Handles a single incoming IPC connection through handshake and command dispatch loop.
pub async fn handle_connection<S>(
    stream: S,
    server_state: Arc<ServerState>,
) -> Result<(), TransportError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    server_state
        .active_connections
        .fetch_add(1, Ordering::SeqCst);
    let _conn_guard = ConnectionGuard(server_state.active_connections.clone());

    let (mut reader, mut writer) = tokio::io::split(stream);

    let (tx, mut rx) = mpsc::channel::<IpcResponse>(256);

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = send_msg(&mut writer, &msg).await {
                debug!("Failed to send frame to client: {:?}", e);
                break;
            }
        }
    });

    let mut session = ConnectionSession {
        session_id: SessionId::generate(),
        key_id: None,
        protocol_version: ProtocolVersion::CURRENT,
        negotiated_capabilities: Vec::new(),
        state: ConnectionState::Connected,
        connected_at: chrono::Utc::now().timestamp(),
    };

    let mut subscription_handles = Vec::new();

    let loop_result = run_command_loop(
        &mut reader,
        &tx,
        &mut session,
        &server_state,
        &mut subscription_handles,
    )
    .await;

    session.state = ConnectionState::Disconnected;

    for handle in subscription_handles {
        handle.abort();
    }
    drop(tx);
    let _ = writer_task.await;

    loop_result
}

async fn run_command_loop<R: AsyncRead + Unpin>(
    reader: &mut R,
    tx: &mpsc::Sender<IpcResponse>,
    session: &mut ConnectionSession,
    server_state: &Arc<ServerState>,
    subscription_handles: &mut Vec<tokio::task::JoinHandle<()>>,
) -> Result<(), TransportError> {
    loop {
        let cmd_res: Result<IpcCommand, TransportError> = tokio::select! {
            _ = server_state.shutdown_token.cancelled() => {
                debug!("Server shutdown signal received; terminating connection");
                break;
            }
            res = recv_msg(reader, DEFAULT_MAX_FRAME_SIZE) => res,
        };

        let cmd = match cmd_res {
            Ok(cmd) => cmd,
            Err(TransportError::ConnectionClosed) => {
                debug!("Client closed connection cleanly");
                break;
            }
            Err(TransportError::Io(ref io_err))
                if io_err.kind() == std::io::ErrorKind::ConnectionReset
                    || io_err.kind() == std::io::ErrorKind::UnexpectedEof
                    || io_err.kind() == std::io::ErrorKind::BrokenPipe =>
            {
                debug!("Client connection reset or broken");
                break;
            }
            Err(e) => {
                debug!("Transport error reading command: {:?}", e);
                return Err(e);
            }
        };

        if session.state != ConnectionState::Ready {
            match cmd {
                IpcCommand::NegotiateCapabilities {
                    protocol_version,
                    capabilities,
                } => {
                    if protocol_version.major != ProtocolVersion::CURRENT.major {
                        let err_msg = format!(
                            "Incompatible protocol version: client={}, server={}",
                            protocol_version,
                            ProtocolVersion::CURRENT
                        );
                        let _ = tx.send(IpcResponse::Error { message: err_msg }).await;
                        return Err(TransportError::Protocol(ProtocolError::VersionMismatch {
                            expected: ProtocolVersion::CURRENT.to_string(),
                            received: protocol_version.to_string(),
                        }));
                    }

                    let agreed: Vec<String> = capabilities
                        .into_iter()
                        .filter(|c| {
                            server_state
                                .supported_capabilities
                                .iter()
                                .any(|sc| sc.eq_ignore_ascii_case(c))
                        })
                        .collect();

                    session.protocol_version = protocol_version;
                    session.negotiated_capabilities = agreed.clone();
                    session.state = ConnectionState::Authenticating;

                    tx.send(IpcResponse::CapabilitiesNegotiated {
                        selected_version: ProtocolVersion::CURRENT,
                        agreed_capabilities: agreed,
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::Authenticate { header, payload } => {
                    let secret = {
                        let keys = server_state.auth_keys.lock().unwrap();
                        keys.get(&header.key_id).cloned()
                    };

                    let secret = match secret {
                        Some(s) => s,
                        None => {
                            let msg = format!("Unknown auth key_id: {}", header.key_id);
                            let _ = tx
                                .send(IpcResponse::Error {
                                    message: msg.clone(),
                                })
                                .await;
                            return Err(TransportError::AuthenticationFailed(msg));
                        }
                    };

                    let payload_bytes = payload.as_deref().unwrap_or(b"");
                    let caps_slice = if session.negotiated_capabilities.is_empty() {
                        None
                    } else {
                        Some(&session.negotiated_capabilities[..])
                    };

                    let verify_res = {
                        let tracker = server_state.nonce_tracker.lock().unwrap();
                        HmacSigner::verify(
                            &secret,
                            &header,
                            payload_bytes,
                            caps_slice,
                            60_000,
                            &tracker,
                        )
                    };

                    match verify_res {
                        Ok(()) => {
                            session.key_id = Some(header.key_id.clone());
                            session.session_id = header.session_id.clone();
                            session.state = ConnectionState::Ready;

                            tx.send(IpcResponse::Authenticated {
                                session_id: session.session_id.clone(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                        Err(e) => {
                            let msg = format!("Authentication failed: {}", e);
                            let _ = tx
                                .send(IpcResponse::Error {
                                    message: msg.clone(),
                                })
                                .await;
                            return Err(TransportError::AuthenticationFailed(msg));
                        }
                    }
                }
                _ => {
                    let msg = "Unauthenticated command: must authenticate first".to_string();
                    let _ = tx
                        .send(IpcResponse::Error {
                            message: msg.clone(),
                        })
                        .await;
                    return Err(TransportError::AuthenticationFailed(msg));
                }
            }
        } else {
            // State is Ready
            match cmd {
                IpcCommand::Ping { payload, timestamp } => {
                    let server_timestamp = chrono::Utc::now().timestamp_millis();
                    tx.send(IpcResponse::Pong {
                        payload,
                        client_timestamp: timestamp,
                        server_timestamp,
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::GetHealth => {
                    let active_clients = server_state.active_connections.load(Ordering::SeqCst);
                    let uptime_secs =
                        (chrono::Utc::now().timestamp() - server_state.start_time).max(0) as u64;
                    let memory_events_count = server_state.replay_buffer.lock().unwrap().len();

                    tx.send(IpcResponse::Health {
                        status: "healthy".to_string(),
                        active_clients,
                        uptime_secs,
                        memory_events_count,
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::SubscribeEvents { from_sequence } => {
                    let mut broadcast_rx = server_state.broadcast_tx.subscribe();
                    let (backfill_result, max_buffered_seq) = {
                        let buffer = server_state.replay_buffer.lock().unwrap();
                        let max_seq = buffer.latest_sequence().unwrap_or(0);
                        let events = match from_sequence {
                            Some(seq) => buffer.replay_from(seq, None),
                            None => Ok(Vec::new()),
                        };
                        (events, max_seq)
                    };

                    match backfill_result {
                        Ok(events) => {
                            for ev in events {
                                tx.send(IpcResponse::Event { event: ev })
                                    .await
                                    .map_err(|_| TransportError::ConnectionClosed)?;
                            }

                            let tx_stream = tx.clone();
                            let shutdown_tok = server_state.shutdown_token.clone();
                            let handle = tokio::spawn(async move {
                                loop {
                                    tokio::select! {
                                        _ = shutdown_tok.cancelled() => break,
                                        res = broadcast_rx.recv() => {
                                            match res {
                                                Ok(event) => {
                                                    if event.sequence <= max_buffered_seq {
                                                        continue;
                                                    }
                                                    if tx_stream.send(IpcResponse::Event { event }).await.is_err() {
                                                        break;
                                                    }
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                                    warn!("Broadcast subscription lagged by {} events", skipped);
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                            subscription_handles.push(handle);
                        }
                        Err(e) => {
                            tx.send(IpcResponse::Error {
                                message: format!("Subscription failed: {}", e),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                    }
                }
                IpcCommand::RequestApproval { contract } => {
                    let approval_id = contract.approval_id.clone();
                    server_state
                        .approvals
                        .lock()
                        .unwrap()
                        .insert(approval_id.clone(), contract);

                    tx.send(IpcResponse::ApprovalRequested { approval_id })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::ResolveApproval {
                    approval_id,
                    decision,
                } => {
                    let parsed_cp_id: Option<agent_studios_protocol::id::ApprovalId> =
                        std::str::FromStr::from_str(approval_id.as_str()).ok();

                    let is_in_cp = if let Some(aid) = parsed_cp_id {
                        let cp = server_state.control_plane.read().await;
                        cp.get_approval(aid).is_some()
                    } else {
                        false
                    };

                    if is_in_cp {
                        let aid = parsed_cp_id.unwrap();
                        let (res, envelopes) = {
                            let mut cp = server_state.control_plane.write().await;
                            let r = match decision {
                                ApprovalDecision::Approved { .. } => cp.grant_approval(aid),
                                ApprovalDecision::Rejected { .. } => cp.reject_approval(aid),
                            };
                            let envs = if r.is_ok() {
                                cp.drain_committed_events()
                            } else {
                                Vec::new()
                            };
                            (r, envs)
                        };

                        match res {
                            Ok(()) => {
                                let _ = server_state.publish_control_plane_events(envelopes);
                                let status = match decision {
                                    ApprovalDecision::Approved { .. } => ApprovalStatus::Authorized,
                                    ApprovalDecision::Rejected { .. } => ApprovalStatus::Rejected,
                                };
                                tx.send(IpcResponse::ApprovalResolved {
                                    approval_id,
                                    status,
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                            }
                            Err(e) => {
                                tx.send(IpcResponse::Error {
                                    message: e.to_string(),
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                            }
                        }
                    } else {
                        let resolution_result = {
                            let mut approvals = server_state.approvals.lock().unwrap();
                            if let Some(contract) = approvals.get_mut(&approval_id) {
                                match contract.resolve(decision) {
                                    Ok(()) => Ok(contract.status),
                                    Err(e) => Err(e.to_string()),
                                }
                            } else {
                                Err(format!(
                                    "Approval {} not found in control plane or transport contracts",
                                    approval_id
                                ))
                            }
                        };

                        match resolution_result {
                            Ok(status) => {
                                let next_seq = {
                                    let buf = server_state.replay_buffer.lock().unwrap();
                                    buf.latest_sequence().unwrap_or(0) + 1
                                };
                                let event = IpcEvent::new(
                                    next_seq,
                                    "approval_resolved",
                                    serde_json::json!({
                                        "approval_id": approval_id,
                                        "status": status,
                                    }),
                                );
                                let _ = server_state.publish_event(event);

                                tx.send(IpcResponse::ApprovalResolved {
                                    approval_id,
                                    status,
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                            }
                            Err(err_msg) => {
                                tx.send(IpcResponse::Error { message: err_msg })
                                    .await
                                    .map_err(|_| TransportError::ConnectionClosed)?;
                            }
                        }
                    }
                }
                IpcCommand::CreateStudio { name } => {
                    let (res, envelopes) = {
                        let mut cp = server_state.control_plane.write().await;
                        let r = cp.create_studio(name);
                        let envs = if r.is_ok() {
                            cp.drain_committed_events()
                        } else {
                            Vec::new()
                        };
                        (r, envs)
                    };

                    match res {
                        Ok(studio) => {
                            let _ = server_state.publish_control_plane_events(envelopes);
                            tx.send(IpcResponse::StudioCreated { studio })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                        Err(e) => {
                            tx.send(IpcResponse::Error {
                                message: e.to_string(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                    }
                }
                IpcCommand::ListStudios => {
                    let studios = {
                        let cp = server_state.control_plane.read().await;
                        cp.all_studios().cloned().collect()
                    };
                    tx.send(IpcResponse::Studios { studios })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::GetStudio { studio_id } => {
                    let studio = {
                        let cp = server_state.control_plane.read().await;
                        cp.get_studio(studio_id).cloned()
                    };
                    tx.send(IpcResponse::Studio { studio })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::SubmitTask {
                    studio_id,
                    task_spec,
                } => {
                    let (res, envelopes) = {
                        let mut cp = server_state.control_plane.write().await;
                        let r = cp.create_task_batch(studio_id, vec![task_spec]);
                        let envs = if r.is_ok() {
                            cp.drain_committed_events()
                        } else {
                            Vec::new()
                        };
                        (r, envs)
                    };

                    match res {
                        Ok(tasks) => {
                            let task_ids = tasks.into_iter().map(|t| t.id).collect();
                            let _ = server_state.publish_control_plane_events(envelopes);
                            tx.send(IpcResponse::TasksSubmitted { task_ids })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                        Err(e) => {
                            tx.send(IpcResponse::Error {
                                message: e.to_string(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                    }
                }
                IpcCommand::CancelTask {
                    studio_id,
                    task_id,
                    reason,
                } => {
                    let (res, envelopes) = {
                        let mut cp = server_state.control_plane.write().await;
                        if let Some(task) = cp.get_task(task_id) {
                            if task.studio_id != studio_id {
                                (
                                    Err(format!(
                                        "Studio mismatch: task {} belongs to studio {}, not {}",
                                        task_id, task.studio_id, studio_id
                                    )),
                                    Vec::new(),
                                )
                            } else {
                                let scope = CancellationScope::Task {
                                    task_id,
                                    include_descendants: true,
                                };
                                match cp.request_cancellation(scope, reason) {
                                    Ok(summary) => {
                                        let envs = cp.drain_committed_events();
                                        (Ok(summary), envs)
                                    }
                                    Err(e) => (Err(e.to_string()), Vec::new()),
                                }
                            }
                        } else {
                            (Err(format!("Task {} not found", task_id)), Vec::new())
                        }
                    };

                    match res {
                        Ok(summary) => {
                            let _ = server_state.publish_control_plane_events(envelopes);
                            tx.send(IpcResponse::TaskCancelled { summary })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                        Err(err_msg) => {
                            tx.send(IpcResponse::Error { message: err_msg })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                        }
                    }
                }
                IpcCommand::ListTasks { studio_id } => {
                    let tasks = {
                        let cp = server_state.control_plane.read().await;
                        cp.all_tasks()
                            .filter(|t| t.studio_id == studio_id)
                            .cloned()
                            .collect()
                    };
                    tx.send(IpcResponse::Tasks { tasks })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::GetTask { studio_id, task_id } => {
                    let task = {
                        let cp = server_state.control_plane.read().await;
                        cp.get_task(task_id)
                            .filter(|t| t.studio_id == studio_id)
                            .cloned()
                    };
                    tx.send(IpcResponse::Task { task })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::ListRuns { studio_id } => {
                    let runs = {
                        let cp = server_state.control_plane.read().await;
                        cp.all_runs()
                            .filter(|r| {
                                cp.get_task(r.task_id).map(|t| t.studio_id) == Some(studio_id)
                            })
                            .cloned()
                            .collect()
                    };
                    tx.send(IpcResponse::Runs { runs })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::GetRun { studio_id, run_id } => {
                    let run = {
                        let cp = server_state.control_plane.read().await;
                        cp.get_run(run_id)
                            .filter(|r| {
                                cp.get_task(r.task_id).map(|t| t.studio_id) == Some(studio_id)
                            })
                            .cloned()
                    };
                    tx.send(IpcResponse::Run { run })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::ListProviders => {
                    let providers = {
                        let catalog = server_state.provider_catalog.read().await;
                        catalog
                            .list_provider_instances()
                            .into_iter()
                            .cloned()
                            .collect()
                    };
                    tx.send(IpcResponse::Providers { providers })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::GetProvider { provider_id } => {
                    let provider = {
                        let catalog = server_state.provider_catalog.read().await;
                        catalog.get_provider_instance(&provider_id).cloned()
                    };
                    tx.send(IpcResponse::Provider { provider })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::ListModels { instance_id } => {
                    let models = {
                        let catalog = server_state.provider_catalog.read().await;
                        match instance_id {
                            Some(id) => catalog
                                .models_for_provider_instance(&id)
                                .into_iter()
                                .cloned()
                                .collect(),
                            None => catalog.list_models().into_iter().cloned().collect(),
                        }
                    };
                    tx.send(IpcResponse::Models { models })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::Shutdown => {
                    let has_privilege = session.negotiated_capabilities.iter().any(|c| {
                        c.eq_ignore_ascii_case("admin") || c.eq_ignore_ascii_case("shutdown")
                    });

                    if has_privilege {
                        info!("Shutdown command authorized; triggering server shutdown");
                        server_state.shutdown_token.cancel();
                        tx.send(IpcResponse::ShutdownAcknowledged)
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        break;
                    } else {
                        tx.send(IpcResponse::Error {
                            message: "Permission denied: requires admin or shutdown capability"
                                .to_string(),
                        })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                    }
                }
                IpcCommand::NegotiateCapabilities {
                    protocol_version,
                    capabilities,
                } => {
                    let agreed: Vec<String> = capabilities
                        .into_iter()
                        .filter(|c| {
                            server_state
                                .supported_capabilities
                                .iter()
                                .any(|sc| sc.eq_ignore_ascii_case(c))
                        })
                        .collect();

                    session.protocol_version = protocol_version;
                    session.negotiated_capabilities = agreed.clone();

                    tx.send(IpcResponse::CapabilitiesNegotiated {
                        selected_version: ProtocolVersion::CURRENT,
                        agreed_capabilities: agreed,
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::Authenticate { .. } => {
                    tx.send(IpcResponse::Authenticated {
                        session_id: session.session_id.clone(),
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::IpcServerBuilder;
    use agent_studios_ipc_protocol::{ApprovalAction, RiskLevel};

    #[tokio::test]
    async fn test_handshake_rejects_unauthenticated_first_command() {
        let server = IpcServerBuilder::new()
            .with_auth_key("key-1", b"secret-pass-1234567890")
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Client illegally sends Ping before Negotiate/Authenticate
        send_msg(
            &mut client_stream,
            &IpcCommand::Ping {
                payload: "unauthed".into(),
                timestamp: 1000,
            },
        )
        .await
        .unwrap();

        let resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        match resp {
            IpcResponse::Error { message } => {
                assert!(message.contains("Unauthenticated command"));
            }
            other => panic!("Expected Error response, got {:?}", other),
        }

        let result = handle.await.unwrap();
        assert!(matches!(
            result,
            Err(TransportError::AuthenticationFailed(_))
        ));
    }

    #[tokio::test]
    async fn test_handshake_negotiate_then_authenticate_and_ping() {
        let secret = b"my-secure-long-secret-key-123456";
        let key_id = "test-agent-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // 1. Send NegotiateCapabilities
        send_msg(
            &mut client_stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec![
                    "event_streaming".into(),
                    "non_existent_cap".into(),
                    "admin".into(),
                ],
            },
        )
        .await
        .unwrap();

        let resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let agreed = match resp {
            IpcResponse::CapabilitiesNegotiated {
                selected_version,
                agreed_capabilities,
            } => {
                assert_eq!(selected_version, ProtocolVersion::CURRENT);
                assert!(agreed_capabilities.contains(&"event_streaming".to_string()));
                assert!(agreed_capabilities.contains(&"admin".to_string()));
                assert!(!agreed_capabilities.contains(&"non_existent_cap".to_string()));
                agreed_capabilities
            }
            other => panic!("Expected CapabilitiesNegotiated, got {:?}", other),
        };

        // 2. Authenticate using HMAC
        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-client-01",
            now,
            b"",
            Some(agreed),
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let auth_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        match auth_resp {
            IpcResponse::Authenticated {
                session_id: returned_session,
            } => {
                assert_eq!(returned_session, session_id);
            }
            other => panic!("Expected Authenticated, got {:?}", other),
        }

        // 3. Now in Ready state: Ping -> Pong
        send_msg(
            &mut client_stream,
            &IpcCommand::Ping {
                payload: "heartbeat".into(),
                timestamp: 42,
            },
        )
        .await
        .unwrap();

        let pong_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        match pong_resp {
            IpcResponse::Pong {
                payload,
                client_timestamp,
                server_timestamp,
            } => {
                assert_eq!(payload, "heartbeat");
                assert_eq!(client_timestamp, 42);
                assert!(server_timestamp > 0);
            }
            other => panic!("Expected Pong, got {:?}", other),
        }

        // Drop client stream cleanly
        drop(client_stream);
        let res = handle.await.unwrap();
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_handshake_direct_authenticate_and_get_health() {
        let secret = b"my-secure-direct-auth-secret-123";
        let key_id = "direct-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Direct authenticate without prior negotiation
        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-direct-1",
            now,
            b"",
            None,
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let auth_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert!(matches!(auth_resp, IpcResponse::Authenticated { .. }));

        // GetHealth
        send_msg(&mut client_stream, &IpcCommand::GetHealth)
            .await
            .unwrap();

        let health_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        match health_resp {
            IpcResponse::Health {
                status,
                active_clients,
                uptime_secs: _,
                memory_events_count,
            } => {
                assert_eq!(status, "healthy");
                assert_eq!(active_clients, 1);
                assert_eq!(memory_events_count, 0);
            }
            other => panic!("Expected Health, got {:?}", other),
        }

        drop(client_stream);
        assert!(handle.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_invalid_hmac_rejected() {
        let secret = b"legit-secret-key-1234567890";
        let wrong_secret = b"wrong-secret-key-9999999999";
        let key_id = "test-agent-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Sign with wrong secret
        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            wrong_secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-bad",
            now,
            b"",
            None,
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert!(matches!(resp, IpcResponse::Error { .. }));

        let result = handle.await.unwrap();
        assert!(matches!(
            result,
            Err(TransportError::AuthenticationFailed(_))
        ));
    }

    #[tokio::test]
    async fn test_event_subscription_backfill_and_streaming() {
        let secret = b"secret-events-123456789012345";
        let key_id = "event-listener";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .with_replay_capacity(100)
            .build()
            .unwrap();

        // Pre-populate 2 events into server replay buffer
        server
            .publish_event(IpcEvent::new(
                1,
                "step_started",
                serde_json::json!({ "step": 1 }),
            ))
            .unwrap();
        server
            .publish_event(IpcEvent::new(
                2,
                "step_progress",
                serde_json::json!({ "progress": 50 }),
            ))
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Direct authenticate
        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-sub",
            now,
            b"",
            None,
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let _auth: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        // Subscribe from sequence 1
        send_msg(
            &mut client_stream,
            &IpcCommand::SubscribeEvents {
                from_sequence: Some(1),
            },
        )
        .await
        .unwrap();

        // Read backfilled events: seq 1 and seq 2
        let ev1: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match ev1 {
            IpcResponse::Event { event } => {
                assert_eq!(event.sequence, 1);
                assert_eq!(event.event_type, "step_started");
            }
            other => panic!("Expected Event 1, got {:?}", other),
        }

        let ev2: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match ev2 {
            IpcResponse::Event { event } => {
                assert_eq!(event.sequence, 2);
                assert_eq!(event.event_type, "step_progress");
            }
            other => panic!("Expected Event 2, got {:?}", other),
        }

        // Live event published by server
        server
            .publish_event(IpcEvent::new(
                3,
                "step_completed",
                serde_json::json!({ "ok": true }),
            ))
            .unwrap();

        let ev3: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match ev3 {
            IpcResponse::Event { event } => {
                assert_eq!(event.sequence, 3);
                assert_eq!(event.event_type, "step_completed");
            }
            other => panic!("Expected Event 3, got {:?}", other),
        }

        drop(client_stream);
        assert!(handle.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_request_and_resolve_approval() {
        let secret = b"secret-approvals-123456789012";
        let key_id = "approval-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Authenticate
        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-appr",
            now,
            b"",
            None,
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let _auth: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        // Request Approval
        let contract = ApprovalContract::new(
            session_id.clone(),
            "task-01".into(),
            "run-01".into(),
            RiskLevel::High,
            ApprovalAction::CommandExecution {
                command: "git push".into(),
                cwd: "/repo".into(),
            },
            "Push branch to remote".into(),
            300,
        );
        let approval_id = contract.approval_id.clone();

        send_msg(
            &mut client_stream,
            &IpcCommand::RequestApproval { contract },
        )
        .await
        .unwrap();

        let req_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match req_resp {
            IpcResponse::ApprovalRequested {
                approval_id: returned_id,
            } => {
                assert_eq!(returned_id, approval_id);
            }
            other => panic!("Expected ApprovalRequested, got {:?}", other),
        }

        // Resolve Approval
        send_msg(
            &mut client_stream,
            &IpcCommand::ResolveApproval {
                approval_id: approval_id.clone(),
                decision: ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("LGTM".into()),
                },
            },
        )
        .await
        .unwrap();

        let resolve_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match resolve_resp {
            IpcResponse::ApprovalResolved {
                approval_id: res_id,
                status,
            } => {
                assert_eq!(res_id, approval_id);
                assert_eq!(status, ApprovalStatus::Authorized);
            }
            other => panic!("Expected ApprovalResolved, got {:?}", other),
        }

        // Verify event was broadcast into server replay buffer
        let state = server.state();
        {
            let buf = state.replay_buffer.lock().unwrap();
            assert_eq!(buf.len(), 1);
            let ev = buf.replay_from(1, None).unwrap();
            assert_eq!(ev[0].event_type, "approval_resolved");
        }

        drop(client_stream);
        assert!(handle.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_capability_authorization() {
        let secret = b"secret-shutdown-1234567890123";
        let key_id = "shutdown-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // 1. Authenticate with ONLY read capability (no admin / shutdown)
        send_msg(
            &mut client_stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["event_streaming".into()],
            },
        )
        .await
        .unwrap();

        let _neg: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session_id,
            "nonce-no-admin",
            now,
            b"",
            Some(vec!["event_streaming".into()]),
        )
        .unwrap();

        send_msg(
            &mut client_stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let _auth: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        // 2. Unauthorized client attempts Shutdown -> Error
        send_msg(&mut client_stream, &IpcCommand::Shutdown)
            .await
            .unwrap();

        let err_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match err_resp {
            IpcResponse::Error { message } => {
                assert!(message.contains("Permission denied"));
            }
            other => panic!("Expected Error response, got {:?}", other),
        }
        assert!(!server.shutdown_token().is_cancelled());

        // 3. Re-negotiate with admin capability
        send_msg(
            &mut client_stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["admin".into()],
            },
        )
        .await
        .unwrap();

        let _neg2: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        // Authorized shutdown attempt
        send_msg(&mut client_stream, &IpcCommand::Shutdown)
            .await
            .unwrap();

        let ack_resp: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert!(matches!(ack_resp, IpcResponse::ShutdownAcknowledged));
        assert!(server.shutdown_token().is_cancelled());

        let res = handle.await.unwrap();
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_graceful_disconnect_decrements_active_connections() {
        let secret = b"secret-disconnect-12345678901";
        let key_id = "dc-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        assert_eq!(server.state().active_connections.load(Ordering::SeqCst), 0);

        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // Connection established, active_connections is incremented
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(server.state().active_connections.load(Ordering::SeqCst), 1);

        // Drop client stream suddenly
        drop(client_stream);

        let res = handle.await.unwrap();
        assert!(res.is_ok());

        // Decremented back to 0
        assert_eq!(server.state().active_connections.load(Ordering::SeqCst), 0);
    }
}
