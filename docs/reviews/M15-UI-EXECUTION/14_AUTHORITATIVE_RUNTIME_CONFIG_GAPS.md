# Authoritative Runtime Configuration Provenance Gaps

**Document ID**: `M15-UI-EXEC-DOC-14`  
**Milestone**: M14.1.1 Final Cross-Platform & Authorization Corrections  
**Author**: Principal VS Code Extension Engineer, Backend Integration Lead  
**Date**: 2026-10-30  
**PR Reference**: PR #25 (`feat/m14-mission-control-connected-integration`)  
**Backend Reference**: `agsd` / `agent-studios-rs` at commit `ce0396c1f1`  
**Relevant Backend Code**: `connection.rs:1568-1574`, `connection.rs:1758-1827`

---

## 1. Executive Summary

A critical finding from the M14 freeze and M14.1.1 security audit is the **authoritative configuration provenance gap** between the VS Code Extension Host and the compiled Rust daemon (`agsd`).

The daemon executes mutating tasks against a local repository on disk and routes LLM requests through backend providers. However, `agsd` at commit `ce0396c1f1` provides **no IPC query command** for a connected client to discover:
1. The daemon's active repository root path (`--repo <PATH>`).
2. The daemon's effective default model and provider (`AGENT_STUDIOS_DEFAULT_MODEL`).

Previously, client-side code fell back to checking `process.env.AGENT_STUDIOS_REPO_PATH` or `process.env.AGENT_STUDIOS_DEFAULT_MODEL` in the Extension Host process. In a distributed, remote, or multi-user setup (e.g. VS Code Remote SSH, WSL, devcontainers, or background daemon processes), the client's local environment variables **cannot be trusted** as authoritative proof of the daemon's runtime configuration.

This document analyzes the architectural risks of trusting local environment variables, specifies the fail-closed defense implemented in `SubmitTaskAdapter.verifyDaemonConfiguration`, and proposes an authoritative IPC protocol contract for the backend team.

---

## 2. Analysis of the Provenance Gap

### 2.1 Backend Daemon Execution Constraints

In `agent-studios-rs/crates/agent-studios-daemon/src/connection.rs`:

1. **Repository Root Boundary (Lines 1568-1574)**:
   The daemon runs within a specific repository workspace determined by CLI flags or daemon-side configuration. Mutating tasks that touch paths outside this repository are rejected.
2. **Model Routing Invariant (Lines 1771-1827)**:
   `IpcCommand::SubmitTask` accepts `task_spec: BatchTaskSpec`. In `BatchTaskSpec`, there is **no field** for `model_id` or `provider_instance_id`. The daemon routes tasks strictly through its own configured default model:
   - If `AGENT_STUDIOS_DEFAULT_MODEL` is set in the daemon's environment, it selects that model from the provider catalog (line 1788).
   - If unset, but the catalog has exactly 1 model, it uses that single model (line 1806).
   - If unset and the catalog has multiple models, the daemon **fails closed** and rejects task submission (line 1809):
     `"Multiple models available in daemon catalog, but no explicit model route configured. Set AGENT_STUDIOS_DEFAULT_MODEL"`.

### 2.2 The Fallacy of Client Environment Trust

When the VS Code extension runs:
- `process.env` in the Extension Host reflects the editor's environment, not the daemon's process environment.
- If the daemon was launched with `--repo /srv/repo-a`, but the user has workspace `/home/user/repo-b` open, assuming the daemon is running in `/home/user/repo-b` leads to cross-repository corruption or submission failure.
- If the Extension Host has `AGENT_STUDIOS_DEFAULT_MODEL=anthropic/claude-3-5-sonnet`, but the daemon is configured with `openai/gpt-4o`, the extension would display misleading model attribution to the user.
- Caller-provided parameters (such as form inputs in webviews) cannot be treated as verified facts.

---

## 3. The Fail-Closed Client Implementation

In M14.1.1, `apps/agent-studios-vscode/src/bridge/ipc/SubmitTaskAdapter.ts` enforces strict verification and fails closed whenever configuration provenance cannot be verified:

### 3.1 Verification Procedure (`verifyDaemonConfiguration`)

```typescript
public static async verifyDaemonConfiguration(
  transport: { sendCommand: (cmd: RustIpcCommand, timeoutMs?: number) => Promise<RustIpcResponse> } | NativeIpcTransport,
  options?: {
    explicitRepoPath?: string;
    defaultModelEnv?: string;
  }
): Promise<VerifiedDaemonConfig> {
  // 1. Verify health & liveness via IpcCommand::GetHealth
  const healthResp = await transport.sendCommand({ type: 'get_health' });
  if (healthResp.type !== 'health' || healthResp.status !== 'healthy') {
    throw new TaskSubmissionError(`Daemon reported unhealthy status...`);
  }

  // 2. Query available providers via IpcCommand::ListProviders
  const provResp = await transport.sendCommand({ type: 'list_providers' });
  const availableProviders = provResp.providers;

  // 3. Query catalog models via IpcCommand::ListModels
  const modelResp = await transport.sendCommand({ type: 'list_models' });
  const availableModels = modelResp.models;

  // 4. Authoritative daemon model resolution parity (connection.rs:1771-1827)
  let defaultModelId: string | undefined;
  let defaultProviderId: string | undefined;

  const parsedEnv = parseDefaultModelEnv(options?.defaultModelEnv);
  if (parsedEnv) {
    const matched = availableModels.find((m) => {
      if (parsedEnv.providerId) {
        return m.provider_instance_id === parsedEnv.providerId && m.id === parsedEnv.modelId;
      }
      return m.id === parsedEnv.modelId;
    });

    if (!matched) {
      throw new TaskSubmissionError(
        `Configured default model '${parsedEnv.modelId}' not found in daemon provider catalog.`
      );
    }
    defaultModelId = matched.id;
    defaultProviderId = matched.provider_instance_id;
  } else {
    if (availableModels.length === 1) {
      defaultModelId = availableModels[0].id;
      defaultProviderId = availableModels[0].provider_instance_id;
    } else if (availableModels.length > 1) {
      // Backend agsd fails closed at connection.rs line 1809
      throw new TaskSubmissionError(
        `Multiple models available (${availableModels.length}) in daemon catalog, but no explicit model route configured. Set AGENT_STUDIOS_DEFAULT_MODEL (parity: connection.rs:1816).`
      );
    }
  }

  return {
    healthy: true,
    status: healthResp.status,
    defaultProviderId,
    defaultModelId,
    availableProviders,
    availableModels,
    daemonRepoRoot: options?.explicitRepoPath,
  };
}
```

### 3.2 Workspace & Route Boundary Invariants

1. **No Guessing**: If `daemonRepoRoot` cannot be authoritatively verified against the backend, mutating submissions requiring workspace verification fail closed.
2. **Strict Matching**: If `workspacePath !== daemonRepoRoot`, task dispatch is rejected before reaching the wire.
3. **No Synthetic Overrides**: Webview requests attempting to specify `requestedModelId` that diverges from the daemon's verified route are rejected with `Model routing mismatch: Requested model differs from daemon effective configuration`.

---

## 4. Contract Proposal for Backend Team (PR-M15-01)

To close this provenance gap definitively before M15 real agent execution, the Rust daemon must expose an authoritative configuration query command.

### 4.1 Proposed IPC Command: `IpcCommand::GetDaemonConfig`

```rust
// Proposed addition to agent-studios-protocol / crates/protocol/src/commands.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcCommand {
    // Existing commands...
    GetHealth,
    ListProviders,
    ListModels,
    
    /// Authoritative configuration query command
    GetDaemonConfig,
}
```

### 4.2 Proposed IPC Response: `IpcResponse::DaemonConfig`

```rust
// Proposed addition to agent-studios-protocol / crates/protocol/src/responses.rs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct DaemonConfigResponse {
    pub repo_root: String,
    pub default_provider_id: Option<String>,
    pub default_model_id: Option<String>,
    pub execution_sandbox_mode: String,
    pub active_studio_id: Option<String>,
}
```

### 4.3 Alternative: Extend `IpcResponse::Health`

If introducing a new command is deferred, `IpcResponse::Health` can be extended with optional authoritative fields:

```rust
pub struct HealthResponse {
    pub status: String,
    pub active_clients: u32,
    pub uptime_secs: u64,
    pub memory_events_count: usize,
    // Add authoritative runtime fields:
    pub repo_root: Option<String>,
    pub effective_default_model: Option<String>,
}
```

Until this contract is landed in `agent-studios-rs`, the VS Code extension host will maintain its strict fail-closed posture, preventing unverified workspace mutations.
