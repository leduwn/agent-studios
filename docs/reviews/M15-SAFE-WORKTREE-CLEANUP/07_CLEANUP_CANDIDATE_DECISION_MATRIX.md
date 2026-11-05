# P0 Safe Worktree Cleanup — Cleanup Candidate Decision Matrix

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/07_CLEANUP_CANDIDATE_DECISION_MATRIX.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Audit Phase**: PHASE A — NON-DESTRUCTIVE SAFETY AUDIT  
**Safety Gate**: Fail-Closed Enforcement (100% Verified)  

---

## 1. Executive Summary

This matrix synthesizes all forensic findings across Git identity, working tree state, process ownership, filesystem structure, and backup reachability. Under fail-closed evaluation, all 7 candidates successfully satisfy every safety criterion and are designated `APPROVED_CANDIDATE` awaiting explicit operator authorization.

---

## 2. Comprehensive Cleanup Decision Matrix

| # | Worktree Candidate | Branch | HEAD SHA | Dirty? | Untracked | Ignored User Data | Unique Commits | Reachable from Unified? | Active Processes? | Reparse Risk? | Shared Cache? | Backup Status | Size (MB) | Expected Savings (GB) | Safety Verdict | Decision Rationale |
|:---:|---|---|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|:---:|---|
| 1 | `agent-studios-m13` | `feat/m13-ipc-02-control-plane` | `ce0396c1f1` | No | 0 | None | 0 | Verified | No | No | No | Verified on Remote | 89.73 MB | ~0.088 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, full ancestor of unified HEAD. |
| 2 | `agent-studios-m15` | `feat/m15-01-per-task-execution-contract` | `a5dbbfc5eb` | No | 0 | None | 0 | Verified | No | No | No | Verified on Remote | 89.85 MB | ~0.088 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, full ancestor of unified HEAD. |
| 3 | `agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` | `b307b9a6e2` | No | 0 | None | 0 | Verified | No | No | No | Verified on Remote (`61cb7e88e4`) | 7,568.46 MB | ~7.391 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, cargo target independent (7.30 GB). |
| 4 | `agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` | `cc1c3e8f39` | No | 0 | None | 0 | Verified | No | No | No | Verified on Remote | 13,880.54 MB | ~13.555 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, cargo target independent (13.47 GB). |
| 5 | `agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening` | `ed80b243c9` | No | 0 | None (only test cache) | 0 | Verified | No | No | No | Verified on Remote | 1,217.57 MB | ~1.189 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, vscode-test runtime (1.02 GB) safely reclaimable. |
| 6 | `agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` | `73b7298ca3` | No | 0 | None | 0 | Verified | No | No | No | Verified on Remote | 193.83 MB | ~0.189 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, full ancestor of frontend & unified HEAD. |
| 7 | `agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` | `a13230ee93` | No | 0 | None (only test cache) | 0 | Verified | No | No | No | Verified on Remote | 1,218.63 MB | ~1.190 GB | **APPROVED_CANDIDATE** | Clean tree, 0 unique commits, vscode-test runtime (1.02 GB) safely reclaimable. |

---

## 3. Total Aggregated Metrics

- **Total Candidates Evaluated**: 7
- **Passed All Safety Gates**: 7
- **Failed Safety Gates**: 0
- **Held / Blocked**: 0
- **Total Reclaimable Space**: **24,258.61 MB (~23.69 GB)**
- **Phase A Status**: **AUDIT COMPLETE — AWAITING OPERATOR APPROVAL**
