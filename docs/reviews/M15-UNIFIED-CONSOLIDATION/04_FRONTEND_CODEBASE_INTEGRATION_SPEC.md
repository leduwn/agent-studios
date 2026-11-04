# 04 — Frontend Codebase Integration Specification: M15 Consolidation

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION/04_FRONTEND_CODEBASE_INTEGRATION_SPEC.md`  
**Milestone**: M15 Consolidation & Workspace Audit  
**Integration Branch**: `integration/m15-unified-codebase`  
**Frontend Path**: `D:\Projects\agent-studios-unified-staging\apps\agent-studios-vscode`  
**Authoritative Commit**: `3de7c85a966d1b46a293235b11170219d53cc7f3` (PR #27, `feat/m15-real-agent-execution-ui`)  
**Auditor**: Lead Integration Engineer & Agent C (Frontend Auditor)  

---

## 1. Executive Summary

The unified VS Code extension codebase (`apps/agent-studios-vscode`) integrates the authoritative frontend implementation from PR #27 (`3de7c85a96`), which subsumes all prior M14 Mission Control, connected client, and M15 UI milestones.

Key frontend capabilities verified in consolidation:
1. **100% Wire Protocol Parity**: `RustIpcProtocolTypes.ts` mirrors the Rust backend's 25 IPC commands, 26 responses, and 8 event envelopes with zero schema discrepancies across all 6 golden wire fixtures.
2. **Native IPC Transport**: Native Named Pipe (`\\.\pipe\agent-studios-*`) and Unix Domain Socket transport with 4-byte big-endian framing, 4 MiB frame bounding, and HMAC-SHA256 challenge-response authentication.
3. **Lossless Artifact Diff Streaming**: `ArtifactClient` implements multi-chunk reassembly, SHA-256 integrity verification, and base64 diff streaming with chunk offset validation and loop detection.
4. **Mission Control Observability**: Comprehensive 9-tab observability workbench (Overview, Fleet, Tasks DAG, Inspector, Timeline, Review, Approvals, Providers, Diagnostics) with zero-dependency SVG DAG rendering.
5. **Fail-Closed Security Model**: Read-only mutation rejection, operator confirmation dialogs for destructive actions, and strict extension host boundary credential scrubbing.

---

## 2. Frontend Subsystems Architecture

```
apps/agent-studios-vscode/src/
├── bridge/
│   ├── AgentStudiosBridge.ts          # Core abstraction interface
│   ├── BridgeManager.ts               # Bridge lifecycle & auto-reconnect coordinator
│   ├── DaemonConnectedBridge.ts       # Live IPC daemon connection implementation
│   ├── DisconnectedBridge.ts          # Fail-closed offline fallback bridge
│   ├── FixtureBridge.ts               # In-memory test fixture provider
│   ├── DiffDocumentProvider.ts        # Virtual document provider for vscode.diff
│   └── ipc/
│       ├── NativeIpcTransport.ts      # Low-level socket/pipe framing & HMAC handshake
│       ├── RustIpcCodec.ts            # Length-prefixed framing encoder/decoder
│       ├── RustIpcProtocolTypes.ts    # Authoritative TypeScript wire contracts
│       ├── ArtifactClient.ts          # Lossless diff streaming & artifact fetching
│       ├── ExecutionClient.ts         # Task submission and streaming handler
│       ├── DaemonDiscovery.ts         # Runtime bootstrap file locator (agsd.json)
│       └── PromptSanitizer.ts         # Unicode sanitization & token safety
├── mission-control/
│   ├── MissionControlPanel.ts         # WebviewPanel lifecycle manager
│   ├── MissionControlHtmlBuilder.ts   # CSP-compliant HTML template generator
│   ├── components/
│   │   ├── DagGraphRenderer.ts        # Zero-dependency SVG topological DAG engine
│   │   ├── KanbanBoard.ts             # Task lifecycle column board
│   │   ├── AttentionQueue.ts          # Operator approval alerts
│   │   └── AgentInspector.ts          # Detailed agent telemetry and log view
│   └── state/
│       └── MissionControlProjection.ts# Live read-model projection from IPC events
└── chat/
    ├── ChatWebviewViewProvider.ts     # Activity Bar chat assistant provider
    ├── ChatHtmlBuilder.ts             # Chat interface renderer
    └── ChatSecurityGuards.ts          # Prompt injection & secret scrubbers
```

---

## 3. Wire Contract & Golden Fixture Parity Matrix

Comparative audit of all 6 golden wire fixtures between PR #27 and backend PR #30 revealed **0 discrepancies**:

| Fixture File | IPC Command / Response | Wire Verification Result |
|---|---|:---:|
| `01_handshake.json` | `Authenticate` / `Authenticated` | **100% Match** |
| `02_studio_management.json` | `CreateStudio`, `ListStudios` | **100% Match** |
| `03_task_execution.json` | `SubmitTask`, `TaskSubmitted`, `TaskState` | **100% Match** |
| `04_streaming_events.json` | `SubscribeEvents`, `Event` frames | **100% Match** |
| `05_approvals_and_2pc.json` | `ResolveApproval`, `ApprovalResolved` | **100% Match** |
| `06_artifact_diff_recovery.json` | `GetTaskDiff`, `GetRecoveryStatus` | **100% Match** |

---

## 4. Verification & Quality Gates Status

| Verification Gate | Command | Scope | Result | Details |
|---|---|---|:---:|---|
| **TypeScript Compilation** | `npm run compile` | Entire extension | **PASS** | `tsc -p ./` exits 0 with 0 errors |
| **Type Check** | `npm run typecheck` | Entire extension | **PASS** | `tsc --noEmit -p ./` exits 0 with 0 errors |
| **Unit Test Suite** | `npm run test:unit` | Unit test runner | **PASS** | **657 passed, 0 failed (4s)** |
| **E2E Integration Suite** | `npm run test:e2e` | E2E test runner | **PASS** | **78 passed, 0 failed (12s)** |
| **Linting Compliance** | `npm run lint` | ESLint rules | **PASS** | 0 errors |
