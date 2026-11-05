# Security & Reliability Audit: M14-IPC-03

**Milestone:** M14-IPC-03  
**Run ID:** RUN-20261030-M14-IPC-03  
**Date:** 2026-10-30  

---

## 1. Threat Model & Security Posture

Connecting an IDE extension host to an OS background daemon introduces specific threat surfaces:
1. **Local Privilege Escalation / Symlink Hijacking:** A malicious local user creates a symlink pointing `agsd.json` to an arbitrary or sensitive file.
2. **Credential Leakage:** Exposing client or administrative keys via logs, settings, or webview DOM.
3. **Buffer Exhaustion / Denial of Service:** Streaming unbounded payloads across the socket to exhaust extension host memory.
4. **Command Injection / Unauthorized Mutation:** Prematurely allowing the extension to trigger shell commands, task executions, or filesystem modifications.

Milestone M14-IPC-03 systematically remediates each of these threat vectors.

---

## 2. Symlink Anti-Hijack & Discovery Security

In `DaemonDiscovery.ts`:
- All candidate bootstrap file locations (`AGSD_BOOTSTRAP_PATH`, active workspace `.agent-studios/agsd.json`, user home `.agent-studios/agsd.json`) are checked using `fs.promises.lstat()`.
- If the bootstrap file is detected as a symbolic link (`stat.isSymbolicLink()`), the discovery process immediately halts and throws an explicit security error:
  ```typescript
  if (stat.isSymbolicLink()) {
    throw new Error(
      `Security violation: Bootstrap file at '${resolvedPath}' is a symbolic link. Symlinks are rejected to prevent file-hijacking attacks.`
    );
  }
  ```
- File permissions are validated and parsing requires strict adherence to schema (presence of `endpoint`, `key_id`, `client_key`).

---

## 3. Cryptographic Authentication & Credential Isolation

### 3.1 Two-Phase Handshake
Authentication adheres to Rust's cryptographic handshake:
1. **Phase 1: Capability Negotiation (`negotiate_capabilities`)**
   - The extension proposes supported protocol version (`1.0.0`) and client capabilities (`['worktree_isolation', 'event_streaming']`).
   - The daemon responds with agreed version and capabilities.
2. **Phase 2: HMAC-SHA256 Authentication (`authenticate`)**
   - The extension generates a unique UUID-based `session_id`, `nonce`, and millisecond timestamp.
   - Computes canonical signature digest:
     `{version}:{key_id}:{session_id}:{nonce}:{timestamp_millis}:{sorted_capabilities}:{payload_hash}`
   - Computes HMAC-SHA256 over canonical digest using `client_key`.
   - Daemon validates signature with constant-time equality check (`crypto.timingSafeEqual`) to prevent timing side channels.

### 3.2 Credential Isolation
- The extension host **only** reads and utilizes the standard `client_key`.
- Administrative credentials (`admin_key`), if present in `agsd.json`, are explicitly omitted from transport configuration.
- `DaemonDiscovery.redact()` masks all keys (`client_key: "secr...cdef"`) before any diagnostic display.

### 3.3 Provider Secret Scrubbing
In `DaemonConnectedBridge.listProviderSummaries()`:
- Provider records retrieved from the daemon may include internal API keys or secret tokens (`authentication: { secret_key: "sk-ant-..." }`).
- The bridge maps these to `ProviderSummary` objects that **completely strip** authentication blocks.
- Verified in automated tests: `assert.strictEqual(JSON.stringify(providers).includes('SUPER-SECRET'), false)`.

---

## 4. Bounded Allocation & Anti-DoS Defenses

1. **4 MiB Length Bound:**
   - Both outbound frames and inbound headers are capped at `DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` (4 MiB).
   - Inbound length header is checked prior to allocating memory for the frame payload.
2. **Accumulator Buffer Bound:**
   - Partial stream accumulation buffer is bounded at `MAX_ACCUMULATED_BUFFER_SIZE = 4 * 1024 * 1024 + 4` bytes.
   - Any attempt to send unbounded non-terminated bytes drops the socket immediately.
3. **Timeout Protection:**
   - Every queued command has an active timeout timer (default 30 seconds). If the daemon hangs or crashes, the pending command is rejected and the queue drains safely.
