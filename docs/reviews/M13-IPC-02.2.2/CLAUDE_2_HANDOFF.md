# Claude 2 Cross-Team Handoff: M13 IPC Daemon Contract & Client Integration

**Date**: 2026-10-30  
**Target Pull Requests**: PR #23 (`feat/m14-ipc-03-connected-client`) & PR #25 (`feat/m14-mission-control-connected-integration`)  
**Source Branch**: `feat/m13-ipc-02-control-plane` (Pull Request #19)  
**Author**: Principal Systems Architect (Claude 1)  
**Recipient**: Principal Frontend / Extension Architect (Claude 2)  
**Status**: OFFICIAL IPC CONTRACT FREEZE & INTEGRATION SPECIFICATION  

---

## 1. Executive Summary & Purpose

This document provides the authoritative, frozen integration specification for connecting the Code-OSS desktop extension (`ConnectedClient`, Mission Control, and VSCode Extension Host) to the native Rust Agent Studios Daemon (`agsd`).

All protocol discrepancies, framing limits, authentication wire structures, approval flows, and discovery mechanisms have been audited, corrected, and verified against the canonical Rust implementation in `agent-studios-rs` (commit `ff4f38fb3d`).

---

## 2. Protocol Contract Freeze: 15 Core Invariants

Claude 2 and all TypeScript client implementations in PR #23 and PR #25 must strictly adhere to the following 15 protocol invariants:

### 2.1 Native Framing Layer & Frame Size Limit
- **Framing**: Every frame on Windows Named Pipes (`\\.\pipe\agent-studios-<name>`) and Unix Domain Sockets is preceded by a **4-byte big-endian unsigned integer length prefix** (`u32::to_be_bytes()`).
- **Maximum Frame Size**: `DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` (4 MiB = 4,194,304 bytes).
- Frames declaring length prefix `> 4 MiB` are immediately rejected prior to memory allocation to prevent denial-of-service memory exhaustion.

### 2.2 Serde Variant Tagging Convention
- All commands and responses are tagged internally using the `"type"` field with `snake_case` values.
- Example: `{"type": "negotiate_capabilities", ...}`, `{"type": "capabilities_negotiated", ...}`.

### 2.3 Capability Negotiation Structure
- **Command**:
  ```json
  {
    "type": "negotiate_capabilities",
    "protocol_version": { "major": 1, "minor": 0, "patch": 0 },
    "capabilities": ["event_streaming", "linearizable_replay"]
  }
  ```
- **Response**:
  ```json
  {
    "type": "capabilities_negotiated",
    "selected_version": { "major": 1, "minor": 0, "patch": 0 },
    "agreed_capabilities": ["event_streaming", "linearizable_replay"]
  }
  ```

### 2.4 HMAC-SHA256 Authentication Wire Format
- **Command Structure**:
  ```json
  {
    "type": "authenticate",
    "header": {
      "protocol_version": { "major": 1, "minor": 0, "patch": 0 },
      "key_id": "ags-client",
      "session_id": "00000000-0000-0000-0000-000000000000",
      "nonce": "a1b2c3d4e5f67890",
      "timestamp_millis": 1793400000000,
      "signature": "8f3b25a1b0c4d5e6..."
    },
    "payload": null
  }
  ```
- **Response Structure**:
  ```json
  {
    "type": "authenticated",
    "session_id": "00000000-0000-0000-0000-000000000000"
  }
  ```

### 2.5 Canonical Digest Computation & Lexicographical Sorting
The HMAC-SHA256 signature is calculated over the canonical UTF-8 digest string:
```
{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities_str}:{payload_hash}
```
- `capabilities_str`: Lexicographically sorted, comma-separated list of capabilities. If no capabilities are supplied, this segment is empty.
- `payload_hash`: Hexadecimal SHA-256 digest of payload bytes (`hex::encode(Sha256::digest(payload))`). For empty or `null` payloads, use the empty string SHA-256 digest: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

### 2.6 Millisecond Unix Timestamps
- Timestamp fields in authentication headers (`timestamp_millis`) and ping/pong diagnostics (`client_timestamp`, `server_timestamp`) use **epoch milliseconds** as 64-bit signed integers (`i64`), not seconds.
- The daemon enforces a clock skew tolerance of `±60,000 ms` (60 seconds).

### 2.7 Task Submission Schema: `task_spec`
- The payload parameter in `IpcCommand::SubmitTask` is strictly named `task_spec` (not `spec`):
  ```json
  {
    "type": "submit_task",
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "task_spec": {
      "key": "task-01",
      "title": "Compile codebase",
      "description": "Run cargo check",
      "assigned_agent_id": null,
      "parent_task_key": null,
      "parent_task_id": null,
      "dependency_keys": [],
      "dependency_task_ids": [],
      "dependency_output_policy": "task_success"
    }
  }
  ```

### 2.8 Event Subscription Watermark: `from_sequence`
- The starting sequence parameter in `IpcCommand::SubscribeEvents` is strictly named `from_sequence` (not `start_sequence` or `since_sequence`):
  ```json
  {
    "type": "subscribe_events",
    "from_sequence": 1
  }
  ```
- Specifying `from_sequence = N` streams all retained events from sequence `N` onward, followed by live real-time events.

### 2.9 Cancellation Summary Response
- `IpcCommand::CancelTask` returns `IpcResponse::TaskCancelled` containing an encapsulated `CancellationSummary`:
  ```json
  {
    "type": "task_cancelled",
    "summary": {
      "cancelled_tasks": ["d0a80101-0000-0000-0000-000000000001"],
      "cancelled_runs": ["e0a80101-0000-0000-0000-000000000001"],
      "cancelled_approvals": []
    }
  }
  ```

### 2.10 Approval Decision Tagging
- `ApprovalDecision` uses internal Serde tagging on the field `"decision"`:
  - **Approved**:
    ```json
    {
      "decision": "approved",
      "modified_details": null,
      "user_note": "Operator confirmed action"
    }
    ```
  - **Rejected**:
    ```json
    {
      "decision": "rejected",
      "reason": "Access denied by security policy"
    }
    ```

### 2.11 Dual Bootstrap Credential Discovery Files
The daemon produces two segregated discovery files on startup:
1. **Standard Client Discovery (`agsd.json`)**: Contains standard credentials only:
   ```json
   {
     "endpoint": "\\\\.\\pipe\\agent-studios-default",
     "key_id": "ags-client",
     "client_key": "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
     "pid": 12345,
     "created_at": 1793400000
   }
   ```
2. **Administrative Discovery (`agsd-admin.json`)**: Contains administrative credentials:
   ```json
   {
     "endpoint": "\\\\.\\pipe\\agent-studios-default",
     "key_id": "ags-client-admin",
     "admin_key": "a81d4fae-7dec-11d0-a765-00a0c91e6bf6",
     "pid": 12345,
     "created_at": 1793400000
   }
   ```

### 2.12 Platform Discovery File Paths
- **Windows**:
  - Standard: `%LOCALAPPDATA%\agent-studios\agsd.json`
  - Admin: `%LOCALAPPDATA%\agent-studios\agsd-admin.json`
- **Linux / macOS**:
  - Primary: `$XDG_RUNTIME_DIR/agent-studios/agsd.json` (and `agsd-admin.json`)
  - Fallback: `/tmp/agent-studios-<UID>/agsd.json` (and `agsd-admin.json`)

### 2.13 Filesystem ACL & Permission Boundaries
- Discovery files and Unix domain sockets are protected with strict permissions:
  - Unix: Mode `0600` (read/write by owner UID only) on socket and credential files; parent directory mode `0700`.
  - Windows: Explicit discretionary access control lists (DACL) granting full control only to the current user SID and local system.
- Note: Both standard and admin discovery files reside in user-scoped directories and share the same OS-level discretionary access boundary. True process-level sandboxing of clients requires separate OS service accounts.

### 2.14 Cryptographic Session Ownership & Rebinding
- Standard clients are authorized to cancel tasks or resolve approvals only if their authenticated `key_id` matches the origin of the task/approval.
- If a client disconnects and reconnects with a new ephemeral `SessionId`, ownership is preserved through the authenticated `key_id`, and the task/approval origin is rebound to the new active session.
- Tasks lacking tracked origins fail closed for standard clients, requiring `ClientRole::Admin`.

### 2.15 Deterministic Model & Runtime Routing
- If the catalog contains multiple models or multiple runtime instances without an explicit route (`IpcServerBuilder::with_default_model_ref` or `AGENT_STUDIOS_DEFAULT_MODEL`), task execution strictly fails closed with an actionable configuration error.
- Clients can query `ListProviders` and `ListModels` to inspect registered models and runtime instances.

---

## 3. Two-Phase Approval Delivery (2PC) Guarantees

When human-in-the-loop approvals are resolved via `IpcCommand::ResolveApproval`:
1. **Pre-Execution Runtime Delivery**: The resolution decision is dispatched to the active agent runtime process (`runtime.send(&session_ref, input)`) *prior* to committing state in the Control Plane.
2. **Fail-Closed on Delivery Failure**: If delivery to the runtime fails (e.g., process terminated or network error), the Control Plane mutation is aborted, and the approval remains `Pending` to allow subsequent operator retry.
3. **Concurrency Serialization**: In-flight approval resolutions are serialized with RAII locks (`InFlightApprovalGuard`). Simultaneous attempts to resolve the same approval fail closed without race conditions.

---

## 4. Test Vectors & Golden Fixture Validation

A complete suite of 11 golden fixture tests has been committed to the repository in:
```
crates/ipc-transport/tests/golden_fixture_tests.rs
```
Detailed wire schemas and JSON payloads are documented in:
```
docs/reviews/M13-IPC-02.2.2/PROTOCOL_GOLDEN_FIXTURES.md
```

All TypeScript integration code in PR #23 (`ConnectedClient`) and PR #25 (`Mission Control`) should validate its serialization directly against these golden fixtures.

---

## 5. Specific Guidance for PR #23 & PR #25

### 5.1 For PR #23 (`feat/m14-ipc-03-connected-client`)
1. **Length-Prefixed Framing Parser**: Ensure the TypeScript socket/pipe reader accumulates buffer chunks until 4 bytes are available, reads `buffer.readUInt32BE(0)`, and asserts `length <= 4194304` before allocating and parsing UTF-8 JSON.
2. **HMAC Canonical Digest**: Verify that `capabilities` array is sorted lexicographically before joining with commas, and that the SHA-256 payload digest uses lowercase hex encoding.
3. **Reconnection Recovery**: On disconnect, reconnect using the `endpoint` and `client_key` from `agsd.json`. Issue `NegotiateCapabilities`, then `Authenticate`, and finally `SubscribeEvents` using the last received `sequence` number as `from_sequence`.

### 5.2 For PR #25 (`feat/m14-mission-control-connected-integration`)
1. **Task Spec Alignment**: When dispatching tasks from Mission Control, ensure the task payload is placed under the key `"task_spec"`.
2. **Indeterminate Progress Handling**: For tasks without measurable progress or token streams, display indeterminate progress state as documented in the Mission Control audit.
3. **Approval Inbox Interaction**: When resolving an approval, transmit the tagged decision (`{"decision": "approved", ...}` or `{"decision": "rejected", ...}`) and handle potential transient delivery errors gracefully.

---

## 6. Sign-Off & Verification Evidence

The M13 IPC Daemon implementation has completed all quality gates:
- **Clippy**: 0 warnings under `-D warnings`.
- **Fmt**: 100% clean formatting.
- **Unit & Integration Tests**: 62/62 tests passing in `agent-studios-ipc-transport`, 54/54 tests passing in `agent-studios-opencode-runtime`.
- **Subtree Invariant**: Subtree `codex-rs/` is 100% untouched (0 diffs).

The contract is now officially frozen and ready for production integration.
