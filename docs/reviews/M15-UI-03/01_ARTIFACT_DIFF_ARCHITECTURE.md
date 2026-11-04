# M15 UI-03 — Artifact & Diff Review Architecture

## Metadata
- **Milestone**: M15 UI-03 (Artifact & Diff Review Workbench)
- **Topic**: Architecture of Lossless Base64 Pagination & Extension Host SHA-256 Verification
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Pinned Backend Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29 / M15-03.1)
- **Date**: 2026-11-02

---

## 1. Architectural Overview

Milestone M15 UI-03 introduces a professional, strictly read-only Artifact & Diff Review Workbench in the VS Code extension for inspecting artifacts captured by the authoritative Rust daemon (`agsd`), reviewing unified Git patches, and verifying patch integrity cryptographically.

```
+-----------------------------------------------------------------------------------------+
|                                    VS Code Extension Host                               |
|                                                                                         |
|  +---------------------------+       +------------------------+                         |
|  |    ArtifactReviewPanel    | <---> |     ArtifactClient     |                         |
|  |  (WebviewPanel / CSP)     |       | - Base64 raw decoding  |                         |
|  +---------------------------+       | - SHA-256 computation  |                         |
|                ^                     | - Chunk integrity      |                         |
|                |                     | - Memory bounds        |                         |
|        +---------------+             +------------------------+                         |
|        |  DiffParser   |                         ^                                      |
|        | (Hunks/Files) |                         | NativeIpcTransport                   |
+--------+---------------+-------------------------+--------------------------------------+
                                                   | (Framed IPC Pipe / UDS)
+--------------------------------------------------v--------------------------------------+
|                               Authoritative Rust Daemon (agsd)                          |
|                                                                                         |
|  get_task_diff / get_artifact / list_task_artifacts                                     |
|  - Flat Serde Envelopes                                                                 |
|  - Dual-Emission: Base64 streaming + legacy string suppression                          |
|  - 4 MiB wire frame budget compliance                                                   |
+-----------------------------------------------------------------------------------------+
```

---

## 2. Invariants & Reliability Guarantees

1. **Lossless Base64 Chunk Transport**:
   - Chunks are decoded directly into binary `Buffer` (`Uint8Array`) before string decoding, preventing multi-byte UTF-8 character slicing corruption across arbitrary chunk boundaries (e.g. Vietnamese diacritics).
   - Binary Git patches (`GIT binary patch`) are faithfully preserved without Unicode coercion.

2. **Extension Host Cryptographic Integrity**:
   - SHA-256 digest is calculated over the fully reassembled binary buffer using Node.js `crypto.createHash('sha256')`.
   - The computed hash is compared against the backend-asserted `patch_sha256` or `content_hash` (normalizing any `sha256:` prefix).
   - Status transitions strictly between: `VERIFIED`, `INTEGRITY_FAILED`, or `UNVERIFIED`. Never hardcoded `true`.

3. **Lossless Pagination & Infinite Loop Guards**:
   - Monotonic offset progression: incoming `offset` must exactly equal `current_offset`.
   - Chunk byte count verification: decoded Base64 length must equal `chunk_bytes` metadata.
   - Zero-progress detection: if `has_more: true` but chunk size is 0 or offset does not advance, fails immediately with `ZERO_PROGRESS`.
   - Total bytes consistency: `total_bytes` cannot mutate mid-stream.
   - Aggregate memory bounds: cumulative buffer cannot exceed 16 MiB default (`EXCEEDED_MAX_BYTES`).
   - Session invalidation: IPC reconnection aborts running assembly with `SESSION_CHANGED`.

4. **Strictly Read-Only Boundary**:
   - Webview is pure visual presentation. No arbitrary code execution, no shell execution, no direct disk writes.
   - `task.submit` remains statically disabled (`enabled: false`).
