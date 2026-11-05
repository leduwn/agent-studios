# P0 Safe Worktree Cleanup — Operator Approval Request

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/08_OPERATOR_APPROVAL_REQUEST.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Audit Phase**: PHASE A COMPLETE — AWAITING OPERATOR APPROVAL  
**Manifest SHA-256**: `0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`  

---

## 1. Formal Approval Request

Phase A Non-Destructive Safety Audit is 100% complete. Zero files and zero worktrees have been modified or deleted. 

To execute Phase B and safely reclaim **~23.69 GB** of disk space, explicit operator authorization is required.

---

## 2. Approved Cleanup Candidate Scope (7 Allowlisted Worktrees)

| # | Exact Absolute Path | Expected Branch | Expected HEAD SHA | Recoverable Space |
|:---:|---|---|:---:|:---:|
| 1 | `D:\Projects\agent-studios-m13` | `feat/m13-ipc-02-control-plane` | `ce0396c1f1` | 89.73 MB (~0.09 GB) |
| 2 | `D:\Projects\agent-studios-m15` | `feat/m15-01-per-task-execution-contract` | `a5dbbfc5eb` | 89.85 MB (~0.09 GB) |
| 3 | `D:\Projects\agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` | `b307b9a6e2` | 7,568.46 MB (~7.39 GB) |
| 4 | `D:\Projects\agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` | `cc1c3e8f39` | 13,880.54 MB (~13.56 GB) |
| 5 | `D:\Projects\agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening`| `ed80b243c9` | 1,217.57 MB (~1.19 GB) |
| 6 | `D:\Projects\agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` | `73b7298ca3` | 193.83 MB (~0.19 GB) |
| 7 | `D:\Projects\agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` | `a13230ee93` | 1,218.63 MB (~1.19 GB) |
| **SUM** | **7 Allowlisted Worktrees** | — | — | **24,258.61 MB (~23.69 GB)** |

---

## 3. Cryptographic Verification & Manifest Binding

- **Manifest Path**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/cleanup_candidate_manifest.json`
- **Manifest SHA-256**: `0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc`
- **Audited Baseline HEAD**: `224f0bf486021afcf3d0773d4fc5fd6c86578411`
- **Initial Documentation Commit**: `583b2ff29dcae508c3ef03c777fa67e73859c718`
- **Ancestry Verification Policy**: Direct descendant of audited baseline `224f0bf486021afcf3d0773d4fc5fd6c86578411` with zero non-documentation diff (`git diff --name-only 224f0bf486021afcf3d0773d4fc5fd6c86578411 HEAD` must show only `docs/` files). No source code, tests, or configurations modified.
- **Provider WIP Backup Branch**: `feat/provider-discovery-v1.2-wip` (`96dfb4946e`)

---

## 4. Operator Approval Command Template

To authorize Phase B destructive cleanup, the operator issues the following directive in a new prompt:

```text
APPROVE P0 CLEANUP PHASE B: 7 allowlisted worktrees, manifest SHA-256: 0ad2ef03f893b448a16e8a618bf916b93445967a0f3a50ef19c5b4827c9b58cc
```

---

## 5. Phase B Execution Safeguards

Upon receipt of the approval directive:
1. Re-validate manifest hash and canonical repository HEAD.
2. Re-validate that working tree in each candidate remains clean.
3. Remove worktrees sequentially using Git-native `git worktree remove` (non-force).
4. Run `git worktree prune` to clear stale metadata.
5. Verify canonical repository integrity and free disk space.
6. Dừng ngay lập tức nếu phát hiện bất kỳ sự sai lệch nào.
