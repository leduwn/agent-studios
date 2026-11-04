# M15 UI-03: Artifact & Diff Review Workbench Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a read-only Artifact & Diff Review experience in the Agent Studios VS Code extension, supporting lossless Base64 unified diff pagination, cryptographic SHA-256 integrity verification, native file-change review, Chat/Mission Control integration, and real Rust daemon E2E tests against pinned backend commit `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`.

**Architecture:** A layered architecture with typed Rust IPC schemas, a lossless byte-accurate streaming pagination client with crypto SHA-256 checks in the Extension Host, a standalone read-only ArtifactReviewPanel webview with diff parsing and file filtering, and tight integration into Chat and Mission Control task cards without modifying the source repository.

**Tech Stack:** TypeScript, VS Code Extension API, Node.js `crypto` & `buffer`, Mocha/Chai, GitHub Actions (Ubuntu & Windows).

**Spec:** [M15-03.1 Claude 2 Frontend Handoff](cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b:docs/reviews/M15-03-ARTIFACT-DIFF/CLAUDE-2-FRONTEND-HANDOFF.md) & [Backend PR #29](https://github.com/leduwn/agent-studios/pull/29).

## Global Constraints

- Pinned backend commit: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`.
- Target branch: `feat/m15-real-agent-execution-ui` on Draft PR #27.
- Strictly read-only: no arbitrary writes to the workspace, no auto-git commits/rebases, no applying patches.
- Keep `task.submit` statically disabled in `CAPABILITY_REGISTRY`.
- No billable AI calls in tests; deterministic mock/fixtures and real local `agsd` IPC tests only.
- Decode Base64 into byte buffers and compute true SHA-256 digests; never trust hardcoded `sha256Matched: true`.
- Support multi-byte UTF-8 (Vietnamese diacritics) and binary Git patches without character slicing corruption.
- Preserve test baseline: >= 534 unit tests, >= 11 mock E2E, >= 20 real agsd E2E, >= 18 extension host tests; zero ESLint errors, <= 324 warnings.

## Review Focus

1. Lossless pagination loops when `has_more` is true but offset fails to advance or chunks are zero-length.
2. UTF-8 multi-byte sequence split across chunk boundaries (e.g., Vietnamese `ế` split across 2 chunks).
3. Cryptographic mismatch detection: tampered chunk bytes or mismatching hash fails closed with `INTEGRITY_FAILED`.
4. Worktree retention and capture failure UX: clear distinction between successful task with failed diff capture vs actual task failure.
5. Large diff memory bounding: reject downloads exceeding client size policy (default 16 MiB).

---

### Task 1: Authoritative Rust IPC Artifact & Diff Protocol Types (Subagent A)

**Files:**
- Modify: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcProtocolTypes.ts`
- Modify: `apps/agent-studios-vscode/src/bridge/ipc/RustIpcCodec.ts`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/artifact_response.json`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/get_artifact_request.json`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/get_task_diff_request.json`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/list_task_artifacts_request.json`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/task_artifacts_response.json`
- Create: `apps/agent-studios-vscode/src/test/unit/fixtures/golden_wire/task_diff_response.json`
- Create: `apps/agent-studios-vscode/src/test/unit/ArtifactProtocol.test.ts`

**Interfaces:**
- Produces: `RustArtifactRecord`, `RustArtifactKind`, `RustDiffSummary`, `RustTaskDiffResponse`, `RustIpcCommand` (`list_task_artifacts`, `get_artifact`, `get_task_diff`), `RustIpcResponse` (`task_artifacts`, `artifact`, `task_diff`).

- [ ] **Step 1: Write failing protocol encoding/decoding tests**
  Add unit tests in `src/test/unit/ArtifactProtocol.test.ts` verifying wire parity against all 6 canonical golden wire fixtures from Rust backend.

- [ ] **Step 2: Run test to verify failure**
  Run: `npm run test:unit`
  Expected: FAIL with missing types/commands.

- [ ] **Step 3: Update `RustIpcProtocolTypes.ts` and `RustIpcCodec.ts`**
  Implement flat, internally-tagged schemas for `list_task_artifacts`, `get_artifact`, and `get_task_diff`.

- [ ] **Step 4: Run test to verify passing**
  Run: `npm run test:unit`
  Expected: PASS for protocol tests.

- [ ] **Step 5: Commit**
  Commit Task 1 changes.

---

### Task 2: Lossless Binary-Safe Artifact Client & Cryptographic Integrity (Subagent B)

**Files:**
- Create: `apps/agent-studios-vscode/src/bridge/ipc/ArtifactClient.ts`
- Create: `apps/agent-studios-vscode/src/test/unit/ArtifactClient.test.ts`

**Interfaces:**
- Produces: `ArtifactClient.fetchTaskDiff`, `ArtifactClient.fetchArtifactContent`, `ArtifactClient.listTaskArtifacts`, `ReassembledTaskDiff`, `ReassembledArtifactContent`, `ArtifactRetrievalError`.

- [ ] **Step 1: Write failing unit tests for `ArtifactClient`**
  Test pagination reassembly, chunk offset validation, zero-progress detection, multi-byte UTF-8 split handling, binary patch detection, and true Node.js crypto SHA-256 verification (success, failure, tampered bytes, truncated chunks).

- [ ] **Step 2: Run test to verify failure**
  Run: `npm run test:unit`
  Expected: FAIL with module not found.

- [ ] **Step 3: Implement `ArtifactClient.ts`**
  Implement byte-accurate Base64 decoding using Buffer/Uint8Array, SHA-256 calculation, and memory boundary checks.

- [ ] **Step 4: Run test to verify passing**
  Run: `npm run test:unit`
  Expected: PASS for all ArtifactClient tests.

- [ ] **Step 5: Commit**
  Commit Task 2 changes.

---

### Task 3: Artifact Review Webview Panel & Unified Git Diff Viewer (Subagent C)

**Files:**
- Create: `apps/agent-studios-vscode/src/webviews/ArtifactReviewPanel.ts`
- Create: `apps/agent-studios-vscode/src/cards/DiffParser.ts`
- Create: `apps/agent-studios-vscode/src/test/unit/DiffParser.test.ts`
- Modify: `apps/agent-studios-vscode/src/extension.ts`

**Interfaces:**
- Produces: `ArtifactReviewPanel.createOrShow(extensionUri, bridge, options: { studioId: string, taskId: string })`, `DiffParser.parseUnifiedDiff(patch: string)`.

- [ ] **Step 1: Write failing tests for `DiffParser`**
  Test parsing unified diff into hunks, files, additions, deletions, renames, and binary files.

- [ ] **Step 2: Run test to verify failure**
  Run: `npm run test:unit`
  Expected: FAIL with module not found.

- [ ] **Step 3: Implement `DiffParser.ts` and `ArtifactReviewPanel.ts`**
  Implement unified diff parsing, Task Summary, Change Summary, File Explorer with status badges, and syntax-highlighted Diff Viewer.

- [ ] **Step 4: Run test to verify passing**
  Run: `npm run test:unit`
  Expected: PASS.

- [ ] **Step 5: Commit**
  Commit Task 3 changes.

---

### Task 4: Chat & Mission Control Integration & Bridge Wiring (Subagent D)

**Files:**
- Modify: `apps/agent-studios-vscode/src/bridge/DaemonConnectedBridge.ts`
- Modify: `apps/agent-studios-vscode/src/bridge/FixtureBridge.ts`
- Modify: `apps/agent-studios-vscode/src/chat/ChatWebviewViewProvider.ts`
- Modify: `apps/agent-studios-vscode/src/mission-control/MissionControlPanel.ts`
- Create: `apps/agent-studios-vscode/src/test/unit/ArtifactBridgeIntegration.test.ts`

**Interfaces:**
- Produces: Bridge methods `getTaskDiff()`, `listTaskArtifacts()`, `getArtifactContent()`, Chat & Mission Control messages `viewTaskDiff`, `inspectArtifact`.

- [ ] **Step 1: Write failing tests for Bridge & UI Message Handlers**
  Verify bridge delegates to `ArtifactClient`, and Chat/Mission Control dispatch `agent-studios.openArtifactReview`.

- [ ] **Step 2: Run test to verify failure**
  Run: `npm run test:unit`
  Expected: FAIL with missing methods.

- [ ] **Step 3: Implement Bridge methods and wire webview providers**
  Add methods to `DaemonConnectedBridge` and `FixtureBridge`. Add message handlers in Chat and Mission Control.

- [ ] **Step 4: Run test to verify passing**
  Run: `npm run test:unit`
  Expected: PASS.

- [ ] **Step 5: Commit**
  Commit Task 4 changes.

---

### Task 5: Real Rust Daemon E2E Tests & Extension Host Integration (Subagent E)

**Files:**
- Modify: `apps/agent-studios-vscode/src/test/e2e/RealRustDaemonE2E.test.ts`
- Modify: `apps/agent-studios-vscode/src/test/integration/suite/commands.test.ts`
- Create: `apps/agent-studios-vscode/src/test/e2e/RealExecutionSmoke.test.ts`

**Interfaces:**
- Tests real daemon commands: `list_task_artifacts`, `get_task_diff`, `get_artifact`, and Extension Host command `agent-studios.openArtifactReview`.

- [ ] **Step 1: Add real daemon artifact IPC tests to `RealRustDaemonE2E.test.ts`**
  Verify authenticated real `agsd` returns valid responses for artifact and diff queries.

- [ ] **Step 2: Create `RealExecutionSmoke.test.ts`**
  Implement isolated real execution smoke test with truthful labeling.

- [ ] **Step 3: Run E2E and Integration test suites**
  Run: `npm run test:e2e` and `npm run test:integration`
  Expected: PASS for all tests.

- [ ] **Step 4: Commit**
  Commit Task 5 changes.

---

### Task 6: Pinned Backend CI Upgrade, Documentation & Delivery (Subagents F & G)

**Files:**
- Modify: `.github/workflows/agent-studios-m14-ui-ci.yml`
- Modify: `docs/reviews/M15-UI-02-2/04_REAL_DAEMON_EXECUTION_EVIDENCE.md`
- Create: `docs/reviews/M15-UI-03/01_PROTOCOL_PARITY.md`
- Create: `docs/reviews/M15-UI-03/02_ARTIFACT_CLIENT_AND_INTEGRITY.md`
- Create: `docs/reviews/M15-UI-03/03_DIFF_REVIEW_UI.md`
- Create: `docs/reviews/M15-UI-03/04_CHAT_AND_MISSION_CONTROL_INTEGRATION.md`
- Create: `docs/reviews/M15-UI-03/05_SECURITY_REVIEW.md`
- Create: `docs/reviews/M15-UI-03/06_REAL_BACKEND_E2E.md`
- Create: `docs/reviews/M15-UI-03/07_CROSS_PLATFORM_CI.md`
- Create: `docs/reviews/M15-UI-03/08_FINAL_VERDICT.md`

- [ ] **Step 1: Upgrade CI backend pin**
  Update `.github/workflows/agent-studios-m14-ui-ci.yml` from `61cb7e88e4ae0e3a0432134b77eee0c30f56c302` to `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`.

- [ ] **Step 2: Correct `04_REAL_DAEMON_EXECUTION_EVIDENCE.md`**
  Align test references with executable test suite.

- [ ] **Step 3: Author M15-UI-03 review documents**
  Write reports 01 through 08.

- [ ] **Step 4: Run full verification suite**
  Run `npm run lint`, `npm run build`, `npm run test:unit`, `npm run test:e2e`, `npm run test:integration`, and `npm run package`.

- [ ] **Step 5: Commit, Push and Verify Remote CI**
  Commit all changes, push to `feat/m15-real-agent-execution-ui`, and verify remote GitHub Actions CI passes on Ubuntu and Windows.
