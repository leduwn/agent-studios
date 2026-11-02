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

| Test Tier / Classification | Execution Mechanism | Automated CI Status | Billable API Tokens | Description & Scope |
|---|---|---|---|---|
| **Tier 1: `UNIT` / `MOCK_IPC`** | In-process TypeScript / Node.js Mocha tests with synthetic transports | Executed & Passing (578 tests) | 0 (Zero) | Unit tests verifying serialization, regex validation, store projections, and state machines without spawning child processes. |
| **Tier 1: `IN_PROCESS_REAL_ARTIFACT_STORE`** | In-process streaming with cryptographic hashing | Executed & Passing (32 tests in `SessionLifecycleRecovery` & `ArtifactSecurityRegression`) | 0 (Zero) | Tests byte-level contiguous memory assembly, non-fatal UTF-8 decoding, Vietnamese diacritics, and SHA-256 tri-state verification. |
| **Tier 2: `REAL_AGSD_READ_ONLY_IPC`** | Cross-process native IPC (`agsd.exe` on Windows Named Pipes; `agsd` on POSIX UDS) | Executed & Passing (77 tests) | 0 (Zero) | Authentic communication against compiled Rust daemon binary. Tests HMAC handshake, ping, health, provider queries, shutdown, and crash recovery. |
| **Tier 2: `EXTENSION_HOST`** | VS Code Extension Host integration environment | Executed & Passing | 0 (Zero) | Headless VS Code extension lifecycle, webview providers, and command registrations. |
| **Tier 3: `REAL_CLI_DETECTION`** | Subprocess invocation of `claude --version` / `claude --help` | Executed locally / Skipped on headless CI | 0 (Zero) | Validates that local CLI binary exists and executes. **NOT REAL AGENT EXECUTION** (see Section 3). |
| **Tier 3: `REAL_AI_MODEL_NOT_TESTED`** | External frontier LLM inference (Anthropic / OpenAI) | **STATICALLY GATED (NOT RUN)** | 0 (Zero) | Automated CI gates strictly do NOT call external model APIs to protect API keys and prevent billable consumption. |
| **Tier 3: `TIER3_NOT_VERIFIED`** | Production mutating task submission (`task.submit`) | **STATICALLY GATED (DISABLED)** | 0 (Zero) | Task mutation capabilities remain gated closed in UI; attempts to submit mutate fail closed. |

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
