# P0 Safe Worktree Cleanup — Git Worktree Forensic Inventory

**Document ID**: `docs/reviews/M15-SAFE-WORKTREE-CLEANUP/02_GIT_WORKTREE_FORENSIC_INVENTORY.md`  
**Date**: 2026-11-05  
**Auditor**: Principal Git Repository Engineer, Windows Filesystem Safety Auditor  
**Audit Phase**: PHASE A — NON-DESTRUCTIVE SAFETY AUDIT  
**Common Git Directory**: `D:\Projects\agent-studios\.git`  

---

## 1. Executive Summary

Drive `D:\` hosts 13 distinct worktrees registered with Git common-dir `D:\Projects\agent-studios\.git`. Every directory was forensically audited for branch binding, HEAD SHA, reachability from the unified monorepo, and classification.

---

## 2. Protected Worktrees (NEVER DELETE)

The following 7 directories are permanently or conditionally protected under the Safety Gate:

| # | Worktree Path | Active Branch / Ref | HEAD SHA | Classification & Safety Rule |
|:---:|---|---|---|---|
| 1 | `D:\Projects\agent-studios` | `integration/m15-unified-codebase` | `224f0bf486` | **CANONICAL MONOREPO ROOT** — Permanent root |
| 2 | `D:\Projects\agent-studios-unified-staging` | `(detached HEAD)` | `843be62033` | **UNIFIED STAGING** — Hold pending PR merge |
| 3 | `D:\Projects\agent-studios-m15-04` | `feat/m15-04-durable-state-recovery`| `cf1e6c0f98` | **AUTHORITATIVE BACKEND** — Hold pending PR #30 |
| 4 | `D:\Projects\agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` | `3de7c85a96` | **AUTHORITATIVE FRONTEND** — Hold pending PR #27 |
| 5 | `D:\Projects\agent-studios-mission-control` | `feat/m14-mission-control-workbench`| `3ee485cacd` | **HOLD** — Divergent commit history |
| 6 | `D:\Projects\agent-studios-ags-cli` | `feat/provider-management-v1` | `a98129ef7e` | **INDEPENDENT** — Retain PR #20 CLI work |
| 7 | `D:\Projects\agent-studios-preservation` | `feat/provider-discovery-v1.2-wip` | `96dfb4946e` | **EXTERNAL BACKUP** — Cryptographic preservation |

---

## 3. Cleanup Candidate Allowlist (Audited for Phase B)

Strictly bounded to the 7 approved candidate directories:

| # | Worktree Path | Active Branch | HEAD SHA | Ancestor of Unified (`224f0bf486`) | Unique Commits |
|:---:|---|---|---|:---:|:---:|
| 1 | `D:\Projects\agent-studios-m13` | `feat/m13-ipc-02-control-plane` | `ce0396c1f1` | **YES** | **0** |
| 2 | `D:\Projects\agent-studios-m15` | `feat/m15-01-per-task-execution-contract` | `a5dbbfc5eb` | **YES** | **0** |
| 3 | `D:\Projects\agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` | `b307b9a6e2` | **YES** | **0** |
| 4 | `D:\Projects\agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` | `cc1c3e8f39` | **YES** | **0** |
| 5 | `D:\Projects\agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening`| `ed80b243c9` | **YES** | **0** |
| 6 | `D:\Projects\agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` | `73b7298ca3` | **YES** | **0** |
| 7 | `D:\Projects\agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` | `a13230ee93` | **YES** | **0** |

---

## 4. Worktree Lock and Registration Status

- Git command output `git worktree list --porcelain` confirms:
  * Zero candidate worktrees are locked (`locked` attribute absent).
  * Zero worktrees have prune metadata blockers.
  * All 7 candidate worktrees point cleanly to common-dir `.git` inside canonical root.
