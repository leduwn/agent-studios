# M13 IPC-01 Architecture Roadmap — Native Local Transport & App Server Skeleton

- **Milestone**: `M13 IPC-01`
- **Predecessor**: `M13 IPC-00 (crates/ipc-protocol foundation)`
- **Date**: `2026-10-28`
- **Status**: **APPROVED SPECIFICATION**

---

## 1. Executive Vision & Scope

`M13 IPC-01` builds directly upon the protocol foundation established in `M13 IPC-00` (`crates/ipc-protocol`). It introduces a cross-platform, high-throughput, low-latency native local transport and application server skeleton for Agent Studios.

The application server decouples UI surfaces (VSCode extension, Code-OSS integration, TUI clients, Web dashboards) from core agent execution, Control Plane state, and workspace orchestrators via local inter-process communication.

---

## 2. Core Transport Architecture

```
+--------------------------------------------------------------------------+
|                        Client Layer (VSCode / TUI)                       |
+--------------------------------------------------------------------------+
                                     |
              IpcEnvelope<IpcCommand> / IpcEnvelope<IpcResponse>
                                     |
+------------------------------------+-------------------------------------+
|                     Platform Native Local Transport                      |
|   Windows: Named Pipes (`\\.\pipe\agent-studios-{session}.sock`)         |
|   Unix/macOS: Domain Sockets (`$XDG_RUNTIME_DIR/agent-studios.sock`)     |
+------------------------------------+-------------------------------------+
                                     |
+------------------------------------+-------------------------------------+
|                App Server Daemon (`crates/app-server`)                   |
|                                                                          |
|  - Connection Multiplexer & Handshake Coordinator                        |
|  - HMAC-SHA256 Authenticator & Nonce Tracker                             |
|  - Capability Negotiator & Version Guard                                 |
|  - Replay Buffer & Linearizable Event Stream                             |
|  - Authoritative Approval Coordinator                                    |
|  - Control Plane & Workspace Orchestration Bridge                        |
+--------------------------------------------------------------------------+
```

### 2.1. Native Transport Layer
1. **Windows Platform**:
   - Win32 Named Pipes using `tokio::net::windows::named_pipe`.
   - Pipe naming convention: `\\.\pipe\agent-studios-{studio_id}-{instance_id}`.
   - Security: Explicit Windows Security Descriptor (DACL) restricting pipe access strictly to the current user's SID (`SECURITY_CREATOR_OWNER_RID`), denying network traverse.
2. **Unix / macOS Platform**:
   - Unix Domain Sockets (AF_UNIX) using `tokio::net::UnixListener` / `UnixStream`.
   - Socket path: `$XDG_RUNTIME_DIR/agent-studios/server.sock` (fallback: `/tmp/agent-studios-{uid}/server.sock`).
   - Permissions: Restricted to file mode `0600` (read/write current user only).

### 2.2. Frame Delimitation & Streaming
- Length-prefixed framing over byte streams:
  `[4-byte big-endian payload length u32][JSON / Bincode serialized IpcEnvelope]`
- Max frame size: 16 MiB (enforced fail-closed; oversized frames drop connection).

---

## 3. App Server Skeleton Specification

### 3.1. Server Subsystem Modules (`crates/app-server`)
1. **`server.rs`**: Main listener loop accepting client streams, spawning isolated connection actors.
2. **`connection.rs`**: Connection handler state machine:
   - State `Connecting`: Awaiting Handshake / CapabilityNegotiation.
   - State `Authenticated`: Nonce verified, capabilities agreed.
   - State `Active`: Processing commands, streaming events.
   - State `Terminated`: Resources released, worktrees retained or cleaned.
3. **`session_manager.rs`**: Tracks active client sessions, leases, and pending approvals.
4. **`event_broker.rs`**: Fan-out broadcast channel connecting Control Plane events to `ReplayBuffer` and client subscriptions.

### 3.2. Lifecycle & Handshake Flow
1. **Connection**: Client establishes pipe/socket connection.
2. **Negotiation**: Client sends `IpcEnvelope<IpcCommand::Negotiate(CapabilityNegotiationRequest)>`.
3. **Authentication**: If `HmacAuth` capability is negotiated, client signs challenge nonce with shared secret key.
4. **Agreed Response**: Server replies with `CapabilityNegotiationResponse`.
5. **Operational Phase**: Client issues commands (`SubmitGoal`, `ResolveApproval`, `SubscribeEvents`).
6. **Graceful Teardown**: Either side sends clean disconnect frame; server unbinds leases and closes socket.

---

## 4. Security & Isolation Realism

1. **Local Access Boundary**:
   - OS-level DACL and permission bits ensure other local accounts cannot connect to the pipe/socket.
2. **Replay & Injection Defense**:
   - Per-envelope HMAC-SHA256 signature and `NonceTracker` prevents intercepted frame replay.
3. **Secret Redaction**:
   - `SecretRedactor` filters all outgoing diagnostic events and logs, preventing API key leakage.
4. **Workspace Isolation**:
   - Mutating commands require physical detached worktrees (`WorkspaceOrchestrator`). Shared source root execution is strictly prohibited under mutating mode.

---

## 5. Phased Implementation Roadmap

| Phase | Milestone | Focus Areas | Deliverables |
|---|---|---|---|
| **Phase 0** | **M13 IPC-00** | Protocol Foundation | `crates/ipc-protocol` (envelopes, framing, auth, replay, redaction) |
| **Phase 1** | **M13 IPC-01** | Transport & Server Skeleton | `crates/app-server` with Named Pipe & Unix Socket listeners, handshake, connection actor |
| **Phase 2** | **M13 IPC-02** | Control Plane Integration | Linking `app-server` to `agent-studios-control-plane` and `WorkspaceOrchestrator` |
| **Phase 3** | **M13 IPC-03** | Code-OSS / VSCode Client | TypeScript IPC client adapter connecting editor UI to native `app-server` |

---

## 6. Success Metrics & Verification Gates

- **Latency**: Local loopback roundtrip command latency `< 1.0 ms` for ping and task status queries.
- **Throughput**: Sustained event broadcast `> 10,000 events/sec` with zero dropped frames.
- **Reliability**: 100% fail-closed behavior on unauthorized connection, bad HMAC signature, or version mismatch.
- **Resource Discipline**: Zero memory leakage on rapid client connect/disconnect cycles.
