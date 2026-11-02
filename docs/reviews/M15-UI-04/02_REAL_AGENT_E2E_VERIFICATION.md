# M15 UI-04 — Report 02: Real Agent Execution & Runtime E2E Verification

## Metadata
- **Milestone**: M15 UI-04: Real Agent Execution UI & Runtime Verification
- **Author**: Agent B (Real Runtime E2E Engineer)
- **Target Branch**: `feat/m15-real-agent-execution-ui` (Draft PR #27)
- **Base Branch**: `feat/m14-mission-control-connected-integration`
- **Backend Executable**: `D:\Projects\agent-studios-m15-03\agent-studios-rs\target\debug\agsd.exe`
- **Runtime CLI**: `C:\Users\Duwn\.local\bin\claude.exe` (Claude Code CLI v2.1.310.0)
- **Date**: 2026-11-02
- **Verification Verdict**: **VERIFIED & GATED**

---

## 1. Executive Summary & Verification Classification

This document provides formal verification evidence for Milestone **M15 UI-04** regarding real runtime executable discovery, multi-runtime adapter capability checks, fail-closed execution gating, and real compiled Rust daemon (`agsd.exe`) cross-process lifecycle testing.

### Exact Tier Classification

| Tier Label | Scope | Status | Notes |
|---|---|---|---|
| `UNIT` | Pure in-memory capability parsing, runtime option builder, and HMAC challenge verification | **PASSED** | 12 tests passing in `RealAgentExecutionE2E.test.ts` |
| `MOCK_IPC` | Synthetic wire framing, preflight validation, and parameter sanitization without live daemon | **PASSED** | 7 tests passing in `RealAgentExecutionE2E.test.ts` |
| `REAL_AGSD_READ_ONLY_IPC` | Real cross-process IPC against compiled Rust `agsd.exe` binary over Windows Named Pipe | **PASSED** | 11 tests passing in `RealAgentExecutionE2E.test.ts` (31 total with prior suites) |
| `IN_PROCESS_REAL_ARTIFACT_STORE` | In-process state preservation and task store immutability during execution gating | **PASSED** | `TaskExecutionStore` verified idle; zero false state transitions |
| `EXTENSION_HOST` | Operator confirmation management, challenge lifecycle, and security guards in extension host | **PASSED** | Cryptographic challenge generation, expiration, and tampering rejection verified |
| `REAL_AI_MODEL_NOT_TESTED` | External LLM frontier provider live inference (Anthropic Claude, OpenAI GPT) | **GATED** | Statically prevented; zero billable tokens or credits consumed |
| `TIER3_NOT_VERIFIED` | Live mutating autonomous agent task execution on workspace | **BLOCKED / GATED** | Gated closed by design; production `task.submit` statically disabled |

---

## 2. Real Environment Capabilities

### 2.1. Runtime CLI Detection (`claude.exe`)
- **Binary Location**: `C:\Users\Duwn\.local\bin\claude.exe`
- **Version Query**: `claude --version` -> `2.1.310 (Claude Code)`
- **CLI Type**: Official Anthropic Claude Code CLI
- **Inspection Safety**: Verified read-only execution with non-interactive flags (`--version`, `--help`).
- **Cost Invariant**: 0 prompt tokens, 0 completion tokens, $0.00 USD billable expenditure.
- **Fail-Closed Behavior**: Non-existent binary paths fail closed with explicit descriptive errors, preventing arbitrary command injection or blind execution.

### 2.2. Native Backend Daemon (`agsd.exe`)
- **Binary Location**: `D:\Projects\agent-studios-m15-03\agent-studios-rs\target\debug\agsd.exe`
- **IPC Transport**: Windows Named Pipe (`\\.\pipe\ags-real-agent-exec-<UUID>`)
- **Two-Phase Authentication**: HMAC-SHA256 handshake with isolated client and admin keys (`agsd.json` vs `agsd-admin.json`).
- **Control Plane Status**: `healthy`, with active client tracking and monotonic sequence watermarking.
- **Model Routing**: Discovered models correctly mapped to provider instances (e.g. Anthropic, OpenAI) via `list_providers` and `list_models`.
- **Administrative Protection**: Standard client is denied administrative shutdown (`permission denied`); graceful shutdown strictly requires `ags-client-admin` credentials.

---

## 3. Detailed Tier Findings

### Tier 1: UNIT & In-Memory Capability Validation
1. **Verified Adapter Detection**:
   - When `claude_code` is reported in `registered_runtimes` and `available_runtimes`, the UI marks the instance as `enabled: true` with tooltip `"Healthy runtime instance"`.
2. **Unverified Adapter Gating**:
   - When `claude_code` is discovered in `runtime_instances` but omitted from `available_runtimes` (backend adapter unimplemented), the UI marks the option as `enabled: false` with suffix `[Unverified Adapter]` and tooltip `"Native claude_code adapter is unverified / not implemented in backend"`.
3. **OpenCode Adapter Lifecycle**:
   - Healthy OpenCode instances are selectable; degraded/unhealthy instances (`healthy: false`) are disabled with suffix `[Unhealthy]`.
4. **Duplicate Instance Identity Guard**:
   - If multiple instances share identical `instance_id`, both are disabled with `[Duplicate ID - Disabled]` to eliminate ambiguous routing.
5. **Tri-State Capability Enforcement**:
   - Validates `streaming_events` and `stop` capabilities. For task execution, both `streaming_events: 'supported'` and `stop: 'supported'` are required. If `stop` is `unsupported`, task cancellation fails closed.

### Tier 2: MOCK_IPC, EXTENSION_HOST & IN_PROCESS_REAL_ARTIFACT_STORE
1. **Preflight Execution Gating**:
   - When `CapabilityMatrix.isCapabilityEnabled('task.submit')` is `false` (default read-only slice), any task submission through `ExecutionClient.submitTask` fails closed with `TaskSubmissionError`.
2. **Operator Confirmation Enforcement**:
   - Submissions without an operator confirmation challenge or with untrusted raw boolean `confirmedByUser: true` are rejected.
3. **Cryptographic Integrity & Tampering Detection**:
   - Forged HMAC challenge signatures (generated with foreign secrets) are rejected with cryptographic verification failure.
   - Expired challenges (`ttlMs < 0`) are rejected.
   - Studio ID tampering (challenge for Studio A used on Studio B) and prompt hash tampering are detected and rejected.
4. **Read-Only Bridge Mutation Defense**:
   - `DaemonConnectedBridge` mutation methods fail closed:
     - `requestApprovalDecision` -> `{ applied: false, message: '...read-only...' }`
     - `submitEditorAction` -> `{ accepted: false, message: '...read-only...' }`
     - `sendMessage` -> throws descriptive read-only error
     - `createSession` -> throws descriptive read-only error
5. **In-Process State Immutability**:
   - During rejected submission attempts, `TaskExecutionStore.instance.state` remains strictly `idle`, `activeTaskId` remains `undefined`, and `streamOutput` remains empty.

### Tier 3: REAL_AGSD_READ_ONLY_IPC & Daemon Lifecycle
1. **Subprocess Lifecycle**:
   - `agsd.exe` spawned with `--runtime-info-path`, `--admin-runtime-info-path`, and isolated pipe path.
   - Bootstrap discovery files generated within 120ms; `DaemonDiscovery.loadBootstrapInfo` parses endpoint, PID, and key ID.
2. **Standard Client Connection**:
   - Authenticated HMAC session established over Windows Named Pipe.
   - Health query returns `status: 'healthy'`, `active_clients >= 1`.
   - Ping roundtrip measures `>= 0ms`.
3. **Provider and Model Discovery**:
   - `listProviderSummaries` and `list_providers` query native provider catalogs.
   - `list_models` queries catalog models and validates routing fields (`id`, `provider_instance_id`, `display_name`).
4. **Role Permission Enforcement & Graceful Shutdown**:
   - Standard client issuing `IpcCommand::Shutdown` receives `permission denied`.
   - Admin client with `ags-client-admin` credentials issues `IpcCommand::Shutdown` and receives `shutdown_acknowledged`.
   - Daemon child process exits cleanly with exit code `0`.
   - Temporary bootstrap files and pipe endpoints cleaned up completely.

---

## 4. Why Billable Models Remain Gated (`REAL_AI_MODEL_NOT_TESTED` / `TIER3_NOT_VERIFIED`)

Live billable AI task execution (disagreeing with simulated or mock execution) was intentionally **NOT** executed during this test suite. The gating is documented below:

### 4.1. Root Causes & Security Justification
1. **Financial & Cost Containment**:
   - Automated integration tests in CI/CD and local development must never incur billable API costs or consume organization quota against commercial frontier models (Anthropic Claude 3.5 Sonnet, OpenAI GPT-4o).
2. **Workspace Containment & Zero-Mutation Policy**:
   - Real autonomous AI agents execute shell commands, edit arbitrary source files, and spawn background tasks. Executing live agent tasks against developer checkouts or production codebases introduces non-deterministic modification hazards.
3. **Capability Matrix Freeze**:
   - `task.submit` is statically disabled (`enabled: false`) in `CapabilityMatrix.ts`. In production builds, this capability requires multi-factor operator challenge tokens, verified workspace containment, and explicit backend runtime licensing.
4. **Truthfulness Invariant**:
   - In accordance with project instructions, testing without live billable credentials is explicitly classified as **`REAL_AI_MODEL_NOT_TESTED`** and **`TIER3_NOT_VERIFIED` (BLOCKED)**. It is never reported as a false "PASS" for live frontier AI model execution.

### 4.2. Conditions for Future Tier 3 Verification
To verify live billable execution in a controlled environment:
1. Provide sandboxed disposable workspace directory (e.g. isolated Git repo in temp storage).
2. Configure authorized provider credentials via environment variables (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`).
3. Explicitly enable `task.submit` capability in developer configuration (`CapabilityMatrix.setCapabilityEnabled('task.submit', true)`).
4. Run operator confirmation workflow to sign cryptographic challenge token.
5. Confirm manual human operator review before initiating any agent mutations.

---

## 5. Automated E2E Test Suite Execution Evidence

All E2E test suites were executed using the Mocha test runner (`node out/test/e2e/runner.js`) with the VS Code Node environment interceptor:

```
  Tier 2: Mock Daemon Mission Control Projection E2E (MOCK_PROJECTED_TASK_STATE_VERIFIED)
    √ 1. Standard client fails closed on mutations while admin/raw IPC can seed data
    √ 2. Connect DaemonConnectedBridge and initialize MissionControlProjection refresh
    √ 3. Studio appears in Mission Control (goalSummary and activeStudio match seeded state)
    √ 4. Real task ID and description display accurately in state.tasks
    √ 5. Task status matches Rust state (succeeded, running)
    √ 6. Task progress is truthful: 100% for succeeded, undefined (indeterminate) for running
    √ 7. Real providers and models hydrated from daemon (Anthropic Claude, OpenAI GPT)
    √ 8. Provider latency is unmeasured (undefined), NOT hardcoded 120ms
    √ 9. Event subscription updates timeline and attention queue when events are published (61ms)
    √ 10. Query errors produce diagnostics alert in AttentionQueue, not false zeroes
    √ 11. Real compiled Rust daemon (agsd.exe) verification if binary present (79ms)

  Tier 3: Real Agent Execution & Runtime E2E Suite
    1. Real Runtime Executable Detection (claude.exe) [REAL_AGSD_READ_ONLY_IPC & EXTENSION_HOST]
      √ 1. locates real claude CLI binary path in environment or local bin
      √ 2. verifies real claude CLI binary is a valid non-empty executable
      √ 3. runs claude --version without billable token usage and validates version output
      √ 4. runs claude --help in non-interactive read-only mode (151ms)
      √ 5. fail-closed detection: non-existent CLI binary path throws error
    2. Multi-Runtime Capability Checks (Claude Code vs OpenCode) [UNIT & EXTENSION_HOST]
      √ 6. detects verified claude_code adapter when present in available_runtimes
      √ 7. detects unverified claude_code adapter when missing from available_runtimes
      √ 8. detects opencode adapter and validates healthy runtime instance
      √ 9. detects degraded/unhealthy opencode runtime and disables option
      √ 10. disables duplicate runtime instance IDs to prevent ambiguous routing
      √ 11. returns safe disabled fallback on null or unready daemon report
      √ 12. validates runtime capabilities tri-state enforcement for execution and cancellation
    3. Fail-Closed Execution Gating & Zero Billable AI Tasks [UNIT, MOCK_IPC & REAL_AI_MODEL_NOT_TESTED]
      √ 13. fails closed when task submission attempted with task.submit disabled in CapabilityMatrix
      √ 14. fails closed when task submission attempted without operator confirmation challenge
      √ 15. fails closed when operator confirmation challenge signature is forged or invalid
      √ 16. fails closed when operator confirmation challenge has expired
      √ 17. fails closed when studio ID or prompt hash does not match challenge
      √ 18. verifies DaemonConnectedBridge mutation methods fail closed in read-only slice
      √ 19. enforces zero wire dispatch and in-process TaskExecutionStore immutability
    4. Real agsd.exe Daemon Lifecycle & Model Routing [REAL_AGSD_READ_ONLY_IPC & TIER3_NOT_VERIFIED]
      √ 20. locates real compiled Rust agsd.exe binary
      √ 21. spawns real agsd.exe daemon with isolated temporary bootstrap and named pipe (119ms)
      √ 22. verifies standard and admin discovery credentials separation
      √ 23. connects DaemonConnectedBridge with HMAC authentication to real daemon
      √ 24. verifies real daemon health and active client count
      √ 25. verifies real daemon ping roundtrip latency
      √ 26. lists available providers from real daemon
      √ 27. lists available models and verifies provider model routing mappings
      √ 28. verifies standard client shutdown denial (fails closed with permission denied)
      √ 29. executes graceful daemon shutdown using authenticated admin key
      √ 30. verifies daemon child process termination with exit code 0 and cleans up resources

  Tier 3: Real Artifact IPC Integration (REAL_AGSD_READ_ONLY_IPC & IN_PROCESS_REAL_ARTIFACT_STORE)
    Tier: REAL_AGSD_READ_ONLY_IPC
      √ 1. real daemon returns error response for non-existent task diff without socket disruption
      √ 2. real daemon rejects get_task_diff with zero limit (infinite loop guard)
      √ 3. real daemon rejects list_task_artifacts for non-existent task
      √ 4. real daemon returns null artifact for non-existent artifact ID
      √ 5. ArtifactClient.fetchTaskDiff throws ArtifactRetrievalError against real daemon
    Tier: IN_PROCESS_REAL_ARTIFACT_STORE & MOCK_IPC
      √ 6. multi-chunk streaming with lossless UTF-8 reassembly and SHA-256 verification (VERIFIED)
      √ 7. preserves multi-byte Vietnamese Unicode split across arbitrary byte boundaries
      √ 8. binary Git patch chunk streaming with isBinary: true and exact byte preservation
      √ 9. cryptographic tampering detection yielding INTEGRITY_FAILED
      √ 10. studio ID mismatch rejection (STUDIO_ID_MISMATCH)
      √ 11. task ID mismatch rejection (TASK_ID_MISMATCH)

  Tier 3: Real Compiled Rust agsd E2E Test Suite (AUTHENTICATED_RUST_IPC_VERIFIED & LIVE_READ_ONLY_RUST_DATA_VERIFIED)
    √ 1. Daemon process startup (115ms)
    √ 2. Bootstrap file discovery (DaemonDiscovery.loadBootstrapInfo)
    √ 3. Standard vs admin credential separation (agsd.json vs agsd-admin.json)
    √ 4. Two-phase capability negotiation and HMAC authentication (DaemonConnectedBridge.connect())
    √ 5. Invalid HMAC rejection (assert auth fails with wrong key)
    √ 6. Health query (getHealth() -> healthy, active_clients: 1)
    √ 7. Ping latency (ping() -> >= 0ms)
    √ 8. Studio listing (list_studios / getCurrentSession())
    √ 9. Task listing (listTasks())
    √ 10. Provider listing (listProviderSummaries())
    √ 11. Model listing (models inside provider summary)
    √ 12. Event subscription (daemon activates streaming on connect) (58ms)
    √ 13. Sequence watermark handling (tracking lastWatermark)
    √ 14. Connection loss handling (disconnect socket)
    √ 15. Reconnect with monotonic sequence watermark and replay dedup
    √ 16. Read-only mutation rejection (assert requestApprovalDecision and submitEditorAction fail closed)
    √ 17. Standard-client shutdown denial (assert standard client sending shutdown receives permission denied)
    √ 18. Authorized admin shutdown (connect with admin key from agsd-admin.json and send shutdown -> shutdown_acknowledged)
    √ 19. Child-process termination (assert child process exits with code 0)
    √ 20. Temporary resource cleanup (assert bootstrap files and pipe/socket cleaned up)

  72 passing (881ms)
```

---

## 6. Static Analysis & Verification Commands

### 6.1. TypeScript Compilation
```powershell
npx tsc src/test/e2e/RealAgentExecutionE2E.test.ts --rootDir src --outDir out --module commonjs --target ES2022 --lib ES2022 --skipLibCheck --esModuleInterop
```
- **Exit Code**: `0`
- **Output**: Clean compilation with 0 errors.

### 6.2. ESLint Static Analysis
```powershell
npx eslint src/test/e2e/RealAgentExecutionE2E.test.ts
```
- **Exit Code**: `0`
- **Output**: 0 errors, 0 warnings.

### 6.3. E2E Test Execution
```powershell
node out/test/e2e/runner.js
```
- **Exit Code**: `0`
- **Result**: `72 passing (881ms)` (30 new tests from `RealAgentExecutionE2E.test.ts`, 42 existing tests).

---

## 7. Sign-off & Delivery Assessment

- **File Ownership Adherence**: Strictly restricted to `apps/agent-studios-vscode/src/test/e2e/RealAgentExecutionE2E.test.ts` and `docs/reviews/M15-UI-04/02_REAL_AGENT_E2E_VERIFICATION.md`.
- **Security & Read-Only Invariants**: 100% compliant. Zero billable model invocations. Zero source tree mutations.
- **Authoritative Classification**: All 7 tier labels accurately reported.
- **Status**: **READY FOR M15 UI-04 MERGE** (Gated on Tier 3 Live Autonomous Execution).
