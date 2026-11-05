# Milestone M13 IPC-00 — Handoff & Draft PR Notice

- **Milestone**: `M13 IPC-00 Protocol Foundation`
- **Execution Run**: `RUN-20261028-M13-IPC-00`
- **Date**: `2026-10-28T16:17:00+07:00`
- **Branch**: `feat/m13-ipc-protocol-foundation`
- **Target Base**: `origin/main` (`dc363d1e89fe8751c001f9829bd2cd0b9f7d37f9`)
- **Status**: **READY FOR DRAFT PR (STRICT NO SELF-MERGE)**

---

## 1. Scope Delivered

1. **Protocol Foundation (`crates/ipc-protocol`)**:
   - `id`: Strongly-typed UUID-backed IDs (`RequestId`, `CorrelationId`, `SessionId`, `ApprovalId`, `ClientId`, `SubscriptionId`).
   - `envelope`: Versioned envelopes with major/minor semver compatibility.
   - `auth`: HMAC-SHA256 signing, constant-time verification, canonical digests, and nonce replay tracking.
   - `capabilities`: Client/server negotiation with mandatory capability verification.
   - `framing`: `IpcCommand` and `IpcResponse` structures with typed `ErrorCode`.
   - `approval`: `ApprovalContract` with risk classification, timeouts, and state transitions.
   - `redaction`: Secret sanitization regexes, recursive JSON redaction, and `Redacted<T>`.
   - `replay`: Monotonic event `ReplayBuffer` with `SequenceGapUnrecoverable` tracking.
2. **Architecture Roadmap**:
   - Documented in `docs/architecture/M13-IPC-01-ROADMAP.md`.
3. **Audit & Evidence**:
   - Comprehensive audit reports in `docs/reviews/M13-IPC-00/RUN-20261028-M13-IPC-00/`.

---

## 2. Draft PR Instructions

Push `feat/m13-ipc-protocol-foundation` to origin and open a Draft Pull Request against `main`. In compliance with project security guidelines, this pull request must strictly remain unmerged pending team review.
