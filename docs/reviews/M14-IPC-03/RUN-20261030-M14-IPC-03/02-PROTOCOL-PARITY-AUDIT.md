# Protocol Parity & Serde Wire Audit: M14-IPC-03

**Milestone:** M14-IPC-03  
**Run ID:** RUN-20261030-M14-IPC-03  
**Date:** 2026-10-30  

---

## 1. Wire Framing & Protocol Parity

Milestone M14-IPC-03 eliminates provisional JSON-RPC 2.0 assumptions and delivers full parity with the native Rust `agsd` wire protocol defined in `agent-studios-rs/crates/ags-ipc`.

### 1.1 Serde Internally Tagged Union Representation
Rust's `IpcCommand` and `IpcResponse` enums use `#[serde(tag = "type", rename_all = "snake_case")]`. Every message on the wire is an internally tagged JSON object:

- **Command Example:**
  ```json
  {
    "type": "get_studio",
    "studio_id": "studio-001"
  }
  ```

- **Response Example:**
  ```json
  {
    "type": "studio",
    "studio": {
      "id": "studio-001",
      "name": "Autonomous Architecture Agent",
      "workspace_path": "/workspace",
      "created_at": "2026-10-30T12:00:00Z"
    }
  }
  ```

- **Event Example:**
  ```json
  {
    "type": "event",
    "event": {
      "sequence": 42,
      "timestamp": "2026-10-30T12:05:00Z",
      "event_type": "task_started",
      "payload": {
        "task_id": "task-01",
        "summary": "Building protobuf schemas"
      }
    }
  }
  ```

### 1.2 Framing & Buffer Bounds Discipline
All frames transmitted across the socket follow strict length-prefixed framing:
1. **Length Header:** 4 bytes, unsigned 32-bit big-endian (`UInt32BE`).
2. **Payload:** UTF-8 encoded JSON string matching the length specified in the header.
3. **Outbound Pre-Allocation Check:** `Buffer.byteLength(json, 'utf8') <= maxFrameSize (4 MiB)`. Any payload exceeding 4 MiB is rejected before memory allocation.
4. **Inbound Frame Bound Check:** Length prefix read from the header must satisfy `0 <= frameLength <= maxFrameSize`. A frame specifying $> 4\text{ MiB}$ throws `RangeError` and drops the connection fail-closed.
5. **Anti-DoS Accumulator Cap:** Unprocessed stream chunks accumulated in `accumulatedBuffer` are strictly capped at `MAX_ACCUMULATED_BUFFER_SIZE = 4 MiB + 4` bytes. An attacker attempting to stream unbounded partial fragments is immediately disconnected.

---

## 2. Serialized Single-Flight Command Queue

### The Protocol Challenge
In the native Rust daemon, command responses do not contain request correlation identifiers (unlike JSON-RPC which carries `"id": 1`). If multiple commands were written concurrently to the socket, responses returning out-of-order or in rapid succession would result in catastrophic promise mismatch or data corruption.

### The Architectural Solution
`NativeIpcTransport` enforces strict single-flight FIFO command serialization:
```typescript
interface QueuedCommand {
  command: RustIpcCommand;
  resolve: (response: RustIpcResponse) => void;
  reject: (error: Error) => void;
  timeoutTimer: NodeJS.Timeout;
}
```
1. Commands sent via `sendCommand(cmd)` are appended to a FIFO `requestQueue`.
2. When the socket is idle (`inFlight === null`), the next command is popped, encoded, and dispatched.
3. A per-command timer (default 30,000ms) guarantees that a dead or hanging daemon does not deadlock the extension host.
4. When a command response arrives, the active `inFlight` command is resolved/rejected, and `drainQueue()` immediately dispatches the next queued command.

---

## 3. Asynchronous Streaming vs. Command Dispatch

### The Protocol Discrepancy
In the Rust backend, `SubscribeEvents` is not an RPC method returning a synchronous confirmation; instead, it is a stream activation signal. Once received, the daemon directly emits asynchronous `RustIpcEvent` frames whenever events occur.

### Out-of-Band Event Routing
In `NativeIpcTransport`:
1. `subscribeEvents(fromSequence?: number)` writes the wire frame directly to the socket without enqueueing a command promise into `inFlight`.
2. When frames arrive:
   - If `response.type === 'event'`: The frame is dispatched asynchronously to `NativeIpcTransport.emit('event', event)` without touching or completing the in-flight command promise.
   - If `response.type !== 'event'`: The frame is treated as the resolution for the current `inFlight` command.

---

## 4. Sequence Watermarking & Replay Deduplication

To provide reliable event streaming over reconnects without losing state or processing duplicate events:
1. `NativeIpcTransport` tracks `lastReceivedSequence: number` monotonically.
2. `DaemonConnectedBridge` maintains `_lastWatermark: number` and a deduplication set `_seenEventSequences: Set<number>`.
3. Upon reconnection, `subscribeEvents(_lastWatermark + 1)` requests replay only from the next expected sequence number.
4. Any duplicates received due to at-least-once transport delivery are checked against `_seenEventSequences` and silently discarded.
