# M15 UI-05A — Report 08: Test Taxonomy & Verification Suite Architecture

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Test Classification, Execution Boundaries, Zero-Cost Verification & Assertion Hierarchy
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Executive Summary & Verification Principles

The Agent Studios verification framework adheres to the Iron Law of verification: **Evidence before claims, always.**

To ensure complete transparency and eliminate ambiguity:
1. **Clear Boundaries**: Every test is categorized into unambiguous, mutually exclusive tiers.
2. **Zero Billable Model Calls**: No commercial LLM tokens (Anthropic / OpenAI) are consumed during automated test execution.
3. **Authentic Native IPC**: Daemon tests execute against the compiled native Rust binary (`agsd.exe` on Windows, `agsd` on Linux) over real Named Pipes and Unix Domain Sockets.

---

## 2. Comprehensive Test Taxonomy Matrix

| Taxonomy Tier | Execution Harness | Mocking Level | Transport Used | Token Cost | Test Count (M15-04.1) | Test Count (M15 UI-05A Achieved) |
|---|---|---|---|---|---|---|
| **Tier 1: Pure Unit Tests** | Mocha in-process | High (in-memory stubs) | None (in-process calls) | 0 | 610 | **657 (100% PASS)** |
| **Tier 2: Real Daemon IPC** | Mocha + real `agsd` process | Zero (real native binary) | Named Pipes / POSIX UDS | 0 | 37 | **37 (100% PASS)** |
| **Tier 3: Daemon Crash Recovery** | Mocha + SIGKILL / restart | Zero (real SQLite DB) | Named Pipes / POSIX UDS | 0 | 6 | **6 (100% PASS)** |
| **Tier 4: Mock Daemon IPC** | Mocha + Node mock server | Medium (protocol mock) | Named Pipes / POSIX UDS | 0 | 10 | **10 (100% PASS)** |
| **Tier 5: Artifact Integrity** | Mocha in-process | Low (real byte streams) | In-memory binary buffers | 0 | 6 | **15 (100% PASS)** |
| **Tier 6: CLI Version Detection** | Mocha + CLI subprocess | Zero (`--version` flag) | Subprocess execution | 0 | 5 | **5 (100% PASS)** |
| **Tier 7: Gating & Capabilities** | Mocha in-process | Zero (real auth tokens) | In-memory cryptographic HMAC | 0 | 14 | **25 (100% PASS)** |
| **Tier 8: Real AI Execution** | Frontier Model API | N/A | HTTPS to model endpoints | Billable | **0 (GATED)** | **0 (GATED)** |

---

## 3. Dedicated M15 UI-05A Test Suites

### Suite A: `ChatMissionControlConsistency.test.ts` (Agent A)
- **Location**: `src/test/unit/ChatMissionControlConsistency.test.ts`
- **Scope**:
  1. Verifies in-flight query coalescing in `MissionControlProjection.refresh()`.
  2. Ensures rapid concurrent refreshes return identical promise instances.
  3. Verifies epoch/generation counter rejection of stale out-of-order responses.
  4. Tests cross-store state synchronization between `TaskExecutionStore` and `MissionControlProjection`.

### Suite B: `IpcReconnectLifecycle.test.ts` (Agent B)
- **Location**: `src/test/unit/IpcReconnectLifecycle.test.ts`
- **Scope**:
  1. Tests transient `ENOENT` / `ECONNREFUSED` connection retry loop (bounded backoff up to 1500ms).
  2. Confirms fatal connection errors teardown socket while `subscription_error` events trigger re-subscribe.
  3. Tests sequence gap recovery without dropping active transport connection.
  4. Verifies monotonic sequence watermark deduplication.

### Suite C: `OperatorConfirmationSafety.test.ts` (Agent C)
- **Location**: `src/test/unit/OperatorConfirmationSafety.test.ts`
- **Scope**:
  1. Tests HMAC-SHA256 operator challenge token generation, binding to `studioId` and `promptHash`.
  2. Enforces single-use token consumption and TTL expiration (60 seconds).
  3. Tests secret isolation: verifies secrets remain in Extension Host and are never exposed to Webviews.
  4. Tests fail-closed execution gating: mutation attempts without valid tokens throw `TaskSubmissionDisabledError`.

### Suite D: `ArtifactSecurityRegression.test.ts` (Agent D)
- **Location**: `src/test/unit/ArtifactSecurityRegression.test.ts`
- **Scope**:
  1. Contiguous raw buffer export verification: gaps or truncated chunks throw error.
  2. Tri-state SHA-256 integrity gating: `VERIFIED` exports cleanly, `INTEGRITY_FAILED` fails closed.
  3. Path traversal sanitization: malicious paths (e.g. `../../etc/passwd`, absolute paths, null bytes) are stripped/rejected.
  4. Multi-byte Vietnamese UTF-8 character splitting resilience across arbitrary buffer boundaries.

---

## 4. Execution Commands & Quality Gates

All tests must be cleanly executed locally and verified in CI:
- **Unit Suite**: `npm run test:unit`
- **E2E Suite**: `npm run test:e2e`
- **TypeScript Compilation**: `npm run compile` (0 errors)
- **Type Checking**: `npm run typecheck` (0 errors)
- **Lint Check**: `npm run lint` (0 errors, <= 324 warning baseline)
