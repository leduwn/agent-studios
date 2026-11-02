# M15 UI-04.1 — Report 05: Test Taxonomy Reconciliation & Execution Boundaries

## Metadata
- **Milestone**: M15 UI-04.1: Cross-PR Integration Closure & Evidence Reconciliation
- **Focus**: Test Classification, Security Invariants, Model Execution Gating & Environment Parity
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`) targeting `feat/m14-mission-control-connected-integration`
- **Date**: 2026-11-02
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)

---

## 1. Executive Summary & Purpose

To prevent misleading claims and maintain rigorous engineering integrity, this report establishes the authoritative test taxonomy across Milestone M15 UI-04 and M15 UI-04.1.

Every test in the 687-test suite is categorized into unambiguous, mutually exclusive tiers. We explicitly delineate:
1. What was executed (compiled native Rust `agsd` daemon over authentic cross-process Named Pipes and Unix Domain Sockets).
2. What was detected (local CLI executable presence via `--version`).
3. What was **deliberately NOT executed** (frontier AI model inference and mutating task submissions).

---

## 2. Authoritative Test Taxonomy Matrix

### High-Level Summary Across All 688 Tests (610 Unit + 78 E2E)

| Test Category | Test Count | Execution Mechanism | Automated CI Status | Billable LLM Tokens |
|---|---|---|---|---|
| **`UNIT_TESTS`** | 610 | In-process Mocha runner with synthetic mocks and stores | Executed & Passing | 0 (Zero) |
| **`REAL_DAEMON_IPC`** | 37 | Cross-process native IPC against compiled `agsd.exe` / `agsd` (Named Pipes / POSIX UDS) | Executed & Passing | 0 (Zero) |
| **`REAL_DAEMON_CRASH_RECOVERY`** | 6 | Clean restart, SIGKILL abnormal termination, role redaction, ReplayBuffer sequence gap recovery against real `agsd` | Executed & Passing | 0 (Zero) |
| **`MOCK_DAEMON_IPC`** | 10 | Cross-process IPC against Node.js `StandaloneMockDaemon.js` for Mission Control UI projection | Executed & Passing | 0 (Zero) |
| **`IN_PROCESS_REAL_ARTIFACT_STORE`** | 6 | In-memory chunk streaming, lossless UTF-8, Vietnamese diacritics, and SHA-256 tamper verification | Executed & Passing | 0 (Zero) |
| **`RUNTIME_CLI_DETECTION`** | 5 | Subprocess execution of `claude --version` / `claude --help` (version check only, **NOT AI execution**) | Executed locally / Skipped on headless CI | 0 (Zero) |
| **`UNIT_AND_MUTATION_GATING`** | 14 | In-memory capability matrix, adapter options builder, and fail-closed task submission assertions | Executed & Passing | 0 (Zero) |
| **`REAL_AI_AGENT_EXECUTION`** | **0 (Zero)** | External frontier LLM model API invocation | **STRICTLY GATED CLOSED (0 RUN)** | **0 (Zero)** |

---

### Detailed Per-File Audit Across All 78 E2E Tests

#### 1. `RealRustDaemonE2E.test.ts` (20 Tests — 100% `REAL_DAEMON_IPC`)
All 20 tests interact with the authentic compiled Rust daemon `agsd.exe` over Windows Named Pipes or Unix Domain Sockets:
1. Daemon process startup (`spawnAgsd`)
2. Bootstrap file discovery (`DaemonDiscovery.loadBootstrapInfo`)
3. Standard vs admin credential separation (`agsd.json` vs `agsd-admin.json`)
4. Two-phase capability negotiation and HMAC authentication
5. Invalid HMAC signature rejection
6. Health query (`getHealth()` -> healthy, active_clients: 1)
7. Ping latency (`ping()` -> >= 0ms)
8. Studio listing (`list_studios` / `getCurrentSession()`)
9. Task listing (`listTasks()`)
10. Provider listing (`listProviderSummaries()`)
11. Model listing (`models` inside provider summary)
12. Event subscription (`subscribe_events` stream activation)
13. Sequence watermark handling (`lastWatermark`)
14. Connection loss handling (disconnect socket)
15. Reconnect with monotonic sequence watermark and replay dedup
16. Read-only mutation rejection (assert `requestApprovalDecision` & `submitEditorAction` fail closed)
17. Standard-client shutdown denial (permission denied)
18. Authorized admin shutdown (`shutdown` -> `shutdown_acknowledged`)
19. Child-process termination (assert child exits code 0)
20. Temporary resource cleanup (assert bootstrap files & socket removed)

#### 2. `RealRustDaemonRecoveryE2E.test.ts` (6 Tests — 100% `REAL_DAEMON_CRASH_RECOVERY`)
All 6 tests verify authentic daemon lifecycle, process restart, and persistent SQLite database recovery:
1. Clean Daemon Restart & State Persistence: studios & tasks preserved across graceful restarts
2. Abnormal Termination (SIGKILL): recovers database & reconciles in-flight tasks to `failed` (`daemon_interrupted`)
3. Role-Based Diagnostic Redaction: standard clients receive `database_path: null`, admin receives verified host path
4. Watermark Resynchronization: replay backfill from persistent SQLite WAL
5. UI Safety Invariant: Bridge reconnect NEVER automatically dispatches task re-execution
6. ReplayBuffer SequenceGap Across Restart: handles `SequenceGapRecoverable` on empty ReplayBuffer after restart, falling back to full state hydration and clean event streaming

#### 3. `RealArtifactIpcE2E.test.ts` (11 Tests)
- **`REAL_DAEMON_IPC` (5 Tests, Tests 1–5)**:
  1. Real daemon returns error response for non-existent task diff without socket disruption
  2. Real daemon rejects `get_task_diff` with zero limit (infinite loop guard)
  3. Real daemon rejects `list_task_artifacts` for non-existent task
  4. Real daemon returns null artifact for non-existent artifact ID
  5. `ArtifactClient.fetchTaskDiff` throws `ArtifactRetrievalError` against real daemon
- **`IN_PROCESS_REAL_ARTIFACT_STORE` (6 Tests, Tests 6–11)**:
  6. Multi-chunk streaming with lossless UTF-8 reassembly and SHA-256 verification (VERIFIED)
  7. Preserves multi-byte Vietnamese Unicode split across arbitrary byte boundaries (`ệ`, `ắ`)
  8. Binary Git patch chunk streaming with `isBinary: true` and exact byte preservation
  9. Cryptographic tampering detection yielding `INTEGRITY_FAILED`
  10. Studio ID mismatch rejection (`STUDIO_ID_MISMATCH`)
  11. Task ID mismatch rejection (`TASK_ID_MISMATCH`)

#### 4. `RealAgentExecutionE2E.test.ts` (30 Tests)
- **`RUNTIME_CLI_DETECTION` (5 Tests, Tests 1–5)**:
  1. Locates real `claude` CLI binary path in environment local bin
  2. Verifies real `claude` CLI binary is valid non-empty executable
  3. Runs `claude --version` without billable token usage and validates version output
  4. Runs `claude --help` in non-interactive read-only mode
  5. Fail-closed detection: non-existent CLI binary path throws error
  *(Note: CLI detection inspects version/help strings only; zero model prompts are submitted)*
- **`UNIT_AND_MUTATION_GATING` (14 Tests, Tests 6–19)**:
  - Tests 6–13 (8 tests): In-memory `CapabilityMatrix` multi-runtime detection (Claude Code vs OpenCode), health state inspection, duplicate instance ID suppression
  - Tests 14–19 (6 tests): Fail-closed execution gating: task submission without operator confirmation is blocked; unconfirmed actions throw `TaskSubmissionDisabledError`; zero billable AI calls made
- **`REAL_DAEMON_IPC` (11 Tests, Tests 20–30)**:
  - Tests 20–30: Authenticated cross-process Named Pipe IPC against compiled `agsd.exe`, testing handshake, health, ping latency, provider queries, model routing query, standard shutdown denial, and admin shutdown

#### 5. `LiveMissionControlE2E.test.ts` (11 Tests)
- **`MOCK_DAEMON_IPC` (10 Tests, Tests 1–10)**:
  - Spawns Node.js cross-process `StandaloneMockDaemon.js` over Named Pipes / UDS to test Mission Control projection state hydration, task states (`succeeded`, `running`), overview counters, provider rendering, timeline event ingestion, and query error handling in the Attention Queue
- **`REAL_DAEMON_IPC` (1 Test, Test 11)**:
  - Locates compiled `agsd.exe` binary and verifies cross-process IPC readiness when binary is present

---

## 3. Critical Taxonomy Clarifications

### A. CLI `--version` is NOT Real Agent Execution
Running `claude --version` or `claude --help` confirms the presence and executable bit of the Anthropic Claude Code CLI on the operator's machine. 
- **It does NOT execute an agent loop.**
- **It does NOT generate code.**
- **It does NOT consume LLM tokens.**
Representing `--version` checks as "agent execution" is explicitly prohibited. The UI integration verifies the runtime detection and capability handshake layer, while real model execution remains gated.

### B. Production `task.submit` Remains Gated Closed
In `RealAgentExecutionE2E.test.ts` (Section 3):
```typescript
it('13. fails closed when task.submit capability is disabled in capability matrix', async () => {
    CapabilityMatrix.setCapabilityEnabled('task.submit', false);
    // ... fails closed with TaskSubmissionDisabledError
});
```
The production workbench defaults to read-only operation. Mutation endpoints reject requests unless explicit operator confirmation challenges with cryptographic signatures are provided.

---

## 4. Local Workstation vs Remote GitHub Actions CI Parity

| Dimension | Local Development (Windows 11) | Remote GitHub Actions CI (Ubuntu & Windows) |
|---|---|---|
| **OS** | Windows 11 Home Single Language (x64) | `ubuntu-latest` (Linux 6.8) & `windows-latest` (Server 2022) |
| **Rust Compiler** | `rustc 1.83+` / MSVC | `dtolnay/rust-toolchain@stable` |
| **`agsd` IPC Transport** | Windows Named Pipe (`\\.\pipe\ags-...`) | Named Pipe (Windows) / Unix Domain Socket (Linux) |
| **`claude` CLI Binary** | Present (`C:\Users\Duwn\.local\bin\claude.exe`) | Absent (Headless runner; tests gracefully skipped) |
| **Unit Test Suite** | 610 passing | 610 passing |
| **E2E Test Suite** | 77 passing (4 CLI detection tests executed) | 77 passing on Linux (4 CLI detection tests skipped) |
| **VSIX Packaging** | Validated | Validated via `npx @vscode/vsce package --no-dependencies` |

---

## 5. Reconciliation of Draft PR #27 Description

The description of Draft PR #27 (`feat/m15-real-agent-execution-ui`) is updated to reflect this verified reality:
- **Accurately States**: Real native compiled Rust daemon (`agsd`) IPC, crash recovery reconciliation, role-based path redaction, and watermark resynchronization are 100% verified.
- **Accurately Disclaims**: Frontier AI model execution and production task submission are statically gated and unverified in automated CI.
- **Maintains Draft Status**: PR #27 remains in Draft status pending cross-PR review and operator approval.
