use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use agent_studios_ipc_protocol::{
    ApprovalContract, ApprovalDecision, ApprovalId, ApprovalStatus, ExecutionDefaultsReadiness,
    ExecutionReadinessReport, HmacAuthHeader, HmacSigner, IpcEvent, ModelReadiness, ProtocolError,
    ProtocolVersion, RuntimeInstanceReadiness, SessionId,
};
use agent_studios_protocol::agent::AgentKind;
use agent_studios_protocol::cancellation::{CancellationScope, CancellationSummary};
use agent_studios_protocol::id::{RunId, StudioId, TaskId};
use agent_studios_protocol::run::{RunRecord, RunState};
use agent_studios_protocol::studio::Studio;
use agent_studios_protocol::task::{BatchTaskSpec, TaskRecord, TaskState};
use agent_studios_provider::{ModelDescriptor, ProviderInstance, ProviderInstanceId};

use crate::error::TransportError;
use crate::framing::{DEFAULT_MAX_FRAME_SIZE, recv_msg, send_msg};
use crate::server::{ClientRole, PendingRuntimeApproval, ServerState};

struct InFlightApprovalGuard {
    in_flight: Arc<Mutex<std::collections::HashSet<agent_studios_protocol::id::ApprovalId>>>,
    approval_id: agent_studios_protocol::id::ApprovalId,
}

impl Drop for InFlightApprovalGuard {
    fn drop(&mut self) {
        let mut set = self.in_flight.lock().unwrap();
        set.remove(&self.approval_id);
    }
}

struct InFlightCancellationGuard {
    in_flight: Arc<Mutex<std::collections::HashSet<agent_studios_protocol::id::TaskId>>>,
    task_id: agent_studios_protocol::id::TaskId,
}

impl Drop for InFlightCancellationGuard {
    fn drop(&mut self) {
        let mut set = self.in_flight.lock().unwrap();
        set.remove(&self.task_id);
    }
}

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
    pub role: ClientRole,
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
    GetExecutionReadiness,
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
    ExecutionReadiness {
        readiness: ExecutionReadinessReport,
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
        role: ClientRole::Standard,
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
                            let role = {
                                let roles = server_state.key_roles.lock().unwrap();
                                roles
                                    .get(&header.key_id)
                                    .copied()
                                    .unwrap_or(ClientRole::Standard)
                            };
                            session.key_id = Some(header.key_id.clone());
                            session.session_id = header.session_id.clone();
                            session.role = role;
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
                    for handle in subscription_handles.drain(..) {
                        handle.abort();
                    }

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
                            let mut last_delivered_seq = 0u64;
                            for ev in events {
                                last_delivered_seq = ev.sequence;
                                tx.send(IpcResponse::Event { event: ev })
                                    .await
                                    .map_err(|_| TransportError::ConnectionClosed)?;
                            }

                            let tx_stream = tx.clone();
                            let shutdown_tok = server_state.shutdown_token.clone();
                            let replay_buffer_arc = server_state.replay_buffer.clone();
                            let handle = tokio::spawn(async move {
                                let mut current_seq = last_delivered_seq.max(max_buffered_seq);
                                loop {
                                    tokio::select! {
                                        _ = shutdown_tok.cancelled() => break,
                                        res = broadcast_rx.recv() => {
                                            match res {
                                                Ok(event) => {
                                                    if event.sequence <= current_seq {
                                                        continue;
                                                    }
                                                    // Gap detection and backfill
                                                    if event.sequence > current_seq + 1 {
                                                        let gap_start = current_seq + 1;
                                                        let gap_end = event.sequence - 1;
                                                        let gap_count = (gap_end.saturating_sub(gap_start) + 1) as usize;
                                                        let gap_res = {
                                                            let buf = replay_buffer_arc.lock().unwrap();
                                                            buf.replay_from(gap_start, Some(gap_count))
                                                        };
                                                        match gap_res {
                                                            Ok(gap_events) => {
                                                                for gev in gap_events {
                                                                    if tx_stream.send(IpcResponse::Event { event: gev }).await.is_err() {
                                                                        return;
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                let err_msg = format!(
                                                                    "Event gap detected: unrecoverable data loss between sequence {} and {}: {}",
                                                                    gap_start, gap_end, e
                                                                );
                                                                warn!("{}", err_msg);
                                                                let _ = tx_stream.send(IpcResponse::Error { message: err_msg }).await;
                                                            }
                                                        }
                                                    }
                                                    current_seq = event.sequence;
                                                    if tx_stream.send(IpcResponse::Event { event }).await.is_err() {
                                                        break;
                                                    }
                                                }
                                                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                                    warn!("Broadcast subscription lagged by {} events; recovering via replay buffer", skipped);
                                                    let gap_start = current_seq + 1;
                                                    let recovery_res = {
                                                        let buf = replay_buffer_arc.lock().unwrap();
                                                        buf.replay_from(gap_start, None)
                                                    };
                                                    match recovery_res {
                                                        Ok(recovered_events) if !recovered_events.is_empty() => {
                                                            for rev in recovered_events {
                                                                current_seq = rev.sequence;
                                                                if tx_stream.send(IpcResponse::Event { event: rev }).await.is_err() {
                                                                    return;
                                                                }
                                                            }
                                                        }
                                                        Ok(_) => {}
                                                        Err(e) => {
                                                            let err_msg = format!(
                                                                "Unrecoverable event loss: client lagged by {} events and replay buffer cannot satisfy gap from seq {}: {}",
                                                                skipped, gap_start, e
                                                            );
                                                            warn!("{}", err_msg);
                                                            let _ = tx_stream.send(IpcResponse::Error { message: err_msg }).await;
                                                        }
                                                    }
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
                IpcCommand::RequestApproval { mut contract } => {
                    if contract.approval_id.as_str().is_empty() {
                        contract.approval_id = ApprovalId::generate();
                    }

                    // Check if associated task or run is in terminal state
                    if let Ok(tid) = contract.task_id.parse::<TaskId>() {
                        let cp = server_state.control_plane.read().await;
                        if let Some(t) = cp.get_task(tid)
                            && t.state.is_terminal()
                        {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Cannot request approval: associated task {} is in terminal state {:?}",
                                    tid, t.state
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                    }

                    if let Ok(rid) = contract.run_id.parse::<RunId>() {
                        let cp = server_state.control_plane.read().await;
                        if let Some(r) = cp.get_run(rid)
                            && r.state.is_terminal()
                        {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Cannot request approval: associated run {} is in terminal state {:?}",
                                    rid, r.state
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                    }

                    contract.session_id = session.session_id.clone();
                    let approval_id = contract.approval_id.clone();
                    if let Some(key_id) = &session.key_id {
                        server_state.approval_origins.lock().unwrap().insert(
                            approval_id.clone(),
                            (session.session_id.clone(), key_id.clone()),
                        );
                    }
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
                    if session.role != ClientRole::Admin {
                        let is_foreign = {
                            let mut origins = server_state.approval_origins.lock().unwrap();
                            if let Some((s_id, k_id)) = origins.get_mut(&approval_id) {
                                if let Some(ref caller_kid) = session.key_id {
                                    if caller_kid == k_id {
                                        if s_id != &session.session_id {
                                            tracing::info!(
                                                approval_id = %approval_id,
                                                old_session = %s_id,
                                                new_session = %session.session_id,
                                                "Rebinding approval origin to reconnected session for authenticated key"
                                            );
                                            *s_id = session.session_id.clone();
                                        }
                                        false
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        };
                        if is_foreign {
                            tx.send(IpcResponse::Error {
                                message: "Permission denied: foreign approval resolution requires admin role (untracked or cross-session origin)"
                                    .to_string(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                    }

                    let parsed_cp_id: Option<agent_studios_protocol::id::ApprovalId> =
                        std::str::FromStr::from_str(approval_id.as_str()).ok();

                    let _in_flight_guard = if let Some(aid) = parsed_cp_id {
                        let is_concurrent = {
                            let mut in_flight = server_state.in_flight_approvals.lock().unwrap();
                            if in_flight.contains(&aid) {
                                true
                            } else {
                                in_flight.insert(aid);
                                false
                            }
                        };
                        if is_concurrent {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Concurrent approval resolution in progress for approval {}",
                                    approval_id
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                        Some(InFlightApprovalGuard {
                            in_flight: Arc::clone(&server_state.in_flight_approvals),
                            approval_id: aid,
                        })
                    } else {
                        None
                    };

                    let is_in_cp = if let Some(aid) = parsed_cp_id {
                        let cp = server_state.control_plane.read().await;
                        cp.get_approval(aid).is_some()
                    } else {
                        false
                    };

                    if is_in_cp {
                        let aid = parsed_cp_id.unwrap();
                        let cp_read = server_state.control_plane.read().await;
                        let cp_app = cp_read.get_approval(aid).unwrap();
                        if cp_app.state.is_terminal() {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Cannot resolve approval {}: already in terminal state {:?}",
                                    approval_id, cp_app.state
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                        if let Some(task) = cp_read.get_task(cp_app.task_id)
                            && task.state.is_terminal()
                        {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Cannot resolve approval {}: associated task {} is in terminal state {:?}",
                                    approval_id, task.id, task.state
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                        drop(cp_read);

                        // Phase 5 transactional delivery: Deliver to runtime FIRST before mutating Control Plane
                        let pending_pra = {
                            let pending = server_state.pending_runtime_approvals.read().await;
                            pending.get(&aid).cloned()
                        };

                        if let Some(ref pra) = pending_pra {
                            let (approved, reason_str) = match &decision {
                                ApprovalDecision::Approved { user_note, .. } => {
                                    (true, user_note.clone())
                                }
                                ApprovalDecision::Rejected { reason } => {
                                    (false, Some(reason.clone()))
                                }
                            };
                            let input =
                                agent_studios_external_runtime::RuntimeInput::approval_response(
                                    pra.runtime_approval_id,
                                    approved,
                                    reason_str,
                                );
                            if let Err(e) = pra.runtime.send(&pra.session_ref, input).await {
                                // Fail closed: keep pending mapping for retry/reconciliation, do NOT commit to CP
                                tx.send(IpcResponse::Error {
                                    message: format!(
                                        "Failed to deliver approval decision to runtime: {e}"
                                    ),
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                                continue;
                            }
                        }

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
                                if pending_pra.is_some() {
                                    server_state
                                        .pending_runtime_approvals
                                        .write()
                                        .await
                                        .remove(&aid);
                                }
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
                        // Check associated task and run state if identifiers can be parsed
                        let (task_terminal_check, run_terminal_check) = {
                            let approvals = server_state.approvals.lock().unwrap();
                            if let Some(contract) = approvals.get(&approval_id) {
                                (
                                    contract.task_id.parse::<TaskId>().ok(),
                                    contract.run_id.parse::<RunId>().ok(),
                                )
                            } else {
                                (None, None)
                            }
                        };

                        if let Some(tid) = task_terminal_check {
                            let cp = server_state.control_plane.read().await;
                            if let Some(t) = cp.get_task(tid)
                                && t.state.is_terminal()
                            {
                                {
                                    let mut approvals = server_state.approvals.lock().unwrap();
                                    if let Some(contract) = approvals.get_mut(&approval_id) {
                                        contract.status = ApprovalStatus::Cancelled;
                                    }
                                }
                                tx.send(IpcResponse::Error {
                                    message: format!(
                                        "Cannot resolve approval {}: associated task {} is in terminal state {:?}",
                                        approval_id, tid, t.state
                                    ),
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                                continue;
                            }
                        }

                        if let Some(rid) = run_terminal_check {
                            let cp = server_state.control_plane.read().await;
                            if let Some(r) = cp.get_run(rid)
                                && r.state.is_terminal()
                            {
                                {
                                    let mut approvals = server_state.approvals.lock().unwrap();
                                    if let Some(contract) = approvals.get_mut(&approval_id) {
                                        contract.status = ApprovalStatus::Cancelled;
                                    }
                                }
                                tx.send(IpcResponse::Error {
                                    message: format!(
                                        "Cannot resolve approval {}: associated run {} is in terminal state {:?}",
                                        approval_id, rid, r.state
                                    ),
                                })
                                .await
                                .map_err(|_| TransportError::ConnectionClosed)?;
                                continue;
                            }
                        }

                        let resolution_result = {
                            let mut approvals = server_state.approvals.lock().unwrap();
                            if let Some(contract) = approvals.get_mut(&approval_id) {
                                if contract.status.is_terminal() {
                                    Err(format!(
                                        "Cannot resolve approval {}: already in terminal state {:?}",
                                        approval_id, contract.status
                                    ))
                                } else if contract.is_expired() {
                                    contract.status = ApprovalStatus::TimedOut;
                                    Err(format!("Approval {} has expired", approval_id))
                                } else {
                                    match contract.resolve(decision) {
                                        Ok(()) => Ok(contract.status),
                                        Err(e) => Err(e.to_string()),
                                    }
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
                                let _ = server_state.emit_event(
                                    "approval_resolved",
                                    serde_json::json!({
                                        "approval_id": approval_id,
                                        "status": status,
                                    }),
                                );

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
                    let (res, envelopes, created_tasks) = {
                        let mut cp = server_state.control_plane.write().await;
                        let r = cp.create_task_batch(studio_id, vec![task_spec]);
                        let (envs, tasks) = match &r {
                            Ok(tasks) => (cp.drain_committed_events(), tasks.clone()),
                            Err(_) => (Vec::new(), Vec::new()),
                        };
                        (r, envs, tasks)
                    };

                    match res {
                        Ok(tasks) => {
                            let task_ids: Vec<TaskId> = tasks.into_iter().map(|t| t.id).collect();
                            if let Some(ref kid) = session.key_id {
                                for tid in &task_ids {
                                    server_state.register_task_origin(
                                        *tid,
                                        session.session_id.clone(),
                                        kid,
                                    );
                                }
                            }
                            let _ = server_state.publish_control_plane_events(envelopes);
                            tx.send(IpcResponse::TasksSubmitted {
                                task_ids: task_ids.clone(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;

                            if server_state.auto_execute {
                                let state_clone = server_state.clone();
                                tokio::spawn(async move {
                                    for task in created_tasks {
                                        execute_task_lifecycle(
                                            state_clone.clone(),
                                            studio_id,
                                            task.id,
                                        )
                                        .await;
                                    }
                                });
                            }
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
                    // Phase 4: Validate caller authorization and allowed cancellation scope
                    if session.role != ClientRole::Admin {
                        let is_authorized = {
                            let mut origins = server_state.task_origins.lock().unwrap();
                            if let Some((s_id, k_id)) = origins.get_mut(&task_id) {
                                if let Some(ref caller_kid) = session.key_id {
                                    if caller_kid == k_id {
                                        if s_id != &session.session_id {
                                            tracing::info!(
                                                task_id = %task_id,
                                                old_session = %s_id,
                                                new_session = %session.session_id,
                                                "Rebinding task origin to reconnected session for authenticated key"
                                            );
                                            *s_id = session.session_id.clone();
                                        }
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        };
                        if !is_authorized {
                            tx.send(IpcResponse::Error {
                                message: "Permission denied: cross-session cancellation requires admin role (untracked or foreign task origin)"
                                    .to_string(),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                    }

                    // Concurrent cancellation deduplication
                    let _cancel_guard = {
                        let is_concurrent = {
                            let mut in_flight =
                                server_state.in_flight_cancellations.lock().unwrap();
                            if in_flight.contains(&task_id) {
                                true
                            } else {
                                in_flight.insert(task_id);
                                false
                            }
                        };
                        if is_concurrent {
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Concurrent cancellation in progress for task {}",
                                    task_id
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                        InFlightCancellationGuard {
                            in_flight: Arc::clone(&server_state.in_flight_cancellations),
                            task_id,
                        }
                    };

                    // Validate task existence, studio ownership, and non-terminal state BEFORE stopping runtime
                    let pre_check = {
                        let cp = server_state.control_plane.read().await;
                        match cp.get_task(task_id) {
                            None => Err(format!("Task {} not found", task_id)),
                            Some(task) if task.studio_id != studio_id => Err(format!(
                                "Studio mismatch: task {} belongs to studio {}, not {}",
                                task_id, task.studio_id, studio_id
                            )),
                            Some(task) if task.state.is_terminal() => Err(format!(
                                "Cannot cancel task {}: already in terminal state {:?}",
                                task_id, task.state
                            )),
                            Some(_) => Ok(()),
                        }
                    };

                    if let Err(err_msg) = pre_check {
                        tx.send(IpcResponse::Error { message: err_msg })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        continue;
                    }

                    // Stop associated runtime if an active session exists
                    let active_session = {
                        let sessions = server_state.active_runtime_sessions.read().await;
                        sessions.get(&task_id).cloned()
                    };
                    if let Some((s_ref, rt)) = active_session {
                        if let Err(e) = rt.stop(&s_ref).await {
                            tracing::warn!(
                                "Failed to stop runtime session for task {}: {}",
                                task_id,
                                e
                            );
                            tx.send(IpcResponse::Error {
                                message: format!(
                                    "Failed to stop runtime session for task {}: {}",
                                    task_id, e
                                ),
                            })
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                            continue;
                        }
                        server_state
                            .active_runtime_sessions
                            .write()
                            .await
                            .remove(&task_id);
                    }

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
                            } else if task.state.is_terminal() {
                                (
                                    Err(format!(
                                        "Cannot cancel task {}: already in terminal state {:?}",
                                        task_id, task.state
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
                IpcCommand::GetExecutionReadiness => {
                    let readiness = build_execution_readiness_report(server_state).await;
                    tx.send(IpcResponse::ExecutionReadiness { readiness })
                        .await
                        .map_err(|_| TransportError::ConnectionClosed)?;
                }
                IpcCommand::Shutdown => {
                    if session.role == ClientRole::Admin {
                        info!(
                            "Shutdown command authorized for admin key {:?}; triggering server shutdown",
                            session.key_id
                        );
                        server_state.shutdown_token.cancel();
                        tx.send(IpcResponse::ShutdownAcknowledged)
                            .await
                            .map_err(|_| TransportError::ConnectionClosed)?;
                        break;
                    } else {
                        tx.send(IpcResponse::Error {
                            message: "Permission denied: shutdown requires admin role".to_string(),
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

async fn build_execution_readiness_report(
    server_state: &Arc<ServerState>,
) -> ExecutionReadinessReport {
    let mut blocking_reasons = Vec::new();
    let mut available_runtimes = Vec::new();
    let mut runtime_instances = Vec::new();
    let mut runtime_capabilities = std::collections::BTreeMap::new();

    // 1. Runtime availability check
    let registered_runtimes = server_state.list_runtimes().await;
    if registered_runtimes.is_empty() {
        blocking_reasons.push("No agent runtime configured on daemon".to_string());
    } else {
        let outcome = server_state.runtime_registry.discover_all().await;
        let mut healthy_implementations = std::collections::BTreeSet::new();
        for inst in &outcome.instances {
            let healthy = inst.availability.is_available();
            if healthy {
                healthy_implementations.insert(inst.implementation_id.to_string());
            }
            // Serialize only the declared tri-state capability profile, never
            // environment, tokens, runtime metadata or provider configuration.
            if let Ok(value) = serde_json::to_value(&inst.capabilities)
                && let Ok(capabilities) =
                    serde_json::from_value::<std::collections::BTreeMap<String, String>>(value)
            {
                runtime_capabilities.insert(inst.instance_id.to_string(), capabilities);
            }
            runtime_instances.push(RuntimeInstanceReadiness {
                instance_id: inst.instance_id.to_string(),
                implementation: inst.implementation_id.to_string(),
                healthy,
            });
        }
        available_runtimes = healthy_implementations.into_iter().collect();
        for (failed_id, reason) in &outcome.failures {
            blocking_reasons.push(format!(
                "Failed to discover instances for runtime '{failed_id}': {reason}"
            ));
        }
    }

    // Check runtime instance availability
    if let Some(ref target_id) = server_state.configured_instance_id {
        if !runtime_instances
            .iter()
            .any(|r| r.instance_id == target_id.to_string() && r.healthy)
        {
            blocking_reasons.push(format!(
                "Configured runtime instance '{target_id}' is not available or unhealthy"
            ));
        }
    } else if !runtime_instances.iter().any(|r| r.healthy) && !registered_runtimes.is_empty() {
        blocking_reasons.push("No healthy runtime instances available".to_string());
    } else {
        // Even two healthy instances of the same implementation are ambiguous.
        let healthy_count = runtime_instances.iter().filter(|r| r.healthy).count();
        if healthy_count > 1 && server_state.configured_instance_id.is_none() {
            blocking_reasons.push(format!(
                "Multiple healthy runtimes available ({healthy_count}) but no default runtime instance configured"
            ));
        }
    }

    // 2. Model catalog check
    let available_models = {
        let catalog = server_state.provider_catalog.read().await;
        catalog
            .list_models()
            .into_iter()
            .map(|m| ModelReadiness {
                provider_instance_id: m.provider_instance_id.to_string(),
                model_id: m.id.to_string(),
                display_name: Some(m.display_name.clone()),
            })
            .collect::<Vec<_>>()
    };

    if available_models.is_empty() {
        blocking_reasons.push("No models available in provider catalog".to_string());
    } else if let Some(ref configured_ref) = server_state.default_model_ref {
        let found = available_models.iter().any(|m| {
            m.provider_instance_id == configured_ref.provider_instance_id.to_string()
                && m.model_id == configured_ref.model_id.as_str()
        });
        if !found {
            blocking_reasons.push(format!(
                "Configured default model '{}' not found in provider catalog",
                configured_ref
            ));
        }
    } else if available_models.len() > 1 {
        blocking_reasons.push(format!(
            "Multiple models available ({}) in catalog but no default model configured. Configure default model via IpcServerBuilder::with_default_model_ref or AGENT_STUDIOS_DEFAULT_MODEL",
            available_models.len()
        ));
    }

    // 3. Workspace authorization & repository validity check
    let authorized_workspaces = {
        let list = server_state.authorized_workspaces.lock().unwrap();
        let mut paths = Vec::new();
        if let Some(ref def) = server_state.default_repo_path {
            if let Ok(canon) = dunce::canonicalize(def) {
                paths.push(canon.to_string_lossy().to_string());
            } else {
                paths.push(def.to_string_lossy().to_string());
            }
        }
        for p in list.iter() {
            let s = dunce::canonicalize(p)
                .map(|c| c.to_string_lossy().to_string())
                .unwrap_or_else(|_| p.to_string_lossy().to_string());
            if !paths.contains(&s) {
                paths.push(s);
            }
        }
        paths
    };

    if let Some(ref def) = server_state.default_repo_path {
        match dunce::canonicalize(def) {
            Ok(canon) => {
                if !canon.join(".git").exists() {
                    blocking_reasons.push(format!(
                        "Default workspace path '{}' is not a valid Git repository (.git not found)",
                        canon.display()
                    ));
                } else if let Err(err) =
                    server_state.verify_workspace_authorization(&canon.to_string_lossy())
                {
                    blocking_reasons.push(format!(
                        "Default workspace path '{}' is not authorized: {err}",
                        canon.display()
                    ));
                }
            }
            Err(e) => {
                blocking_reasons.push(format!(
                    "Default workspace path '{}' cannot be canonicalized: {e}",
                    def.display()
                ));
            }
        }
    } else if authorized_workspaces.is_empty() {
        blocking_reasons.push("No authorized workspaces configured on daemon".to_string());
    } else {
        blocking_reasons.push("No default repository path configured on daemon".to_string());
    }

    let defaults = ExecutionDefaultsReadiness {
        default_repo_path: server_state
            .default_repo_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        default_provider_instance_id: server_state
            .default_model_ref
            .as_ref()
            .map(|r| r.provider_instance_id.to_string()),
        default_model_id: server_state
            .default_model_ref
            .as_ref()
            .map(|r| r.model_id.to_string()),
        configured_instance_id: server_state
            .configured_instance_id
            .as_ref()
            .map(|id| id.to_string()),
    };

    let is_ready = blocking_reasons.is_empty();

    ExecutionReadinessReport {
        is_ready,
        registered_runtimes: registered_runtimes
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        available_runtimes,
        runtime_capabilities,
        runtime_instances,
        available_models,
        authorized_workspaces,
        defaults,
        blocking_reasons,
    }
}

async fn execute_task_lifecycle(
    server_state: Arc<ServerState>,
    studio_id: StudioId,
    task_id: TaskId,
) {
    let maybe_run = {
        let mut cp = server_state.control_plane.write().await;
        let task = match cp.get_task(task_id) {
            Some(t) if !t.state.is_terminal() => t.clone(),
            _ => return,
        };
        let exec_spec = task.execution.clone();

        let agent_id = if let Some(aid) = task.assigned_agent_id {
            aid
        } else {
            let existing = cp.all_agents().find(|a| a.studio_id == studio_id);
            if let Some(a) = existing {
                a.id
            } else {
                match cp.register_agent(
                    studio_id,
                    "Default Executor",
                    AgentKind::Internal,
                    Some("executor".to_string()),
                ) {
                    Ok(a) => a.id,
                    Err(_) => return,
                }
            }
        };

        if task.state == TaskState::Pending {
            let _ = cp.transition_task_state(task_id, TaskState::Ready);
        }

        let run = match cp.create_run(task_id, agent_id) {
            Ok(r) => r,
            Err(_) => return,
        };

        if let Err(e) = cp.transition_run_state(run.id, RunState::Starting) {
            let _ = cp.record_run_outcome(
                run.id,
                "failed",
                Some(format!("Failed transition to Starting: {e}")),
            );
            let _ = cp.transition_run_state(run.id, RunState::Failed);
            let _ = cp.transition_task_state(task_id, TaskState::Failed);
            let envs = cp.drain_committed_events();
            let _ = server_state.publish_control_plane_events(envs);
            return;
        }

        if let Err(e) = cp.transition_run_state(run.id, RunState::Running) {
            let _ = cp.record_run_outcome(
                run.id,
                "failed",
                Some(format!("Failed transition to Running: {e}")),
            );
            let _ = cp.transition_run_state(run.id, RunState::Failed);
            let _ = cp.transition_task_state(task_id, TaskState::Failed);
            let envs = cp.drain_committed_events();
            let _ = server_state.publish_control_plane_events(envs);
            return;
        }

        if let Err(e) = cp.transition_task_state(task_id, TaskState::Running) {
            let _ = cp.record_run_outcome(
                run.id,
                "failed",
                Some(format!("Failed transition task to Running: {e}")),
            );
            let _ = cp.transition_run_state(run.id, RunState::Failed);
            let _ = cp.transition_task_state(task_id, TaskState::Failed);
            let envs = cp.drain_committed_events();
            let _ = server_state.publish_control_plane_events(envs);
            return;
        }

        let envs = cp.drain_committed_events();
        (run.id, agent_id, exec_spec, envs)
    };

    let (run_id, agent_id, exec_spec, start_envs) = maybe_run;
    let _ = server_state.publish_control_plane_events(start_envs);

    // --- M15-02 Deterministic Runtime and Instance Resolution ---
    let outcome = server_state.runtime_registry.discover_all().await;
    let registered_runtimes = server_state.list_runtimes().await;

    if registered_runtimes.is_empty() {
        let fail_envs = {
            let mut cp = server_state.control_plane.write().await;
            let _ = cp.record_run_outcome(
                run_id,
                "failed",
                Some("No agent runtime configured for execution".to_string()),
            );
            let _ = cp.transition_run_state(run_id, RunState::Failed);
            let _ = cp.transition_task_state(task_id, TaskState::Failed);
            cp.drain_committed_events()
        };
        let _ = server_state.publish_control_plane_events(fail_envs);
        return;
    }

    let req_pref = exec_spec
        .as_ref()
        .and_then(|e| e.runtime_preference.as_deref());
    let req_inst = exec_spec
        .as_ref()
        .and_then(|e| e.runtime_instance_id.as_deref());

    let resolution_result: Result<
        (
            Arc<dyn agent_studios_external_runtime::AgentRuntime>,
            agent_studios_external_runtime::discovery::DiscoveredRuntimeInstance,
        ),
        String,
    > = match (req_pref, req_inst) {
        // Case 1: Both explicit preference AND explicit instance
        (Some(pref), Some(inst_id_str)) => {
            let pref_id = match pref
                .parse::<agent_studios_external_runtime::id::RuntimeImplementationId>()
            {
                Ok(id) => id,
                Err(_) => {
                    return {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!("Requested runtime preference '{pref}' is invalid")),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                    };
                }
            };
            let runtime = match server_state.get_runtime(&pref_id).await {
                Ok(r) => r,
                Err(_) => {
                    return {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!(
                                    "Requested runtime implementation '{pref}' is not registered"
                                )),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                    };
                }
            };
            match outcome
                .instances
                .iter()
                .find(|i| i.instance_id.to_string() == inst_id_str)
            {
                Some(inst) => {
                    if inst.implementation_id != pref_id {
                        Err(format!(
                            "Requested instance '{inst_id_str}' belongs to runtime '{}', not requested preference '{pref}'",
                            inst.implementation_id
                        ))
                    } else if !inst.is_available() {
                        Err(format!(
                            "Requested runtime instance '{inst_id_str}' is not available"
                        ))
                    } else {
                        Ok((runtime, inst.clone()))
                    }
                }
                None => {
                    if let Some(err) = outcome.failures.get(&pref_id) {
                        Err(format!("Runtime discovery error: {err}"))
                    } else {
                        Err(format!(
                            "Requested runtime instance '{inst_id_str}' not found"
                        ))
                    }
                }
            }
        }
        // Case 2: Explicit instance only (no preference)
        (None, Some(inst_id_str)) => {
            match outcome
                .instances
                .iter()
                .find(|i| i.instance_id.to_string() == inst_id_str)
            {
                Some(inst) => {
                    if !inst.is_available() {
                        Err(format!(
                            "Requested runtime instance '{inst_id_str}' is not available"
                        ))
                    } else {
                        match server_state.get_runtime(&inst.implementation_id).await {
                            Ok(runtime) => Ok((runtime, inst.clone())),
                            Err(_) => Err(format!(
                                "Runtime implementation '{}' for instance '{inst_id_str}' is not registered",
                                inst.implementation_id
                            )),
                        }
                    }
                }
                None => Err(format!(
                    "Requested runtime instance '{inst_id_str}' not found"
                )),
            }
        }
        // Case 3: Explicit preference only (no explicit instance)
        (Some(pref), None) => {
            let pref_id = match pref
                .parse::<agent_studios_external_runtime::id::RuntimeImplementationId>()
            {
                Ok(id) => id,
                Err(_) => {
                    return {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!("Requested runtime preference '{pref}' is invalid")),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                    };
                }
            };
            let runtime = match server_state.get_runtime(&pref_id).await {
                Ok(r) => r,
                Err(_) => {
                    return {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!(
                                    "Requested runtime implementation '{pref}' is not registered"
                                )),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                    };
                }
            };
            let matching_instances: Vec<_> = outcome
                .instances
                .iter()
                .filter(|i| i.implementation_id == pref_id)
                .collect();
            if matching_instances.is_empty() {
                if let Some(err) = outcome.failures.get(&pref_id) {
                    Err(format!("Runtime discovery error: {err}"))
                } else {
                    Err(format!(
                        "No runtime instances found for implementation '{pref}'"
                    ))
                }
            } else if let Some(target_id) = &server_state.configured_instance_id {
                match matching_instances
                    .iter()
                    .find(|i| &i.instance_id == target_id)
                {
                    Some(inst) => {
                        if !inst.is_available() {
                            Err(format!(
                                "Configured runtime instance '{target_id}' is not available"
                            ))
                        } else {
                            Ok((runtime, (*inst).clone()))
                        }
                    }
                    None => Err(format!(
                        "Configured runtime instance '{target_id}' does not belong to requested implementation '{pref}'"
                    )),
                }
            } else {
                let available: Vec<_> = matching_instances
                    .into_iter()
                    .filter(|i| i.is_available())
                    .collect();
                if available.is_empty() {
                    Err(format!(
                        "No available runtime instances for implementation '{pref}'"
                    ))
                } else if available.len() == 1 {
                    Ok((runtime, (*available[0]).clone()))
                } else {
                    Err(format!(
                        "Multiple runtime instances available ({}) for implementation '{pref}' but no explicit instance selected. Configure target instance ID",
                        available.len()
                    ))
                }
            }
        }
        // Case 4: Neither explicit preference NOR explicit instance
        (None, None) => {
            if let Some(target_id) = &server_state.configured_instance_id {
                match outcome
                    .instances
                    .iter()
                    .find(|i| &i.instance_id == target_id)
                {
                    Some(inst) => {
                        if !inst.is_available() {
                            Err(format!(
                                "Configured runtime instance '{target_id}' is not available"
                            ))
                        } else {
                            match server_state.get_runtime(&inst.implementation_id).await {
                                Ok(runtime) => Ok((runtime, inst.clone())),
                                Err(_) => Err(format!(
                                    "Runtime implementation '{}' for configured instance '{target_id}' is not registered",
                                    inst.implementation_id
                                )),
                            }
                        }
                    }
                    None => Err(format!(
                        "Configured runtime instance '{target_id}' not found"
                    )),
                }
            } else {
                let available: Vec<_> = outcome
                    .instances
                    .iter()
                    .filter(|i| i.is_available())
                    .collect();
                if let Some((failed_id, err)) = outcome.failures.iter().next() {
                    // Without an explicit route, never substitute the surviving adapter
                    // while another registered implementation failed discovery.
                    Err(format!(
                        "Runtime discovery incomplete for '{failed_id}': {err}. Select a runtime explicitly"
                    ))
                } else if available.is_empty() {
                    Err("No available runtime instance discovered. Check the registered runtime adapters and their installation paths".to_string())
                } else {
                    let distinct_impls: std::collections::BTreeSet<_> =
                        available.iter().map(|i| &i.implementation_id).collect();
                    if distinct_impls.len() > 1 {
                        Err(format!(
                            "Ambiguous runtime routing: multiple available runtimes ({}) without explicit preference or configured instance",
                            distinct_impls.len()
                        ))
                    } else if available.len() > 1 {
                        Err(format!(
                            "Multiple runtime instances available ({}) but no explicit instance selected. Configure target instance ID",
                            available.len()
                        ))
                    } else {
                        let inst = available[0].clone();
                        match server_state.get_runtime(&inst.implementation_id).await {
                            Ok(runtime) => Ok((runtime, inst)),
                            Err(_) => Err(format!(
                                "Runtime implementation '{}' for instance '{}' is not registered",
                                inst.implementation_id, inst.instance_id
                            )),
                        }
                    }
                }
            }
        }
    };

    let (runtime, instance) = match resolution_result {
        Ok(pair) => pair,
        Err(err_msg) => {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(run_id, "failed", Some(err_msg));
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    };

    let (wt_id, wt_root, wt_cwd, wt_source_root, wt_source_cwd, wt_base_sha) = {
        let existing = {
            let cp = server_state.control_plane.read().await;
            cp.state()
                .worktrees
                .values()
                .find(|w| w.assigned_task_id == Some(task_id))
                .cloned()
        };

        if let Some(w) = existing {
            (
                w.id,
                w.root,
                w.cwd,
                w.source_root,
                w.source_cwd,
                w.base_commit,
            )
        } else if let Some(ref orchestrator) = server_state.workspace_orchestrator {
            let source_cwd = if let Some(req_repo) =
                exec_spec.as_ref().and_then(|e| e.repository_path.as_ref())
            {
                match server_state.verify_workspace_authorization(req_repo) {
                    Ok(canon) => canon,
                    Err(err) => {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!("Workspace authorization error: {err}")),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                        return;
                    }
                }
            } else if let Some(ref p) = server_state.default_repo_path {
                p.clone()
            } else if let Ok(repo_env) = std::env::var("AGENT_STUDIOS_REPO_PATH") {
                PathBuf::from(repo_env)
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            };

            // Phase 3: Physical git worktree creation OUTSIDE Control Plane write lock
            let mut managed_res = orchestrator
                .create_worktree_async(source_cwd.clone(), None)
                .await;
            if managed_res.is_err() {
                for backoff_ms in [50, 150, 300] {
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                    managed_res = orchestrator
                        .create_worktree_async(source_cwd.clone(), None)
                        .await;
                    if managed_res.is_ok() {
                        break;
                    }
                }
            }

            let managed = match managed_res {
                Ok(m) => m,
                Err(e) => {
                    let fail_envs = {
                        let mut cp = server_state.control_plane.write().await;
                        let _ = cp.record_run_outcome(
                            run_id,
                            "failed",
                            Some(format!("Physical git worktree creation failed: {e}")),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Failed);
                        let _ = cp.transition_task_state(task_id, TaskState::Failed);
                        cp.drain_committed_events()
                    };
                    let _ = server_state.publish_control_plane_events(fail_envs);
                    return;
                }
            };

            if let Err(e) = orchestrator.validate_worktree_path(&managed.root, &managed.cwd) {
                let _ = orchestrator
                    .release_worktree_async(
                        managed.source_cwd.clone(),
                        managed.root.clone(),
                        false,
                        Some("validation failed".to_string()),
                    )
                    .await;
                let fail_envs = {
                    let mut cp = server_state.control_plane.write().await;
                    let _ = cp.record_run_outcome(
                        run_id,
                        "failed",
                        Some(format!("Worktree path validation failed: {e}")),
                    );
                    let _ = cp.transition_run_state(run_id, RunState::Failed);
                    let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    cp.drain_committed_events()
                };
                let _ = server_state.publish_control_plane_events(fail_envs);
                return;
            }

            // Phase 4: Re-acquire CP write lock — validate task still active, attach worktree
            let attach_res = {
                let mut cp = server_state.control_plane.write().await;
                let task_active = cp
                    .get_task(task_id)
                    .map(|t| !t.state.is_terminal())
                    .unwrap_or(false);
                let run_active = cp
                    .get_run(run_id)
                    .map(|r| !r.state.is_terminal())
                    .unwrap_or(false);

                if !task_active || !run_active {
                    Err("Task or run terminated during worktree creation".to_string())
                } else {
                    match cp.create_worktree_with_cwds(
                        studio_id,
                        format!("worktree-{}", task_id),
                        managed.source_root.clone(),
                        managed.source_cwd.clone(),
                        managed.root.clone(),
                        managed.cwd.clone(),
                        managed.head_sha.clone(),
                    ) {
                        Ok(wt) => {
                            let _ = cp.transition_worktree_state(
                                wt.id,
                                agent_studios_protocol::worktree::WorktreeState::Ready,
                            );
                            let _ = cp.assign_worktree(wt.id, task_id, agent_id, Some(run_id));
                            let _ = cp.transition_worktree_state(
                                wt.id,
                                agent_studios_protocol::worktree::WorktreeState::InUse,
                            );
                            let envs = cp.drain_committed_events();
                            Ok((wt.id, envs))
                        }
                        Err(e) => Err(format!("Failed to register worktree in control plane: {e}")),
                    }
                }
            };

            match attach_res {
                Ok((wt_id, envs)) => {
                    let _ = server_state.publish_control_plane_events(envs);
                    (
                        wt_id,
                        managed.root,
                        managed.cwd,
                        managed.source_root,
                        managed.source_cwd,
                        managed.head_sha,
                    )
                }
                Err(err) => {
                    // Phase 5: Rollback worktree, record failure, exit cleanly
                    let _ = orchestrator
                        .release_worktree_async(
                            managed.source_cwd.clone(),
                            managed.root.clone(),
                            false,
                            Some("rollback".to_string()),
                        )
                        .await;
                    let fail_envs = {
                        let mut cp = server_state.control_plane.write().await;
                        if let Some(r) = cp.get_run(run_id)
                            && !r.state.is_terminal()
                        {
                            let _ = cp.record_run_outcome(run_id, "failed", Some(err));
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                        }
                        if let Some(t) = cp.get_task(task_id)
                            && !t.state.is_terminal()
                        {
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                        }
                        cp.drain_committed_events()
                    };
                    let _ = server_state.publish_control_plane_events(fail_envs);
                    return;
                }
            }
        } else {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(
                        "Workspace orchestrator not configured; physical isolation required"
                            .to_string(),
                    ),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    };

    let workspace = agent_studios_protocol::worktree::ExecutionWorkspace::managed(
        wt_id,
        wt_root,
        wt_cwd,
        wt_source_root,
        wt_source_cwd.clone(),
        wt_base_sha,
    );

    let correlation = agent_studios_external_runtime::RuntimeCorrelation::new()
        .with_studio_id(studio_id)
        .with_task_id(task_id)
        .with_run_id(run_id)
        .with_agent_id(agent_id);

    let instance_ref = agent_studios_external_runtime::RuntimeInstanceRef::new(
        runtime.implementation_id().clone(),
        instance.instance_id,
    );

    let prompt = if let Some(explicit_prompt) =
        exec_spec.as_ref().and_then(|e| e.initial_prompt.as_deref())
    {
        explicit_prompt.to_string()
    } else {
        let cp = server_state.control_plane.read().await;
        cp.get_task(task_id)
            .map(|t| {
                if t.description.is_empty() {
                    t.title.clone()
                } else {
                    format!("{}: {}", t.title, t.description)
                }
            })
            .unwrap_or_default()
    };

    let model_ref = if let Some(ref model_id_str) =
        exec_spec.as_ref().and_then(|e| e.model_id.as_ref())
    {
        let catalog = server_state.provider_catalog.read().await;
        if let Some(ref prov_id_str) = exec_spec
            .as_ref()
            .and_then(|e| e.provider_instance_id.as_ref())
        {
            let prov_id =
                match prov_id_str.parse::<agent_studios_provider::id::ProviderInstanceId>() {
                    Ok(id) => id,
                    Err(_) => {
                        let fail_envs = {
                            let mut cp = server_state.control_plane.write().await;
                            let _ = cp.record_run_outcome(
                                run_id,
                                "failed",
                                Some(format!("Invalid provider instance ID '{prov_id_str}'")),
                            );
                            let _ = cp.transition_run_state(run_id, RunState::Failed);
                            let _ = cp.transition_task_state(task_id, TaskState::Failed);
                            cp.drain_committed_events()
                        };
                        let _ = server_state.publish_control_plane_events(fail_envs);
                        return;
                    }
                };
            let m_id = match agent_studios_provider::id::ModelId::new(model_id_str.as_str()) {
                Ok(id) => id,
                Err(e) => {
                    let fail_envs = {
                        let mut cp = server_state.control_plane.write().await;
                        let _ = cp.record_run_outcome(
                            run_id,
                            "failed",
                            Some(format!("Invalid model ID '{model_id_str}': {e}")),
                        );
                        let _ = cp.transition_run_state(run_id, RunState::Failed);
                        let _ = cp.transition_task_state(task_id, TaskState::Failed);
                        cp.drain_committed_events()
                    };
                    let _ = server_state.publish_control_plane_events(fail_envs);
                    return;
                }
            };
            if catalog.get_model(&prov_id, &m_id).is_some() {
                Some(agent_studios_provider::model::ModelRef::new(prov_id, m_id))
            } else {
                let fail_envs = {
                    let mut cp = server_state.control_plane.write().await;
                    let _ = cp.record_run_outcome(
                        run_id,
                        "failed",
                        Some(format!(
                            "Requested model '{m_id}' not found for provider instance '{prov_id}' in catalog"
                        )),
                    );
                    let _ = cp.transition_run_state(run_id, RunState::Failed);
                    let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    cp.drain_committed_events()
                };
                let _ = server_state.publish_control_plane_events(fail_envs);
                return;
            }
        } else {
            let matching: Vec<_> = catalog
                .list_models()
                .into_iter()
                .filter(|m| m.id.as_str() == model_id_str.as_str())
                .collect();
            if matching.is_empty() {
                let fail_envs = {
                    let mut cp = server_state.control_plane.write().await;
                    let _ = cp.record_run_outcome(
                        run_id,
                        "failed",
                        Some(format!(
                            "Requested model '{model_id_str}' not found in any provider instance in catalog"
                        )),
                    );
                    let _ = cp.transition_run_state(run_id, RunState::Failed);
                    let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    cp.drain_committed_events()
                };
                let _ = server_state.publish_control_plane_events(fail_envs);
                return;
            } else if matching.len() == 1 {
                Some(matching[0].model_ref())
            } else {
                let fail_envs = {
                    let mut cp = server_state.control_plane.write().await;
                    let _ = cp.record_run_outcome(
                        run_id,
                        "failed",
                        Some(format!(
                            "Requested model '{model_id_str}' matches multiple provider instances ({}). Specify explicit provider_instance_id",
                            matching.len()
                        )),
                    );
                    let _ = cp.transition_run_state(run_id, RunState::Failed);
                    let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    cp.drain_committed_events()
                };
                let _ = server_state.publish_control_plane_events(fail_envs);
                return;
            }
        }
    } else if let Some(ref prov_id_str) = exec_spec
        .as_ref()
        .and_then(|e| e.provider_instance_id.as_ref())
    {
        let prov_id = match prov_id_str.parse::<agent_studios_provider::id::ProviderInstanceId>() {
            Ok(id) => id,
            Err(_) => {
                let fail_envs = {
                    let mut cp = server_state.control_plane.write().await;
                    let _ = cp.record_run_outcome(
                        run_id,
                        "failed",
                        Some(format!("Invalid provider instance ID '{prov_id_str}'")),
                    );
                    let _ = cp.transition_run_state(run_id, RunState::Failed);
                    let _ = cp.transition_task_state(task_id, TaskState::Failed);
                    cp.drain_committed_events()
                };
                let _ = server_state.publish_control_plane_events(fail_envs);
                return;
            }
        };
        let catalog = server_state.provider_catalog.read().await;
        let models = catalog.models_for_provider_instance(&prov_id);
        if models.is_empty() {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!(
                        "No models available for provider instance '{prov_id}' in catalog"
                    )),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        } else if models.len() == 1 {
            Some(models[0].model_ref())
        } else {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!(
                        "Multiple models ({}) for provider instance '{prov_id}'. Specify explicit model_id",
                        models.len()
                    )),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    } else if let Some(ref configured_ref) = server_state.default_model_ref {
        let catalog = server_state.provider_catalog.read().await;
        if catalog
            .get_model(
                &configured_ref.provider_instance_id,
                &configured_ref.model_id,
            )
            .is_some()
        {
            Some(configured_ref.clone())
        } else {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!(
                        "Configured model '{}' not found in provider catalog",
                        configured_ref
                    )),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    } else {
        let catalog = server_state.provider_catalog.read().await;
        let models = catalog.list_models();
        if models.is_empty() {
            // Backward compatibility for test fixtures where no models are registered in catalog
            None
        } else if models.len() == 1 {
            // Unambiguous single-model catalog
            Some(models[0].model_ref())
        } else {
            // Non-deterministic in production: multiple models exist without explicit selection
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!(
                        "Multiple models available ({}) in catalog but no explicit model route configured. Specify default model via IpcServerBuilder::with_default_model_ref or AGENT_STUDIOS_DEFAULT_MODEL environment variable",
                        models.len()
                    )),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    };

    let mut start_req =
        agent_studios_external_runtime::RuntimeStartRequest::new(instance_ref, workspace)
            .with_workspace_access_mode(
                agent_studios_external_runtime::WorkspaceAccessMode::Mutating,
            )
            .with_initial_prompt(prompt)
            .with_correlation(correlation);

    if let Some(m) = model_ref.clone() {
        start_req = start_req.with_model_ref(m);
    }

    let handle = match runtime.start(start_req).await {
        Ok(h) => h,
        Err(e) => {
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!("Failed to start runtime session: {e}")),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    };

    let session_ref = handle.session_ref();
    server_state
        .active_runtime_sessions
        .write()
        .await
        .insert(task_id, (session_ref.clone(), runtime.clone()));

    {
        let mut cp = server_state.control_plane.write().await;
        if let Some(r) = cp.get_run_mut(run_id) {
            let mut final_exec = exec_spec.clone().unwrap_or_default();
            if final_exec.repository_path.is_none() {
                final_exec.repository_path = Some(wt_source_cwd.to_string_lossy().to_string());
            }
            if final_exec.runtime_instance_id.is_none() {
                final_exec.runtime_instance_id = Some(instance.instance_id.to_string());
            }
            if final_exec.runtime_preference.is_none() {
                final_exec.runtime_preference = Some(instance.implementation_id.to_string());
            }
            if let Some(ref mr) = model_ref {
                if final_exec.provider_instance_id.is_none() {
                    final_exec.provider_instance_id = Some(mr.provider_instance_id.to_string());
                }
                if final_exec.model_id.is_none() {
                    final_exec.model_id = Some(mr.model_id.to_string());
                }
            }
            r.execution = Some(final_exec);
        }
    }

    let mut sub = match runtime.events_after(&session_ref, None).await {
        Ok(s) => s,
        Err(e) => {
            let _ = runtime.stop(&session_ref).await;
            server_state
                .active_runtime_sessions
                .write()
                .await
                .remove(&task_id);
            let fail_envs = {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(format!("Failed to subscribe to runtime events: {e}")),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                cp.drain_committed_events()
            };
            let _ = server_state.publish_control_plane_events(fail_envs);
            return;
        }
    };

    let mut terminal_seen = false;
    while let Ok(Some(ev)) = sub.next_event().await {
        match &ev.kind {
            agent_studios_external_runtime::RuntimeEventKind::OutputDelta { text } => {
                let _ = server_state.emit_event(
                    "output_delta",
                    serde_json::json!({
                        "studio_id": studio_id,
                        "task_id": task_id,
                        "run_id": run_id,
                        "text": text
                    }),
                );
            }
            agent_studios_external_runtime::RuntimeEventKind::Diagnostic { level, message } => {
                let _ = server_state.emit_event(
                    "diagnostic",
                    serde_json::json!({
                        "studio_id": studio_id,
                        "task_id": task_id,
                        "run_id": run_id,
                        "level": level,
                        "message": message.as_str()
                    }),
                );
            }
            agent_studios_external_runtime::RuntimeEventKind::ToolStarted {
                tool_name,
                tool_call_id,
            } => {
                let mut cp = server_state.control_plane.write().await;
                let _ = cp.record_tool_started(
                    studio_id,
                    agent_id,
                    Some(task_id),
                    Some(run_id),
                    tool_name,
                    tool_call_id,
                    chrono::Utc::now(),
                );
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
            }
            agent_studios_external_runtime::RuntimeEventKind::ToolCompleted {
                tool_name,
                tool_call_id,
                duration_ms,
                success,
            } => {
                let mut cp = server_state.control_plane.write().await;
                if *success {
                    let _ = cp.record_tool_completed(
                        studio_id,
                        agent_id,
                        Some(task_id),
                        Some(run_id),
                        tool_name,
                        tool_call_id,
                        *duration_ms,
                        chrono::Utc::now(),
                    );
                } else {
                    let _ = cp.record_tool_failed(
                        studio_id,
                        agent_id,
                        Some(task_id),
                        Some(run_id),
                        tool_name,
                        tool_call_id,
                        "Tool execution failed",
                        *duration_ms,
                        chrono::Utc::now(),
                    );
                }
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
            }
            agent_studios_external_runtime::RuntimeEventKind::ApprovalRequested {
                approval_id,
                description,
                command,
            } => {
                let kind = if command.is_some() {
                    agent_studios_protocol::approval::ApprovalKind::CommandExecution
                } else {
                    agent_studios_protocol::approval::ApprovalKind::Other
                };
                let mut cp = server_state.control_plane.write().await;
                if let Ok(app) = cp.request_approval(
                    studio_id,
                    task_id,
                    agent_id,
                    kind,
                    description.as_str().to_string(),
                ) {
                    let envs = cp.drain_committed_events();
                    drop(cp);
                    server_state.pending_runtime_approvals.write().await.insert(
                        app.id,
                        PendingRuntimeApproval {
                            session_ref: session_ref.clone(),
                            runtime_approval_id: *approval_id,
                            runtime: runtime.clone(),
                        },
                    );
                    if let Some((s_id, k_id)) = server_state
                        .task_origins
                        .lock()
                        .unwrap()
                        .get(&task_id)
                        .cloned()
                    {
                        server_state.register_approval_origin(app.id.to_string(), s_id, k_id);
                    }
                    let _ = server_state.publish_control_plane_events(envs);
                    let _ = server_state.emit_event(
                        "approval_requested",
                        serde_json::json!({
                            "approval_id": app.id,
                            "runtime_approval_id": approval_id,
                            "task_id": task_id,
                            "run_id": run_id
                        }),
                    );
                }
            }
            agent_studios_external_runtime::RuntimeEventKind::Completed { summary } => {
                terminal_seen = true;
                let mut cp = server_state.control_plane.write().await;
                let is_task_terminal = cp
                    .get_task(task_id)
                    .map(|t| t.state.is_terminal())
                    .unwrap_or(true);
                let is_run_terminal = cp
                    .get_run(run_id)
                    .map(|r| r.state.is_terminal())
                    .unwrap_or(true);
                if is_task_terminal || is_run_terminal {
                    break;
                }
                let _ = cp.record_run_outcome(run_id, "succeeded", summary.clone());
                let _ = cp.transition_run_state(run_id, RunState::Succeeded);
                let _ = cp.transition_task_state(task_id, TaskState::Succeeded);
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
                break;
            }
            agent_studios_external_runtime::RuntimeEventKind::Failed { safe_error_summary } => {
                terminal_seen = true;
                let mut cp = server_state.control_plane.write().await;
                let is_task_terminal = cp
                    .get_task(task_id)
                    .map(|t| t.state.is_terminal())
                    .unwrap_or(true);
                let is_run_terminal = cp
                    .get_run(run_id)
                    .map(|r| r.state.is_terminal())
                    .unwrap_or(true);
                if is_task_terminal || is_run_terminal {
                    break;
                }
                let _ = cp.record_run_outcome(
                    run_id,
                    "failed",
                    Some(safe_error_summary.as_str().to_string()),
                );
                let _ = cp.transition_run_state(run_id, RunState::Failed);
                let _ = cp.transition_task_state(task_id, TaskState::Failed);
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
                break;
            }
            agent_studios_external_runtime::RuntimeEventKind::Stopped => {
                terminal_seen = true;
                let mut cp = server_state.control_plane.write().await;
                let is_task_terminal = cp
                    .get_task(task_id)
                    .map(|t| t.state.is_terminal())
                    .unwrap_or(true);
                let is_run_terminal = cp
                    .get_run(run_id)
                    .map(|r| r.state.is_terminal())
                    .unwrap_or(true);
                if is_task_terminal || is_run_terminal {
                    break;
                }
                let _ = cp.record_run_outcome(
                    run_id,
                    "stopped",
                    Some("Runtime session stopped".to_string()),
                );
                let _ = cp.transition_run_state(run_id, RunState::Cancelled);
                let _ = cp.transition_task_state(task_id, TaskState::Cancelled);
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
                break;
            }
            agent_studios_external_runtime::RuntimeEventKind::Interrupted { reason } => {
                terminal_seen = true;
                let mut cp = server_state.control_plane.write().await;
                let is_task_terminal = cp
                    .get_task(task_id)
                    .map(|t| t.state.is_terminal())
                    .unwrap_or(true);
                let is_run_terminal = cp
                    .get_run(run_id)
                    .map(|r| r.state.is_terminal())
                    .unwrap_or(true);
                if is_task_terminal || is_run_terminal {
                    break;
                }
                let _ =
                    cp.record_run_outcome(run_id, "interrupted", Some(reason.as_str().to_string()));
                let _ = cp.transition_run_state(run_id, RunState::Cancelled);
                let _ = cp.transition_task_state(task_id, TaskState::Cancelled);
                let envs = cp.drain_committed_events();
                drop(cp);
                let _ = server_state.publish_control_plane_events(envs);
                break;
            }
            _ => {}
        }
    }

    if !terminal_seen {
        let mut cp = server_state.control_plane.write().await;
        if let Some(r) = cp.state().runs.get(&run_id)
            && !r.state.is_terminal()
        {
            let _ = cp.record_run_outcome(
                run_id,
                "failed",
                Some("Runtime event stream terminated before terminal outcome".to_string()),
            );
            let _ = cp.transition_run_state(run_id, RunState::Failed);
            let _ = cp.transition_task_state(task_id, TaskState::Failed);
            let envs = cp.drain_committed_events();
            drop(cp);
            let _ = server_state.publish_control_plane_events(envs);
        }
    }

    server_state
        .active_runtime_sessions
        .write()
        .await
        .remove(&task_id);
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
                    "worktree_isolation".into(),
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
                assert!(agreed_capabilities.contains(&"worktree_isolation".to_string()));
                assert!(!agreed_capabilities.contains(&"admin".to_string()));
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
        let std_secret = b"secret-shutdown-1234567890123";
        let std_key_id = "shutdown-client";
        let admin_secret = b"admin-secret-shutdown-12345678";
        let admin_key_id = "admin-client";

        let server = IpcServerBuilder::new()
            .with_auth_key(std_key_id, std_secret)
            .with_admin_auth_key(admin_key_id, admin_secret)
            .build()
            .unwrap();

        let (mut client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let server_state = server.state();

        let handle =
            tokio::spawn(async move { handle_connection(server_stream, server_state).await });

        // 1. Authenticate standard client
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
            std_secret,
            ProtocolVersion::CURRENT,
            std_key_id,
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

        // 2. Standard client attempts Shutdown -> Rejected
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

        // 3. Attempt privilege escalation via NegotiateCapabilities -> Still rejected!
        send_msg(
            &mut client_stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["admin".into(), "shutdown".into()],
            },
        )
        .await
        .unwrap();

        let _neg2: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        send_msg(&mut client_stream, &IpcCommand::Shutdown)
            .await
            .unwrap();

        let err_resp2: IpcResponse = recv_msg(&mut client_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match err_resp2 {
            IpcResponse::Error { message } => {
                assert!(message.contains("Permission denied"));
            }
            other => panic!("Expected Error response, got {:?}", other),
        }
        assert!(!server.shutdown_token().is_cancelled());

        drop(client_stream);
        let _ = handle.await;

        // 4. Authenticate admin client and verify authorized shutdown
        let (mut admin_stream, admin_server_stream) = tokio::io::duplex(64 * 1024);
        let admin_server_state = server.state();
        let admin_handle = tokio::spawn(async move {
            handle_connection(admin_server_stream, admin_server_state).await
        });

        send_msg(
            &mut admin_stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["event_streaming".into()],
            },
        )
        .await
        .unwrap();

        let _neg3: IpcResponse = recv_msg(&mut admin_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let admin_session_id = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let admin_header = HmacSigner::sign_parts(
            admin_secret,
            ProtocolVersion::CURRENT,
            admin_key_id,
            &admin_session_id,
            "nonce-admin",
            now,
            b"",
            Some(vec!["event_streaming".into()]),
        )
        .unwrap();

        send_msg(
            &mut admin_stream,
            &IpcCommand::Authenticate {
                header: admin_header,
                payload: None,
            },
        )
        .await
        .unwrap();

        let _auth_admin: IpcResponse = recv_msg(&mut admin_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        send_msg(&mut admin_stream, &IpcCommand::Shutdown)
            .await
            .unwrap();

        let ack_resp: IpcResponse = recv_msg(&mut admin_stream, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert!(matches!(ack_resp, IpcResponse::ShutdownAcknowledged));
        assert!(server.shutdown_token().is_cancelled());

        let res = admin_handle.await.unwrap();
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_foreign_approval_resolution_rejected_for_standard_client() {
        let secret1 = b"secret-client-1-1234567890123";
        let secret2 = b"secret-client-2-1234567890123";
        let key1 = "client-1";
        let key2 = "client-2";

        let server = IpcServerBuilder::new()
            .with_auth_key(key1, secret1)
            .with_auth_key(key2, secret2)
            .build()
            .unwrap();

        // Client 1 requests an approval
        let (mut stream1, server_stream1) = tokio::io::duplex(64 * 1024);
        let sstate1 = server.state();
        let _h1 = tokio::spawn(async move { handle_connection(server_stream1, sstate1).await });

        let _ = send_msg(
            &mut stream1,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["approval_routing".into()],
            },
        )
        .await;
        let _res_neg: IpcResponse = recv_msg(&mut stream1, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let session1 = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let h1 = HmacSigner::sign_parts(
            secret1,
            ProtocolVersion::CURRENT,
            key1,
            &session1,
            "nonce-1",
            now,
            b"",
            Some(vec!["approval_routing".into()]),
        )
        .unwrap();
        send_msg(
            &mut stream1,
            &IpcCommand::Authenticate {
                header: h1,
                payload: None,
            },
        )
        .await
        .unwrap();
        let _res_auth: IpcResponse = recv_msg(&mut stream1, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let contract = ApprovalContract::new(
            session1.clone(),
            "task-42".into(),
            "run-42".into(),
            RiskLevel::High,
            ApprovalAction::CommandExecution {
                command: "rm -rf /".into(),
                cwd: "/root".into(),
            },
            "Dangerous operation".into(),
            300,
        );
        let approval_id = contract.approval_id.clone();

        send_msg(&mut stream1, &IpcCommand::RequestApproval { contract })
            .await
            .unwrap();
        let resp1: IpcResponse = recv_msg(&mut stream1, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert!(matches!(resp1, IpcResponse::ApprovalRequested { .. }));

        // Client 2 connects and tries to resolve Client 1's approval
        let (mut stream2, server_stream2) = tokio::io::duplex(64 * 1024);
        let sstate2 = server.state();
        let _h2 = tokio::spawn(async move { handle_connection(server_stream2, sstate2).await });

        send_msg(
            &mut stream2,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["approval_routing".into()],
            },
        )
        .await
        .unwrap();
        let _res_neg2: IpcResponse = recv_msg(&mut stream2, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        let session2 = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let h2 = HmacSigner::sign_parts(
            secret2,
            ProtocolVersion::CURRENT,
            key2,
            &session2,
            "nonce-2",
            now,
            b"",
            Some(vec!["approval_routing".into()]),
        )
        .unwrap();
        send_msg(
            &mut stream2,
            &IpcCommand::Authenticate {
                header: h2,
                payload: None,
            },
        )
        .await
        .unwrap();
        let _res_auth2: IpcResponse = recv_msg(&mut stream2, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();

        send_msg(
            &mut stream2,
            &IpcCommand::ResolveApproval {
                approval_id,
                decision: ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("Sneaky resolution".into()),
                },
            },
        )
        .await
        .unwrap();

        let resp2: IpcResponse = recv_msg(&mut stream2, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        match resp2 {
            IpcResponse::Error { message } => {
                assert!(message.contains("Permission denied"));
                assert!(message.contains("foreign approval"));
            }
            other => panic!("Expected permission denied error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_untracked_approval_origin_fails_closed_for_standard_client() {
        let secret = b"secret-untracked-origin-test-12";
        let key_id = "std-client";
        let server = IpcServerBuilder::new()
            .with_auth_key(key_id, secret)
            .build()
            .unwrap();

        let (mut stream, server_stream) = tokio::io::duplex(64 * 1024);
        let sstate = server.state();
        let _h = tokio::spawn(async move { handle_connection(server_stream, sstate).await });

        send_msg(
            &mut stream,
            &IpcCommand::NegotiateCapabilities {
                protocol_version: ProtocolVersion::CURRENT,
                capabilities: vec!["approval_routing".into()],
            },
        )
        .await
        .unwrap();
        let _res_neg: IpcResponse = recv_msg(&mut stream, DEFAULT_MAX_FRAME_SIZE).await.unwrap();

        let session = SessionId::generate();
        let now = chrono::Utc::now().timestamp_millis();
        let header = HmacSigner::sign_parts(
            secret,
            ProtocolVersion::CURRENT,
            key_id,
            &session,
            "nonce-untracked-1",
            now,
            b"",
            Some(vec!["approval_routing".into()]),
        )
        .unwrap();

        send_msg(
            &mut stream,
            &IpcCommand::Authenticate {
                header,
                payload: None,
            },
        )
        .await
        .unwrap();
        let _res_auth: IpcResponse = recv_msg(&mut stream, DEFAULT_MAX_FRAME_SIZE).await.unwrap();

        // Attempt resolving untracked approval (missing in approval_origins map)
        send_msg(
            &mut stream,
            &IpcCommand::ResolveApproval {
                approval_id: ApprovalId::new("untracked-approval-id-999"),
                decision: ApprovalDecision::Approved {
                    modified_details: None,
                    user_note: Some("Attempt on untracked approval".into()),
                },
            },
        )
        .await
        .unwrap();

        let resp: IpcResponse = recv_msg(&mut stream, DEFAULT_MAX_FRAME_SIZE).await.unwrap();
        match resp {
            IpcResponse::Error { message } => {
                assert!(message.contains("Permission denied"));
                assert!(message.contains("foreign approval"));
                assert!(message.contains("untracked"));
            }
            other => panic!("Expected permission denied error, got {:?}", other),
        }
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
