# IPC-03 Integration Layer & Capability Readiness Matrix

**Milestone:** M14-UI-02.1  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** IPC-03 wire framing, anti-DoS bounds, correlation engine, and truthful capability readiness mapping.

---

## 1. IPC-03 Framing & Transport Protocol

Communication between the VS Code extension host and the native Agent Studios daemon occurs over Unix Domain Sockets (Linux/macOS) and Windows Named Pipes.

To prevent framing ambiguities, buffer overruns, and stream corruption, `IpcFrameCodec.ts` and `RustIpcCodec.ts` implement a deterministic 4-byte big-endian length-prefixed protocol:

```
┌─────────────────┬──────────────────────────────────────────────────────┐
│ Length (4 Bytes)│ Payload (N Bytes UTF-8 JSON)                         │
├─────────────────┼──────────────────────────────────────────────────────┤
│ 0x00 0x00 0x00 N│ {"version":"1.0","id":"req-01","method":"...","..."} │
└─────────────────┴──────────────────────────────────────────────────────┘
```

### Safety & Robustness Guarantees:
1. **Length-Prefixed Framing:** Big-endian 32-bit unsigned integer encodes exact byte length of subsequent JSON string (`FRAME_HEADER_SIZE = 4`).
2. **Authoritative 4 MiB Max Frame Size:** Aligned with native Rust transport limits (`DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` bytes / 4 MiB). Inbound length prefixes exceeding 4 MiB immediately throw `RangeError`, terminating the connection.
3. **Outbound Fast Pre-Allocation Validation:** Outbound frames measure UTF-8 byte length before allocating buffers, rejecting oversized payloads without memory spikes.
4. **Anti-DoS Bounded Buffer Accumulation:** Partial socket stream accumulation is strictly capped at `MAX_ACCUMULATED_BUFFER_SIZE = 4 * 1024 * 1024 + 4` bytes (4,194,308 bytes). Chunk streams attempting unbounded memory exhaustion are blocked.
5. **Serde Tagged Wire Format Codec:** `RustIpcCodec.ts` provides native encoding/decoding for Rust internally tagged commands (`type: ...`), eliminating wire incompatibility risks.
6. **Canonical HMAC-SHA256 Authenticator:** `RustHmacSigner.ts` produces cryptographic headers matching `ipc-protocol/src/auth.rs`, incorporating canonical digest formatting, lexicographical capability sorting, and constant-time signature verification.

---

## 2. Request Correlation Engine (`IpcCorrelationManager.ts`)

Asynchronous RPC calls are correlated through monotonic, collision-resistant identifiers:

- **ID Format:** `req-${Date.now()}-${monotonicCounter}`
- **Correlation Lifecycle:**
  1. `createRequest(method, params, timeoutMs)`: Allocates deferred promise and sets timer (default 30,000ms).
  2. `handleResponse(frame)`: Resolves promise if `result` present; rejects with `IpcRpcError` if `error` present.
  3. `failAll(error)`: Rejects all active requests during unexpected socket disconnects or process crash.

---

## 3. Truthful Machine-Readable Readiness Matrix (`IPC_03_READINESS_MATRIX`)

The table below catalogs all 12 core control plane capabilities with truthful status classifications:

| Capability ID | Method | Description | Implementation State | Security Verdict | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `daemon.handshake` | `daemon.handshake` | Version negotiation & auth | `INCOMPATIBLE_WIRE_FORMAT` | `FAIL_CLOSED` | Rust uses `negotiate_capabilities` + HMAC `authenticate` |
| `daemon.getStatus` | `daemon.getStatus` | Daemon health & status | `INCOMPATIBLE_WIRE_FORMAT` | `VERIFIED` | Rust uses `get_health` command |
| `session.list` | `session.list` | Enumerate active sessions | `INCOMPATIBLE_WIRE_FORMAT` | `VERIFIED` | Rust uses `list_studios` command |
| `session.create` | `session.create` | Create multi-agent session | `INCOMPATIBLE_WIRE_FORMAT` | `FAIL_CLOSED` | Rust uses `create_studio` command |
| `session.switch` | `session.switch` | Switch active session | `UNIMPLEMENTED` | `FAIL_CLOSED` | Client stream re-subscription required |
| `session.sendMessage` | `session.sendMessage` | Submit task or chat prompt | `INCOMPATIBLE_WIRE_FORMAT` | `FAIL_CLOSED` | Rust uses `submit_task` with task spec |
| `session.cancelRun` | `session.cancelRun` | Cancel running task execution | `INCOMPATIBLE_WIRE_FORMAT` | `VERIFIED` | Rust uses `cancel_task` command |
| `approval.list` | `approval.list` | Enumerate pending approvals | `UNIMPLEMENTED` | `FAIL_CLOSED` | Approvals handled as reactive event stream |
| `approval.decide` | `approval.decide` | Approve or deny action | `INCOMPATIBLE_WIRE_FORMAT` | `FAIL_CLOSED` | Rust uses `resolve_approval` command |
| `artifact.list` | `artifact.list` | Enumerate diff artifacts | `UNIMPLEMENTED` | `FAIL_CLOSED` | Artifacts tracked via task graph events |
| `artifact.getDiff` | `artifact.getDiff` | Inspect artifact diff patch | `TYPESCRIPT_CONTRACT_ONLY` | `VERIFIED` | Virtual `agent-studios-diff://` document provider |
| `provider.list` | `provider.list` | Enumerate configured LLMs | `INCOMPATIBLE_WIRE_FORMAT` | `VERIFIED` | Rust uses `list_providers` + `list_models` |

### Truthful Readiness Summary:
- **Total Capabilities Cataloged:** 12
- **Verified Rust Implementation:** 0 (Honest: no direct wire RPC verified against running daemon socket)
- **TypeScript Contract Only:** 1 (`artifact.getDiff`)
- **Incompatible Wire Format:** 8 (Serde tagged union vs JSON-RPC 2.0; bridged by `RustIpcCodec`)
- **Unimplemented:** 3 (`session.switch`, `approval.list`, `artifact.list`)
- **Fail-Closed Security Gated:** 7
