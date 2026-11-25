//! Model Context Protocol (MCP) Configuration, Discovery, Invocation & Secret Redaction (M16)

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use agent_studios_ipc_protocol::SecretRedactor;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpRiskTier {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    Http {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub risk_tier: McpRiskTier,
    pub transport: McpTransportConfig,
    #[serde(skip_deserializing, default = "default_scope")]
    pub scope: McpScope,
}

fn default_enabled() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_scope() -> McpScope {
    McpScope::Global
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct McpConfigFile {
    #[serde(default)]
    pub servers: BTreeMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    pub server_id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolCallResult {
    pub server_id: String,
    pub tool_name: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpInitializeResult {
    pub server_id: String,
    pub protocol_version: String,
    pub server_name: Option<String>,
    pub capabilities: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceDescriptor {
    pub server_id: String,
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceContents {
    pub server_id: String,
    pub uri: String,
    pub text: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptDescriptor {
    pub server_id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptResult {
    pub server_id: String,
    pub description: Option<String>,
    pub messages: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpInvocationDecision {
    Allowed,
    RequiresApproval {
        server_id: String,
        tool_name: String,
        risk_tier: McpRiskTier,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpError {
    #[error("MCP config error: {0}")]
    Config(String),
    #[error("MCP server not found or disabled: {0}")]
    ServerUnavailable(String),
    #[error("MCP approval required for {server_id}:{tool_name} (tier: {risk_tier:?})")]
    ApprovalRequired {
        server_id: String,
        tool_name: String,
        risk_tier: McpRiskTier,
    },
    #[error("MCP transport error: {0}")]
    Transport(String),
    #[error("MCP timeout after {0}s")]
    Timeout(u64),
}

#[derive(Debug, Clone, Default)]
pub struct McpRegistry {
    servers: BTreeMap<String, McpServerConfig>,
}

impl McpRegistry {
    pub fn load_from_paths(
        global_home: &Path,
        project_root: Option<&Path>,
    ) -> Result<Self, McpError> {
        let mut servers = BTreeMap::new();

        let global_file = global_home.join("mcp.json");
        if global_file.is_file() {
            let parsed = load_mcp_json(&global_file, McpScope::Global)?;
            servers.extend(parsed);
        }

        if let Some(proj) = project_root {
            let proj_file = proj.join(".agent-studios").join("mcp.json");
            if proj_file.is_file() {
                let parsed = load_mcp_json(&proj_file, McpScope::Project)?;
                servers.extend(parsed);
            }
        }

        Ok(Self { servers })
    }

    pub fn register_server(&mut self, server: McpServerConfig) {
        self.servers.insert(server.id.clone(), server);
    }

    pub fn list_servers(&self) -> Vec<McpServerConfig> {
        self.servers.values().cloned().collect()
    }

    pub fn get_server(&self, id: &str) -> Option<&McpServerConfig> {
        self.servers.get(id)
    }

    /// Evaluates approval policy before calling an MCP tool.
    pub fn evaluate_approval(
        &self,
        server_id: &str,
        tool_name: &str,
        auto_approve_max_tier: Option<McpRiskTier>,
        explicitly_approved: bool,
    ) -> Result<McpInvocationDecision, McpError> {
        let srv = self
            .servers
            .get(server_id)
            .filter(|s| s.enabled)
            .ok_or_else(|| McpError::ServerUnavailable(server_id.to_string()))?;

        if explicitly_approved {
            return Ok(McpInvocationDecision::Allowed);
        }

        if let Some(max_tier) = auto_approve_max_tier
            && srv.risk_tier <= max_tier
        {
            return Ok(McpInvocationDecision::Allowed);
        }

        if srv.risk_tier == McpRiskTier::Low {
            return Ok(McpInvocationDecision::Allowed);
        }

        Ok(McpInvocationDecision::RequiresApproval {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            risk_tier: srv.risk_tier,
        })
    }

    /// Returns a secret-redacted view of server configurations safe for UI/logging.
    pub fn redacted_server_summaries(&self) -> Vec<serde_json::Value> {
        let redactor = SecretRedactor::new();
        self.servers
            .values()
            .map(|s| {
                let transport = match &s.transport {
                    McpTransportConfig::Stdio { command, args, env } => {
                        let clean_env: BTreeMap<String, String> = env
                            .keys()
                            .map(|k| (k.clone(), "[REDACTED]".to_string()))
                            .collect();
                        serde_json::json!({
                            "type": "stdio",
                            "command": redactor.redact_text(command),
                            "args": args.iter().map(|a| redactor.redact_text(a)).collect::<Vec<_>>(),
                            "env": clean_env,
                        })
                    }
                    McpTransportConfig::Http { url, headers } => {
                        let clean_headers: BTreeMap<String, String> = headers
                            .keys()
                            .map(|k| (k.clone(), "[REDACTED]".to_string()))
                            .collect();
                        serde_json::json!({
                            "type": "http",
                            "url": redactor.redact_text(url),
                            "headers": clean_headers,
                        })
                    }
                };
                serde_json::json!({
                    "id": s.id,
                    "name": s.name,
                    "scope": s.scope,
                    "enabled": s.enabled,
                    "risk_tier": s.risk_tier,
                    "timeout_secs": s.timeout_secs,
                    "transport": transport,
                })
            })
            .collect()
    }

    /// Performs explicit MCP JSON-RPC `initialize` handshake against Stdio or HTTP server.
    pub async fn initialize_server(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<McpInitializeResult, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = initialize_request(1);
        let resp = send_jsonrpc_single(&srv, working_dir, &req, false).await?;
        let result = &resp["result"];
        Ok(McpInitializeResult {
            server_id: server_id.to_string(),
            protocol_version: result["protocolVersion"]
                .as_str()
                .unwrap_or("2024-11-05")
                .to_string(),
            server_name: result["serverInfo"]["name"].as_str().map(str::to_string),
            capabilities: result["capabilities"].clone(),
        })
    }

    /// Discovers tools from a configured Stdio or Streamable HTTP MCP server.
    pub async fn discover_tools(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<McpToolDescriptor>, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let tools = resp["result"]["tools"].as_array().ok_or_else(|| {
            McpError::Transport("Missing result.tools in MCP response".to_string())
        })?;
        let redactor = SecretRedactor::new();
        let mut out = Vec::with_capacity(tools.len());
        for t in tools {
            if let Some(name) = t["name"].as_str() {
                out.push(McpToolDescriptor {
                    server_id: server_id.to_string(),
                    name: name.to_string(),
                    description: redactor
                        .redact_text(t["description"].as_str().unwrap_or_default()),
                });
            }
        }
        Ok(out)
    }

    /// Discovers tools from a configured Stdio MCP server via JSON-RPC `tools/list`.
    pub async fn discover_tools_stdio(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<McpToolDescriptor>, McpError> {
        self.discover_tools(server_id, working_dir).await
    }

    /// Invokes a tool on a configured Stdio or Streamable HTTP MCP server with fail-closed approval.
    pub async fn invoke_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        working_dir: &Path,
        auto_approve_max_tier: Option<McpRiskTier>,
        explicitly_approved: bool,
    ) -> Result<McpToolCallResult, McpError> {
        match self.evaluate_approval(
            server_id,
            tool_name,
            auto_approve_max_tier,
            explicitly_approved,
        )? {
            McpInvocationDecision::Allowed => {}
            McpInvocationDecision::RequiresApproval {
                server_id,
                tool_name,
                risk_tier,
            } => {
                return Err(McpError::ApprovalRequired {
                    server_id,
                    tool_name,
                    risk_tier,
                });
            }
        }

        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            }
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let redactor = SecretRedactor::new();
        if let Some(err) = resp.get("error") {
            return Ok(McpToolCallResult {
                server_id: server_id.to_string(),
                tool_name: tool_name.to_string(),
                output: redactor.redact_text(&err.to_string()),
                is_error: true,
            });
        }
        let raw_text = resp["result"]["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|item| item["text"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| resp["result"].to_string());

        Ok(McpToolCallResult {
            server_id: server_id.to_string(),
            tool_name: tool_name.to_string(),
            output: redactor.redact_text(&raw_text),
            is_error: resp["result"]["isError"].as_bool().unwrap_or(false),
        })
    }

    /// Invokes a tool on a Stdio MCP server with fail-closed approval interception and secret redaction.
    pub async fn invoke_tool_stdio(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
        working_dir: &Path,
        auto_approve_max_tier: Option<McpRiskTier>,
        explicitly_approved: bool,
    ) -> Result<McpToolCallResult, McpError> {
        self.invoke_tool(
            server_id,
            tool_name,
            arguments,
            working_dir,
            auto_approve_max_tier,
            explicitly_approved,
        )
        .await
    }

    /// Lists MCP resources via JSON-RPC `resources/list` (Stdio or HTTP).
    pub async fn list_resources(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<McpResourceDescriptor>, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/list",
            "params": {}
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let items = resp["result"]["resources"].as_array().ok_or_else(|| {
            McpError::Transport("Missing result.resources in MCP response".to_string())
        })?;
        let redactor = SecretRedactor::new();
        Ok(items
            .iter()
            .filter_map(|item| {
                let uri = item["uri"].as_str()?;
                Some(McpResourceDescriptor {
                    server_id: server_id.to_string(),
                    uri: redactor.redact_text(uri),
                    name: item["name"].as_str().unwrap_or(uri).to_string(),
                    description: redactor
                        .redact_text(item["description"].as_str().unwrap_or_default()),
                    mime_type: item["mimeType"].as_str().map(str::to_string),
                })
            })
            .collect())
    }

    /// Reads an MCP resource via JSON-RPC `resources/read` (Stdio or HTTP).
    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
        working_dir: &Path,
    ) -> Result<McpResourceContents, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": { "uri": uri }
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let first = resp["result"]["contents"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| {
                McpError::Transport("Missing result.contents in MCP resources/read".to_string())
            })?;
        let redactor = SecretRedactor::new();
        Ok(McpResourceContents {
            server_id: server_id.to_string(),
            uri: first["uri"].as_str().unwrap_or(uri).to_string(),
            text: redactor.redact_text(first["text"].as_str().unwrap_or_default()),
            mime_type: first["mimeType"].as_str().map(str::to_string),
        })
    }

    /// Lists MCP prompt templates via JSON-RPC `prompts/list` (Stdio or HTTP).
    pub async fn list_prompts(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<McpPromptDescriptor>, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "prompts/list",
            "params": {}
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let prompts = resp["result"]["prompts"].as_array().ok_or_else(|| {
            McpError::Transport("Missing result.prompts in MCP response".to_string())
        })?;
        let redactor = SecretRedactor::new();
        Ok(prompts
            .iter()
            .filter_map(|p| {
                let name = p["name"].as_str()?;
                Some(McpPromptDescriptor {
                    server_id: server_id.to_string(),
                    name: name.to_string(),
                    description: redactor
                        .redact_text(p["description"].as_str().unwrap_or_default()),
                })
            })
            .collect())
    }

    /// Fetches a resolved MCP prompt via JSON-RPC `prompts/get` (Stdio or HTTP).
    pub async fn get_prompt(
        &self,
        server_id: &str,
        prompt_name: &str,
        arguments: serde_json::Value,
        working_dir: &Path,
    ) -> Result<McpPromptResult, McpError> {
        let srv = self.enabled_server(server_id)?;
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "prompts/get",
            "params": {
                "name": prompt_name,
                "arguments": arguments,
            }
        });
        let include_handshake = matches!(srv.transport, McpTransportConfig::Http { .. });
        let resp = send_jsonrpc_single(&srv, working_dir, &req, include_handshake).await?;
        let redactor = SecretRedactor::new();
        let messages = resp["result"]["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let raw = m.to_string();
                serde_json::from_str(&redactor.redact_text(&raw)).unwrap_or(m)
            })
            .collect();
        Ok(McpPromptResult {
            server_id: server_id.to_string(),
            description: resp["result"]["description"]
                .as_str()
                .map(|d| redactor.redact_text(d)),
            messages,
        })
    }

    fn enabled_server(&self, server_id: &str) -> Result<McpServerConfig, McpError> {
        self.servers
            .get(server_id)
            .filter(|s| s.enabled)
            .cloned()
            .ok_or_else(|| McpError::ServerUnavailable(server_id.to_string()))
    }
}

fn initialize_request(id: u64) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "roots": { "listChanged": false },
                "sampling": {}
            },
            "clientInfo": {
                "name": "agent-studios",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

async fn send_jsonrpc_single(
    srv: &McpServerConfig,
    working_dir: &Path,
    request: &serde_json::Value,
    include_handshake: bool,
) -> Result<serde_json::Value, McpError> {
    let timeout_secs = srv.timeout_secs.max(1);
    match &srv.transport {
        McpTransportConfig::Stdio { command, args, env } => {
            let cmd = command.clone();
            let args = args.clone();
            let env = env.clone();
            let cwd = working_dir.to_path_buf();
            let req = request.clone();
            let task = tokio::task::spawn_blocking(move || {
                invoke_stdio_jsonrpc(&cmd, &args, &env, &cwd, &req)
            });
            match tokio::time::timeout(Duration::from_secs(timeout_secs), task).await {
                Ok(Ok(res)) => res,
                Ok(Err(e)) => Err(McpError::Transport(format!("MCP worker panicked: {e}"))),
                Err(_) => Err(McpError::Timeout(timeout_secs)),
            }
        }
        McpTransportConfig::Http { url, headers } => {
            send_http_jsonrpc(url, headers, timeout_secs, request, include_handshake).await
        }
    }
}

async fn send_http_jsonrpc(
    url: &str,
    headers: &BTreeMap<String, String>,
    timeout_secs: u64,
    request: &serde_json::Value,
    include_handshake: bool,
) -> Result<serde_json::Value, McpError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| McpError::Transport(format!("Build HTTP MCP client: {e}")))?;

    let mut session_id: Option<String> = None;

    if include_handshake {
        let init_req = initialize_request(0);
        let (_, sid) = post_http_jsonrpc(&client, url, headers, None, &init_req).await?;
        session_id = sid;
        let initialized_notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let mut notif_builder = client
            .post(url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json");
        for (k, v) in headers {
            notif_builder = notif_builder.header(k.as_str(), v.as_str());
        }
        if let Some(ref sid_val) = session_id {
            notif_builder = notif_builder.header("Mcp-Session-Id", sid_val.as_str());
        }
        let _ = notif_builder.json(&initialized_notif).send().await;
    }

    let (value, _) =
        post_http_jsonrpc(&client, url, headers, session_id.as_deref(), request).await?;
    Ok(value)
}

async fn post_http_jsonrpc(
    client: &reqwest::Client,
    url: &str,
    headers: &BTreeMap<String, String>,
    session_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<(serde_json::Value, Option<String>), McpError> {
    let mut builder = client
        .post(url)
        .header("Accept", "application/json, text/event-stream")
        .header("Content-Type", "application/json");
    for (k, v) in headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(sid) = session_id {
        builder = builder.header("Mcp-Session-Id", sid);
    }

    let resp = builder
        .json(payload)
        .send()
        .await
        .map_err(|e| McpError::Transport(format!("HTTP MCP request to '{url}' failed: {e}")))?;

    let returned_sid = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let body = resp
        .text()
        .await
        .map_err(|e| McpError::Transport(format!("Read HTTP MCP body: {e}")))?;

    if !status.is_success() {
        return Err(McpError::Transport(format!(
            "HTTP MCP server returned status {status}: {}",
            SecretRedactor::new().redact_text(&body)
        )));
    }

    if content_type.contains("text/event-stream") {
        let mut last_json: Option<serde_json::Value> = None;
        for line in body.lines() {
            if let Some(data) = line.strip_prefix("data:").map(str::trim)
                && !data.is_empty()
                && data != "[DONE]"
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data)
            {
                last_json = Some(parsed);
            }
        }
        return last_json
            .map(|val| (val, returned_sid))
            .ok_or_else(|| McpError::Transport("Empty SSE stream from HTTP MCP server".into()));
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| McpError::Transport(format!("Invalid JSON-RPC HTTP response: {e}")))?;
    Ok((parsed, returned_sid))
}

fn invoke_stdio_jsonrpc(
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
    request: &serde_json::Value,
) -> Result<serde_json::Value, McpError> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(|e| {
        McpError::Transport(format!("Failed to spawn MCP command '{command}': {e}"))
    })?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| McpError::Transport("Missing stdin pipe on MCP server".to_string()))?;
        let line = serde_json::to_string(request)
            .map_err(|e| McpError::Transport(format!("Serialize MCP JSON-RPC: {e}")))?;
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .map_err(|e| McpError::Transport(format!("Write MCP stdin: {e}")))?;
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| McpError::Transport("Missing stdout pipe on MCP server".to_string()))?;
    let mut reader = BufReader::new(stdout);
    let mut line_buf = String::new();
    let bytes = reader
        .read_line(&mut line_buf)
        .map_err(|e| McpError::Transport(format!("Read MCP stdout: {e}")))?;
    let _ = child.wait();

    if bytes == 0 {
        return Err(McpError::Transport(
            "MCP server exited without sending JSON-RPC response".to_string(),
        ));
    }

    serde_json::from_str(&line_buf)
        .map_err(|e| McpError::Transport(format!("Invalid JSON-RPC response from MCP server: {e}")))
}

fn load_mcp_json(
    path: &Path,
    scope: McpScope,
) -> Result<HashMap<String, McpServerConfig>, McpError> {
    let raw = fs::read_to_string(path)
        .map_err(|e| McpError::Config(format!("Failed to read {}: {e}", path.display())))?;
    let parsed: McpConfigFile = serde_json::from_str(&raw)
        .map_err(|e| McpError::Config(format!("Invalid MCP config in {}: {e}", path.display())))?;

    let mut out = HashMap::new();
    for (key, mut cfg) in parsed.servers {
        if cfg.id.is_empty() {
            cfg.id = key.clone();
        }
        cfg.scope = scope;
        out.insert(key, cfg);
    }
    Ok(out)
}

pub fn default_mcp_paths(global_home: &Path, project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = vec![global_home.join("mcp.json")];
    if let Some(proj) = project_root {
        paths.push(proj.join(".agent-studios").join("mcp.json"));
    }
    paths
}
