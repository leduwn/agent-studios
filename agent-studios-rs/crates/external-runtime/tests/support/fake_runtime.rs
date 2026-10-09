use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{RwLock, broadcast};

use agent_studios_external_runtime::capabilities::{RuntimeCapabilities, RuntimeCapability};
use agent_studios_external_runtime::discovery::DiscoveredRuntimeInstance;
use agent_studios_external_runtime::error::RuntimeError;
use agent_studios_external_runtime::event::{RuntimeEvent, RuntimeEventKind};
use agent_studios_external_runtime::handle::{
    RuntimeSessionHandle, RuntimeSessionRef, RuntimeStartRequest,
};
use agent_studios_external_runtime::id::{
    RuntimeImplementationId, RuntimeInstanceId, RuntimeSessionId,
};
use agent_studios_external_runtime::input::RuntimeInput;
use agent_studios_external_runtime::lifecycle::RuntimeLifecycleState;
use agent_studios_external_runtime::traits::AgentRuntime;

/// Recorded invocations on the fake runtime for test assertions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FakeCallRecord {
    Discover,
    Capabilities,
    Start {
        instance_id: RuntimeInstanceId,
        prompt: Option<String>,
    },
    Send {
        session_ref: RuntimeSessionRef,
        input: RuntimeInput,
    },
    Interrupt {
        session_ref: RuntimeSessionRef,
    },
    Resume {
        session_ref: RuntimeSessionRef,
        input: Option<RuntimeInput>,
    },
    Stop {
        session_ref: RuntimeSessionRef,
    },
    Status {
        session_ref: RuntimeSessionRef,
    },
}

#[derive(Clone)]
struct FakeSessionState {
    handle: RuntimeSessionHandle,
    state: RuntimeLifecycleState,
    events_tx: broadcast::Sender<RuntimeEvent>,
    recorded_events: Vec<RuntimeEvent>,
    received_inputs: Vec<RuntimeInput>,
    next_sequence: u64,
}

/// In-process fake runtime implementing `AgentRuntime` for testing and contract verification.
#[derive(Clone)]
#[allow(dead_code)]
pub struct FakeAgentRuntime {
    implementation_id: RuntimeImplementationId,
    display_name: String,
    capabilities: Arc<RwLock<RuntimeCapabilities>>,
    instances: Arc<RwLock<Vec<DiscoveredRuntimeInstance>>>,
    sessions: Arc<RwLock<HashMap<RuntimeSessionId, FakeSessionState>>>,
    calls: Arc<RwLock<Vec<FakeCallRecord>>>,
    fail_discovery: Arc<RwLock<Option<String>>>,
    fail_start: Arc<RwLock<Option<String>>>,
    fail_send: Arc<RwLock<Option<String>>>,
    fail_interrupt: Arc<RwLock<Option<String>>>,
    fail_resume: Arc<RwLock<Option<String>>>,
    fail_stop: Arc<RwLock<Option<String>>>,
}

#[allow(dead_code)]
impl FakeAgentRuntime {
    /// Creates a fake runtime with full default capabilities.
    pub fn new(implementation_id: impl Into<String>) -> Self {
        let caps = RuntimeCapabilities::default()
            .with_streaming_events(true)
            .with_interrupt(true)
            .with_resume(true)
            .with_stop(true)
            .with_persistent_session(true)
            .with_workspace_binding(true)
            .with_model_selection(true)
            .with_provider_selection(true)
            .with_tools(true)
            .with_approvals(true)
            .with_mcp(true)
            .with_skills(true)
            .with_image_input(true)
            .with_structured_output(true)
            .with_subagents(true)
            .with_usage_reporting(true);

        let impl_id = RuntimeImplementationId::new(implementation_id)
            .expect("valid fake runtime implementation id");
        let default_inst_id = RuntimeInstanceId::generate();
        let default_instance = DiscoveredRuntimeInstance::available(
            default_inst_id,
            impl_id.clone(),
            "Default Fake Instance",
            caps.clone(),
        );

        Self {
            implementation_id: impl_id,
            display_name: "In-Process Fake Agent Runtime".to_string(),
            capabilities: Arc::new(RwLock::new(caps)),
            instances: Arc::new(RwLock::new(vec![default_instance])),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            calls: Arc::new(RwLock::new(Vec::new())),
            fail_discovery: Arc::new(RwLock::new(None)),
            fail_start: Arc::new(RwLock::new(None)),
            fail_send: Arc::new(RwLock::new(None)),
            fail_interrupt: Arc::new(RwLock::new(None)),
            fail_resume: Arc::new(RwLock::new(None)),
            fail_stop: Arc::new(RwLock::new(None)),
        }
    }

    /// Sets explicit instances returned by discovery.
    pub async fn set_instances(&self, instances: Vec<DiscoveredRuntimeInstance>) {
        let mut guard = self.instances.write().await;
        *guard = instances;
    }

    /// Adds an instance returned by discovery.
    pub async fn add_instance(&self, instance: DiscoveredRuntimeInstance) {
        let mut guard = self.instances.write().await;
        guard.push(instance);
    }

    /// Sets custom capabilities for testing missing or minimal profiles.
    pub async fn set_capabilities(&self, capabilities: RuntimeCapabilities) {
        let mut guard = self.capabilities.write().await;
        *guard = capabilities;
    }

    /// Injects discovery failure.
    pub async fn set_fail_discovery(&self, reason: Option<String>) {
        let mut guard = self.fail_discovery.write().await;
        *guard = reason;
    }

    /// Injects startup failure.
    pub async fn set_fail_start(&self, reason: Option<String>) {
        let mut guard = self.fail_start.write().await;
        *guard = reason;
    }

    /// Injects send failure.
    pub async fn set_fail_send(&self, reason: Option<String>) {
        let mut guard = self.fail_send.write().await;
        *guard = reason;
    }

    /// Injects interrupt failure.
    pub async fn set_fail_interrupt(&self, reason: Option<String>) {
        let mut guard = self.fail_interrupt.write().await;
        *guard = reason;
    }

    /// Injects resume failure.
    pub async fn set_fail_resume(&self, reason: Option<String>) {
        let mut guard = self.fail_resume.write().await;
        *guard = reason;
    }

    /// Injects stop failure.
    pub async fn set_fail_stop(&self, reason: Option<String>) {
        let mut guard = self.fail_stop.write().await;
        *guard = reason;
    }

    /// Emits a structured event into an existing session.
    pub async fn emit_event(
        &self,
        session_id: &RuntimeSessionId,
        kind: RuntimeEventKind,
    ) -> Result<RuntimeEvent, RuntimeError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: *session_id,
            })?;

        let event = RuntimeEvent::new(*session_id, session.next_sequence, kind);
        session.next_sequence += 1;
        session.recorded_events.push(event.clone());
        let _ = session.events_tx.send(event.clone());
        Ok(event)
    }

    /// Transitions session state, verifying deterministic lifecycle transition invariants.
    pub async fn transition_session(
        &self,
        session_id: &RuntimeSessionId,
        target: RuntimeLifecycleState,
    ) -> Result<(), RuntimeError> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: *session_id,
            })?;

        session.state.validate_transition_to(target)?;
        let prev = session.state;
        session.state = target;
        session.handle.state = target;

        let event = RuntimeEvent::new(
            *session_id,
            session.next_sequence,
            RuntimeEventKind::StatusChanged {
                previous_state: prev,
                new_state: target,
            },
        );
        session.next_sequence += 1;
        session.recorded_events.push(event.clone());
        let _ = session.events_tx.send(event);
        Ok(())
    }

    /// Returns recorded call records for verification.
    pub async fn get_calls(&self) -> Vec<FakeCallRecord> {
        let guard = self.calls.read().await;
        guard.clone()
    }

    /// Returns all inputs received by a session.
    pub async fn get_received_inputs(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<Vec<RuntimeInput>, RuntimeError> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: *session_id,
            })?;
        Ok(session.received_inputs.clone())
    }

    /// Returns all events recorded for a session.
    pub async fn get_recorded_events(
        &self,
        session_id: &RuntimeSessionId,
    ) -> Result<Vec<RuntimeEvent>, RuntimeError> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: *session_id,
            })?;
        Ok(session.recorded_events.clone())
    }

    fn validate_session_ref<'a>(
        &self,
        session: &RuntimeSessionRef,
        sessions: &'a HashMap<RuntimeSessionId, FakeSessionState>,
    ) -> Result<&'a FakeSessionState, RuntimeError> {
        if session.implementation_id != self.implementation_id {
            return Err(RuntimeError::SessionImplementationMismatch {
                expected: self.implementation_id.clone(),
                actual: session.implementation_id.clone(),
                session_id: session.session_id,
            });
        }
        let stored = sessions
            .get(&session.session_id)
            .ok_or(RuntimeError::SessionNotFound {
                session_id: session.session_id,
            })?;
        if stored.handle.instance_id != session.instance_id {
            return Err(RuntimeError::SessionInstanceMismatch {
                expected: stored.handle.instance_id,
                actual: session.instance_id,
                session_id: session.session_id,
            });
        }
        Ok(stored)
    }

    fn validate_session_ref_mut<'a>(
        &self,
        session: &RuntimeSessionRef,
        sessions: &'a mut HashMap<RuntimeSessionId, FakeSessionState>,
    ) -> Result<&'a mut FakeSessionState, RuntimeError> {
        if session.implementation_id != self.implementation_id {
            return Err(RuntimeError::SessionImplementationMismatch {
                expected: self.implementation_id.clone(),
                actual: session.implementation_id.clone(),
                session_id: session.session_id,
            });
        }
        let stored =
            sessions
                .get_mut(&session.session_id)
                .ok_or(RuntimeError::SessionNotFound {
                    session_id: session.session_id,
                })?;
        if stored.handle.instance_id != session.instance_id {
            return Err(RuntimeError::SessionInstanceMismatch {
                expected: stored.handle.instance_id,
                actual: session.instance_id,
                session_id: session.session_id,
            });
        }
        Ok(stored)
    }
}

#[async_trait]
impl AgentRuntime for FakeAgentRuntime {
    fn implementation_id(&self) -> &RuntimeImplementationId {
        &self.implementation_id
    }

    async fn discover(&self) -> Result<Vec<DiscoveredRuntimeInstance>, RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Discover);

        if let Some(reason) = self.fail_discovery.read().await.as_ref() {
            return Err(RuntimeError::discovery_failed(reason.clone()));
        }

        let instances = self.instances.read().await.clone();
        Ok(instances)
    }

    async fn capabilities(&self) -> RuntimeCapabilities {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Capabilities);
        self.capabilities.read().await.clone()
    }

    async fn start(
        &self,
        request: RuntimeStartRequest,
    ) -> Result<RuntimeSessionHandle, RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Start {
            instance_id: *request.instance_id(),
            prompt: request.initial_prompt.clone(),
        });

        if *request.implementation_id() != self.implementation_id {
            return Err(RuntimeError::InstanceImplementationMismatch {
                expected: self.implementation_id.clone(),
                actual: request.implementation_id().clone(),
                instance_id: *request.instance_id(),
            });
        }

        let known_instances = self.instances.read().await;
        if !known_instances.is_empty()
            && !known_instances
                .iter()
                .any(|inst| inst.instance_id == *request.instance_id())
        {
            return Err(RuntimeError::UnknownInstance {
                instance_id: *request.instance_id(),
            });
        }
        drop(known_instances);

        if let Some(reason) = self.fail_start.read().await.as_ref() {
            return Err(RuntimeError::startup_failed(reason.clone()));
        }

        let session_id = RuntimeSessionId::generate();
        let (events_tx, _) = broadcast::channel(512);

        let handle = RuntimeSessionHandle::new(
            session_id,
            *request.instance_id(),
            self.implementation_id.clone(),
            RuntimeLifecycleState::Running,
            request.workspace.clone(),
            request.correlation.clone(),
            Utc::now(),
        );

        let mut recorded_events = Vec::new();
        let mut next_seq = 1;

        // Emit SessionStarted with matching session_id
        let start_event = RuntimeEvent::new(
            session_id,
            next_seq,
            RuntimeEventKind::SessionStarted {
                session_id,
                instance_id: *request.instance_id(),
            },
        );
        next_seq += 1;
        recorded_events.push(start_event.clone());
        let _ = events_tx.send(start_event);

        // Emit StatusChanged: Starting -> Running
        let status_event = RuntimeEvent::new(
            session_id,
            next_seq,
            RuntimeEventKind::StatusChanged {
                previous_state: RuntimeLifecycleState::Starting,
                new_state: RuntimeLifecycleState::Running,
            },
        );
        next_seq += 1;
        recorded_events.push(status_event.clone());
        let _ = events_tx.send(status_event);

        let mut received_inputs = Vec::new();
        if let Some(prompt) = request.initial_prompt {
            received_inputs.push(RuntimeInput::text(prompt.clone()));
            let output_event = RuntimeEvent::new(
                session_id,
                next_seq,
                RuntimeEventKind::OutputDelta {
                    text: format!("Acknowledged prompt: {}", prompt),
                },
            );
            next_seq += 1;
            recorded_events.push(output_event.clone());
            let _ = events_tx.send(output_event);
        }

        let session_state = FakeSessionState {
            handle: handle.clone(),
            state: RuntimeLifecycleState::Running,
            events_tx,
            recorded_events,
            received_inputs,
            next_sequence: next_seq,
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id, session_state);

        Ok(handle)
    }

    async fn send(
        &self,
        session: &RuntimeSessionRef,
        input: RuntimeInput,
    ) -> Result<(), RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Send {
            session_ref: session.clone(),
            input: input.clone(),
        });

        if let Some(reason) = self.fail_send.read().await.as_ref() {
            return Err(RuntimeError::send_failed(
                session.session_id,
                reason.clone(),
            ));
        }

        let mut sessions = self.sessions.write().await;
        let session_state = self.validate_session_ref_mut(session, &mut sessions)?;

        if session_state.state.is_terminal() {
            return Err(RuntimeError::terminal_state_error(
                session.session_id,
                session_state.state,
                "send",
            ));
        }

        session_state.received_inputs.push(input.clone());

        match &input {
            RuntimeInput::Text { content } => {
                let event = RuntimeEvent::new(
                    session.session_id,
                    session_state.next_sequence,
                    RuntimeEventKind::OutputDelta {
                        text: format!("Processed text: {}", content),
                    },
                );
                session_state.next_sequence += 1;
                session_state.recorded_events.push(event.clone());
                let _ = session_state.events_tx.send(event);
            }
            RuntimeInput::Continuation { context } => {
                let event = RuntimeEvent::new(
                    session.session_id,
                    session_state.next_sequence,
                    RuntimeEventKind::OutputDelta {
                        text: format!("Continued turn with context: {:?}", context),
                    },
                );
                session_state.next_sequence += 1;
                session_state.recorded_events.push(event.clone());
                let _ = session_state.events_tx.send(event);
            }
            RuntimeInput::ApprovalResponse {
                approval_id,
                approved,
                ..
            } => {
                let event = RuntimeEvent::new(
                    session.session_id,
                    session_state.next_sequence,
                    RuntimeEventKind::Diagnostic {
                        level: "info".to_string(),
                        message: format!("Approval {} resolved: {}", approval_id, approved),
                    },
                );
                session_state.next_sequence += 1;
                session_state.recorded_events.push(event.clone());
                let _ = session_state.events_tx.send(event);
            }
        }

        Ok(())
    }

    async fn interrupt(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Interrupt {
            session_ref: session.clone(),
        });

        let caps = self.capabilities.read().await;
        caps.ensure_supported(RuntimeCapability::Interrupt)?;

        if let Some(reason) = self.fail_interrupt.read().await.as_ref() {
            return Err(RuntimeError::interruption_failed(
                session.session_id,
                reason.clone(),
            ));
        }

        let mut sessions = self.sessions.write().await;
        let session_state = self.validate_session_ref_mut(session, &mut sessions)?;

        session_state
            .state
            .validate_transition_to(RuntimeLifecycleState::Interrupted)?;
        let prev = session_state.state;
        session_state.state = RuntimeLifecycleState::Interrupted;
        session_state.handle.state = RuntimeLifecycleState::Interrupted;

        let status_event = RuntimeEvent::new(
            session.session_id,
            session_state.next_sequence,
            RuntimeEventKind::StatusChanged {
                previous_state: prev,
                new_state: RuntimeLifecycleState::Interrupted,
            },
        );
        session_state.next_sequence += 1;
        session_state.recorded_events.push(status_event.clone());
        let _ = session_state.events_tx.send(status_event);

        let interrupt_event = RuntimeEvent::new(
            session.session_id,
            session_state.next_sequence,
            RuntimeEventKind::Interrupted {
                reason: "Interrupt requested by control plane".to_string(),
            },
        );
        session_state.next_sequence += 1;
        session_state.recorded_events.push(interrupt_event.clone());
        let _ = session_state.events_tx.send(interrupt_event);

        Ok(())
    }

    async fn resume(
        &self,
        session: &RuntimeSessionRef,
        input: Option<RuntimeInput>,
    ) -> Result<(), RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Resume {
            session_ref: session.clone(),
            input: input.clone(),
        });

        let caps = self.capabilities.read().await;
        caps.ensure_supported(RuntimeCapability::Resume)?;

        if let Some(reason) = self.fail_resume.read().await.as_ref() {
            return Err(RuntimeError::resume_failed(
                session.session_id,
                reason.clone(),
            ));
        }

        let mut sessions = self.sessions.write().await;
        let session_state = self.validate_session_ref_mut(session, &mut sessions)?;

        if session_state.state != RuntimeLifecycleState::Interrupted {
            return Err(RuntimeError::InvalidLifecycleTransition {
                from: session_state.state,
                to: RuntimeLifecycleState::Running,
                reason: "Can only resume from Interrupted state".into(),
            });
        }

        session_state
            .state
            .validate_transition_to(RuntimeLifecycleState::Running)?;
        let prev = session_state.state;
        session_state.state = RuntimeLifecycleState::Running;
        session_state.handle.state = RuntimeLifecycleState::Running;

        let status_event = RuntimeEvent::new(
            session.session_id,
            session_state.next_sequence,
            RuntimeEventKind::StatusChanged {
                previous_state: prev,
                new_state: RuntimeLifecycleState::Running,
            },
        );
        session_state.next_sequence += 1;
        session_state.recorded_events.push(status_event.clone());
        let _ = session_state.events_tx.send(status_event);

        if let Some(inp) = input {
            session_state.received_inputs.push(inp);
            let event = RuntimeEvent::new(
                session.session_id,
                session_state.next_sequence,
                RuntimeEventKind::OutputDelta {
                    text: "Resumed turn".to_string(),
                },
            );
            session_state.next_sequence += 1;
            session_state.recorded_events.push(event.clone());
            let _ = session_state.events_tx.send(event);
        }

        Ok(())
    }

    async fn stop(&self, session: &RuntimeSessionRef) -> Result<(), RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Stop {
            session_ref: session.clone(),
        });

        let caps = self.capabilities.read().await;
        caps.ensure_supported(RuntimeCapability::Stop)?;

        if let Some(reason) = self.fail_stop.read().await.as_ref() {
            return Err(RuntimeError::stop_failed(
                session.session_id,
                reason.clone(),
            ));
        }

        let mut sessions = self.sessions.write().await;
        let session_state = self.validate_session_ref_mut(session, &mut sessions)?;

        // Stop semantics:
        // 1. If already Stopped: IDEMPOTENT (Ok(()))
        // 2. If Completed or Failed: TerminalStateError
        // 3. Otherwise: transition to Stopped
        match session_state.state {
            RuntimeLifecycleState::Stopped => return Ok(()),
            RuntimeLifecycleState::Completed => {
                return Err(RuntimeError::terminal_state_error(
                    session.session_id,
                    RuntimeLifecycleState::Completed,
                    "stop",
                ));
            }
            RuntimeLifecycleState::Failed => {
                return Err(RuntimeError::terminal_state_error(
                    session.session_id,
                    RuntimeLifecycleState::Failed,
                    "stop",
                ));
            }
            _ => {}
        }

        let prev = session_state.state;
        session_state.state = RuntimeLifecycleState::Stopped;
        session_state.handle.state = RuntimeLifecycleState::Stopped;

        let status_event = RuntimeEvent::new(
            session.session_id,
            session_state.next_sequence,
            RuntimeEventKind::StatusChanged {
                previous_state: prev,
                new_state: RuntimeLifecycleState::Stopped,
            },
        );
        session_state.next_sequence += 1;
        session_state.recorded_events.push(status_event.clone());
        let _ = session_state.events_tx.send(status_event);

        let stop_event = RuntimeEvent::new(
            session.session_id,
            session_state.next_sequence,
            RuntimeEventKind::Stopped,
        );
        session_state.next_sequence += 1;
        session_state.recorded_events.push(stop_event.clone());
        let _ = session_state.events_tx.send(stop_event);

        Ok(())
    }

    async fn status(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<RuntimeLifecycleState, RuntimeError> {
        let mut calls = self.calls.write().await;
        calls.push(FakeCallRecord::Status {
            session_ref: session.clone(),
        });

        let sessions = self.sessions.read().await;
        let session_state = self.validate_session_ref(session, &sessions)?;

        Ok(session_state.state)
    }

    async fn events(
        &self,
        session: &RuntimeSessionRef,
    ) -> Result<broadcast::Receiver<RuntimeEvent>, RuntimeError> {
        let sessions = self.sessions.read().await;
        let session_state = self.validate_session_ref(session, &sessions)?;

        Ok(session_state.events_tx.subscribe())
    }
}
