# Agent Studios — M15 UI-06 Release Hardening
## Document 06: Draft PR Handoff & Merge Readiness

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**PR Status:** DRAFT PULL REQUEST  
**Lead Engineer:** Claude 2 (Principal VS Code Extension Engineer & UI Integration QA Lead)  
**Date:** 2026-11-06  

---

### 1. PR Metadata & Target Alignment

- **Repository:** `https://github.com/leduwn/agent-studios`
- **Head Branch:** `feat/m15-ui-06-agent-ide-continuity`
- **Base Branch:** `integration/m15-unified-codebase`
- **PR Type:** `Draft Pull Request` (Explicitly flagged as Draft; NO auto-merge; NO retargeting to `main` without integration lead sign-off)

---

### 2. Summary of Changes

1. **Deterministic Binary Provenance Resolver (`AgsdBinaryResolver.ts`):**
   - Replaced ad-hoc historical worktree traversal with strict resolver.
   - Proactively rejects stale historical paths (`agent-studios-m15-04`, `m15-03`, `m15-02`, `m15`).
   - Enforces CI workspace checkout requirement and clear local build remediation.
   - Added unit test suite `AgsdBinaryResolver.test.ts` (7/7 pass).

2. **Real Extension Host Mode Continuity Verification (`modeContinuity.test.ts`):**
   - Implemented unmocked tests running inside authentic VS Code Extension Host (`@vscode/test-electron`).
   - Verified registration and execution of `openInIde`, `switchToAgentMode`, and `continueSession`.
   - Updated `activation.test.ts` to assert all 18 commands.
   - Full suite passes: 25/25 integration tests clean.

3. **Multi-Tier Acceptance Hierarchy Clarity:**
   - Explicitly delineated unit mock testing (`vscodeMock`), authentic native daemon tests (`agsd.exe` PID, SQLite WAL), real extension host tests (`@vscode/test-electron`), and future standalone Code-OSS shell integration (M17).

4. **Preserved Frontend Baseline & Security:**
   - 100% pass rate across 679 unit tests, 84 real daemon E2E tests, and 25 extension host integration tests (788 total tests).
   - Zero modifications to backend Rust crates (`agent-studios-rs/`, `codex-rs/`).
   - Verified webview security, prototype pollution protections, session ID verification, and fail-closed state machines.

---

### 3. File Manifest

```
M  apps/agent-studios-vscode/src/test/e2e/RealAgentIdeContinuityE2E.test.ts
M  apps/agent-studios-vscode/src/test/integration/activation.test.ts
A  apps/agent-studios-vscode/src/test/e2e/AgsdBinaryResolver.ts
A  apps/agent-studios-vscode/src/test/integration/modeContinuity.test.ts
A  apps/agent-studios-vscode/src/test/unit/AgsdBinaryResolver.test.ts
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/01_REAL_DAEMON_BINARY_PROVENANCE.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/02_EXTENSION_HOST_ACCEPTANCE.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/03_FRONTEND_REGRESSION_MATRIX.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/04_SECURITY_REVIEW.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/05_CROSS_COMPONENT_DEPENDENCIES.md
A  docs/reviews/M15-UI-06-RELEASE-HARDENING/06_DRAFT_PR_HANDOFF.md
```

---

### 4. Verification Checkoff

- [x] `npm run compile` passes with 0 errors.
- [x] `npm run typecheck` passes with 0 errors.
- [x] `npm run lint` passes with 0 errors.
- [x] `npm run test:unit` passes 679/679 tests.
- [x] `npm run test:e2e` passes 84/84 real daemon tests.
- [x] `npm run test:integration` passes 25/25 real extension host tests.
- [x] Zero backend Rust files modified.
- [x] Zero historical worktrees or binaries deleted.
- [x] Draft PR targeting `integration/m15-unified-codebase`.
