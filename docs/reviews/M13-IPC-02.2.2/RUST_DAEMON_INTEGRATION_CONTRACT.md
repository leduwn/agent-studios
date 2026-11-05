# M13 IPC-02.2.2 Rust Daemon Integration Contract

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Consumer Milestones**: PR #23 (`ConnectedClient` / Desktop Daemon Client), PR #24 (`Mission Control` CLI & TUI)  
**Status**: STABLE BACKEND CONTRACT  

---

## 1. Transport Endpoints & Permissions

`agsd` background daemon exposes native OS IPC endpoints:

### 1.1 Windows: Named Pipes
- **Default Endpoint**: `\\.\pipe\agent-studios-default`
- **Security Descriptor**: Custom Win32 Security Descriptor granting `GENERIC_ALL` strictly and exclusively to the launching user's SID (`TokenUser`); `World` / `Everyone` access is explicitly denied, and NULL DACLs are strictly prohibited.

### 1.2 Linux / macOS: Unix Domain Sockets (UDS)
- **Primary Search Paths**:
  1. `$XDG_RUNTIME_DIR/agent-studios/agsd.sock` (preferred when `$XDG_RUNTIME_DIR` is set and non-empty)
  2. `/tmp/agent-studios-<UID>/agsd.sock` (secure UID-scoped fallback directory)
  3. `/tmp/agent-studios-agsd.sock` (static fallback in daemon configuration)
- **Permissions**: Mode `0600` (read/write only by owner UID); parent directory mode `0700`. Ownership must match current process UID. Pre-existing stale socket files are unlinked prior to bind; symlinks in the path are rejected to prevent hijacking.

---

## 2. Framing & Wire Format

Every IPC frame sent across socket or named pipe consists of:
1. **Length Prefix**: 4-byte unsigned integer in **big-endian** order (`u32::to_be_bytes()`).
2. **Payload**: UTF-8 encoded JSON string corresponding to length prefix.
3. **Framing Limits**:
   - `DEFAULT_MAX_FRAME_SIZE = 4 MiB` (4,194,304 bytes).
   - Frames exceeding 4 MiB are rejected before buffer allocation with `TransportError::FrameTooLarge` / `ProtocolError::FrameTooLarge`.
   - *(Note: Server capability negotiator declares `max_payload_bytes: 16 MiB`, but wire framing layer enforces strict 4 MiB frame boundary).*

---

## 3. Connection Handshake & Authentication

All connections must execute a two-step handshake prior to issuing operational commands:

```
Client                                                  Server (agsd)
  |                                                           |
  |--- 1. NegotiateCapabilities(protocol_version, caps) ----->|
  |<-- 2. CapabilitiesNegotiated(selected_version, caps) -----|
  |                                                           |
  |--- 3. Authenticate(header: HmacAuthHeader, payload) ----->|
  |<-- 4. Authenticated(session_id) --------------------------|
  |                                                           |
  |=== Operational Commands (Submit, Cancel, Approval) =======|
```

### 3.1 Step 1: Capability Negotiation
Command (`IpcCommand::NegotiateCapabilities`):
```json
{
  "type": "negotiate_capabilities",
  "protocol_version": {
    "major": 1,
    "minor": 0,
    "patch": 0
  },
  "capabilities": [
    "worktree_isolation",
    "approval_routing",
    "event_streaming",
    "event_replay",
    "secret_redaction",
    "hmac_auth"
  ]
}
```
*(Note: Serde alias allows `"version"` in place of `"protocol_version"`, but the value must be a `ProtocolVersion` object, not an integer).*

Server Response (`IpcResponse::CapabilitiesNegotiated`):
```json
{
  "type": "capabilities_negotiated",
  "selected_version": {
    "major": 1,
    "minor": 0,
    "patch": 0
  },
  "agreed_capabilities": [
    "worktree_isolation",
    "approval_routing",
    "event_streaming",
    "event_replay",
    "secret_redaction",
    "hmac_auth"
  ]
}
```

### 3.2 Step 2: HMAC-SHA256 Authentication
- **Canonical Digest Format**:
  ```
  {protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}
  ```
  - `protocol_version`: SemVer string representation (e.g. `"1.0.0"`).
  - `key_id`: Client identity key string.
  - `session_id`: Session UUID string.
  - `nonce`: Unique cryptographic nonce string.
  - `timestamp_millis`: Decimal string representation of Unix epoch milliseconds (i64).
  - `capabilities_str`: Lexicographically sorted, comma-joined list of agreed capabilities (or empty string if none).
  - `payload_hash`: Hex-encoded SHA-256 digest of payload bytes (`hex::encode(Sha256::digest(payload))`).
- **Signature Calculation**:
  `signature = hex::encode(HMAC_SHA256(secret_key, canonical_digest.as_bytes()))`
- **Replay Protection & Skew**:
  - Server enforces timestamp skew tolerance of `60_000` ms (±60 seconds from server clock). Timestamps in milliseconds exceeding skew are rejected.
  - `NonceTracker` tracks `(key_id, session_id, nonce)` within a sliding memory-bounded window with TTL eviction. Duplicate nonces fail closed with `AuthenticationFailed`.

Client Command (`IpcCommand::Authenticate`):
```json
{
  "type": "authenticate",
  "header": {
    "protocol_version": {
      "major": 1,
      "minor": 0,
      "patch": 0
    },
    "key_id": "ags-client",
    "session_id": "00000000-0000-0000-0000-000000000000",
    "nonce": "a1b2c3d4e5f67890",
    "timestamp_millis": 1793400000000,
    "signature": "8f3b25a1b0c4d5e6..."
  },
  "payload": null
}
```

Server Response (`IpcResponse::Authenticated`):
```json
{
  "type": "authenticated",
  "session_id": "9a7c0000-0000-0000-0000-000000000001"
}
```
*(Note: Role is determined server-side from `key_id` registration and not echoed in `IpcResponse::Authenticated`).*

---

## 4. Command Reference

All operational commands and responses use the tag `"type"` under Serde serialization (`#[serde(rename_all = "snake_case", tag = "type")]`).

### 4.1 Submit Task
Command (`IpcCommand::SubmitTask`):
```json
{
  "type": "submit_task",
  "studio_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "task_spec": {
    "key": "feature-task-1",
    "title": "Implement auth middleware",
    "description": "Add JWT validation HTTP handler",
    "assigned_agent_id": "2b0d7b3d-4bad-9bdd-3b7d-9b1deb4dcb6d",
    "parent_task_key": null,
    "parent_task_id": null,
    "dependency_keys": [],
    "dependency_task_ids": [],
    "dependency_output_policy": "task_success"
  }
}
```
*(Notice: Field name is `task_spec: BatchTaskSpec`, not `spec`).*

Response (`IpcResponse::TasksSubmitted`):
```json
{
  "type": "tasks_submitted",
  "task_ids": [
    "c0a80101-0000-0000-0000-000000000001"
  ]
}
```

### 4.2 Cancel Task
Command (`IpcCommand::CancelTask`):
```json
{
  "type": "cancel_task",
  "studio_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "task_id": "c0a80101-0000-0000-0000-000000000001",
  "reason": "Operator cancelled execution"
}
```

Response (`IpcResponse::TaskCancelled`):
```json
{
  "type": "task_cancelled",
  "summary": {
    "cancelled_tasks": [
      "c0a80101-0000-0000-0000-000000000001"
    ],
    "cancelled_runs": [
      "r0a80101-0000-0000-0000-000000000001"
    ],
    "cancelled_approvals": []
  }
}
```
*(Notice: Response contains `summary: CancellationSummary`, not `task_id`).*  
*Authorization Note: Standard clients can only cancel tasks originating from their session/key identity. Untracked or cross-session cancellations require admin role.*

### 4.3 Resolve Approval
`ApprovalDecision` uses internally tagged Serde encoding: `#[serde(rename_all = "snake_case", tag = "decision")]`.

Approved Command (`IpcCommand::ResolveApproval`):
```json
{
  "type": "resolve_approval",
  "approval_id": "d1a80101-0000-0000-0000-000000000002",
  "decision": {
    "decision": "approved",
    "user_note": "Confirmed safe to proceed"
  }
}
```

Rejected Command (`IpcCommand::ResolveApproval`):
```json
{
  "type": "resolve_approval",
  "approval_id": "d1a80101-0000-0000-0000-000000000002",
  "decision": {
    "decision": "rejected",
    "reason": "Operator denied permission for shell command execution"
  }
}
```

Response (`IpcResponse::ApprovalResolved`):
```json
{
  "type": "approval_resolved",
  "approval_id": "d1a80101-0000-0000-0000-000000000002",
  "status": "authorized"
}
```
*(Notice: Response includes `approval_id` alongside `status`).*  
*Consistency Note: Decision is delivered to external runtime before mutating Control Plane state. Concurrent resolutions are deduplicated; second attempts fail closed.*

### 4.4 Event Subscription & Linearizable Replay
Command (`IpcCommand::SubscribeEvents`):
```json
{
  "type": "subscribe_events",
  "from_sequence": 42
}
```
*(Notice: Field is `from_sequence: Option<u64>`, not `start_sequence`).*

Response stream emits framed `event` responses (`IpcResponse::Event`):
```json
{
  "type": "event",
  "event": {
    "sequence": 42,
    "timestamp": "2026-10-30T12:00:00Z",
    "event_type": "task_state_transitioned",
    "payload": {
      "task_id": "c0a80101-0000-0000-0000-000000000001",
      "from_state": "ready",
      "to_state": "running"
    }
  }
}
```

### 4.5 Ping & Health
Ping Command (`IpcCommand::Ping`):
```json
{
  "type": "ping",
  "payload": "heartbeat",
  "timestamp": 1793400000000
}
```
Pong Response (`IpcResponse::Pong`):
```json
{
  "type": "pong",
  "payload": "heartbeat",
  "client_timestamp": 1793400000000,
  "server_timestamp": 1793400000015
}
```

GetHealth Command (`IpcCommand::GetHealth`):
```json
{
  "type": "get_health"
}
```
Health Response (`IpcResponse::Health`):
```json
{
  "type": "health",
  "status": "healthy",
  "active_clients": 1,
  "uptime_secs": 3600,
  "memory_events_count": 128
}
```

### 4.6 Control Plane & Catalog Query Commands
- **`create_studio`**: `{"type": "create_studio", "name": "Main Studio"}` -> `{"type": "studio_created", "studio": { ... }}`
- **`list_studios`**: `{"type": "list_studios"}` -> `{"type": "studios", "studios": [ ... ]}`
- **`get_studio`**: `{"type": "get_studio", "studio_id": "..."}` -> `{"type": "studio", "studio": { ... }}`
- **`list_tasks`**: `{"type": "list_tasks", "studio_id": "..."}` -> `{"type": "tasks", "tasks": [ ... ]}`
- **`get_task`**: `{"type": "get_task", "studio_id": "...", "task_id": "..."}` -> `{"type": "task", "task": { ... }}`
- **`list_runs`**: `{"type": "list_runs", "studio_id": "..."}` -> `{"type": "runs", "runs": [ ... ]}`
- **`get_run`**: `{"type": "get_run", "studio_id": "...", "run_id": "..."}` -> `{"type": "run", "run": { ... }}`
- **`list_providers`**: `{"type": "list_providers"}` -> `{"type": "providers", "providers": [ ... ]}`
- **`get_provider`**: `{"type": "get_provider", "provider_id": "..."}` -> `{"type": "provider", "provider": { ... }}`
- **`list_models`**: `{"type": "list_models", "instance_id": null}` -> `{"type": "models", "models": [ ... ]}`
- **`shutdown`**: `{"type": "shutdown"}` (Admin only) -> `{"type": "shutdown_acknowledged"}`
- **`error`**: `{"type": "error", "message": "..."}`

---

## 5. Bootstrap Credential Segregation

Daemon startup produces credential files for client auto-discovery with restricted file permissions (`0600` on Unix, User-only DACL on Windows).

### 5.1 Standard Client Discovery: `agsd.json`
Contains client credentials for standard operational tasks. Does **never** contain admin keys:
```json
{
  "endpoint": "\\\\.\\pipe\\agent-studios-default",
  "key_id": "ags-client",
  "client_key": "c3d4e5f6-a7b8-4c9d-0e1f-2a3b4c5d6e7f",
  "pid": 12345,
  "created_at": 1793400000
}
```
- **Fields**:
  - `endpoint`: Native IPC connection endpoint string (`\\.\pipe\...` on Windows, `/path/to/sock` on Unix).
  - `key_id`: Client identity key ID.
  - `client_key`: Shared HMAC authentication secret string.
  - `pid`: Process ID of daemon.
  - `created_at`: Unix epoch seconds timestamp.

**Standard Discovery Search Paths**:
- Windows:
  1. `%LOCALAPPDATA%\agent-studios\agsd.json` (if `%LOCALAPPDATA%` set)
  2. `%TEMP%\agent-studios\agsd.json` (fallback)
- Linux / macOS:
  1. `$XDG_RUNTIME_DIR/agent-studios/agsd.json` (if `$XDG_RUNTIME_DIR` set and non-empty)
  2. `/tmp/agent-studios-<UID>/agsd.json` (fallback)

### 5.2 Admin Client Discovery: `agsd-admin.json`
Contains administrative credentials for operations requiring administrative escalation (e.g. foreign task/approval resolution, daemon shutdown):
```json
{
  "endpoint": "\\\\.\\pipe\\agent-studios-default",
  "key_id": "ags-client-admin",
  "admin_key": "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d",
  "pid": 12345,
  "created_at": 1793400000
}
```
- **Fields**:
  - `endpoint`: Native IPC connection endpoint string.
  - `key_id`: Administrative key ID (`{key_id}-admin`).
  - `admin_key`: Administrative HMAC authentication secret string.
  - `pid`: Process ID of daemon.
  - `created_at`: Unix epoch seconds timestamp.

**Admin Discovery Search Paths**:
- Windows:
  1. `%LOCALAPPDATA%\agent-studios\agsd-admin.json` (if `%LOCALAPPDATA%` set)
  2. `%TEMP%\agent-studios\agsd-admin.json` (fallback)
- Linux / macOS:
  1. `$XDG_RUNTIME_DIR/agent-studios/agsd-admin.json` (if `$XDG_RUNTIME_DIR` set and non-empty)
  2. `/tmp/agent-studios-<UID>/agsd-admin.json` (fallback)

---

## 6. Runtime & Provider/Model Selection Rules

When auto-execution is enabled (`config.auto_execute = true`), task execution follows deterministic resolution rules before starting an agent runtime:

### 6.1 Agent Runtime Instance Selection
1. **Configured Runtime**: Server checks `server_state.runtime`. If `None`, task/run fails immediately with error: `"No agent runtime configured for execution"`.
2. **Discovery**: Server invokes `runtime.discover().await`. If discovery encounters an error, task/run fails with `"Runtime discovery error: {e}"`.
3. **Explicit Instance**: If `server_state.configured_instance_id` is set:
   - Server searches discovered instances for a matching `instance_id` where `is_available() == true`.
   - If not found or unavailable, task/run fails with `"Configured runtime instance '{target_id}' is not available"`.
4. **Automatic Instance Selection**: If `configured_instance_id` is `None`:
   - Server filters discovered instances by `is_available()`.
   - **0 available**: Fails with `"No available runtime instance discovered. OpenCode runtime binary not found. Install via: npm i -g opencode-ai or configure AGENT_STUDIOS_OPENCODE_PATH"`.
   - **Exactly 1 available**: Automatically selects that instance.
   - **> 1 available**: Fails closed to avoid non-deterministic execution: `"Multiple runtime instances available ({count}) but no explicit instance selected. Configure target instance ID"`.

### 6.2 Provider & Model Selection
1. **Explicit Model Route**: If `server_state.default_model_ref` is set (configured via `IpcServerBuilder::with_default_model_ref` or `AGENT_STUDIOS_DEFAULT_MODEL` environment variable):
   - Server checks `provider_catalog.get_model(&configured_ref.provider_instance_id, &configured_ref.model_id)`.
   - If model is registered in catalog, it is selected.
   - If model is not registered, task/run fails with `"Configured model '{configured_ref}' not found in provider catalog"`.
2. **Automatic Model Selection**: If `default_model_ref` is `None`:
   - Server queries `catalog.list_models()`.
   - **0 models**: Sets `model_ref = None` (allowed for test environments without LLM dependencies).
   - **Exactly 1 model**: Automatically selects `models[0].model_ref()`.
   - **> 1 models**: Fails closed to avoid arbitrary model routing: `"Multiple models available ({count}) in catalog but no explicit model route configured. Specify default model via IpcServerBuilder::with_default_model_ref or AGENT_STUDIOS_DEFAULT_MODEL environment variable"`.
