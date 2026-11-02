# M15 UI-04 — Report 06: CI & Test Evidence

## Metadata
- **Milestone**: M15 UI-04: Backend Dependency Update & Crash Recovery Integration
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`) targeting `feat/m14-mission-control-connected-integration`
- **Pinned Backend**: `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` (PR #30 / M15-04.1)
- **Base Commit**: `054dc0328fa1e35713cf13a7ab8181de5195ef9b`
- **Date**: 2026-11-02
- **Lead Integration**: Claude 2 (Frontend / VSCode Extension Integration Lead)

---

## 1. Test Suite Execution & Baseline Comparison

All tests executed locally in the Windows extension development environment with the authentic compiled Rust daemon `agsd.exe` (`D:\Projects\agent-studios-m15-04\agent-studios-rs\target\debug\agsd.exe`) and CLI `claude.exe` (`C:\Users\Duwn\.local\bin\claude.exe`).

| Quality Gate | Baseline (M15-UI-03.1) | Current (M15-UI-04) | Delta | Status |
|---|---|---|---|---|
| **TypeScript Compile** (`npm run compile`) | 0 errors | 0 errors | 0 | **PASSED** |
| **ESLint Static Analysis** (`npm run lint`) | 0 errors, 324 warnings | 0 errors, 324 warnings* | 0 | **PASSED** |
| **Unit Test Suite** (`npm test`) | 578 passing | 610 passing | +32 tests | **PASSED** |
| **E2E Test Suite** (`npm run test:e2e`) | 42 passing | 77 passing | +35 tests | **PASSED** |
| **Combined Tests** | 620 passing | 687 passing | +67 tests | **PASSED** |

*\*Note on Lint Warnings: All 324 warnings are legacy `@typescript-eslint/naming-convention` in existing test fixtures matching snake_case Rust serde protocol names. Zero lint errors were introduced in M15-UI-04.*

---

## 2. Test Accounting & Delta Breakdown

### A. Unit Tests: 578 -> 610 (+32 tests)
1. **`SessionLifecycleRecovery.test.ts` (+12 tests)** (Agent C):
   - Sequence gap detection and degradation firing (`onStateDegraded`).
   - Stale sequence number rejection and deduplication.
   - Event seeding FIFO buffer bounds (1000 items) and sorting.
   - Task switching isolation: diff reset and late event discard.
   - Cross-task event leakage prevention in `TaskExecutionStore`.
   - In-flight fetch cancellation via `AbortController`.
   - Rapid task switching generation counter protection.
   - Bridge disconnect fail-closed invalidation.
   - Reconnect subscription watermark resynchronization.
   - Panel disposal cleanup and error suppression.
2. **`ArtifactSecurityRegression.test.ts` (+20 tests)** (Agent D):
   - TOCTOU resistance: export directly from verified contiguous `rawBuffer`.
   - CRLF and LF byte preservation byte-for-byte.
   - Non-fatal UTF-8 decoding on malformed byte sequences.
   - Vietnamese diacritic octal unquoting (`\341\273\207` -> `ệ`).
   - Path traversal prevention in `taskId` and diff file headers.
   - Memory bounds (`EXCEEDED_MAX_BYTES`) and chunk count caps.
   - Anti-infinite loop zero-progress termination.
   - Tri-state cryptographic gating (`INTEGRITY_FAILED`, `UNVERIFIED`, `VERIFIED`).
   - Identity spoofing rejection (`STUDIO_ID_MISMATCH`, `TASK_ID_MISMATCH`).

### B. E2E Tests: 42 -> 77 (+35 tests)
1. **`RealRustDaemonRecoveryE2E.test.ts` (+5 tests)** (Agent C):
   - Clean daemon restart and state persistence across restarts.
   - Abnormal termination (SIGKILL / forced kill) and crash state reconciliation.
   - Role-based diagnostic redaction: standard clients receive `database_path: null`, admin receives verified path.
   - Watermark resynchronization: stream replay backfill from persistent SQLite WAL.
   - UI Safety Invariant: Bridge reconnect never automatically triggers task re-execution.
2. **`RealAgentExecutionE2E.test.ts` (+30 tests)** (Agent B):
   - Runtime CLI detection (`claude.exe` v2.1.310).
   - Adapter capability matrix validation (Claude Code vs OpenCode).
   - Read-only execution gating: mutation fail-closed assertions.
   - Cross-process Windows Named Pipe IPC to compiled `agsd.exe`.
   - HMAC handshake, health checks, ping latency.
   - Provider summaries and model routing query.
   - Permission-denied standard client shutdown.
   - Admin credential shutdown and child process exit code 0.

---

## 3. Execution Logs & Commands

### TypeScript Compilation
```powershell
Command: npm run compile
Output:
> agent-studios-vscode@0.1.0 compile
> tsc -p ./
Exit code: 0
```

### Unit Tests
```powershell
Command: npm test
Output:
...
  610 passing (1s)
Exit code: 0
```

### E2E Tests
```powershell
Command: npm run test:e2e
Output:
...
  77 passing (3s)
Exit code: 0
```

### ESLint Analysis
```powershell
Command: npm run lint
Output:
✖ 324 problems (0 errors, 324 warnings)
```

---

## 4. GitHub Actions Remote CI Verification

- **Workflow**: `Agent Studios M14 UI CI` (`.github/workflows/agent-studios-m14-ui-ci.yml`)
- **Run ID**: `41383213765` (Commit `1a081f37173b0a2426c35a48540fa5f0e3e23383`)
- **URL**: https://github.com/leduwn/agent-studios/actions/runs/41383213765
- **Conclusion**: **`success`** (100% Green on all platforms)

| Platform Runner | Job Name | Duration | Status | Key Steps Verified |
|---|---|---|---|---|
| `ubuntu-latest` | Build & Test (ubuntu-latest) | 1m 55s | **SUCCESS** | Checkout pinned backend `dcbcb39b`, Build native `agsd`, Unit tests, Real daemon E2E tests (Linux), Extension host integration tests, VSIX packaging |
| `windows-latest` | Build & Test (windows-latest) | 3m 48s | **SUCCESS** | Checkout pinned backend `dcbcb39b`, Build native `agsd.exe`, Unit tests, Real daemon E2E tests (Windows), Extension host integration tests, VSIX packaging |

