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

    /// Discovers tools from a configured Stdio MCP server via JSON-RPC `tools/list`.
    pub async fn discover_tools_stdio(
        &self,
        server_id: &str,
        working_dir: &Path,
    ) -> Result<Vec<McpToolDescriptor>, McpError> {
        let srv = self
            .servers
            .get(server_id)
            .filter(|s| s.enabled)
            .ok_or_else(|| McpError::ServerUnavailable(server_id.to_string()))?
            .clone();

        let (command, args, env) = match srv.transport {
            McpTransportConfig::Stdio { command, args, env } => (command, args, env),
            McpTransportConfig::Http { .. } => {
                return Err(McpError::Config(
                    "HTTP MCP transport requires live endpoint client".to_string(),
                ));
            }
        };

        let timeout_secs = srv.timeout_secs.max(1);
        let cwd = working_dir.to_path_buf();
        let sid = server_id.to_string();

        let task = tokio::task::spawn_blocking(move || {
            let req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/list",
                "params": {}
            });
            let resp = invoke_stdio_jsonrpc(&command, &args, &env, &cwd, &req)?;
            let tools = resp["result"]["tools"].as_array().ok_or_else(|| {
                McpError::Transport("Missing result.tools in MCP response".to_string())
            })?;
            let mut out = Vec::with_capacity(tools.len());
            for t in tools {
                if let Some(name) = t["name"].as_str() {
                    out.push(McpToolDescriptor {
                        server_id: sid.clone(),
                        name: name.to_string(),
                        description: SecretRedactor::new()
                            .redact_text(t["description"].as_str().unwrap_or_default()),
                    });
                }
            }
            Ok(out)
        });

        match tokio::time::timeout(Duration::from_secs(timeout_secs), task).await {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => Err(McpError::Transport(format!("MCP worker panicked: {e}"))),
            Err(_) => Err(McpError::Timeout(timeout_secs)),
        }
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

        let srv = self
            .servers
            .get(server_id)
            .filter(|s| s.enabled)
            .ok_or_else(|| McpError::ServerUnavailable(server_id.to_string()))?
            .clone();

        let (command, args, env) = match srv.transport {
            McpTransportConfig::Stdio { command, args, env } => (command, args, env),
            McpTransportConfig::Http { .. } => {
                return Err(McpError::Config(
                    "HTTP MCP transport requires live endpoint client".to_string(),
                ));
            }
        };

        let timeout_secs = srv.timeout_secs.max(1);
        let cwd = working_dir.to_path_buf();
        let sid = server_id.to_string();
        let tname = tool_name.to_string();

        let task = tokio::task::spawn_blocking(move || {
            let req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": tname,
                    "arguments": arguments,
                }
            });
            let resp = invoke_stdio_jsonrpc(&command, &args, &env, &cwd, &req)?;
            let redactor = SecretRedactor::new();
            if let Some(err) = resp.get("error") {
                return Ok(McpToolCallResult {
                    server_id: sid,
                    tool_name: tname,
                    output: redactor.redact_text(&err.to_string()),
                    is_error: true,
                });
            }
            let raw_text = resp["result"]["content"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|item| item["text"].as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| resp["result"].to_string());

            Ok(McpToolCallResult {
                server_id: sid,
                tool_name: tname,
                output: redactor.redact_text(&raw_text),
                is_error: resp["result"]["isError"].as_bool().unwrap_or(false),
            })
        });

        match tokio::time::timeout(Duration::from_secs(timeout_secs), task).await {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => Err(McpError::Transport(format!("MCP worker panicked: {e}"))),
            Err(_) => Err(McpError::Timeout(timeout_secs)),
        }
    }
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
