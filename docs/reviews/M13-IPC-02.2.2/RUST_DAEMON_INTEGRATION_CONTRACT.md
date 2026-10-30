# M13 IPC-02.2.2 Rust Daemon Integration Contract

**Date**: 2026-10-30  
**Target Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Consumer Milestones**: PR #23 (`ConnectedClient` / Desktop Daemon Client), PR #24 (`Mission Control` CLI & TUI)  
**Status**: STABLE BACKEND CONTRACT  

---

## 1. Transport Endpoints & Permissions

The `agsd` background daemon exposes native OS IPC endpoints:

### 1.1 Windows: Named Pipes
- **Default Endpoint**: `\\.\pipe\agent-studios-default`
- **Security Descriptor**: Custom Win32 Security Descriptor granting `FILE_GENERIC_READ | FILE_GENERIC_WRITE` strictly to the launching user's SID and local administrators; `World` / `Everyone` access is explicitly denied.

### 1.2 Linux / macOS: Unix Domain Sockets (UDS)
- **Primary Search Paths**:
  1. `$XDG_RUNTIME_DIR/agent-studios/agsd.sock`
  2. `/run/user/<UID>/agent-studios/agsd.sock`
  3. `~/.agent-studios/run/agsd.sock`
  4. `/var/run/agent-studios/agsd.sock` (system-wide admin daemon)
- **Permissions**: Mode `0600` (read/write only by owner user); parent directory mode `0700`.

---

## 2. Framing & Wire Format

Every IPC frame sent across the socket or pipe consists of:
1. **Length Prefix**: 4-byte unsigned integer in **big-endian** order (`u32::to_be_bytes()`).
2. **Payload**: UTF-8 encoded JSON string of length matching the prefix.
3. **Framing Limits**:
   - `DEFAULT_MAX_FRAME_SIZE = 16 MiB` (16,777,216 bytes). Frames exceeding this length are rejected before allocation.

---

## 3. Connection Handshake & Authentication

All connections must execute a two-step handshake prior to issuing operational commands:

```
Client                                     Server (agsd)
  |                                              |
  |--- 1. Negotiate(version, capabilities) ----->|
  |<-- 2. NegotiateOk(supported_capabilities) ---|
  |                                              |
  |--- 3. Authenticate(key_id, nonce, sig) ----->|
  |<-- 4. Authenticated(session_id, role) -------|
  |                                              |
  |=== Operational Commands (Submit, Cancel) ====|
```

### 3.1 Step 1: Protocol Negotiation
```json
{
  "type": "negotiate",
  "version": 1,
  "client_capabilities": ["event_streaming", "linearizable_replay", "worktree_isolation"]
}
```
Server responds with `negotiate_ok`:
```json
{
  "type": "negotiate_ok",
  "version": 1,
  "server_capabilities": ["worktree_isolation", "approval_routing", "event_streaming", "event_replay", "secret_redaction", "hmac_auth"]
}
```

### 3.2 Step 2: HMAC-SHA256 Authentication
- **Message Canonical Format**: `canonical = "{key_id}:{timestamp}:{nonce}"`
- **Signature**: `HMAC_SHA256(secret_key, canonical.as_bytes())` hex-encoded.
- **Replay Protection**: Server enforces timestamp skew tolerance (±60 seconds) and tracks nonces within a sliding window. Duplicate nonces are rejected with `InvalidSignatureOrNonce`.

Client Command:
```json
{
  "type": "authenticate",
  "key_id": "client-user-1",
  "timestamp": 1793400000,
  "nonce": "a1b2c3d4e5f67890",
  "signature": "8f3b...hex..."
}
```
Server Response:
```json
{
  "type": "authenticated",
  "session_id": "sess-9a7c...uuid...",
  "role": "standard"
}
```
*(Roles: `"admin"` or `"standard"`)*

---

## 4. Command Reference

### 4.1 Submit Task
```json
{
  "type": "submit_task",
  "studio_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "spec": {
    "key": "feature-task-1",
    "title": "Implement auth middleware",
    "description": "Add JWT validation to HTTP handler",
    "assigned_agent_id": "2b0d7b3d-4bad-9bdd-3b7d-9b1deb4dcb6d",
    "parent_task_key": null,
    "parent_task_id": null,
    "dependency_keys": [],
    "dependency_task_ids": [],
    "dependency_output_policy": "task_success"
  }
}
```
Response:
```json
{
  "type": "task_submitted",
  "task_ids": ["c0a80101-0000-0000-0000-000000000001"]
}
```

### 4.2 Cancel Task
```json
{
  "type": "cancel_task",
  "studio_id": "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d",
  "task_id": "c0a80101-0000-0000-0000-000000000001",
  "reason": "Operator cancelled execution"
}
```
Response:
```json
{
  "type": "task_cancelled",
  "task_id": "c0a80101-0000-0000-0000-000000000001"
}
```
*Authorization Note: Standard clients can only cancel tasks they positively own. Reconnected clients using the same `key_id` automatically rebind ownership. Untracked tasks require `"admin"` role.*

### 4.3 Resolve Approval
```json
{
  "type": "resolve_approval",
  "approval_id": "d1a80101-0000-0000-0000-000000000002",
  "decision": {
    "type": "approved",
    "modified_details": null,
    "user_note": "Confirmed safe to proceed"
  }
}
```
Response:
```json
{
  "type": "approval_resolved",
  "status": "authorized"
}
```
*Consistency Note: The decision is delivered to the agent runtime process before the Control Plane state changes. Concurrent resolutions are serialized with deduplication.*

### 4.4 Event Subscription & Linearizable Replay
```json
{
  "type": "subscribe_events",
  "start_sequence": 42
}
```
Response stream emits `event` payloads:
```json
{
  "type": "event",
  "event": {
    "sequence": 42,
    "timestamp": "2026-10-30T12:00:00Z",
    "event_type": "task_state_transitioned",
    "payload": { ... }
  }
}
```

---

## 5. Bootstrap Credential Segregation

Daemon startup produces credential files for client auto-discovery:
- `agsd.json`: Contains standard client credentials (`key_id`, `secret`, `socket_path` / `pipe_name`).
- `agsd-admin.json`: Contains administrative credentials with `role: "admin"`.
- Mode permissions are strictly enforced on creation (`0600`).
