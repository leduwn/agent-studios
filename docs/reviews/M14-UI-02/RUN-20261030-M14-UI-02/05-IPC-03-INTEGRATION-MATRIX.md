# IPC-03 Integration Layer & Capability Readiness Matrix

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Scope:** IPC-03 wire framing, correlation engine, and daemon capability readiness mapping.

---

## 1. IPC-03 Framing & Transport Protocol

Communication between the VS Code extension host and the native Agent Studios daemon occurs over Unix Domain Sockets (Linux/macOS) and Windows Named Pipes.

To prevent framing ambiguities and stream corruption, `IpcFrameCodec.ts` implements a deterministic 4-byte big-endian length-prefixed protocol:

```
┌─────────────────┬──────────────────────────────────────────────────────┐
│ Length (4 Bytes)│ Payload (N Bytes UTF-8 JSON)                         │
├─────────────────┼──────────────────────────────────────────────────────┤
│ 0x00 0x00 0x00 N│ {"version":"1.0","id":"req-01","method":"...","..."} │
└─────────────────┴──────────────────────────────────────────────────────┘
```

### Safety & Robustness Guarantees:
1. **Length-Prefixed Framing:** Big-endian 32-bit unsigned integer encodes exact byte length of subsequent JSON string.
2. **Chunk Reassembly:** Handles fragmented incoming socket chunks by maintaining residual buffers until complete frame arrives.
3. **DoS Protection:** Throws immediate protocol error if frame size exceeds **16 MB** (`16 * 1024 * 1024`), blocking memory exhaustion attacks.

---

## 2. Request Correlation Engine (`IpcCorrelationManager.ts`)

Asynchronous RPC calls are correlated through monotonic, collision-resistant identifiers:

- **ID Format:** `req-${Date.now()}-${monotonicCounter}`
- **Correlation Lifecycle:**
  1. `createRequest(method, params, timeoutMs)`: Allocates deferred promise and sets timer (default 30,000ms).
  2. `handleResponse(frame)`: Resolves promise if `result` present; rejects with `IpcRpcError` if `error` present.
  3. `failAll(error)`: Rejects all active requests during unexpected socket disconnects or process crash.

---

## 3. Machine-Readable Readiness Matrix (`IPC_03_READINESS_MATRIX`)

The table below catalogs all 9 core control plane RPC capabilities between VS Code extension and native daemon:

| Capability ID | RPC Method | Description | Implementation State | Security Verdict | Notes |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `daemon.handshake` | `daemon.handshake` | Protocol version negotiation & client auth | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Validates protocol version `1.0` |
| `session.list` | `session.list` | Enumerate active and past sessions | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Populates Sessions TreeView |
| `session.create` | `session.create` | Allocate new multi-agent session | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Returns fresh session ID |
| `session.switch` | `session.switch` | Switch active session context | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Resets message history view |
| `chat.send` | `chat.send` | Send prompt with mode & attachments | `IMPLEMENTED_UNVERIFIED` | `FAIL_CLOSED` | Streaming chunks via SSE/events |
| `chat.cancel` | `chat.cancel` | Abort in-flight model execution | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Stops daemon token generation |
| `approval.decide` | `approval.decide` | Approve/deny tool action | `IMPLEMENTED_IN_RUST` | `FAIL_CLOSED` | Critical security gate |
| `artifact.list` | `artifact.list` | List modified diff artifacts | `MOCK_ONLY` | `FAIL_CLOSED` | Pending Rust diff provider hook |
| `provider.list` | `provider.list` | Enumerate configured AI model providers | `IMPLEMENTED_IN_RUST` | `VERIFIED` | Ingests native daemon providers |

### Readiness Summary Metrics:
- **Total Capabilities Cataloged:** 9
- **Implemented in Native Rust Daemon:** 6 (5 verified, 1 unverified)
- **Mock/Fixture Fallback:** 1 (`artifact.list`)
- **Missing / Unimplemented:** 0
- **Fail-Closed Security Gated:** 3 (`chat.send`, `approval.decide`, `artifact.list`)
