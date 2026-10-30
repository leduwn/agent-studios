# Automated Test Suite Audit & Coverage Report

**Milestone:** M14-UI-02  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Test Runners:** Mocha 10 (Unit) & `@vscode/test-electron` (Integration)

---

## 1. Overall Test Verification Metrics

| Test Category | Suite Count | Test Count | Passing | Failing | Regressions | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Pure Unit Tests (`test:unit`)** | 14 suites | 244 | 244 | 0 | 0 | **PASS (100%)** |
| **Extension Host Tests (`test:integration`)**| 4 suites | 14 | 14 | 0 | 0 | **PASS (100%)** |
| **Combined Milestone Total** | **18 suites** | **258** | **258** | **0** | **0** | **PASS (100%)** |

---

## 2. Unit Test Suite Breakdown (244 Tests)

```
  Agent Studios — Disconnected & Fixture Bridge Tests: 18 passing
  Agent Studios — Action Cards & Diff Presentation Tests: 19 passing
  Agent Studios — Chat Provider & Stream Processing Tests: 21 passing
  Agent Studios — Prompt Composer & Context Attachment Tests: 20 passing
  Agent Studios — Mode Selector & Provider Dropdown Controls Tests: 22 passing
  Agent Studios — Commands Registry & Execution Tests: 18 passing
  Agent Studios — DAG Task Graph Visualization Tests: 20 passing
  Agent Studios — Virtual Diff Document Provider Tests: 17 passing
  Agent Studios — Session Detail Webview Panel Tests: 19 passing
  Agent Studios — Session State Projection Tests: 24 passing
  Agent Studios — Native TreeView Providers Tests: 21 passing
  Agent Studios — Security Guards & Runtime Validation Unit Tests: 11 passing
    √ approves all legitimate webview commands in allowlist
    √ rejects unrecognized or injected commands
    √ rejects malformed message payloads
    √ detects top-level __proto__ property injection
    √ detects constructor or prototype property injection
    √ detects nested prototype pollution attacks
    √ blocks relative path traversal sequences (..)
    √ blocks null-byte injection in paths
    √ blocks dangerous URI protocols while permitting agent-studios-diff://
    √ allows clean workspace-relative paths
    √ allows non-mutating commands in untrusted workspace
  Agent Studios — Onboarding & Truthful Interaction Unit Tests: 4 passing
    √ renders disconnected onboarding card with retry and preview mode actions
    √ renders developer preview onboarding card with clear fixture explanation
    √ renders connected onboarding card for first run
    √ returns empty string when onboarding has been dismissed
  IPC-03 Bridge Contract & Framing Tests: 10 passing
    √ encodes a request frame with 4-byte big-endian length prefix
    √ decodes complete length-prefixed frames and handles chunked buffers
    √ leaves incomplete frame bytes in remaining buffer
    √ guards against oversized frame (> 16MB) to prevent denial of service
    √ creates request with monotonic id and resolves when matching response arrives
    √ rejects request promise with typed error when response contains error frame
    √ times out and rejects when response is not received within timeoutMs
    √ failAll rejects all pending requests on connection close
    √ contains all 9 core control plane capabilities with verified security verdicts
    √ computes accurate readiness summary metrics
```

---

## 3. Real Extension Host Integration Tests (14 Tests)

Executed inside official VS Code 1.144.0 Electron runtime with isolated `--user-data-dir` and `--extensions-dir`:

### A. Activation & Commands (`activation.test.ts`)
1. `verifies extension is present in VS Code extensions catalog` (PASS)
2. `activates extension cleanly in real extension host` (PASS)
3. `registers all 14 workbench and editor commands in vscode.commands` (PASS)
4. `verifies default extension settings schema in vscode.workspace` (PASS)
5. `executes agent-studios.toggleFixturePreview command and toggles preview mode` (PASS)

### B. Views & Chat Navigation (`navigation.test.ts`)
6. `executes agent-studios.openAgents without error` (PASS)
7. `executes agent-studios.showApprovals view navigation` (PASS)
8. `executes agent-studios.refreshViews command across all TreeViews` (PASS)
9. `executes agent-studios.disconnectDaemon cleanly returning to fail-closed state` (PASS)

### C. Virtual Diff Document Provider (`diffProvider.test.ts`)
10. `opens and reads virtual document from agent-studios-diff URI scheme` (PASS)
11. `triggers native vscode.diff command with virtual URI without workspace disk mutation` (PASS)

### D. Security & Fail-Closed Guards (`securityHost.test.ts`)
12. `blocks editor action safely when no active editor is open` (PASS)
13. `blocks refactor action safely when selection is empty` (PASS)
14. `blocks test generation safely when disconnected` (PASS)

---

## 4. Zero Flakiness & Determinism Assurance

- **Timing & Asynchrony:** All extension host tests await command promises and state change events rather than relying on arbitrary `setTimeout` delays.
- **Fixture Isolation:** In-memory fixtures generate deterministic mock IDs and timestamps.
- **Cleanup Handlers:** Both unit and integration suites implement `afterEach` and `dispose()` teardown hooks to ensure zero memory or port leaks.
