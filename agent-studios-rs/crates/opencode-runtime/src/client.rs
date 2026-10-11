use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

use crate::error::OpenCodeError;

/// Request payload to create an OpenCode session.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateSessionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
}

/// Response payload from session creation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenCodeSessionResponse {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// Request payload for sending input or starting a turn.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenCodeMessageRequest {
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

/// Response payload from sending message.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OpenCodePromptResponse {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub message_id: Option<String>,
}

/// Session status query response.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct OpenCodeStatusResponse {
    pub status: String,
    #[serde(default)]
    pub is_running: bool,
}

/// Typed HTTP client for interacting with the OpenCode HTTP server.
#[derive(Clone)]
pub struct OpenCodeHttpClient {
    base_url: Url,
    auth_token: String,
    client: reqwest::Client,
}

impl OpenCodeHttpClient {
    pub fn new(base_url: Url, auth_token: impl Into<String>) -> Result<Self, OpenCodeError> {
        let auth_token = auth_token.into();
        let mut headers = HeaderMap::new();
        let auth_header_val = format!("Bearer {auth_token}");
        if let Ok(val) = HeaderValue::from_str(&auth_header_val) {
            headers.insert(AUTHORIZATION, val);
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        Ok(Self {
            base_url,
            auth_token,
            client,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub fn auth_token(&self) -> &str {
        &self.auth_token
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
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(OpenCodeError::HealthCheckFailed {
                url: url.to_string(),
                status: Some(resp.status().as_u16()),
                reason: format!("Server returned status {}", resp.status()),
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
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::Http(format!(
                "Failed to create session (status {status}): {body}"
            )));
        }

        resp.json::<OpenCodeSessionResponse>().await.map_err(|e| {
            OpenCodeError::InvalidEventPayload(format!("Failed to parse session response: {e}"))
        })
    }

    /// Sends a prompt turn or message via `POST /session/:id/message`.
    pub async fn send_message(
        &self,
        session_id: &str,
        req: OpenCodeMessageRequest,
    ) -> Result<OpenCodePromptResponse, OpenCodeError> {
        let path = format!("/session/{session_id}/message");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::Http(format!(
                "Failed to send message to session {session_id} (status {status}): {body}"
            )));
        }

        // Parse optional response or default success
        match resp.json::<OpenCodePromptResponse>().await {
            Ok(parsed) => Ok(parsed),
            Err(_) => Ok(OpenCodePromptResponse {
                success: true,
                message_id: None,
            }),
        }
    }

    /// Interrupts active execution turn via `POST /session/:id/abort`.
    pub async fn abort_session(&self, session_id: &str) -> Result<(), OpenCodeError> {
        let path = format!("/session/{session_id}/abort");
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
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::Http(format!(
                "Failed to abort session {session_id} (status {status}): {body}"
            )));
        }

        Ok(())
    }

    /// Queries session status via `GET /session/:id`.
    pub async fn get_session_status(
        &self,
        session_id: &str,
    ) -> Result<OpenCodeStatusResponse, OpenCodeError> {
        let path = format!("/session/{session_id}");
        let url = self
            .base_url
            .join(&path)
            .map_err(|e| OpenCodeError::InvalidConfiguration(e.to_string()))?;

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if resp.status().as_u16() == 404 {
            return Err(OpenCodeError::SessionNotFound(session_id.to_string()));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::Http(format!(
                "Failed to get status for session {session_id} (status {status}): {body}"
            )));
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
            .map_err(|e| OpenCodeError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(OpenCodeError::Http(format!(
                "Failed to open SSE stream (status {status}): {body}"
            )));
        }

        Ok(resp)
    }
}
