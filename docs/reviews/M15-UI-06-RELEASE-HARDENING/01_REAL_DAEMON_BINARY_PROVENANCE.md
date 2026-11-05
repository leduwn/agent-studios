# Agent Studios — M15 UI-06 Release Hardening
## Document 01: Real Daemon Binary Provenance & Verification Audit

**Milestone:** M15 UI-06 (Agent-First Experience, Shared Session Continuity & Real Daemon Integration)  
**Branch:** `feat/m15-ui-06-agent-ide-continuity`  
**Target Base:** `integration/m15-unified-codebase`  
**Status:** VERIFIED & HARDENED  
**Date:** 2026-11-06  

---

### 1. Executive Summary & Problem Statement

Prior to Milestone M15 UI-06 hardening, `RealAgentIdeContinuityE2E.test.ts` utilized an ad-hoc local file discovery loop that traversed candidate file paths including obsolete historical worktrees:
- `D:\Projects\agent-studios-m15-04\agent-studios-rs\target\debug\agsd.exe`
- `D:\Projects\agent-studios-m15-03\agent-studios-rs\target\debug\agsd.exe`
- `D:\Projects\agent-studios-m15-02\agent-studios-rs\target\debug\agsd.exe`
- `D:\Projects\agent-studios-m15\agent-studios-rs\target\debug\agsd.exe`

Falling back to binaries in older, unmerged, or superseded worktrees created unacceptable risks:
1. Tests could pass against stale binary protocols while failing against the checked-out commit.
2. In Continuous Integration (CI), tests could search non-existent developer directories or mask missing compilation artifacts.
3. Test provenance lacked deterministic traceability to the checked-out GitHub SHA.

### 2. Strict Provenance Resolution Architecture

To eliminate historical fallbacks and establish verifiable provenance, `AgsdBinaryResolver.ts` was introduced with a multi-layered policy:

```
                          resolveAgsdBinaryPath()
                                     │
           ┌─────────────────────────┴────────────────────────┐
           ▼                                                  ▼
   [AGSD_BIN_PATH Set?]                               [CI Environment?]
     │            │                                     │            │
    Yes           No                                   Yes           No
     │            │                                     │            │
     ▼            └───────────────┐                     ▼            ▼
[Stale Check]                     │              [Checked-out Root] [Local Search]
     │                            │                     │            │
Pass │ Fail                       │                Pass │ Fail  Pass │ Fail
     ▼  └─────────► Error         │                     ▼   └─►Error ▼   └─►Error
  Return             (Reject      │                  Return        Return
  Explicit           Historical)  │                  Target        Target
                                  │                  Binary        Binary
                                  ▼
                         [Local Workspace]
```

#### Rule 1: Explicit `AGSD_BIN_PATH` Priority with Stale Rejection
When `AGSD_BIN_PATH` is specified, it takes immediate precedence, but is subjected to strict pattern matching:
```typescript
export const STALE_WORKTREE_PATTERNS: readonly RegExp[] = [
  /[\\/]agent-studios-m15-04[\\/]/i,
  /[\\/]agent-studios-m15-03[\\/]/i,
  /[\\/]agent-studios-m15-02[\\/]/i,
  /[\\/]agent-studios-m15[\\/]/i,
];
```
Any target matching historical worktrees is proactively rejected with an actionable error. Non-existent files throw immediately.

#### Rule 2: CI Provenance Enforcement
When running under `CI=true` or `GITHUB_ACTIONS=true`:
- The resolver strictly requires the binary to reside within the checked-out repository root (`GITHUB_WORKSPACE` or current workspace root).
- Historical directories and external project folders are prohibited.
- If missing, the runner fails fast with:
  ```
  [AgsdBinaryResolver] CI provenance error: agsd daemon binary for checked-out commit was not found in workspace...
  Ensure cargo build --bin agsd was executed prior to running E2E test suites.
  ```

#### Rule 3: Local Workspace Discovery & Remediation
In local development, the resolver inspects:
1. `<current-worktree>/agent-studios-rs/target/debug/agsd` (or `.exe`)
2. `<current-worktree>/agent-studios-rs/target/release/agsd` (or `.exe`)
3. `<main-repo-root>/agent-studios-rs/target/debug/agsd` (or `.exe`)
4. Canonical root fallback (`D:\Projects\agent-studios`)

If no binary exists, it fails with exact compilation instructions:
```bash
cd agent-studios-rs && cargo build --bin agsd -p agent-studios-ipc-transport
```

---

### 3. Unit Test Verification Matrix

A dedicated test suite, `AgsdBinaryResolver.test.ts`, was created and executes in the fast unit runner (7/7 tests passing):

| Test Case | Scenario / Condition | Expected Behavior | Verification Status |
| :--- | :--- | :--- | :--- |
| **P-01** | Obsolete worktree paths (`m15-04`, `m15-03`, `m15-02`, `m15`) | Flagged as `isStaleHistoricalPath === true` | **PASS** |
| **P-02** | Current worktree (`m15-ui-06`) & unified root | Flagged as `isStaleHistoricalPath === false` | **PASS** |
| **P-03** | Valid `AGSD_BIN_PATH` | Returns verified path | **PASS** |
| **P-04** | Stale `AGSD_BIN_PATH` targeting old worktree | Throws error rejecting stale path | **PASS** |
| **P-05** | Missing `AGSD_BIN_PATH` file | Throws disk existence error | **PASS** |
| **P-06** | CI environment with valid checkout binary | Resolves checked-out binary | **PASS** |
| **P-07** | CI environment with missing binary | Throws CI provenance error with build instructions | **PASS** |

---

### 4. Non-Destructive Preservation Audit

Per architectural constraints, no historical worktrees, branches, or binaries were modified or deleted:
- Existing worktree directories remain intact on disk.
- No `rmdir` or file deletions were performed against historical paths.
- Provenance safety is enforced entirely through code-level validation and rejection policies.
