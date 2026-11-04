# 01 — Inventory Reconciliation: M15 Consolidation 01.1

**Document Reference**: `docs/reviews/M15-UNIFIED-CONSOLIDATION-01-1/01_INVENTORY_RECONCILIATION.md`  
**Milestone**: M15 Consolidation & Canonical Preflight  
**Integration Branch**: `integration/m15-unified-codebase`  
**Unified HEAD**: `5c8f5fd6d84376c4b1c2420f7f8b911fb58d3a3c`  
**Staging Path**: `D:\Projects\agent-studios-unified-staging`  
**Canonical Target**: `D:\Projects\agent-studios`  
**Auditor**: Lead Integration Engineer & Release Manager  

---

## 1. Executive Summary

This inventory reconciliation establishes ground truth across all 12 local directories under `D:\Projects` and reconciles classification ambiguities identified in initial scans.

Key reconciliation determinations:
1. **Resolution of `agent-studios-mission-control` (HOLD vs SAFE_TO_ARCHIVE)**:
   - Git DAG analysis proves that commits `e988e785ac`, `4693c63486`, and `3ee485cacd` are **NOT** direct ancestors in the commit DAG of `3de7c85a96` or unified HEAD (`git merge-base --is-ancestor` fails).
   - Although their code and documentation was incorporated and improved in PR #27 (`3de7c85a96`), the commit objects themselves diverge from common ancestor `ed80b243c9`.
   - **Verdict**: Per the operator rule, `agent-studios-mission-control` is classified as **`HOLD`**. It must NOT be deleted. The branch remains safely tracked on remote `origin/feat/m14-mission-control-workbench`.
2. **Drive `E:\project\agent-studios` Verification**:
   - Explicit host check via `Test-Path "E:\"` and `Test-Path "E:\project\agent-studios"` confirmed drive `E:` **does not exist** on this Windows host. No split repositories exist on other local volumes.
3. **Canonical Workspace Protection (`D:\Projects\agent-studios`)**:
   - Contains uncommitted provider discovery work (`crates/provider/src/discovery.rs`, `tests/discovery_tests.rs`, `Cargo.toml`, `secret.rs`, `Cargo.lock`, `AGENTS.md`).
   - Retained as `CANONICAL_CANDIDATE` with zero modifications or stashes.
4. **Staging Worktree (`D:\Projects\agent-studios-unified-staging`)**:
   - Designated as the unified active workspace holding all consolidated crates and documentation.

---

## 2. Comprehensive Directory Reconciliation Matrix

| # | Directory Path | Branch / HEAD SHA | Classification | Disk Size | Git Status | Active Handles | Reconciled Status |
|:---:|---|---|:---:|:---:|:---:|:---:|:---:|
| 1 | `D:\Projects\agent-studios` | `feat/m12-claude-code-runtime` (`3207b746`) | `CANONICAL_CANDIDATE` | 1.01 GB | Dirty (uncommitted provider work) | VS Code, Node | **RETAIN (CANONICAL ROOT)** |
| 2 | `D:\Projects\agent-studios-unified-staging` | `integration/m15-unified-codebase` (`5c8f5fd6`) | `ACTIVE_WORKTREE` | 306 MB | Clean (staging workspace) | Cargo build | **RETAIN (UNIFIED STAGING)** |
| 3 | `D:\Projects\agent-studios-m15-04` | `feat/m15-04-durable-state-recovery` (`cf1e6c0f`)| `ACTIVE_WORKTREE` | 4.64 GB | Clean | None | **HOLD (PENDING PR #30 MERGE)** |
| 4 | `D:\Projects\agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` (`3de7c85a`) | `ACTIVE_WORKTREE` | 461 MB | Clean | None | **HOLD (PENDING PR #27 MERGE)** |
| 5 | `D:\Projects\agent-studios-mission-control` | `feat/m14-mission-control-workbench` (`3ee485ca`)| **`HOLD`** | 431 MB | Clean | None | **`HOLD` (UNIQUE COMMITS DIVERGE)** |
| 6 | `D:\Projects\agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` (`a13230ee`) | `APPROVED_CANDIDATE` | 434 MB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 7 | `D:\Projects\agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` (`73b7298c`)| `APPROVED_CANDIDATE` | 427 MB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 8 | `D:\Projects\agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening` (`ed80b243`)| `APPROVED_CANDIDATE` | 421 MB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 9 | `D:\Projects\agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` (`cc1c3e8f`)| `APPROVED_CANDIDATE` | 4.50 GB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 10 | `D:\Projects\agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` (`b307b9a6`)| `APPROVED_CANDIDATE` | 4.48 GB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 11 | `D:\Projects\agent-studios-m15` | `feat/m15-01-per-task-contract` (`a5dbbfc5`)| `APPROVED_CANDIDATE` | 4.47 GB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 12 | `D:\Projects\agent-studios-m13` | `feat/m13-ipc-02-control-plane` (`ce0396c1`)| `APPROVED_CANDIDATE` | 4.46 GB | Clean | None | **APPROVED FOR REMOVAL (POST-APPROVAL)** |
| 13 | `D:\Projects\agent-studios-ags-cli` | `feat/provider-management-v1` (`a98129ef`)| `INDEPENDENT_PROJECT` | 4.57 GB | Clean | None | **INDEPENDENT (SEPARATE MERGE)** |

---

## 3. Deep Analysis: `agent-studios-mission-control` Reconciled to `HOLD`

The operator noted a conflict between `SAFE_TO_ARCHIVE` and `HOLD` for `agent-studios-mission-control`.

### 3.1 Divergence Analysis
- Common Ancestor: `ed80b243c9e9fce911a86e023790131c6d3688c7` (Merge of PR #22)
- Unique Commits on `feat/m14-mission-control-workbench`:
  1. `e988e785ac`: `docs(mission-control): establish M14 Mission Control architecture and specifications`
  2. `4693c63486`: `feat(vscode): deliver M14 Mission Control workbench multi-tab monitoring, DAG & review interface`
  3. `3ee485cacd`: `docs(mission-control): align product spec and reference inventory with research findings`

### 3.2 Git Rev-List & Ancestry Verification
```bash
git merge-base --is-ancestor 3ee485cacd 3de7c85a96
# Exit code: 1 (NOT an ancestor)

git merge-base --is-ancestor 3ee485cacd 5c8f5fd6d8
# Exit code: 1 (NOT an ancestor)
```

### 3.3 Code & Semantic Comparison
Comparing `3ee485cacd` and downstream frontend tip `3de7c85a96`:
- **Documentation**: All doc files under `docs/agent-studios/` and `docs/reviews/M14-MISSION-CONTROL/` are 100% identical (`git diff` is empty).
- **Source Code**: PR #27 took the Mission Control components (`MissionControlPanel.ts`, `MissionControlProjection.ts`, `DagGraphRenderer.ts`) and enhanced them with live IPC streaming (+662 lines, -113 lines).
- **Git History Invariant**: Because Git commit ancestry does not directly link `3ee485cacd` to `3de7c85a96`, deleting the worktree or deleting the local branch without explicit operator sign-off violates the non-destructive principle.

**Reconciliation Ruling**: `agent-studios-mission-control` is classified as **`HOLD`**. No automatic deletion or cherry-pick is permitted. The remote branch `origin/feat/m14-mission-control-workbench` remains fully preserved.
