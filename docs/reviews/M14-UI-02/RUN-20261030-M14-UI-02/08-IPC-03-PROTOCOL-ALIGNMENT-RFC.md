# RFC 08: IPC-03 Protocol Alignment & Wire Contract Specification

**Milestone:** M14-UI-02.1  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Authors:** Principal VS Code Extension Engineer, Security Architect  
**Audience:** Rust Daemon Core Team, IPC Transport Owners, VS Code Extension Host Engineers  
**Status:** PROPOSED / ARCHITECTURAL RFC  

---

## 1. Executive Summary

During the implementation review for Milestone M14-UI-02.1, an architectural discrepancy was identified between the VS Code extension host IPC contract assumptions and the authoritative native Rust daemon protocol checked into `agent-studios-rs/crates/ipc-protocol` and `ipc-transport`.

Specifically:
1. **Wire Encoding Discrepancy:** The VS Code extension contract previously assumed a standard JSON-RPC 2.0 framing format (`{"jsonrpc": "2.0", "method": "...", "id": "...", "params": {}}`). In contrast, the authoritative Rust daemon implements a Serde internally tagged command format (`{"type": "command_name", ...}`).
2. **Authentication Flow Discrepancy:** The Rust daemon enforces a two-step handshake: capability negotiation (`negotiate_capabilities`) followed by HMAC-SHA256 canonical digest authentication (`authenticate`), rather than an ad-hoc token exchange.
3. **Frame Boundary Discrepancy:** The VS Code framing layer permitted frames up to 16 MiB, whereas the authoritative Rust transport (`ipc-transport/src/framing.rs`) strictly caps frames at 4 MiB (`DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024`), rejecting larger headers on the wire.

To resolve this without destabilizing either code base, this RFC articulates the authoritative protocol contract and presents two concrete engineering paths:
- **Approach A (Immediate TypeScript Adapter):** Implement a TypeScript codec (`RustIpcCodec` and `RustHmacSigner`) that natively speaks the current Rust Serde protocol, allowing immediate integration without modifying backend Rust crates.
- **Approach B (Versioned Daemon Evolution):** Propose a future versioned protocol envelope (e.g., Protocol v1.1 or v2.0) that unifies JSON-RPC 2.0 semantics across language boundaries.

---

## 2. Authoritative Rust Wire Protocol Analysis

Inspection of `agent-studios-rs/crates/ipc-protocol/src/` and `ipc-transport/src/` establishes the following authoritative facts:

### 2.1 Wire Framing (`ipc-transport/src/framing.rs`)
- **Header:** 4 bytes, unsigned 32-bit big-endian integer representing payload byte length (`u32::to_be_bytes()`).
- **Maximum Frame Size:** `4,194,304` bytes (4 MiB). Frames with length prefix `> 4 * 1024 * 1024` cause immediate transport disconnection.
- **Payload:** Raw UTF-8 encoded JSON string without trailing delimiters.

```
0                   1                   2                   3
0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Payload Length (u32 BE)                  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                    UTF-8 JSON Payload                         +
|                     (Length bytes)                            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### 2.2 Wire Serialization (`ipc-protocol/src/envelope.rs`)
The Rust protocol uses `#[serde(tag = "type", rename_all = "snake_case")]`. Commands sent to the daemon and responses emitted by the daemon are internally tagged JSON objects:

#### Example: Client Command
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
    "provider_selection"
  ]
}
```

#### Example: Daemon Response
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
    "provider_selection"
  ]
}
```

### 2.3 Authentication Protocol (`ipc-protocol/src/auth.rs`)
Authentication requires an HMAC-SHA256 signature calculated over an exact canonical digest string:

`{protocol_version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{capabilities}:{payload_hash}`

Where:
- `protocol_version`: Formatted as `{major}.{minor}.{patch}` (e.g. `1.0.0`).
- `key_id`: Client identifier string.
- `session_id`: Unique session UUID.
- `nonce`: Cryptographic random string.
- `timestamp_millis`: UTC timestamp in milliseconds. Max skew: 60,000ms.
- `capabilities`: Comma-separated list of capabilities, **sorted in ascending lexicographical order**.
- `payload_hash`: Lowercase hex-encoded SHA-256 hash of the command payload (or empty string SHA-256 if no body).
- `signature`: 64-character lowercase hex string of the HMAC-SHA256 computed using the shared pre-shared key (PSK).
- Comparison: Constant-time comparison to prevent timing attacks.

---

## 3. Command & Response Compatibility Matrix

The following table contrasts the existing TypeScript RPC abstraction with the authoritative Rust daemon commands:

| VS Code RPC Method | Rust Command (`type`) | Rust Response (`type`) | Auth Required | Status | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `daemon.handshake` | `negotiate_capabilities`<br>`authenticate` | `capabilities_negotiated`<br>`authenticated` | Yes (HMAC) | **INCOMPATIBLE_WIRE_FORMAT** | Requires 2-step Serde handshake |
| `daemon.getStatus` | `get_health` | `health_status` | No | **INCOMPATIBLE_WIRE_FORMAT** | Returns daemon uptime & health |
| `session.list` | `list_studios` | `studio_list` | Yes | **INCOMPATIBLE_WIRE_FORMAT** | UI "Sessions" = Rust "Studios" |
| `session.create` | `create_studio` | `studio_created` | Yes | **INCOMPATIBLE_WIRE_FORMAT** | Creates studio workspace |
| `session.switch` | N/A (Event stream) | N/A | Yes | **UNIMPLEMENTED** | Client-side stream re-subscription |
| `session.sendMessage` | `submit_task` | `task_submitted` | Yes | **INCOMPATIBLE_WIRE_FORMAT** | Prompt submitted as task spec |
| `session.cancelRun` | `cancel_task` | `task_cancelled` | Yes | **INCOMPATIBLE_WIRE_FORMAT** | Aborts running studio task |
| `approval.list` | N/A (Event stream) | N/A | Yes | **UNIMPLEMENTED** | Approvals emitted via task events |
| `approval.decide` | `resolve_approval` | `approval_resolved` | Yes | **INCOMPATIBLE_WIRE_FORMAT** | Decision: `approved` / `rejected` |
| `artifact.list` | N/A (Event stream) | N/A | Yes | **UNIMPLEMENTED** | Managed in task artifacts map |
| `artifact.getDiff` | Virtual URI Provider | In-Memory Document | No | **TYPESCRIPT_CONTRACT_ONLY** | Local `agent-studios-diff://` |
| `provider.list` | `list_providers`<br>`list_models` | `provider_list`<br>`model_list` | No | **INCOMPATIBLE_WIRE_FORMAT** | Two commands consolidated in UI |

---

## 4. Architectural Alternatives

### Approach A: TypeScript Native Rust Codec (Implemented & Verified)
In Approach A, the TypeScript extension host adapts to the daemon's existing protocol without requiring changes to backend Rust crates:
- **Component 1 (`RustIpcCodec.ts`):** Encodes `RustIpcCommand` objects into 4-byte BE length-prefixed Serde JSON frames and decodes inbound `RustIpcResponse` frames.
- **Component 2 (`RustHmacSigner.ts`):** Computes canonical digest strings, generates HMAC-SHA256 headers, and verifies signatures using `crypto.createHmac` and `crypto.timingSafeEqual`.
- **Component 3 (`IpcFrameCodec.ts` Hardening):** Aligned maximum frame size from 16 MiB down to the authoritative 4 MiB limit, added fast pre-allocation validation, and bounded partial stream buffer accumulation to 4 MiB + 4 bytes.
- **Pros:** Zero modifications to Rust code; respects parallel ownership of daemon; immediate testability.
- **Cons:** Extension host must maintain translation layer between UI events and Rust studio/task constructs.

### Approach B: Versioned Daemon Evolution (Proposed RFC)
In Approach B, the daemon introduces a versioned multiplexer (e.g. `v1.1` or `v2.0`) supporting standard JSON-RPC 2.0 encapsulation:
- **Wire Envelope:**
  ```json
  {
    "jsonrpc": "2.0",
    "id": "req-1761820800000-1",
    "method": "session.list",
    "params": { ... }
  }
  ```
- **Pros:** Aligns with standard language server / debug adapter protocols; simpler client tooling across languages.
- **Cons:** Requires backend daemon refactoring and migration window.

---

## 5. Security & Verification Guarantees

1. **Anti-DoS Memory Bounds:** Both TypeScript codecs (`IpcFrameCodec` and `RustIpcCodec`) enforce `DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` bytes. Inbound frames exceeding 4 MiB are rejected before buffer allocation.
2. **Buffer Exhaustion Defense:** Inbound stream buffer accumulation is capped at `MAX_ACCUMULATED_BUFFER_SIZE = 4 * 1024 * 1024 + 4` bytes. Streams attempting unbounded chunk accumulation are abruptly terminated.
3. **Replay & Tampering Defense:** HMAC authentication mandates timestamp skew validation (`skew <= 60000ms`) and canonical capability sorting.
4. **Fail-Closed Security Posture:** Unverified or incompatible wire commands fail closed, maintaining UI read-only or developer preview states rather than executing unauthenticated operations.

---

## 6. Implementation Recommendation

1. **Short Term (M14-UI-02.1):** Adopt Approach A. Ship `RustIpcCodec.ts` and `RustHmacSigner.ts` within the extension host repository, keeping all 274 unit tests and 14 integration tests passing.
2. **Medium Term (M14-UI-03 / IPC-03):** When connecting live sockets to the Rust daemon, initialize the connection using `RustHmacSigner` and `RustIpcCodec` with the daemon's local PSK socket credential.
3. **Long Term (M15):** Evaluate Approach B during future cross-language RPC standardization.
