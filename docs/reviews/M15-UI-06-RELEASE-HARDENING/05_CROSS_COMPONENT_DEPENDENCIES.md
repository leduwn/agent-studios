# Agent Studios — M15 UI-06 Release Hardening
## Document 05: Cross-Component Dependencies & Integration Readiness

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**Status:** ALIGNED & ISOLATED  
**Date:** 2026-11-06  

---

### 1. Cross-Component Architecture & Boundaries

The Agent Studios architecture spans three distinct functional planes:
1. **Frontend IDE Plane (`apps/agent-studios-vscode`):**
   - TypeScript extension host logic, VS Code contributions, webview views, session projections, and bridge adapters.
2. **Native Daemon Plane (`agent-studios-rs`):**
   - Rust Tokio-based IPC server (`agsd`), SQLite WAL storage engine, HMAC handshake, state machines.
3. **Execution Engine Plane (`codex-rs`):**
   - Agent loop orchestration, tool execution, model provider clients.

---

### 2. Strict Boundary Isolation Audit

During Milestone M15 UI-06 release hardening:
- **Backend Rust Crates (`agent-studios-rs`):** ZERO modifications made.
- **Execution Crates (`codex-rs`):** ZERO modifications made.
- **Cargo Manifests (`Cargo.toml`, `Cargo.lock`):** ZERO modifications made.
- **Shared Rust Schemas:** ZERO protocol changes or breaking payload modifications introduced.

All changes were strictly confined to:
- `apps/agent-studios-vscode/src/test/e2e/AgsdBinaryResolver.ts` (created)
- `apps/agent-studios-vscode/src/test/unit/AgsdBinaryResolver.test.ts` (created)
- `apps/agent-studios-vscode/src/test/integration/modeContinuity.test.ts` (created)
- `apps/agent-studios-vscode/src/test/e2e/RealAgentIdeContinuityE2E.test.ts` (updated to use resolver)
- `apps/agent-studios-vscode/src/test/integration/activation.test.ts` (updated command assertions)
- Documentation reports in `docs/reviews/M15-UI-06-RELEASE-HARDENING/`

---

### 3. Upstream CI Status & Baseline Alignment

#### GitHub Actions Run `42070017450`
Pushing branch `feat/m15-ui-06-agent-ide-continuity` triggered GitHub Actions CI run `42070017450`:
- **Frontend Validation:** 100% passing across compilation, unit tests, and E2E suites.
- **Backend Rust Jobs:** Reported failures inherited from base branch `integration/m15-unified-codebase` (missing Cargo package dependencies / compilation errors in unmerged backend crates).

#### Transparent Classification:
- We do **NOT** claim full CI is green.
- The backend failures are pre-existing in `integration/m15-unified-codebase` and belong to Claude 1 / Backend Lead's scope.
- Frontend code maintains 100% pass rate and introduces zero backend mutations.

---

### 4. Downstream / Milestone M16 Contract Readiness

1. **Provider Discovery Handshake:**
   - The proposed Provider Discovery IPC methods (`list_provider_instances`, `probe_provider_health`) remain deferred until Claude 1 provides the backend daemon implementation in `agent-studios-rs`.
   - The frontend extension is prepared with mock/fallback fixtures and clear UI indicator labels.
2. **Desktop Shell Integration (Milestone M17):**
   - Real VS Code Extension Development Host execution is certified (`modeContinuity.test.ts`).
   - Standalone Code-OSS window-frame integration will be addressed when the desktop Electron wrapper repository is integrated in M17.
