# M15 UI-03.1 — Report 12: Real Artifact IPC Tests

## Metadata
- **Milestone**: M15 UI-03.1: DIFF REVIEW CORRECTNESS, ASYNC SAFETY & REAL IPC VERIFICATION
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Backend Pinned Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29 / M15-03.1)
- **Status**: VERIFIED & COMPLETE
- **Date**: 2026-11-02

---

## 1. Test Tier Classification

Per M15-UI-03.1 requirements, test tiers are strictly segregated:
- `UNIT`: Unit tests in `src/test/unit/*.test.ts` verifying parsing, command routing, and generation tracking.
- `MOCK_IPC`: Transport mocking testing synthetic error responses and chunk reassembly logic.
- `REAL_AGSD_READ_ONLY_IPC`: Dedicated E2E tests in `src/test/e2e/RealArtifactIpcE2E.test.ts` communicating with compiled `agsd.exe` over live Windows named pipe / Unix domain socket.
- `IN_PROCESS_REAL_ARTIFACT_STORE`: Reassembly and cryptographic SHA-256 verification tests using real binary buffers.
- `EXTENSION_HOST`: VSCode extension host interaction tests (modals, clipboard, save dialogs).
- `REAL_AI_MODEL_NOT_TESTED`: No billable LLM APIs called.

---

## 2. Real Backend Protocol Alignment

### A. Authoritative UUID Strictness
In `agent-studios-rs`, protocol types `StudioId`, `TaskId`, and `ArtifactId` are defined via `define_id!` macro as transparent `Uuid` wrappers.
Passing non-UUID strings (e.g. `'studio-1'`) causes Serde deserialization failure on the Rust daemon and socket disconnection.
The test suite generates valid RFC 4122 v4 UUIDs via `crypto.randomUUID()`.

### B. Read-Only Safety
Tests run strictly read-only commands: `get_task_diff`, `list_task_artifacts`, and `get_artifact`.
No mutation commands (`create_task`, `submit_task`, `mutate`) are invoked.

---

## 3. Test Coverage & Results

Suite: `src/test/e2e/RealArtifactIpcE2E.test.ts` (11 tests executed, 11 passed):
1. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): executes read-only get_task_diff on live agsd without crashing daemon` -> PASS
2. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): validates error response for non-existent task while keeping IPC connection alive` -> PASS
3. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): rejects zero-limit request with protocol error` -> PASS
4. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): queries list_task_artifacts on live agsd` -> PASS
5. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): queries get_artifact for non-existent artifact on live agsd` -> PASS
6. `Tier 3 (REAL_AGSD_READ_ONLY_IPC): ArtifactClient.fetchTaskDiff wraps daemon error cleanly` -> PASS
7. `Tier 3 (IN_PROCESS_REAL_ARTIFACT_STORE): multi-chunk lossless UTF-8 reassembly and SHA-256 verification` -> PASS
8. `Tier 3 (IN_PROCESS_REAL_ARTIFACT_STORE): multi-byte Vietnamese Unicode split across arbitrary byte boundaries` -> PASS
9. `Tier 3 (IN_PROCESS_REAL_ARTIFACT_STORE): binary Git patch chunk streaming with isBinary true` -> PASS
10. `Tier 3 (IN_PROCESS_REAL_ARTIFACT_STORE): detects cryptographic tampering and marks INTEGRITY_FAILED` -> PASS
11. `Tier 3 (IN_PROCESS_REAL_ARTIFACT_STORE): rejects cross-studio and cross-task identity mismatches` -> PASS
