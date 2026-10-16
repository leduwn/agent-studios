use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use url::Url;

use crate::error::OpenCodeError;

/// Sanitizes a URL by stripping embedded username and password before display/logging.
pub fn sanitize_url(url: &Url) -> String {
    let mut safe = url.clone();
    let _ = safe.set_username("");
    let _ = safe.set_password(None);
    safe.to_string()
}

/// Request payload to create an OpenCode session.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateSessionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}

impl CreateSessionRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_directory(mut self, directory: impl Into<String>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Response payload from session creation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeSessionResponse {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// A structured message part in OpenCode's message envelope.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessagePart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: String,
}

impl MessagePart {
    pub fn text(t: impl Into<String>) -> Self {
        Self {
            part_type: "text".to_string(),
            text: t.into(),
        }
    }
}

/// Nested model specification aligned with upstream OpenCode API.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSpec {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
}

impl ModelSpec {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }
}

/// Request payload for sending input or starting a turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenCodeMessageRequest {
    #[serde(default)]
    pub parts: Vec<MessagePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl OpenCodeMessageRequest {
    pub fn from_content(content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            parts: vec![MessagePart::text(&text)],
            model: None,
            content: Some(text),
            provider: None,
        }
    }

    pub fn with_parts(parts: Vec<MessagePart>) -> Self {
        let content = parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            parts,
            model: None,
            content: Some(content),
            provider: None,
        }
    }

    pub fn with_model(
        mut self,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        self.model = Some(ModelSpec {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        });
        self
    }
}

/// Request payload for dedicated permission decision responses.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDecisionRequest {
    pub granted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PermissionDecisionRequest {
    pub fn new(granted: bool) -> Self {
        Self {
            granted,
            approved: Some(granted),
            reason: None,
        }
    }

    pub fn with_reason(granted: bool, reason: impl Into<String>) -> Self {
        Self {
            granted,
            approved: Some(granted),
            reason: Some(reason.into()),
        }
    }

    pub fn is_granted(&self) -> bool {
        self.granted || self.approved.unwrap_or(false)
    }
}

fn default_true() -> bool {
    true
}

/// Response payload from sending a message.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodePromptResponse {
    #[serde(default = "default_true")]
    pub success: bool,
    #[serde(default, rename = "message_id", alias = "id", alias = "messageID")]
    pub message_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

impl Default for OpenCodePromptResponse {
    fn default() -> Self {
        Self {
            success: true,
            message_id: None,
            status: None,
        }
    }
}

fn default_status() -> String {
    "unknown".to_string()
}

/// Session status query response.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeStatusResponse {
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub is_running: bool,
    #[serde(default)]
    pub id: Option<String>,
}

impl Default for OpenCodeStatusResponse {
    fn default() -> Self {
        Self {
            status: default_status(),
            is_running: false,
            id: None,
        }
    }
}

/// Typed HTTP client for interacting with the OpenCode HTTP server.
#[derive(Clone)]
pub struct OpenCodeHttpClient {
    base_url: Url,
    username: String,
    password: String,
    client: reqwest::Client,
}

impl fmt::Debug for OpenCodeHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenCodeHttpClient")
            .field("base_url", &sanitize_url(&self.base_url))
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

impl OpenCodeHttpClient {
    pub fn new(base_url: Url, auth_token: impl Into<String>) -> Result<Self, OpenCodeError> {
        Self::with_credentials(base_url, "opencode", auth_token)
    }

    pub fn with_credentials(
        base_url: Url,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, OpenCodeError> {
        let username = username.into();
        let password = password.into();

        let mut headers = HeaderMap::new();
        let creds = format!("{username}:{password}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
        let auth_header_val = format!("Basic {encoded}");
        let header_val = HeaderValue::from_str(&auth_header_val).map_err(|e| {
            OpenCodeError::InvalidConfiguration(format!("Invalid auth header: {e}"))
        })?;
        headers.insert(AUTHORIZATION, header_val);

        // Security: enforce Policy::none() to prevent forwarding Basic Auth credentials
        // across redirects, hosts, or unexpected scheme downgrades.
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        Ok(Self {
            base_url,
            username,
            password,
            client,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn password(&self) -> &str {
        &self.password
    }

    pub fn auth_token(&self) -> &str {
        &self.password
    }

    /// Checks server health at `/global/health`.
    pub async fn health_check(&self) -> Result<(), OpenCodeError> {
        let url = self
            .base_url
            .join("/global/health")
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Health check request failed: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::HealthCheckFailed {
                url: sanitize_url(&url),
                status: Some(status.as_u16()),
                reason: "Unexpected HTTP redirect; refusing to forward credentials".to_string(),
            });
        }

        if status.is_success() {
            Ok(())
        } else {
            Err(OpenCodeError::HealthCheckFailed {
                url: sanitize_url(&url),
                status: Some(status.as_u16()),
                reason: format!("Server returned status {status}"),
            })
        }
    }

    /// Creates a new session via `POST /session`.
    pub async fn create_session(
        &self,
        req: CreateSessionRequest,
    ) -> Result<OpenCodeSessionResponse, OpenCodeError> {
        let url = self
            .base_url
            .join("/session")
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .post(url.clone())
            .json(&req)
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Session creation request failed: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on create_session ({status}); refusing to follow"
            )));
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: "Authentication failed: invalid OPENCODE_SERVER_PASSWORD or credentials"
                    .to_string(),
            });
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let sanitized_summary = if body.len() > 256 {
                format!("{}...", &body[..256])
            } else {
                body
            };
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!("Failed to create session (status {status}): {sanitized_summary}"),
            });
        }

        let parsed: OpenCodeSessionResponse = resp.json().await.map_err(|e| {
            OpenCodeError::InvalidEventPayload(format!(
                "Failed to parse session creation response: {e}"
            ))
        })?;

        if parsed.id.trim().is_empty() {
            return Err(OpenCodeError::InvalidEventPayload(
                "OpenCode session response contained empty session ID".to_string(),
            ));
        }

        Ok(parsed)
    }

    /// Sends a prompt turn or message via `POST /session/:id/message`.
    pub async fn send_message(
        &self,
        session_id: &str,
        req: OpenCodeMessageRequest,
    ) -> Result<OpenCodePromptResponse, OpenCodeError> {
        let clean_sid = session_id.trim();
        if clean_sid.is_empty() {
            return Err(OpenCodeError::SessionNotFound(
                "Session ID must not be empty".to_string(),
            ));
        }

        let path = format!("/session/{clean_sid}/message");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        // Ensure structured parts are populated from content if empty
        let mut final_req = req;
        if final_req.parts.is_empty() {
            final_req.parts = final_req
                .content
                .as_deref()
                .map(MessagePart::text)
                .into_iter()
                .collect();
        }

        let resp = self
            .client
            .post(url)
            .json(&final_req)
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Send message request failed: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on send_message ({status}); refusing to follow"
            )));
        }

        if status.as_u16() == 404 {
            return Err(OpenCodeError::SessionNotFound(clean_sid.to_string()));
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: "Authentication failed for send_message".to_string(),
            });
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let sanitized_summary = if body.len() > 256 {
                format!("{}...", &body[..256])
            } else {
                body
            };
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!(
                    "Failed to send message to session {clean_sid} (status {status}): {sanitized_summary}"
                ),
            });
        }

        let resp_bytes = resp
            .bytes()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Failed to read response body: {e}")))?;

        if resp_bytes.is_empty() {
            return Ok(OpenCodePromptResponse {
                success: true,
                message_id: None,
                status: Some("accepted".to_string()),
            });
        }

        serde_json::from_slice::<OpenCodePromptResponse>(&resp_bytes).map_err(|e| {
            OpenCodeError::InvalidEventPayload(format!(
                "Failed to parse prompt response schema: {e}"
            ))
        })
    }

    /// Dispatches a dedicated permission approval/rejection decision via `POST /session/:id/permission/:permission_id`.
    pub async fn respond_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        req: PermissionDecisionRequest,
    ) -> Result<(), OpenCodeError> {
        let clean_sid = session_id.trim();
        let clean_pid = permission_id.trim();
        if clean_sid.is_empty() || clean_pid.is_empty() {
            return Err(OpenCodeError::Permission(
                "Session ID and permission ID must not be empty".to_string(),
            ));
        }

        let path = format!("/session/{clean_sid}/permission/{clean_pid}");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp =
            self.client.post(url).json(&req).send().await.map_err(|e| {
                OpenCodeError::Http(format!("Permission dispatch request failed: {e}"))
            })?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on respond_permission ({status}); refusing to follow"
            )));
        }

        if status.as_u16() == 404 {
            return Err(OpenCodeError::SessionNotFound(format!(
                "Session {clean_sid} or permission {clean_pid} not found"
            )));
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: "Authentication failed for respond_permission".to_string(),
            });
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let sanitized_summary = if body.len() > 256 {
                format!("{}...", &body[..256])
            } else {
                body
            };
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!(
                    "Permission dispatch failed for session {clean_sid}, permission {clean_pid} (status {status}): {sanitized_summary}"
                ),
            });
        }

        Ok(())
    }

    /// Interrupts active execution turn via `POST /session/:id/abort`.
    pub async fn abort_session(&self, session_id: &str) -> Result<(), OpenCodeError> {
        let clean_sid = session_id.trim();
        if clean_sid.is_empty() {
            return Err(OpenCodeError::SessionNotFound(
                "Session ID must not be empty".to_string(),
            ));
        }

        let path = format!("/session/{clean_sid}/abort");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .post(url)
            .json(&serde_json::json!({ "reason": "user_interrupt" }))
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Abort session request failed: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on abort_session ({status}); refusing to follow"
            )));
        }

        if status.as_u16() == 404 {
            return Err(OpenCodeError::SessionNotFound(clean_sid.to_string()));
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!("Failed to abort session {clean_sid} (status {status}): {body}"),
            });
        }

        Ok(())
    }

    /// Queries session status via `GET /session/:id`.
    pub async fn get_session_status(
        &self,
        session_id: &str,
    ) -> Result<OpenCodeStatusResponse, OpenCodeError> {
        let clean_sid = session_id.trim();
        if clean_sid.is_empty() {
            return Err(OpenCodeError::SessionNotFound(
                "Session ID must not be empty".to_string(),
            ));
        }

        let path = format!("/session/{clean_sid}");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp =
            self.client.get(url).send().await.map_err(|e| {
                OpenCodeError::Http(format!("Get session status request failed: {e}"))
            })?;

        let status = resp.status();
        if status.as_u16() == 404 {
            return Err(OpenCodeError::SessionNotFound(clean_sid.to_string()));
        }

        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on get_session_status ({status}); refusing to follow"
            )));
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!(
                    "Failed to get status for session {clean_sid} (status {status}): {body}"
                ),
            });
        }

        resp.json::<OpenCodeStatusResponse>().await.map_err(|e| {
            OpenCodeError::InvalidEventPayload(format!("Failed to parse status response: {e}"))
        })
    }

    /// Opens an SSE connection to `/event` for streaming events.
    pub async fn open_event_stream(&self) -> Result<reqwest::Response, OpenCodeError> {
        let url = self
            .base_url
            .join("/event")
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .get(url)
            .header(ACCEPT, "text/event-stream")
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(format!("Open SSE event stream failed: {e}")))?;

        let status = resp.status();
        if status.is_redirection() {
            return Err(OpenCodeError::Http(format!(
                "Unexpected HTTP redirect on open_event_stream ({status}); refusing to follow"
            )));
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: "Authentication failed: invalid OPENCODE_SERVER_PASSWORD for SSE stream"
                    .to_string(),
            });
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::ApiError {
                status: status.as_u16(),
                message: format!("Failed to open SSE stream (status {status}): {body}"),
            });
        }

        Ok(resp)
    }
}
