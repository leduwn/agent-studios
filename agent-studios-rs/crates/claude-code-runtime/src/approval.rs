use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use agent_studios_protocol::id::ApprovalId;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::ClaudeCodeError;
use crate::protocol::{ControlDecision, ControlRequest, ControlRequestPayload, ControlResponse};

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
        serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request_id,
                "response": {
                    "behavior": self.behavior()
                }
            }
        })
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
    pub fn from_protocol(request_id: impl Into<String>, req: &crate::protocol::CanUseToolRequest) -> Self {
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
            .ok_or_else(|| ClaudeCodeError::Protocol("control_request missing request_id".into()))?;

        let request = val
            .get("request")
            .ok_or_else(|| ClaudeCodeError::Protocol("control_request missing request field".into()))?;

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
#[derive(Clone, Debug)]
pub struct PendingApproval {
    pub approval_id: ApprovalId,
    pub request_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_use_id: Option<String>,
    pub agent_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Thread-safe host approval coordinator for `--permission-prompts=host` workflows.
/// Manages in-flight pre-execution tool approval requests, deterministic ID derivation,
/// and fail-closed timeout/cancellation semantics.
#[derive(Debug, Default)]
pub struct ApprovalAuthority {
    pending_oneshots: Mutex<HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>>,
    pending_by_id: RwLock<BTreeMap<ApprovalId, PendingApproval>>,
}

impl ApprovalAuthority {
    pub fn new() -> Self {
        Self {
            pending_oneshots: Mutex::new(HashMap::new()),
            pending_by_id: RwLock::new(BTreeMap::new()),
        }
    }

    /// Derives a deterministic `ApprovalId` from a wire `request_id`.
    pub fn derive_approval_id(request_id: &str) -> ApprovalId {
        let uuid = Uuid::new_v5(&Uuid::NAMESPACE_DNS, request_id.as_bytes());
        ApprovalId::from_uuid(uuid)
    }

    /// Registers a pending control request received from Claude Code stdout.
    pub fn register_request(&self, request: &ControlRequest) -> Option<ApprovalId> {
        let payload = match &request.request {
            ControlRequestPayload::CanUseTool(can_use) => can_use,
            _ => return None,
        };

        let approval_id = Self::derive_approval_id(&request.request_id);
        let pending = PendingApproval {
            approval_id,
            request_id: request.request_id.clone(),
            tool_name: payload.tool_name.clone(),
            tool_input: payload.input.clone(),
            tool_use_id: payload.tool_use_id.clone(),
            agent_id: payload.agent_id.clone(),
            created_at: chrono::Utc::now(),
        };

        debug!(
            approval_id = %approval_id,
            request_id = %request.request_id,
            tool_name = %payload.tool_name,
            "Registered tool approval request"
        );

        let mut lock = self.pending_by_id.write().expect("lock poisoned");
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
            let mut pending = self.pending_oneshots.lock().map_err(|e| {
                ClaudeCodeError::Internal(format!("Approval mutex poisoned: {e}"))
            })?;
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
            let mut pending = self.pending_oneshots.lock().map_err(|e| {
                ClaudeCodeError::Internal(format!("Approval mutex poisoned: {e}"))
            })?;
            pending.remove(request_id)
        };

        match sender {
            Some(tx) => {
                let _ = tx.send(decision);
                Ok(())
            }
            None => Err(ClaudeCodeError::Protocol(format!(
                "No pending approval request found for request_id: '{request_id}'"
            ))),
        }
    }

    /// Resolves a pending approval by `ApprovalId` with an operator decision (allow / deny).
    pub fn resolve_approval_by_id(
        &self,
        approval_id: ApprovalId,
        approved: bool,
        reason: Option<String>,
    ) -> Result<ControlResponse, ClaudeCodeError> {
        let pending = {
            let mut lock = self.pending_by_id.write().expect("lock poisoned");
            lock.remove(&approval_id).ok_or_else(|| {
                ClaudeCodeError::ApprovalDenied(format!(
                    "Approval request {approval_id} not found or already resolved"
                ))
            })?
        };

        let decision = if approved {
            ControlDecision::Allow {
                updated_input: Some(pending.tool_input),
            }
        } else {
            ControlDecision::Deny {
                message: reason.or_else(|| Some("Operator denied tool execution".to_string())),
                interrupt: Some(false),
            }
        };

        Ok(ControlResponse {
            r#type: "control_response".to_string(),
            request_id: pending.request_id,
            response: decision,
        })
    }

    /// Cancels all pending approval requests, failing them closed to Deny.
    pub fn cancel_all_pending(&self) {
        if let Ok(mut pending) = self.pending_oneshots.lock() {
            for (_, tx) in pending.drain() {
                let _ = tx.send(ApprovalDecision::Deny);
            }
        }
        let mut lock = self.pending_by_id.write().expect("lock poisoned");
        if !lock.is_empty() {
            warn!(count = lock.len(), "Cancelling all pending approvals fail-closed");
            lock.clear();
        }
    }

    /// Returns the number of currently pending approval requests.
    pub fn pending_count(&self) -> usize {
        let oneshot_count = self
            .pending_oneshots
            .lock()
            .map(|p| p.len())
            .unwrap_or(0);
        let id_count = self.pending_by_id.read().map(|p| p.len()).unwrap_or(0);
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
        let approval_id = Self::derive_approval_id(request_id);
        self.pending_by_id
            .read()
            .map(|p| p.contains_key(&approval_id))
            .unwrap_or(false)
    }

    /// Checks if a specific ApprovalId is pending.
    pub fn is_pending_id(&self, id: &ApprovalId) -> bool {
        let lock = self.pending_by_id.read().expect("lock poisoned");
        lock.contains_key(id)
    }
}

/// Validates a budget ceiling amount in USD.
/// Rejects negative amounts, NaN, and infinite values.
pub fn validate_max_budget_usd(amount: f64) -> Result<f64, ClaudeCodeError> {
    if amount.is_nan() {
        return Err(ClaudeCodeError::Protocol(
            "Budget ceiling cannot be NaN".into(),
        ));
    }
    if amount < 0.0 {
        return Err(ClaudeCodeError::Protocol(format!(
            "Budget ceiling cannot be negative: {amount}"
        )));
    }
    if amount.is_infinite() {
        return Err(ClaudeCodeError::Protocol(
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
        return Err(ClaudeCodeError::Protocol(
            "Budget ceiling string is empty".into(),
        ));
    }
    let parsed: f64 = trimmed.parse().map_err(|e| {
        ClaudeCodeError::Protocol(format!(
            "Invalid budget ceiling format '{trimmed}': {e}"
        ))
    })?;
    validate_max_budget_usd(parsed)
}

/// Sanitizes diagnostic logs, error messages, and debug outputs by redacting
/// credentials, authorization headers, bearer tokens, and API keys.
pub fn sanitize_diagnostic(msg: &str) -> String {
    agent_studios_external_runtime::sanitize_error_message(msg)
}
