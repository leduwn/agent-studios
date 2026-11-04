# M15 Canonical Migration — Worktree Cleanup Approval Specification

**Document ID**: `docs/reviews/M15-CANONICAL-MIGRATION/07_WORKTREE_CLEANUP_APPROVAL.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Integration Engineer & Release Manager  
**Safety Gate**: Zero Deletion Safety Gate (100% Enforced)  
**Status**: PENDING OPERATOR SIGN-OFF  

---

## 1. Executive Summary

During sprints M10 through M15, 12 distinct worktree directories were spawned on drive `D:\`. This document establishes the formal classification, disk usage breakdown, and step-by-step cleanup procedure requiring explicit human operator authorization.

**Zero Deletion Invariant Confirmation**:
During this canonical migration pass, **zero directories were deleted**, **zero worktrees were removed**, and **no compiler caches were cleared**.

---

## 2. Directory Classification & Reclaimable Footprint

```text
D:\Projects/
├── agent-studios                         [CANONICAL_ACTIVE] -> RETAIN PERMANENTLY
├── agent-studios-unified-staging         [STAGING_WORKTREE] -> RETAIN TEMPORARILY
│
├── agent-studios-m15-04                  [HOLD] -> Retain pending PR #30 merge
├── agent-studios-ide-ui                  [HOLD] -> Retain pending PR #27 merge
├── agent-studios-mission-control         [HOLD] -> Retain (divergent commit 3ee485ca)
├── agent-studios-ags-cli                 [INDEPENDENT] -> Retain (PR #20 CLI work)
│
├── agent-studios-m13                     [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~4.46 GB)
├── agent-studios-m15                     [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~4.47 GB)
├── agent-studios-m15-02                  [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~4.48 GB)
├── agent-studios-m15-03                  [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~4.50 GB)
├── agent-studios-vscode-m14-02           [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~421 MB)
├── agent-studios-vscode-ipc-03           [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~427 MB)
└── agent-studios-mission-control-integration [APPROVED_CANDIDATE] -> Phase 2 Cleanup (~434 MB)
```

### Complete Classification Matrix

| # | Worktree Path | Classification | Total Disk | Cache Size | Reclaimable | Operator Action |
|:---:|---|:---:|:---:|:---:|:---:|:---:|
| 1 | `agent-studios` | `CANONICAL_ACTIVE` | 1.01 GB | N/A | 0.00 GB | **PERMANENT ROOT** |
| 2 | `agent-studios-unified-staging` | `ACTIVE_STAGING` | 306 MB | N/A | 0.31 GB | Post-migration decommission |
| 3 | `agent-studios-m15-04` | `HOLD` | 4.64 GB | ~4.20 GB | 0.00 GB | **HOLD** (PR #30 tip) |
| 4 | `agent-studios-ide-ui` | `HOLD` | 461 MB | ~150 MB | 0.00 GB | **HOLD** (PR #27 tip) |
| 5 | `agent-studios-mission-control` | `HOLD` | 431 MB | ~140 MB | 0.00 GB | **HOLD** (Divergent history) |
| 6 | `agent-studios-ags-cli` | `INDEPENDENT` | 4.57 GB | ~4.10 GB | 0.00 GB | **RETAIN** (PR #20) |
| 7 | `agent-studios-m13` | `APPROVED_CANDIDATE` | 4.46 GB | ~4.12 GB | 4.46 GB | Safe to prune |
| 8 | `agent-studios-m15` | `APPROVED_CANDIDATE` | 4.47 GB | ~4.13 GB | 4.47 GB | Safe to prune |
| 9 | `agent-studios-m15-02` | `APPROVED_CANDIDATE` | 4.48 GB | ~4.14 GB | 4.48 GB | Safe to prune |
| 10 | `agent-studios-m15-03` | `APPROVED_CANDIDATE` | 4.50 GB | ~4.15 GB | 4.50 GB | Safe to prune |
| 11 | `agent-studios-vscode-m14-02` | `APPROVED_CANDIDATE` | 421 MB | ~145 MB | 0.42 GB | Safe to prune |
| 12 | `agent-studios-vscode-ipc-03` | `APPROVED_CANDIDATE` | 427 MB | ~150 MB | 0.43 GB | Safe to prune |
| 13 | `agent-studios-mission-control-integration` | `APPROVED_CANDIDATE` | 434 MB | ~155 MB | 0.43 GB | Safe to prune |

Total Reclaimable across 7 candidate worktrees: **~19.19 GB to ~23.49 GB**.

---

## 3. Post-Approval Execution Script (For Operator Use Only)

Once the operator explicitly authorizes cleanup, execute the following staged commands:

```powershell
# Phase 1: Clean build caches in candidate worktrees
cargo clean --manifest-path "D:\Projects\agent-studios-m13\agent-studios-rs\Cargo.toml"
cargo clean --manifest-path "D:\Projects\agent-studios-m15\agent-studios-rs\Cargo.toml"
cargo clean --manifest-path "D:\Projects\agent-studios-m15-02\agent-studios-rs\Cargo.toml"
cargo clean --manifest-path "D:\Projects\agent-studios-m15-03\agent-studios-rs\Cargo.toml"

# Phase 2: Prune candidate worktrees cleanly
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m13"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15-02"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-m15-03"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-vscode-m14-02"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-vscode-ipc-03"
git -C "D:\Projects\agent-studios" worktree remove "D:\Projects\agent-studios-mission-control-integration"

# Phase 3: Prune worktree metadata
git -C "D:\Projects\agent-studios" worktree prune
```
