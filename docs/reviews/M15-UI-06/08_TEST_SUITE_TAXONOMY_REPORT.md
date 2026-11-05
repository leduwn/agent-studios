# M15 UI-06 — Report 08: Test Suite Taxonomy & Verification Report

## Metadata
- **Milestone**: M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration Acceptance
- **Worktree**: `D:\Projects\agent-studios-active\m15-ui-06`
- **Total Test Suites**: 2 primary runners (`test:unit`, `test:e2e`)
- **Total Tests**: **755 passing (100%)**
- **Date**: 2026-11-05
- **Status**: **ALL PASSING (0 FAILURES)**

---

## 1. Test Suite Taxonomy Overview

The Agent Studios extension test architecture enforces strict environmental tiers to guarantee that mock tests remain deterministic while real daemon tests verify authentic native IPC behavior:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Agent Studios Test Suite                        │
├───────────────────────────────────┬────────────────────────────────────┤
│ Tier 1: Unit & Component Tests    │ Tier 3: Real Daemon & E2E Tests    │
│ (671 tests passing)               │ (84 tests passing)                 │
├───────────────────────────────────┼────────────────────────────────────┤
│ • Agent-First & Continuity (14)   │ • Real Daemon Mode Continuity (6)  │
│ • State Projections & Hydration   │ • Real agsd Process & IPC (20)     │
│ • Bridge & Protocol Encoders      │ • Crash Recovery & SQLite WAL (6)  │
│ • ChatSecurityGuards & Webview    │ • Real Artifact IPC Streaming (11) │
│ • Approval Flow & Risk Gates      │ • Runtime CLI Detection (12)       │
│ • Tree Views & Commands           │ • Mock Mission Control E2E (11)    │
│ • Operator Safety Challenges      │ • Extended Real Daemon IPC (18)    │
└───────────────────────────────────┴────────────────────────────────────┘
```

---

## 2. Granular Test Suite Breakdown

### 2.1 Unit Test Suite (`npm run test:unit`) — 671 Tests Passing
- `AgentIdeContinuity.test.ts` (14 tests): Welcome surface, progressive disclosure, 14 UX states styling, mode transition command registration, `ChatSecurityGuards` prototype pollution and payload validation.
- `BridgeManager.test.ts` / `DaemonConnectedBridge.test.ts`: Reconnection logic, bounded resubscription attempts, watermark deduplication.
- `SessionStateProjection.test.ts`: Projection initialization from bridge, task execution store synchronization, epoch invalidation.
- `ChatSecurityGuards.test.ts`: Schema validation, directory traversal prevention, payload size enforcement.
- `ApprovalCommands.test.ts`: Medium/high risk confirmation modal logic, operator override challenge handling.
- `ArtifactSecurity.test.ts`: SHA-256 hash calculation, byte-level chunk reassembly, path traversal blocking.
- `CapabilityMatrixTruthfulness.test.ts`: Capability matrix gating and truthful runtime representation.

### 2.2 Real Daemon & E2E Suite (`npm run test:e2e`) — 84 Tests Passing
- `RealAgentIdeContinuityE2E.test.ts` (6 tests):
  1. Operator HMAC handshake authentication.
  2. Live session hydration into `SessionStateProjection`.
  3. Mode transition (`openInIde` / `switchToAgentMode`) invariance (PID, session ID, task count).
  4. SQLite WAL session persistence across transport disconnect/reconnect.
  5. Operator shutdown denial (privilege enforcement).
  6. Admin clean shutdown terminating child process with exit code 0.
- `RealRustDaemonE2E.test.ts` (20 tests): Named pipe setup, bootstrap discovery files, ping/pong latency, studio listing, task listing, provider/model queries, sequence watermarks, admin termination.
- `CrashRecoveryE2E.test.ts` (6 tests): Graceful daemon restart, forced SIGKILL recovery, interrupted tasks reconciliation in SQLite, watermark resynchronization.
- `RealArtifactIpcE2E.test.ts` (11 tests): Multi-chunk UTF-8 reassembly, Vietnamese Unicode character preservation, binary Git patch streaming, integrity tampering detection.
- `RealAgentExecutionE2E.test.ts` (12 tests): Claude CLI detection, non-interactive version/help checks, multi-runtime adapter capability checks.
- `LiveMissionControlE2E.test.ts` (11 tests): Mission Control projection hydration, read-only mutation rejection.
- Other live IPC subtests (18 tests): Stream event delivery, health queries, ping roundtrips.

---

## 3. Flake Resistance & Determinism Safeguards

1. **Deterministic Named Pipe Naming**: Every E2E test run generates an isolated UUID-scoped pipe name (`\\.\pipe\ags-continuity-e2e-<uuid>`) avoiding port/pipe collisions.
2. **Dedicated SQLite Database**: Each daemon instance spawns with its own temporary database path (`state.sqlite`) avoiding lock contention.
3. **Stdio Drain Handlers**: Child process `stdout` and `stderr` streams are piped and continuously consumed to prevent OS pipe buffer backpressure deadlocks.
4. **Admin Clean Teardown**: The `after` hook terminates the native daemon using authenticated admin shutdown, falling back to process kill only if unresponsive, ensuring zero orphaned `agsd.exe` processes.
