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
2. In Continuous Integration (CI), an explicit `AGSD_BIN_PATH` fast-path evaluated before CI checks could accept an external binary outside the checked-out workspace.
3. Binary existence alone does not equate to proof of matching the source commit.

### 2. Strict Provenance Resolution Architecture & Threat Model

To eliminate historical fallbacks and establish verifiable provenance, `AgsdBinaryResolver.ts` enforces an unambiguous hierarchy:

```
                          resolveAgsdBinaryPath()
                                     │
           ┌─────────────────────────┴────────────────────────┐
           ▼                                                  ▼
     [CI Mode Active?]                              [Local Dev Mode (CI=false)]
    (CI=true / GITHUB_ACTIONS)                                │
           │                                                  │
     ┌─────┴─────────────────┐                          ┌─────┴────────────────┐
     ▼                       ▼                          ▼                      ▼
[Explicit AGSD_BIN_PATH]  [Default CI Search]     [Explicit AGSD_BIN_PATH]  [Local Search]
     │                       │                          │                      │
 1. Stale Check?        1. Checked-out Root        1. Stale Check?         1. Current Worktree
    -> Reject               target/debug or            -> Reject               target/debug or
 2. Exists on Disk?         target/release         2. Exists on Disk?          target/release
    -> Reject           2. Stale Check?                -> Reject           2. Main Repo Root
 3. Inside Checkout Root?   -> Reject              3. Return Explicit Path 3. Canonical Root
    -> Reject           3. Missing?                                        4. Missing?
 4. Inside Target Dir?      -> Throw with build                               -> Throw with cargo
    -> Reject                  instructions                                      instructions
 5. Return Path
```

#### Provenance Boundary Specification
1. **Path Validation Guarantees:** Proves that the resolved `agsd` binary resides strictly within the designated compiled output directory (`target/debug` or `target/release`) of the authorized repository checkout, preventing execution of stale historical worktrees or untrusted external binaries.
2. **Build Provenance Requirement:** Path validation DOES NOT, by itself, cryptographically prove that the binary was compiled from the exact source commit currently checked out. Complete source-to-binary build provenance requires the CI workflow to execute `cargo build --bin agsd` from the checked-out commit within the same isolated, immutable runner workspace before invoking tests.
3. **No Silent Fallbacks:** Any verification failure immediately throws an actionable error and halts execution. The resolver never falls back silently to alternative worktrees.

---

### 3. Unit Test Verification Matrix

A dedicated test suite, `AgsdBinaryResolver.test.ts`, executes in the fast unit runner (13/13 tests passing):

| Test Case | Scenario / Condition | Expected Behavior | Verification Status |
| :--- | :--- | :--- | :--- |
| **P-01** | Obsolete worktree paths (`m15-04`, `m15-03`, `m15-02`, `m15`) | Flagged as `isStaleHistoricalPath === true` | **PASS** |
| **P-02** | Current worktree (`m15-ui-06`) & unified root | Flagged as `isStaleHistoricalPath === false` | **PASS** |
| **P-03** | Windows path normalization & drive-letter case | Lowercased & normalized comparison matches | **PASS** |
| **P-04** | Subpath containment check on Windows | Validates child inside root; rejects outside | **PASS** |
| **P-05** | Local dev override with valid external binary | Permits explicit path override | **PASS** |
| **P-06** | Local dev override with stale historical path | Throws error rejecting stale path | **PASS** |
| **P-07** | Local dev override with non-existent file | Throws disk existence error | **PASS** |
| **P-08** | Local dev mode with missing binary | Throws error with cargo build instructions | **PASS** |
| **P-09** | CI mode with explicit path inside checkout target | Accepts valid checked-out binary | **PASS** |
| **P-10** | CI mode with explicit path outside checkout root | Throws strict CI provenance error | **PASS** |
| **P-11** | CI mode with explicit path outside target dir | Throws strict CI provenance error | **PASS** |
| **P-12** | CI mode with explicit path targeting stale worktree | Throws error rejecting stale path | **PASS** |
| **P-13** | CI mode default candidate resolution & missing error | Resolves checked-out binary; fails if missing | **PASS** |

---

### 4. Non-Destructive Preservation Audit

Per architectural constraints, no historical worktrees, branches, or binaries were modified or deleted:
- Existing worktree directories remain intact on disk.
- No `rmdir` or file deletions were performed against historical paths.
- Provenance safety is enforced entirely through code-level validation and rejection policies.
