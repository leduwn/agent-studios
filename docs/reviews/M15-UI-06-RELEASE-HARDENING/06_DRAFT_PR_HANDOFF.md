# Agent Studios — M15 UI-06 Release Hardening
## Document 06: Draft PR Handoff & Merge Readiness

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**PR Status:** DRAFT PULL REQUEST CREATED  
**PR URL:** https://github.com/leduwn/agent-studios/pull/34  
**PR Number:** #34  
**Head Commit:** `7c8014dc520cd054c228cc9125833e1fe892f71c`  
**Lead Engineer:** Claude 2 (Principal VS Code Extension Engineer & UI Integration QA Lead)  
**Date:** 2026-11-06  

---

### 1. PR Metadata & Target Alignment

- **Repository:** `https://github.com/leduwn/agent-studios`
- **Head Branch:** `feat/m15-ui-06-agent-ide-continuity`
- **Base Branch:** `integration/m15-unified-codebase`
- **Draft PR URL:** https://github.com/leduwn/agent-studios/pull/34
- **Draft PR Number:** #34
- **PR State:** `Draft` (Integration verification gate; NO auto-merge; NO retargeting to `main` without integration-lead sign-off)

---

### 2. Summary of Changes

1. **Deterministic Binary Provenance Resolver (`AgsdBinaryResolver.ts`):**
   - CI mode evaluated before accepting explicit `AGSD_BIN_PATH`.
   - Proactively rejects stale historical paths (`agent-studios-m15-04`, `m15-03`, `m15-02`, `m15`).
   - In CI, strictly requires binary to reside within the checked-out workspace build target directory.
   - Preserves local developer overrides with disk existence validation.
   - Comprehensive unit test suite `AgsdBinaryResolver.test.ts` (13/13 pass).

2. **Real Extension Host Mode Continuity Verification (`modeContinuity.test.ts`):**
   - Unmocked tests executed inside authentic VS Code Extension Host (`@vscode/test-electron` v1.145.0).
   - Real command registry verified for `openInIde`, `switchToAgentMode`, and `continueSession`.
   - Full suite passes: 25/25 integration tests clean.

3. **Multi-Tier Acceptance Hierarchy Clarity:**
   - Delineates unit mock testing (`vscodeMock`), authentic native daemon tests (`agsd.exe` PID, SQLite WAL), real extension host tests (`@vscode/test-electron`), and future standalone Code-OSS shell integration (M17).

4. **Preserved Frontend Baseline & Security:**
   - 100% pass rate across 685 unit tests, 84 real daemon E2E tests, and 25 extension host integration tests (794 total tests).
   - VSIX packaging verified (87 files, 260 KB).
   - Zero modifications to backend Rust crates (`agent-studios-rs/`, `codex-rs/`).
   - Verified webview security, prototype pollution protections, session ID verification, and fail-closed state machines.

---

### 3. CI Run Evidence & Analysis

- **M14 UI CI (Run 42081071217):**
  - URL: https://github.com/leduwn/agent-studios/actions/runs/42081071217
  - Status: `completed` / `success` (100% green across both Ubuntu and Windows).
- **Unified Full CI (Run 42081071268):**
  - URL: https://github.com/leduwn/agent-studios/actions/runs/42081071268
  - Status: `completed` / `cancelled` (Ubuntu job failed on `Cargo Clippy`; Windows job cancelled).
  - Root Cause: Failure is pre-existing on base branch `integration/m15-unified-codebase` in backend Rust crates.
  - Regression Impact: Zero frontend regressions; no Rust code was modified in this PR.

---

### 4. File Manifest

```
M  apps/agent-studios-vscode/src/test/e2e/AgsdBinaryResolver.ts
M  apps/agent-studios-vscode/src/test/e2e/RealAgentIdeContinuityE2E.test.ts
M  apps/agent-studios-vscode/src/test/integration/activation.test.ts
A  apps/agent-studios-vscode/src/test/integration/modeContinuity.test.ts
M  apps/agent-studios-vscode/src/test/unit/AgsdBinaryResolver.test.ts
M  docs/reviews/M15-UI-06-RELEASE-HARDENING/01_REAL_DAEMON_BINARY_PROVENANCE.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/02_EXTENSION_HOST_ACCEPTANCE.md
M  docs/reviews/M15-UI-06-RELEASE-HARDENING/03_FRONTEND_REGRESSION_MATRIX.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/04_SECURITY_REVIEW.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/05_CROSS_COMPONENT_DEPENDENCIES.md
M  docs/reviews/M15-UI-06-RELEASE-HARDENING/06_DRAFT_PR_HANDOFF.md
```

---

### 5. Verification Checkoff

- [x] Genuine GitHub Draft PR created: PR #34.
- [x] `npm run compile` passes with 0 errors.
- [x] `npm run typecheck` passes with 0 errors.
- [x] `npm run lint` passes with 0 errors (336 naming warnings).
- [x] `npm run test:unit` passes 685/685 tests.
- [x] `npm run test:e2e` passes 84/84 real daemon tests.
- [x] `npm run test:integration` passes 25/25 real extension host tests.
- [x] `npm run package` succeeds packaging VSIX extension.
- [x] Zero backend Rust files modified.
- [x] Zero historical worktrees or binaries deleted.
- [x] Draft PR targeting `integration/m15-unified-codebase`.
- [x] Final Verdict: `UI_06_FINAL_PR_READY`.
