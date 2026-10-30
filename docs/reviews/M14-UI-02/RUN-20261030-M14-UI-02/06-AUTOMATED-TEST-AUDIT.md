# Automated Test Suite Audit & Coverage Report

**Milestone:** M14-UI-02.1  
**Run ID:** RUN-20261030-M14-UI-02  
**Date:** 2026-10-30  
**Test Runners:** Mocha 10 (Unit) & `@vscode/test-electron` (Integration)

---

## 1. Overall Test Verification Metrics

| Test Category | Suite Count | Test Count | Passing | Failing | Regressions | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Pure Unit Tests (`test:unit`)** | 15 suites | 274 | 274 | 0 | 0 | **PASS (100%)** |
| **Extension Host Tests (`test:integration`)**| 4 suites | 14 | 14 | 0 | 0 | **PASS (100%)** |
| **Combined Milestone Total** | **19 suites** | **288** | **288** | **0** | **0** | **PASS (100%)** |

---

## 2. Unit Test Suite Breakdown (274 Tests)

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
  Agent Studios — Onboarding & Truthful Interaction Unit Tests: 4 passing

  ChatSecurityGuards Unit Tests: 24 passing (+13 expanded)
    Command Allowlist Validation:
      √ approves all legitimate webview commands in allowlist
      √ rejects unrecognized or injected commands
      √ rejects malformed message payloads
    Prototype Pollution Protection (Fail-Closed & Array Inspection):
      √ detects top-level __proto__ property injection
      √ detects constructor or prototype property injection
      √ detects nested prototype pollution attacks
      √ fails closed when nesting depth exceeds MAX_OBJECT_DEPTH (depth 6+)
      √ detects prototype pollution nested inside arrays
      √ detects constructor pollution in multidimensional arrays
    Command Schemas & Size Bounds:
      √ rejects oversized prompt strings in sendMessage
      √ rejects oversized draft strings in saveDraft
      √ rejects invalid mode enums in switchMode and sendMessage
      √ rejects excessive attachments count (limit 10)
      √ rejects empty or oversized approvalId in approveAction/denyAction
      √ rejects approvalId containing illegal injection characters
    Path Traversal & Protocol Breakout Protection:
      √ blocks relative path traversal sequences (..)
      √ blocks null-byte injection in paths
      √ blocks Windows UNC paths
      √ blocks URL-encoded traversal sequences
      √ blocks dangerous URI protocols while permitting agent-studios-diff://
      √ distinguishes Windows drive letter prefixes from URI schemes
      √ allows clean workspace-relative paths
    Workspace Trust Enforcement:
      √ allows non-mutating commands in untrusted workspace

  IPC-03 Bridge Contract & Framing Tests: 17 passing (+7 expanded)
    IpcFrameCodec Framing & 4 MiB Limits:
      √ encodes a request frame with 4-byte big-endian length prefix
      √ decodes complete length-prefixed frames and handles chunked buffers
      √ leaves incomplete frame bytes in remaining buffer
      √ guards against oversized outbound payload before allocation
      √ guards against oversized inbound length prefix (> 4 MiB)
      √ guards against max u32 malformed length prefix
      √ handles exactly maximum frame size (boundary condition)
      √ handles truncated frame with partial header (< 4 bytes)
      √ handles multiple concatenated back-to-back frames
      √ throws error when JSON payload is malformed
      √ guards against unbounded partial-frame accumulation (anti-DoS)
    IpcCorrelationManager Request & Response Tracking:
      √ creates request with monotonic id and resolves when matching response arrives
      √ rejects request promise with typed error when response contains error frame
      √ times out and rejects when response is not received within timeoutMs
      √ failAll rejects all pending requests on connection close
    IPC-03 Machine-Readable Readiness Matrix:
      √ contains all core control plane capabilities with truthful status classifications
      √ computes accurate readiness summary metrics

  Rust IPC Codec & HMAC Authentication Unit Tests (Approach A): 11 passing (NEW)
    RustIpcCodec Wire Framing & Serialization:
      √ encodes Rust Serde tagged commands with 4-byte big-endian length prefix
      √ decodes complete Rust Serde tagged responses from stream buffer
      √ rejects outbound commands exceeding maximum frame size before buffer allocation
      √ rejects inbound length prefixes exceeding 4 MiB limit
      √ bounds accumulated stream buffers against memory exhaustion DoS
    RustHmacSigner Canonical Digest & Verification:
      √ computes exact canonical digest with empty payload matching Rust test vector
      √ sorts capabilities lexicographically ascending in canonical digest
      √ generates valid HMAC-SHA256 signature and verifies successfully
      √ rejects tampered payload with signature mismatch
      √ rejects verification with wrong secret key
      √ rejects stale timestamp exceeding maximum skew
```

---

## 3. Real Extension Host Integration Tests (14 Tests)

Integration test execution harness running inside genuine VS Code 1.144.0 Electron runtime (`@vscode/test-electron`):

```
  Extension Host: Activation & Commands Integration Tests
    √ verifies extension is present in VS Code extensions catalog
    √ activates extension cleanly in real extension host
    √ registers all 14 workbench and editor commands in vscode.commands
    √ verifies default extension settings schema in vscode.workspace
    √ executes agent-studios.toggleFixturePreview command and toggles preview mode

  Extension Host: Views & Chat Navigation Integration Tests
    √ executes agent-studios.openAgents without error
    √ executes agent-studios.showApprovals view navigation
    √ executes agent-studios.refreshViews command across all TreeViews
    √ executes agent-studios.disconnectDaemon cleanly returning to fail-closed state

  Extension Host: Virtual Diff Document Provider Integration Tests
    √ opens and reads virtual document from agent-studios-diff URI scheme
    √ triggers native vscode.diff command with virtual URI without workspace disk mutation

  Extension Host: Security & Fail-Closed Guard Integration Tests
    √ blocks editor action safely when no active editor is open
    √ blocks refactor action safely when selection is empty
    √ blocks test generation safely when disconnected
```

**Result:** 14/14 Passing in 1s. Exit code: 0.
