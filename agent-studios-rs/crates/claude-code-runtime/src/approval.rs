use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use agent_studios_protocol::id::ApprovalId;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::ClaudeCodeError;
use crate::protocol::{ControlRequest, ControlRequestPayload, ControlResponse};

/// Execution state of a tool approval request throughout its lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Awaiting host decision or watchdog timeout.
    Pending,
    /// In two-phase commit: resolution prepared, awaiting stdin delivery confirmation.
    Resolving,
    /// Successfully written to child stdin and confirmed.
    Delivered,
    /// Write to child stdin or serialization failed (fail-closed).
    DeliveryFailed,
    /// Cancelled via session interrupt, stop, or explicit cancellation (fail-closed).
    Cancelled,
    /// Watchdog timer elapsed before host decision (fail-closed).
    TimedOut,
}

impl ApprovalState {
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    pub fn is_resolving(&self) -> bool {
        matches!(self, Self::Resolving)
    }

    pub fn is_delivered(&self) -> bool {
        matches!(self, Self::Delivered)
    }

    pub fn is_delivery_failed(&self) -> bool {
        matches!(self, Self::DeliveryFailed)
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    pub fn is_timed_out(&self) -> bool {
        matches!(self, Self::TimedOut)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Delivered | Self::DeliveryFailed | Self::Cancelled | Self::TimedOut
        )
    }
}

/// Two-phase commit token linking resolution preparation to delivery confirmation or failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolutionToken {
    pub approval_id: ApprovalId,
    pub request_id: String,
    pub decision: ApprovalDecision,
}

/// Tool approval decision emitted by the host or default policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Allow,
    Deny,
}

impl ApprovalDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Deny)
    }

    pub fn behavior(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    /// Formats this decision into Claude Code's control_response JSON value:
    /// `{"type": "control_response", "response": {"subtype": "success", "request_id": "<id>", "response": {"behavior": "allow" | "deny"}}}`
    pub fn to_control_response_value(&self, request_id: &str) -> serde_json::Value {
        match self {
            Self::Allow => {
                serde_json::to_value(ControlResponse::allow(request_id, None)).unwrap_or_default()
            }
            Self::Deny => serde_json::to_value(ControlResponse::deny(request_id, None, None))
                .unwrap_or_default(),
        }
    }

    /// Formats this decision into a single-line NDJSON string.
    pub fn to_control_response(&self, request_id: &str) -> String {
        serde_json::to_string(&self.to_control_response_value(request_id)).unwrap_or_default()
    }

    /// Formats this decision into an NDJSON line ending with `\n`.
    pub fn to_control_response_frame(&self, request_id: &str) -> String {
        let mut s = self.to_control_response(request_id);
        s.push('\n');
        s
    }
}

/// Incoming tool approval request emitted by Claude Code CLI under `--permission-prompts=host`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanUseToolRequest {
    pub request_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub decision_reason: Option<String>,
    #[serde(default)]
    pub blocked_path: Option<String>,
    #[serde(default)]
    pub default_to_no: Option<bool>,
    #[serde(default)]
    pub suggestions: Vec<serde_json::Value>,
}

impl CanUseToolRequest {
    pub fn new(
        request_id: impl Into<String>,
        tool_name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            tool_name: tool_name.into(),
            input,
            tool_use_id: None,
            agent_id: None,
            decision_reason: None,
            blocked_path: None,
            default_to_no: None,
            suggestions: Vec::new(),
        }
    }

    pub fn with_tool_use_id(mut self, id: impl Into<String>) -> Self {
        self.tool_use_id = Some(id.into());
        self
    }

    pub fn with_agent_id(mut self, id: impl Into<String>) -> Self {
        self.agent_id = Some(id.into());
        self
    }

    pub fn with_decision_reason(mut self, reason: impl Into<String>) -> Self {
        self.decision_reason = Some(reason.into());
        self
    }

    /// Creates a `CanUseToolRequest` from a protocol request and ID.
    pub fn from_protocol(
        request_id: impl Into<String>,
        req: &crate::protocol::CanUseToolRequest,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            tool_name: req.tool_name.clone(),
            input: req.input.clone(),
            tool_use_id: req.tool_use_id.clone(),
            agent_id: req.agent_id.clone(),
            decision_reason: req.decision_reason.clone(),
            blocked_path: req.blocked_path.clone(),
            default_to_no: req.default_to_no,
            suggestions: req.suggestions.clone().unwrap_or_default(),
        }
    }

    /// Parses a `CanUseToolRequest` from a raw Claude Code NDJSON `control_request` frame.
    pub fn from_control_request(val: &serde_json::Value) -> Result<Self, ClaudeCodeError> {
        let request_id = val
            .get("request_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ClaudeCodeError::Protocol("control_request missing request_id".into())
            })?;

        let request = val.get("request").ok_or_else(|| {
            ClaudeCodeError::Protocol("control_request missing request field".into())
        })?;

        let tool_name = request
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let input = request
            .get("input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let tool_use_id = request
            .get("tool_use_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let agent_id = request
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let decision_reason = request
            .get("decision_reason")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let blocked_path = request
            .get("blocked_path")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let default_to_no = request.get("default_to_no").and_then(|v| v.as_bool());
        let suggestions = request
            .get("suggestions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(Self {
            request_id: request_id.to_string(),
            tool_name,
            input,
            tool_use_id,
            agent_id,
            decision_reason,
            blocked_path,
            default_to_no,
            suggestions,
        })
    }
}

/// A recorded tool execution authorization request awaiting host/operator decision.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingApproval {
    pub approval_id: ApprovalId,
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_use_id: Option<String>,
    pub agent_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub state: ApprovalState,
    pub decision: Option<ApprovalDecision>,
}

/// Thread-safe host approval coordinator for `--permission-prompts=host` workflows.
/// Manages in-flight pre-execution tool approval requests, deterministic ID derivation,
/// two-phase delivery verification, and fail-closed timeout/cancellation semantics.
#[derive(Debug)]
pub struct ApprovalAuthority {
    session_id: String,
    pending_oneshots: Mutex<HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>>,
    approvals: RwLock<BTreeMap<ApprovalId, PendingApproval>>,
    request_to_approval: RwLock<HashMap<String, ApprovalId>>,
    watchdog_handles: Mutex<HashMap<ApprovalId, tokio::task::JoinHandle<()>>>,
}

impl Default for ApprovalAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalAuthority {
    pub fn new() -> Self {
        Self::with_session_id("default")
    }

    pub fn with_session_id(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            pending_oneshots: Mutex::new(HashMap::new()),
            approvals: RwLock::new(BTreeMap::new()),
            request_to_approval: RwLock::new(HashMap::new()),
            watchdog_handles: Mutex::new(HashMap::new()),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Derives a deterministic `ApprovalId` from a wire `request_id` using default session namespace.
    pub fn derive_approval_id(request_id: &str) -> ApprovalId {
        Self::derive_scoped_approval_id("default", request_id)
    }

    /// Derives a deterministic `ApprovalId` scoped to session namespace and wire `request_id`.
    pub fn derive_scoped_approval_id(session_id: &str, request_id: &str) -> ApprovalId {
        let session_ns = Uuid::new_v5(&Uuid::NAMESPACE_DNS, session_id.as_bytes());
        let uuid = Uuid::new_v5(&session_ns, request_id.as_bytes());
        ApprovalId::from_uuid(uuid)
    }

    /// Derives a deterministic `ApprovalId` for a `request_id` scoped to this authority's session.
    pub fn derive_id(&self, request_id: &str) -> ApprovalId {
        Self::derive_scoped_approval_id(&self.session_id, request_id)
    }

    /// Registers a pending control request received from Claude Code stdout.
    /// Safely and idempotently handles duplicate `request_id` inside the session.
    pub fn register_request(&self, request: &ControlRequest) -> Option<ApprovalId> {
        let payload = match &request.request {
            ControlRequestPayload::CanUseTool(can_use) => can_use,
            _ => return None,
        };

        // Check for duplicate request_id inside this session (idempotent handling)
        {
            let wire_ids = self.request_to_approval.read().expect("lock poisoned");
            if let Some(&existing_id) = wire_ids.get(&request.request_id) {
                debug!(
                    session_id = %self.session_id,
                    request_id = %request.request_id,
                    approval_id = %existing_id,
                    "Duplicate tool approval request_id detected in session; returning existing approval ID idempotently"
                );
                return Some(existing_id);
            }
        }

        let approval_id = self.derive_id(&request.request_id);

        let pending = PendingApproval {
            approval_id,
            request_id: request.request_id.clone(),
            tool_name: payload.tool_name.clone(),
            tool_input: payload.input.clone(),
            tool_use_id: payload.tool_use_id.clone(),
            agent_id: payload.agent_id.clone(),
            created_at: chrono::Utc::now(),
            state: ApprovalState::Pending,
            decision: None,
        };

        {
            let mut wire_ids = self.request_to_approval.write().expect("lock poisoned");
            if let Some(&existing_id) = wire_ids.get(&request.request_id) {
                return Some(existing_id);
            }
            wire_ids.insert(request.request_id.clone(), approval_id);
        }

        debug!(
            approval_id = %approval_id,
            session_id = %self.session_id,
            request_id = %request.request_id,
            tool_name = %payload.tool_name,
            "Registered tool approval request"
        );

        let mut lock = self.approvals.write().expect("lock poisoned");
        lock.insert(approval_id, pending);

        Some(approval_id)
    }

    /// Registers an approval request and awaits a decision with a timeout.
    /// Fails closed: On timeout or channel drop, defaults to `ApprovalDecision::Deny`.
    pub async fn request_approval(
        &self,
        request: CanUseToolRequest,
        timeout: Duration,
    ) -> Result<ApprovalDecision, ClaudeCodeError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request_id = request.request_id.clone();

        {
            let mut pending = self
                .pending_oneshots
                .lock()
                .map_err(|e| ClaudeCodeError::Internal(format!("Approval mutex poisoned: {e}")))?;
            if pending.contains_key(&request_id) {
                return Err(ClaudeCodeError::Protocol(format!(
                    "Duplicate in-flight request_approval for request_id '{request_id}'"
                )));
            }
            pending.insert(request_id.clone(), tx);
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(decision)) => {
                info!(
                    request_id = %request_id,
                    tool = %request.tool_name,
                    decision = ?decision,
                    "Tool approval resolved by host"
                );
                Ok(decision)
            }
            Ok(Err(_closed)) => {
                warn!(
                    request_id = %request_id,
                    tool = %request.tool_name,
                    "Approval channel dropped before resolution; failing closed to Deny"
                );
                if let Ok(mut pending) = self.pending_oneshots.lock() {
                    pending.remove(&request_id);
                }
                Ok(ApprovalDecision::Deny)
            }
            Err(_elapsed) => {
                warn!(
                    request_id = %request_id,
                    tool = %request.tool_name,
                    timeout = ?timeout,
                    "Tool approval timed out; failing closed to Deny"
                );
                if let Ok(mut pending) = self.pending_oneshots.lock() {
                    pending.remove(&request_id);
                }
                Ok(ApprovalDecision::Deny)
            }
        }
    }

    /// Resolves an in-flight approval request by `request_id` with the specified decision.
    pub fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), ClaudeCodeError> {
        let sender = {
            let mut pending = self
                .pending_oneshots
                .lock()
                .map_err(|e| ClaudeCodeError::Internal(format!("Approval mutex poisoned: {e}")))?;
            pending.remove(request_id)
        };

        if let Some(tx) = sender {
            let _ = tx.send(decision);
            return Ok(());
        }

        let approval_id = {
            let map = self.request_to_approval.read().expect("lock poisoned");
            map.get(request_id).copied()
        };

        if let Some(id) = approval_id {
            self.resolve_approval_by_id(id, decision.is_allowed(), None)?;
            return Ok(());
        }

        Err(ClaudeCodeError::Protocol(format!(
            "No pending approval request found for request_id: '{request_id}'"
        )))
    }

    /// Registers a watchdog cancellation sender for backwards compatibility.
    pub fn register_watchdog_cancellation(
        &self,
        _approval_id: ApprovalId,
        _cancel_tx: tokio::sync::oneshot::Sender<()>,
    ) {
    }

    /// Checks if a watchdog task is actively tracked for an approval request.
    pub fn is_watchdog_active(&self, id: &ApprovalId) -> bool {
        let mut lock = match self.watchdog_handles.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        if let Some(handle) = lock.get(id) {
            if handle.is_finished() {
                lock.remove(id);
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    /// Prepares an approval resolution in a two-phase commit:
    /// Atomically transitions state from `Pending` to `Resolving`, aborts watchdog task,
    /// and generates formatted `ControlResponse` alongside a `ResolutionToken`.
    pub fn prepare_resolution(
        &self,
        approval_id: ApprovalId,
        approved: bool,
        reason: Option<String>,
    ) -> Result<(ControlResponse, ResolutionToken), ClaudeCodeError> {
        // Abort watchdog task immediately to prevent race
        if let Ok(mut lock) = self.watchdog_handles.lock()
            && let Some(handle) = lock.remove(&approval_id)
        {
            handle.abort();
        }

        let mut lock = self.approvals.write().expect("lock poisoned");
        let pending = lock.get_mut(&approval_id).ok_or_else(|| {
            ClaudeCodeError::ApprovalDenied(format!(
                "Approval request {approval_id} not found or already resolved"
            ))
        })?;

        match pending.state {
            ApprovalState::Pending => {}
            ApprovalState::TimedOut => {
                return Err(ClaudeCodeError::ApprovalDenied(format!(
                    "Approval request {approval_id} timed out"
                )));
            }
            ApprovalState::Cancelled => {
                return Err(ClaudeCodeError::ApprovalDenied(format!(
                    "Approval request {approval_id} not found or already resolved (cancelled)"
                )));
            }
            ApprovalState::DeliveryFailed => {
                return Err(ClaudeCodeError::ApprovalDenied(format!(
                    "Approval request {approval_id} delivery failed"
                )));
            }
            ApprovalState::Resolving | ApprovalState::Delivered => {
                return Err(ClaudeCodeError::ApprovalDenied(format!(
                    "Approval request {approval_id} not found or already resolved"
                )));
            }
        }

        let decision = if approved {
            ApprovalDecision::Allow
        } else {
            ApprovalDecision::Deny
        };

        pending.state = ApprovalState::Resolving;
        pending.decision = Some(decision);

        let control_response = if approved {
            ControlResponse::allow(pending.request_id.clone(), Some(pending.tool_input.clone()))
        } else {
            let message = reason.or_else(|| Some("Operator denied tool execution".to_string()));
            ControlResponse::deny(pending.request_id.clone(), message, Some(false))
        };

        let token = ResolutionToken {
            approval_id,
            request_id: pending.request_id.clone(),
            decision,
        };

        Ok((control_response, token))
    }

    /// Confirms successful delivery of the resolution to child process stdin.
    /// Transitions state from `Resolving` to `Delivered`.
    pub fn confirm_delivery(&self, token: ResolutionToken) -> Result<(), ClaudeCodeError> {
        let mut lock = self.approvals.write().expect("lock poisoned");
        let pending = lock.get_mut(&token.approval_id).ok_or_else(|| {
            ClaudeCodeError::ApprovalDenied(format!(
                "Approval request {} not found for delivery confirmation",
                token.approval_id
            ))
        })?;

        if pending.state != ApprovalState::Resolving {
            return Err(ClaudeCodeError::ApprovalDenied(format!(
                "Approval request {} in unexpected state {:?}; expected Resolving",
                token.approval_id, pending.state
            )));
        }

        pending.state = ApprovalState::Delivered;

        if let Ok(mut pending_oneshots) = self.pending_oneshots.lock()
            && let Some(tx) = pending_oneshots.remove(&token.request_id)
        {
            let _ = tx.send(token.decision);
        }

        info!(
            approval_id = %token.approval_id,
            request_id = %token.request_id,
            decision = ?token.decision,
            "Approval resolution successfully delivered"
        );

        Ok(())
    }

    /// Marks delivery to child process stdin as failed (fail-closed).
    /// Does NOT drop the pending record; marks state as `DeliveryFailed`.
    pub fn fail_delivery(&self, token: ResolutionToken) {
        let mut lock = self.approvals.write().expect("lock poisoned");
        if let Some(pending) = lock.get_mut(&token.approval_id) {
            pending.state = ApprovalState::DeliveryFailed;
            warn!(
                approval_id = %token.approval_id,
                request_id = %token.request_id,
                "Approval delivery to child stdin failed (fail-closed)"
            );
        }

        if let Ok(mut pending_oneshots) = self.pending_oneshots.lock()
            && let Some(tx) = pending_oneshots.remove(&token.request_id)
        {
            let _ = tx.send(ApprovalDecision::Deny);
        }
    }

    /// Resolves a pending approval by `ApprovalId` in a single combined operation (prepare + confirm).
    pub fn resolve_approval_by_id(
        &self,
        approval_id: ApprovalId,
        approved: bool,
        reason: Option<String>,
    ) -> Result<ControlResponse, ClaudeCodeError> {
        let (response, token) = self.prepare_resolution(approval_id, approved, reason)?;
        self.confirm_delivery(token)?;
        Ok(response)
    }

    /// Resolves an approval request on timeout, returning a formatted Deny `ControlResponse`.
    /// Fails closed: atomically marks the request `TimedOut` so it cannot be later approved.
    pub fn resolve_timeout_by_id(
        &self,
        approval_id: ApprovalId,
        timeout_message: Option<String>,
    ) -> Option<ControlResponse> {
        // Clean up handle from watchdog map without aborting the executing task
        if let Ok(mut lock) = self.watchdog_handles.lock() {
            lock.remove(&approval_id);
        }

        let mut lock = self.approvals.write().expect("lock poisoned");
        let pending = lock.get_mut(&approval_id)?;

        // Only transition to TimedOut if currently Pending
        if pending.state != ApprovalState::Pending {
            return None;
        }

        pending.state = ApprovalState::TimedOut;
        pending.decision = Some(ApprovalDecision::Deny);

        if let Ok(mut pending_oneshots) = self.pending_oneshots.lock()
            && let Some(tx) = pending_oneshots.remove(&pending.request_id)
        {
            let _ = tx.send(ApprovalDecision::Deny);
        }

        let message = timeout_message
            .or_else(|| Some("Approval timed out after 60s (fail-closed)".to_string()));

        Some(ControlResponse::deny(
            pending.request_id.clone(),
            message,
            Some(false),
        ))
    }

    /// Spawns an async timeout watchdog task for a registered approval request.
    ///
    /// If timeout elapses before host resolution:
    /// - Resolves approval fail-closed as Deny
    /// - Invokes `on_timeout` with the generated deny `ControlResponse`
    ///
    /// If host resolves approval before timeout, the watchdog is aborted.
    pub fn spawn_watchdog<F, Fut>(
        self: &Arc<Self>,
        approval_id: ApprovalId,
        timeout: Duration,
        on_timeout: F,
    ) -> tokio::task::JoinHandle<()>
    where
        F: FnOnce(ControlResponse) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let authority = Arc::clone(self);

        let inner_authority = Arc::clone(&authority);
        let inner_task = tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            let timeout_secs = timeout.as_secs();
            let timeout_desc = if timeout_secs > 0 {
                format!("{timeout_secs}s")
            } else {
                format!("{}ms", timeout.as_millis())
            };
            let deny_msg = format!("Approval timed out after {timeout_desc} (fail-closed)");
            if let Some(deny_resp) =
                inner_authority.resolve_timeout_by_id(approval_id, Some(deny_msg))
            {
                on_timeout(deny_resp).await;
            }
            let _ = done_tx.send(());
        });

        if let Ok(mut lock) = self.watchdog_handles.lock()
            && self.is_pending_id(&approval_id)
            && !inner_task.is_finished()
        {
            lock.insert(approval_id, inner_task);
        }

        tokio::spawn(async move {
            let _ = done_rx.await;
        })
    }

    /// Cancels all pending approval requests, failing them closed to Deny,
    /// and aborts all active watchdog join handles.
    pub fn cancel_all_pending(&self) {
        if let Ok(mut pending) = self.pending_oneshots.lock() {
            for (_, tx) in pending.drain() {
                let _ = tx.send(ApprovalDecision::Deny);
            }
        }
        if let Ok(mut handles) = self.watchdog_handles.lock() {
            for (_, handle) in handles.drain() {
                handle.abort();
            }
        }
        let mut lock = self.approvals.write().expect("lock poisoned");
        for approval in lock.values_mut() {
            if approval.state == ApprovalState::Pending
                || approval.state == ApprovalState::Resolving
            {
                approval.state = ApprovalState::Cancelled;
                approval.decision = Some(ApprovalDecision::Deny);
            }
        }
    }

    /// Cancels an individual approval by ApprovalId fail-closed to Deny.
    pub fn cancel_approval_by_id(&self, approval_id: ApprovalId) -> Result<(), ClaudeCodeError> {
        if let Ok(mut lock) = self.watchdog_handles.lock()
            && let Some(handle) = lock.remove(&approval_id)
        {
            handle.abort();
        }

        let mut lock = self.approvals.write().expect("lock poisoned");
        let pending = lock.get_mut(&approval_id).ok_or_else(|| {
            ClaudeCodeError::ApprovalDenied(format!("Approval request {approval_id} not found"))
        })?;

        if pending.state.is_terminal() {
            return Ok(());
        }

        pending.state = ApprovalState::Cancelled;
        pending.decision = Some(ApprovalDecision::Deny);

        if let Ok(mut pending_oneshots) = self.pending_oneshots.lock()
            && let Some(tx) = pending_oneshots.remove(&pending.request_id)
        {
            let _ = tx.send(ApprovalDecision::Deny);
        }

        Ok(())
    }

    /// Returns the number of currently pending/resolving approval requests.
    pub fn pending_count(&self) -> usize {
        let oneshot_count = self.pending_oneshots.lock().map(|p| p.len()).unwrap_or(0);
        let id_count = self
            .approvals
            .read()
            .map(|p| {
                p.values()
                    .filter(|a| {
                        a.state == ApprovalState::Pending || a.state == ApprovalState::Resolving
                    })
                    .count()
            })
            .unwrap_or(0);
        oneshot_count.max(id_count)
    }

    /// Returns true if an approval request for the given request_id is currently pending.
    pub fn is_pending(&self, request_id: &str) -> bool {
        let in_oneshot = self
            .pending_oneshots
            .lock()
            .map(|p| p.contains_key(request_id))
            .unwrap_or(false);
        if in_oneshot {
            return true;
        }

        let approval_id = {
            let map = self.request_to_approval.read().expect("lock poisoned");
            map.get(request_id).copied()
        };

        if let Some(id) = approval_id {
            self.is_pending_id(&id)
        } else {
            let derived = self.derive_id(request_id);
            self.is_pending_id(&derived)
        }
    }

    /// Checks if a specific ApprovalId is pending or resolving.
    pub fn is_pending_id(&self, id: &ApprovalId) -> bool {
        let lock = self.approvals.read().expect("lock poisoned");
        lock.get(id)
            .map(|a| a.state == ApprovalState::Pending || a.state == ApprovalState::Resolving)
            .unwrap_or(false)
    }

    /// Retrieves a clone of the recorded approval request by ID.
    pub fn get_approval(&self, id: &ApprovalId) -> Option<PendingApproval> {
        let lock = self.approvals.read().ok()?;
        lock.get(id).cloned()
    }

    /// Retrieves the current state of an approval request by ID.
    pub fn get_approval_state(&self, id: &ApprovalId) -> Option<ApprovalState> {
        let lock = self.approvals.read().ok()?;
        lock.get(id).map(|a| a.state)
    }

    /// Maps a wire request_id to its registered ApprovalId.
    pub fn get_approval_id_by_request_id(&self, request_id: &str) -> Option<ApprovalId> {
        let lock = self.request_to_approval.read().ok()?;
        lock.get(request_id).copied()
    }
}

/// Validates a budget ceiling amount in USD.
/// Rejects negative amounts, NaN, and infinite values.
pub fn validate_max_budget_usd(amount: f64) -> Result<f64, ClaudeCodeError> {
    if amount.is_nan() {
        return Err(ClaudeCodeError::InvalidBudget(
            "Budget ceiling cannot be NaN".into(),
        ));
    }
    if amount < 0.0 {
        return Err(ClaudeCodeError::InvalidBudget(format!(
            "Budget ceiling cannot be negative: {amount}"
        )));
    }
    if amount.is_infinite() {
        return Err(ClaudeCodeError::InvalidBudget(
            "Budget ceiling cannot be infinite".into(),
        ));
    }
    Ok(amount)
}

/// Parses and validates a `--max-budget-usd` argument string.
/// Rejects non-numeric, negative, NaN, and infinite inputs.
pub fn parse_max_budget_usd(val: &str) -> Result<f64, ClaudeCodeError> {
    let trimmed = val.trim();
    if trimmed.is_empty() {
        return Err(ClaudeCodeError::InvalidBudget(
            "Budget ceiling string is empty".into(),
        ));
    }
    let parsed: f64 = trimmed.parse().map_err(|e| {
        ClaudeCodeError::InvalidBudget(format!("Invalid budget ceiling format '{trimmed}': {e}"))
    })?;
    validate_max_budget_usd(parsed)
}

/// Sanitizes diagnostic logs, error messages, and debug outputs by redacting
/// credentials, authorization headers, bearer tokens, and API keys.
pub fn sanitize_diagnostic(msg: &str) -> String {
    agent_studios_external_runtime::sanitize_error_message(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CanUseToolRequest as ProtocolCanUseToolRequest;

    fn sample_control_request(req_id: &str, tool: &str) -> ControlRequest {
        ControlRequest {
            request_id: req_id.to_string(),
            request: ControlRequestPayload::CanUseTool(ProtocolCanUseToolRequest {
                tool_name: tool.to_string(),
                input: serde_json::json!({"cmd": "echo test"}),
                tool_use_id: Some("call_01".to_string()),
                agent_id: None,
                decision_reason: Some("Testing tool".to_string()),
                blocked_path: None,
                default_to_no: Some(false),
                suggestions: None,
            }),
        }
    }

    #[test]
    fn test_scoped_approval_id_prevents_cross_session_collision() {
        let id_session_a = ApprovalAuthority::derive_scoped_approval_id("session-a", "req_1");
        let id_session_b = ApprovalAuthority::derive_scoped_approval_id("session-b", "req_1");

        assert_ne!(
            id_session_a, id_session_b,
            "ApprovalId must be scoped to session namespace to avoid cross-session collisions"
        );

        let id_session_a_again = ApprovalAuthority::derive_scoped_approval_id("session-a", "req_1");
        assert_eq!(id_session_a, id_session_a_again);
    }

    #[test]
    fn test_duplicate_request_id_in_session_idempotent() {
        let authority = ApprovalAuthority::with_session_id("test-session-dup");
        let req1 = sample_control_request("req_unique_01", "Bash");
        let req1_dup = sample_control_request("req_unique_01", "Bash");

        let id1 = authority.register_request(&req1);
        assert!(id1.is_some(), "First request registration must succeed");

        let id2 = authority.register_request(&req1_dup);
        assert_eq!(
            id1, id2,
            "Duplicate request_id in same session must return existing id idempotently"
        );

        assert_eq!(authority.pending_count(), 1);
    }

    #[test]
    fn test_two_phase_commit_delivery_success() {
        let authority = ApprovalAuthority::with_session_id("test-session-2pc");
        let req = sample_control_request("req_2pc_01", "Bash");
        let approval_id = authority.register_request(&req).unwrap();

        assert_eq!(
            authority.get_approval_state(&approval_id),
            Some(ApprovalState::Pending)
        );
        assert!(authority.is_pending("req_2pc_01"));

        // Phase 1: Prepare
        let (control_response, token) = authority
            .prepare_resolution(approval_id, true, None)
            .expect("prepare_resolution should succeed");

        assert_eq!(control_response.request_id(), "req_2pc_01");
        assert_eq!(
            authority.get_approval_state(&approval_id),
            Some(ApprovalState::Resolving)
        );
        assert!(authority.is_pending("req_2pc_01"));

        // Phase 2: Confirm
        authority
            .confirm_delivery(token)
            .expect("confirm_delivery should succeed");

        assert_eq!(
            authority.get_approval_state(&approval_id),
            Some(ApprovalState::Delivered)
        );
        assert_eq!(authority.pending_count(), 0);
        assert!(!authority.is_pending("req_2pc_01"));

        // Late re-resolution must fail
        assert!(
            authority
                .prepare_resolution(approval_id, true, None)
                .is_err()
        );
    }

    #[test]
    fn test_two_phase_commit_delivery_failure_retains_record_and_fails_closed() {
        let authority = ApprovalAuthority::with_session_id("test-session-fail");
        let req = sample_control_request("req_fail_01", "Bash");
        let approval_id = authority.register_request(&req).unwrap();

        // Phase 1: Prepare
        let (_resp, token) = authority
            .prepare_resolution(approval_id, true, None)
            .expect("prepare_resolution should succeed");

        // Phase 2: Delivery fails
        authority.fail_delivery(token);

        // Record must NOT be dropped or treated as approved!
        let record = authority.get_approval(&approval_id);
        assert!(
            record.is_some(),
            "Failed delivery must retain the approval record"
        );
        let unwrapped = record.unwrap();
        assert_eq!(unwrapped.state, ApprovalState::DeliveryFailed);
        assert_eq!(
            authority.get_approval_state(&approval_id),
            Some(ApprovalState::DeliveryFailed)
        );

        // Not pending, cannot be approved later (fail-closed)
        assert_eq!(authority.pending_count(), 0);
        assert!(!authority.is_pending("req_fail_01"));
        let late_res = authority.prepare_resolution(approval_id, true, None);
        assert!(
            late_res.is_err(),
            "Failed delivery approval cannot be re-resolved"
        );
    }

    #[tokio::test]
    async fn test_timeout_racing_with_host_resolution_atomic() {
        let authority = Arc::new(ApprovalAuthority::with_session_id("test-session-race"));
        let req = sample_control_request("req_race_01", "Bash");
        let approval_id = authority.register_request(&req).unwrap();

        // 1. If timeout resolves first:
        let deny_resp = authority
            .resolve_timeout_by_id(approval_id, Some("Timeout fired".to_string()))
            .expect("resolve_timeout_by_id should succeed for pending request");
        assert_eq!(deny_resp.request_id(), "req_race_01");

        // Host resolution attempt after timeout must receive ApprovalDenied with 'timed out'
        let host_res = authority.prepare_resolution(approval_id, true, None);
        assert!(host_res.is_err());
        match host_res.err().unwrap() {
            ClaudeCodeError::ApprovalDenied(msg) => {
                assert!(
                    msg.contains("timed out"),
                    "Error must state that request timed out: '{msg}'"
                );
            }
            other => panic!("Expected ApprovalDenied with 'timed out', got: {other:?}"),
        }

        // 2. Second request where host resolves first:
        let req2 = sample_control_request("req_race_02", "Bash");
        let approval_id2 = authority.register_request(&req2).unwrap();

        let (_resp, token) = authority
            .prepare_resolution(approval_id2, true, None)
            .expect("prepare_resolution should succeed");

        // Watchdog timeout firing after host resolution must return None (no double resolve)
        let wd_res = authority.resolve_timeout_by_id(approval_id2, None);
        assert!(
            wd_res.is_none(),
            "Watchdog cannot double-resolve already resolving/delivered request"
        );

        authority.confirm_delivery(token).unwrap();
        assert_eq!(
            authority.get_approval_state(&approval_id2),
            Some(ApprovalState::Delivered)
        );
    }

    #[tokio::test]
    async fn test_watchdog_handles_abort_and_cleanup() {
        let authority = Arc::new(ApprovalAuthority::with_session_id("test-session-wd"));
        let req = sample_control_request("req_wd_01", "Bash");
        let approval_id = authority.register_request(&req).unwrap();

        let (timeout_tx, mut timeout_rx) = tokio::sync::oneshot::channel();
        let handle = authority.spawn_watchdog(
            approval_id,
            Duration::from_millis(150),
            move |deny_resp| async move {
                let _ = timeout_tx.send(deny_resp);
            },
        );

        assert!(authority.is_watchdog_active(&approval_id));

        // Host resolves before timeout
        let _ = authority
            .resolve_approval_by_id(approval_id, true, None)
            .unwrap();

        assert!(!authority.is_watchdog_active(&approval_id));

        // Wait for handle to complete
        let _ = handle.await;

        // Callback must not have fired
        assert!(timeout_rx.try_recv().is_err());
    }

    #[test]
    fn test_cancel_all_pending_retains_records_as_cancelled() {
        let authority = ApprovalAuthority::with_session_id("test-session-cancel");
        let req1 = sample_control_request("req_c_01", "Bash");
        let req2 = sample_control_request("req_c_02", "Write");

        let id1 = authority.register_request(&req1).unwrap();
        let id2 = authority.register_request(&req2).unwrap();

        assert_eq!(authority.pending_count(), 2);

        authority.cancel_all_pending();

        assert_eq!(authority.pending_count(), 0);
        assert_eq!(
            authority.get_approval_state(&id1),
            Some(ApprovalState::Cancelled)
        );
        assert_eq!(
            authority.get_approval_state(&id2),
            Some(ApprovalState::Cancelled)
        );

        // Records still exist
        assert!(authority.get_approval(&id1).is_some());
        assert!(authority.get_approval(&id2).is_some());

        // Resolving cancelled requests fails
        assert!(authority.prepare_resolution(id1, true, None).is_err());
        assert!(authority.prepare_resolution(id2, true, None).is_err());
    }
}
