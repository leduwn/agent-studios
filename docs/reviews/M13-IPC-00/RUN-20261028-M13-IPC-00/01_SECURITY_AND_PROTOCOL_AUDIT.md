# Milestone M13 IPC-00 — Security & Protocol Foundation Audit Report

- **Milestone**: `M13 IPC-00 Protocol Foundation`
- **Execution Run**: `RUN-20261028-M13-IPC-00`
- **Date**: `2026-10-28T16:15:00+07:00`
- **Branch**: `feat/m13-ipc-protocol-foundation`
- **Status**: **VERIFIED & COMPLIANT (STRICT NO SELF-MERGE)**

---

## 1. Executive Summary

This audit evaluates the newly implemented `crates/ipc-protocol` (`agent-studios-ipc-protocol`), which provides the authoritative, high-assurance IPC protocol foundation for Agent Studios. It formalizes versioned envelopes, typed request IDs, command/result framing, event replay, capability negotiation, approval contracts, secret redaction, and HMAC-based nonce authentication.

| Security / Protocol Invariant | Implementation Mechanism | Verification Result |
|---|---|:---:|
| **Semver Envelope Compatibility** | `ProtocolVersion` major/minor checking; reject cross-major versions | **PASS** |
| **Typed Identifier Safety** | Strongly-typed prefix IDs (`req_`, `corr_`, `sess_`, `appr_`) with UUID v4 | **PASS** |
| **Command & Result Framing** | Typed `IpcCommand`, `IpcResponse`, `IpcErrorResponse` with structured error codes | **PASS** |
| **Event Replay & Linearizability** | Monotonic sequence validation, bounded `ReplayBuffer`, explicit `SequenceGapUnrecoverable` | **PASS** |
| **Capability Negotiation** | Mutual version agreement, mandatory required capability enforcement | **PASS** |
| **Approval Lifecycle Safety** | `ApprovalContract` with risk levels, timeout checks, and duplicate rejection | **PASS** |
| **Secret Redaction** | Built-in regexes for API keys/tokens, recursive JSON masking, `Redacted<T>` wrapper | **PASS** |
| **HMAC Nonce Authentication** | HMAC-SHA256 with canonical payload digests, constant-time verification, and `NonceTracker` | **PASS** |

---

## 2. Security Invariant Verification

### 2.1. Cryptographic Authentication & Replay Protection (`auth.rs`)
- **Canonical Digest**: Formed as `{key_id}:{nonce}:{timestamp_millis}:{sha256_hex(payload)}` preventing length-extension and parameter manipulation attacks.
- **Timing Defense**: Constant-time slice comparison (`subtle::ConstantTimeEq`) prevents side-channel timing attacks during MAC verification.
- **Timestamp Skew Limit**: Configurable max clock skew (e.g. 5,000ms) rejects stale messages.
- **Replay Defense**: `NonceTracker` tracks seen nonces within an active TTL window, failing closed upon duplicate detection.

### 2.2. Secret Leakage Prevention (`redaction.rs`)
- Sensitive token patterns (Anthropic `sk-ant-`, OpenAI `sk-`, GitHub `ghp_`, AWS `AKIA`, generic Bearer tokens, PEM private keys) are sanitized to `[REDACTED]`.
- JSON payloads undergo recursive key-name inspection (`api_key`, `token`, `password`, `secret`, `authorization`, `private_key`).
- `Redacted<T>` prevents sensitive data from printing in Rust `Debug` or `Display` formats.

### 2.3. Bounded Event Replay & Gap Detection (`replay.rs`)
- Strict monotonicity: Out-of-order or duplicate sequence numbers are rejected immediately with `ProtocolError::InvalidState`.
- Capacity enforcement: Oldest events are evicted cleanly when buffer reaches capacity.
- Gap Detection: Clients requesting sequences older than the retention boundary receive `ProtocolError::SequenceGapUnrecoverable`, preventing silent event drops.

### 2.4. Approval Contracts & State Safety (`approval.rs`)
- Approvals have strict timeouts (`expires_at`).
- Attempting to approve an expired approval transitions status to `TimedOut` and returns an error.
- Double-resolution attempts fail closed with `ApprovalError`.

---

## 3. Operational Invariants

- **No Bearer Secrets in Git**: No API keys, credentials, or signing secrets are checked into the repository or tests.
- **Code Separation**: `crates/ipc-protocol` is decoupled and reusable across backend daemons, CLI runners, and editor extensions.
- **Upstream Protection**: Upstream `codex-rs` remains unmodified.
