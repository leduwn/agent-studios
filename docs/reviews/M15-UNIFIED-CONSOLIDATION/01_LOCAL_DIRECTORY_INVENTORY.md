# 01_LOCAL_DIRECTORY_INVENTORY.md — Agent Studios M15 Consolidation Inventory

**Document Reference**: `01_LOCAL_DIRECTORY_INVENTORY.md`  
**Milestone**: M15 Consolidation & Workspace Audit  
**Auditor**: Agent D (Workspace Inventory & Cleanup Auditor)  
**Audit Date**: 2026-11-04  
**Workspace Root**: `D:\Projects`  
**Canonical Git Repository**: `D:\Projects\agent-studios` (`https://github.com/leduwn/agent-studios.git`)  

---

## 1. Executive Summary

This inventory audits all 12 directories matching `D:\Projects\agent-studios*` on the host machine. All 12 directories share a single underlying Git repository object database located at `D:\Projects\agent-studios\.git`. Specifically:
- **1 Canonical Repository Root**: `D:\Projects\agent-studios` containing the full `.git` metadata directory.
- **11 Linked Git Worktrees**: All other directories are linked worktrees pointing back to `D:\Projects\agent-studios\.git/worktrees/<name>`.
- **Git Worktree Registry Bloat**: There are **292 total worktrees** registered in `D:\Projects\agent-studios\.git/worktrees`. Of these, **11** are the long-lived worktrees in `D:\Projects`, while **281** are stale test/subagent worktrees located in `C:\Users\Duwn\AppData\Local\Temp\agent-studios\worktrees`.
- **Total Disk Footprint**: **49.55 GB** across **204,497 files**. The primary disk consumers are Rust build artifacts (`target` directories totaling ~40 GB in `m15-02`, `m15-03`, `m15-04`) and Code Review graphs (`.code-review-graph` totaling 1.83 GB in `agent-studios`).
- **Working Tree Cleanliness**: 11 of the 12 directories have **0 uncommitted changes** and are 100% clean. Only `agent-studios` has **9 uncommitted files** (5 modified, 4 untracked).
- **Branch Merges**: None of the 12 feature branches are merged into `origin/main`. However, intermediate milestone worktrees have been linearly superseded by downstream milestone branches.

### Classification Summary Table

| Category | Count | Directories | Rationale |
| :--- | :---: | :--- | :--- |
| **`CANONICAL_CANDIDATE`** | 1 | `agent-studios` | Primary Git root and object store. Holds Claude Code runtime branch (`feat/m12-claude-code-runtime`) and 9 uncommitted files. Cannot be pruned. |
| **`ACTIVE_WORKTREE`** | 2 | `agent-studios-m15-04`<br>`agent-studios-ide-ui` | The active development tips for the backend execution pipeline (M15-05) and the frontend VS Code UI (M15 UI) respectively. Core consolidation targets. |
| **`UNMERGED_WORK`** | 1 | `agent-studios-ags-cli` | Provider Management V1 feature (`feat/provider-management-v1`). Fully committed and pushed to remote, but unmerged to main. Clean worktree. |
| **`HOLD`** | 1 | `agent-studios-mission-control` | Workbench branch (`feat/m14-mission-control-workbench`). Contains 3 unique commits (DAG, review UI, architecture specs) not present in M14 integration or M15 UI. |
| **`ARCHIVE_CANDIDATE`** | 7 | `agent-studios-m13`<br>`agent-studios-m15`<br>`agent-studios-m15-02`<br>`agent-studios-m15-03`<br>`agent-studios-mission-control-integration`<br>`agent-studios-vscode-ipc-03`<br>`agent-studios-vscode-m14-02` | Intermediate milestone worktrees. All commits are fully subsumed by downstream tip branches (`m15-04` or `ide-ui`). Clean working copies. Safe to prune once gated. |

---

## 2. Master Directory Inventory

| # | Directory Name | Branch | HEAD SHA | Upstream Branch | Ahead / Behind Upstream | Porcelain Status | Disk Size (MB) | Disk Size (GB) | Classification |
| :--- | :--- | :--- | :--- | :--- | :---: | :---: | :---: | :---: | :--- |
| 1 | `agent-studios` | `feat/m12-claude-code-runtime` | `3207b7462f` | `origin/feat/m12-claude-code-runtime` | 0 / 0 | **9 Dirty** | 5,527.52 | 5.398 | **`CANONICAL_CANDIDATE`** |
| 2 | `agent-studios-ags-cli` | `feat/provider-management-v1` | `a98129ef7e` | `origin/feat/provider-management-v1` | 0 / 0 | Clean (0) | 94.69 | 0.092 | **`UNMERGED_WORK`** |
| 3 | `agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` | `3de7c85a96` | `origin/feat/m15-real-agent-execution-ui` | 0 / 0 | Clean (0) | 1,221.46 | 1.193 | **`ACTIVE_WORKTREE`** |
| 4 | `agent-studios-m13` | `feat/m13-ipc-02-control-plane` | `ce0396c1f1` | *(none configured)* `origin/...` matches | 0 / 0 | Clean (0) | 89.73 | 0.088 | **`ARCHIVE_CANDIDATE`** |
| 5 | `agent-studios-m15` | `feat/m15-01-per-task-execution-contract` | `a5dbbfc5eb` | `origin/feat/m15-01-per-task-execution-contract` | 0 / 0 | Clean (0) | 89.85 | 0.088 | **`ARCHIVE_CANDIDATE`** |
| 6 | `agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` | `b307b9a6e2` | `origin/feat/m15-02-multi-runtime-registry` | 0 / **12** | Clean (0) | 7,568.46 | 7.391 | **`ARCHIVE_CANDIDATE`** |
| 7 | `agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` | `cc1c3e8f39` | *(none configured)* `origin/...` matches | 0 / 0 | Clean (0) | 13,880.54 | 13.555 | **`ARCHIVE_CANDIDATE`** |
| 8 | `agent-studios-m15-04` | `feat/m15-04-durable-state-recovery` | `cf1e6c0f98` | `origin/feat/m15-04-durable-state-recovery` | 0 / 0 | Clean (0) | 19,438.73 | 18.983 | **`ACTIVE_WORKTREE`** |
| 9 | `agent-studios-mission-control` | `feat/m14-mission-control-workbench` | `3ee485cacd` | `origin/feat/m14-mission-control-workbench` | 0 / 0 | Clean (0) | 193.98 | 0.189 | **`HOLD`** |
| 10 | `agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` | `a13230ee93` | `origin/feat/m14-mission-control-connected-integration` | 0 / 0 | Clean (0) | 1,218.63 | 1.190 | **`ARCHIVE_CANDIDATE`** |
| 11 | `agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` | `73b7298ca3` | `origin/feat/m14-ipc-03-connected-client` | 0 / 0 | Clean (0) | 193.83 | 0.189 | **`ARCHIVE_CANDIDATE`** |
| 12 | `agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening` | `ed80b243c9` | `origin/feat/m14-vscode-extension-host-hardening` | 0 / 0 | Clean (0) | 1,217.57 | 1.189 | **`ARCHIVE_CANDIDATE`** |

---

## 3. Detailed Directory Dossiers

### Directory 1: `agent-studios` (Canonical Repository Root)
- **Absolute Path**: `D:\Projects\agent-studios`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios`
  - `common-dir`: `.git`
  - `git-dir`: `.git` (Primary physical repository directory; contains all Git objects, refs, and 292 worktree control pointers)
  - `is_root_repository`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m12-claude-code-runtime`
  - HEAD SHA: `3207b7462f9e3055ceee6996c4bfc914cf26fae7`
  - Subject: `docs(reviews): add M12.3 security hardening audit report and handoff`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-28 12:47:04 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m12-claude-code-runtime`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 7 commits (`710051e656`..`3207b7462f`), Behind 0
- **Working Tree Status (DIRTY - 9 files)**:
  - Tracked Modifications (5):
    - `M agent-studios-rs/Cargo.lock`
    - `M agent-studios-rs/crates/provider/Cargo.toml`
    - `M agent-studios-rs/crates/provider/src/error.rs`
    - `M agent-studios-rs/crates/provider/src/lib.rs`
    - `M agent-studios-rs/crates/provider/src/secret.rs`
  - Untracked Files (4):
    - `?? AGENTS.md`
    - `?? agent-studios-rs/crates/provider/src/discovery.rs`
    - `?? agent-studios-rs/crates/provider/tests/discovery_tests.rs`
    - `?? docs/reviews/M13-IPC-02.2.2/`
  - Note: These uncommitted files represent an early working copy of provider discovery that was subsequently committed in `feat/provider-management-v1` (`agent-studios-ags-cli`) and M13 documentation (`agent-studios-m13`).
- **Disk Usage**:
  - Total Size: **5,527.52 MB** (5.398 GB)
  - File Count: 23,280 files
  - Key Directories:
    - `.git`: 1,289.56 MB
    - `.code-review-graph`: 1,834.60 MB
    - `agent-studios-rs`: 638.50 MB
    - `codex-rs`: 552.88 MB
- **Process Locks**: VS Code workspace cache has record of `file:///d%3A/Projects/agent-studios`. No active exclusive process file lock.
- **Classification**: **`CANONICAL_CANDIDATE`**
  - **Rationale**: Must be retained as the permanent workspace root. The 9 uncommitted changes must be preserved (stashed or diffed against `feat/provider-management-v1`) before any branch switching or consolidation.

---

### Directory 2: `agent-studios-ags-cli`
- **Absolute Path**: `D:\Projects\agent-studios-ags-cli`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-ags-cli`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-ags-cli`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/provider-management-v1`
  - HEAD SHA: `a98129ef7e03816b936fe194adede0c25e287674`
  - Subject: `fix(provider): report RateLimited on HTTP 429 and standardize PR governance`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 10:37:49 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/provider-management-v1`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 15 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **94.69 MB** (0.092 GB)
  - File Count: 9,238 files
- **Process Locks**: None
- **Classification**: **`UNMERGED_WORK`**
  - **Rationale**: Contains completed Provider Management V1 CLI feature. Fully pushed to remote branch `origin/feat/provider-management-v1`. Unmerged to `main`. Worktree is lightweight (94 MB) and clean. Can be kept until PR merge or archived once confirmed remote branch is authoritative.

---

### Directory 3: `agent-studios-ide-ui`
- **Absolute Path**: `D:\Projects\agent-studios-ide-ui`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-ide-ui`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-ide-ui`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m15-real-agent-execution-ui`
  - HEAD SHA: `3de7c85a966d1b46a293235b11170219d53cc7f3`
  - Subject: `feat(m15-ui): frontend reliability, shared state consistency and integration readiness (M15-UI-05A)`
  - Author: `Duwn <coigayto@gmail.com>` (2026-11-02 12:13:30 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m15-real-agent-execution-ui`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 34 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **1,221.46 MB** (1.193 GB)
  - File Count: 25,068 files (includes VS Code extension packages and `node_modules`)
- **Process Locks**: None
- **Classification**: **`ACTIVE_WORKTREE`**
  - **Rationale**: The active frontend tip for M15 Real Agent Execution UI. Subsumes `vscode-m14-02`, `vscode-ipc-03`, and `mission-control-integration`. Must be preserved during consolidation to merge frontend changes into canonical branch.

---

### Directory 4: `agent-studios-m13`
- **Absolute Path**: `D:\Projects\agent-studios-m13`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-m13`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-m13`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m13-ipc-02-control-plane`
  - HEAD SHA: `ce0396c1f165760ea271110eaf9bfd6dc84a67a7`
  - Subject: `feat(ipc): freeze protocol contract, add golden fixtures, audit M15 readiness and author cross-team handoff (M13 IPC-02.2.2)`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 18:51:24 +0700)
- **Upstream Status**:
  - Tracking Branch: None set locally; `origin/feat/m13-ipc-02-control-plane` exists on remote and matches HEAD exactly (0/0)
  - vs `origin/main`: Ahead 17 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **89.73 MB** (0.088 GB)
  - File Count: 9,188 files
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: 100% of this branch's commits are an exact ancestor of `m15-01`, `m15-02`, `m15-03`, and `m15-04`. Fully pushed to remote. Clean. Ready for safe removal.

---

### Directory 5: `agent-studios-m15`
- **Absolute Path**: `D:\Projects\agent-studios-m15`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-m15`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-m15`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m15-01-per-task-execution-contract`
  - HEAD SHA: `a5dbbfc5ebada254727db9a788c0ee40e3035ea8`
  - Subject: `feat(ipc): harden readiness semantics, workspace containment, and golden fixtures (M15-01.1)`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-31 08:34:10 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m15-01-per-task-execution-contract`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 19 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **89.85 MB** (0.088 GB)
  - File Count: 9,194 files
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: 100% of this branch's commits are an exact ancestor of `m15-02`, `m15-03`, and `m15-04`. Fully pushed to remote. Clean. Ready for safe removal.

---

### Directory 6: `agent-studios-m15-02`
- **Absolute Path**: `D:\Projects\agent-studios-m15-02`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-m15-02`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-m15-02`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m15-02-multi-runtime-registry`
  - HEAD SHA: `b307b9a6e2bea51873d22e98127562b745eed4fb`
  - Subject: `feat(ipc): implement M15-02 multi-runtime adapter registry and deterministic routing`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-31 09:37:37 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m15-02-multi-runtime-registry`
  - Ahead: 0 | **Behind: 12** | Unpushed Commits: 0
  - Note: Remote origin has 12 newer commits (`6752d88494`..`61cb7e88e4`) that were pushed during CI/review. Those 12 commits are already present in downstream branch `feat/m15-03-artifact-diff-review`.
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **7,568.46 MB** (7.391 GB)
  - File Count: 26,211 files
  - Key Component: `agent-studios-rs/target` = 7,481.81 MB
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: Intermediate milestone. All commits (including the 12 remote commits) are included in `m15-03` and `m15-04`. Worktree consumes 7.39 GB due to build target. Prime candidate for disk reclamation and worktree removal.

---

### Directory 7: `agent-studios-m15-03`
- **Absolute Path**: `D:\Projects\agent-studios-m15-03`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-m15-03`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-m15-03`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m15-03-artifact-diff-review`
  - HEAD SHA: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`
  - Subject: `feat(ipc): implement lossless diff streaming, binary pagination and protocol hardening (M15-03.1)`
  - Author: `Duwn <coigayto@gmail.com>` (2026-11-01 22:45:51 +0700)
- **Upstream Status**:
  - Tracking Branch: None set locally; `origin/feat/m15-03-artifact-diff-review` matches HEAD exactly (0/0)
  - vs `origin/main`: Ahead 34 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **13,880.54 MB** (13.555 GB)
  - File Count: 35,364 files
  - Key Component: `agent-studios-rs/target` = 13,793.79 MB
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: Direct ancestor of `m15-04`. Consumes 13.56 GB on disk. Fully pushed to origin. Clean. Ready for safe removal once `m15-04` is preserved.

---

### Directory 8: `agent-studios-m15-04`
- **Absolute Path**: `D:\Projects\agent-studios-m15-04`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-m15-04`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-m15-04`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m15-04-durable-state-recovery`
  - HEAD SHA: `cf1e6c0f98d568ed0b4c9da9ecf80c6d3eea456d`
  - Subject: `feat(backend): implement durable event replay, crash-safe reconnect & real execution readiness (M15-05)`
  - Author: `Duwn <coigayto@gmail.com>` (2026-11-02 01:34:04 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m15-04-durable-state-recovery`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 38 commits (incorporates M13, M15-01, M15-02, M15-03, M15-04, and M15-05)
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **19,438.73 MB** (18.983 GB)
  - File Count: 34,364 files
  - Key Component: `agent-studios-rs/target` = ~19 GB
- **Process Locks**: None
- **Classification**: **`ACTIVE_WORKTREE`**
  - **Rationale**: The definitive backend execution tip. Contains full event replay, SQLite durable recovery, crash-safe reconnect, lossless diff streaming, and multi-runtime adapter registry. Crucial consolidation source.

---

### Directory 9: `agent-studios-mission-control`
- **Absolute Path**: `D:\Projects\agent-studios-mission-control`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-mission-control`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-mission-control`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m14-mission-control-workbench`
  - HEAD SHA: `3ee485cacd778ffec063f3d328661e74111ecc2a`
  - Subject: `docs(mission-control): align product spec and reference inventory with research findings`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 14:45:00 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m14-mission-control-workbench`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 7 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **193.98 MB** (0.189 GB)
  - File Count: 21,389 files
- **Process Locks**: None
- **Classification**: **`HOLD`**
  - **Rationale**: **CRITICAL AUDIT FINDING**. This branch contains 3 commits (`e988e785ac`, `4693c63486`, `3ee485cacd`) that were NOT merged into `feat/m14-mission-control-connected-integration` or `feat/m15-real-agent-execution-ui`. It includes Mission Control DAG visualization, multi-tab review interface, and product specs. Must be held and reviewed for cherry-picking before any cleanup.

---

### Directory 10: `agent-studios-mission-control-integration`
- **Absolute Path**: `D:\Projects\agent-studios-mission-control-integration`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-mission-control-integration`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-mission-control-integration`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m14-mission-control-connected-integration`
  - HEAD SHA: `a13230ee93afd101f8b0f0c5b6b0e3d61013c9af`
  - Subject: `fix(cross-platform): validate foreign-platform path before confirmation challenge evaluation`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 15:56:06 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m14-mission-control-connected-integration`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 18 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **1,218.63 MB** (1.190 GB)
  - File Count: 24,851 files
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: Exact ancestor of `agent-studios-ide-ui` (`feat/m15-real-agent-execution-ui`). Fully pushed to remote. Clean. Ready for safe removal once M15 UI is verified.

---

### Directory 11: `agent-studios-vscode-ipc-03`
- **Absolute Path**: `D:\Projects\agent-studios-vscode-ipc-03`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-vscode-ipc-03`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-vscode-ipc-03`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m14-ipc-03-connected-client`
  - HEAD SHA: `73b7298ca38cd0eaf2ce0e6ae169cd6919670550`
  - Subject: `feat(vscode): deliver M14-IPC-03 real ConnectedClient vertical slice`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 12:16:38 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m14-ipc-03-connected-client`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 5 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **193.83 MB** (0.189 GB)
  - File Count: 21,342 files
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: Exact ancestor of `feat/m14-mission-control-connected-integration` and `feat/m15-real-agent-execution-ui`. Fully pushed to remote. Clean. Ready for safe removal.

---

### Directory 12: `agent-studios-vscode-m14-02`
- **Absolute Path**: `D:\Projects\agent-studios-vscode-m14-02`
- **Git Identity**:
  - `top-level`: `D:/Projects/agent-studios-vscode-m14-02`
  - `common-dir`: `D:/Projects/agent-studios/.git`
  - `git-dir`: `D:/Projects/agent-studios/.git/worktrees/agent-studios-vscode-m14-02`
  - `is_linked_worktree`: `true`
- **Branch & Commit**:
  - Current Branch: `feat/m14-vscode-extension-host-hardening`
  - HEAD SHA: `ed80b243c9e9fce911a86e023790131c6d3688c7`
  - Subject: `feat(vscode): deliver M14-UI-02.1 protocol correctness and security hardening`
  - Author: `Duwn <coigayto@gmail.com>` (2026-10-30 10:08:44 +0700)
- **Upstream Status**:
  - Tracking Branch: `origin/feat/m14-vscode-extension-host-hardening`
  - Ahead: 0 | Behind: 0 | Unpushed Commits: 0
  - vs `origin/main`: Ahead 4 commits, Behind 0
- **Working Tree Status**: **Clean** (0 uncommitted changes)
- **Disk Usage**:
  - Total Size: **1,217.57 MB** (1.189 GB)
  - File Count: 24,745 files
- **Process Locks**: None
- **Classification**: **`ARCHIVE_CANDIDATE`**
  - **Rationale**: Common ancestor of all M14/M15 VS Code branches (`ipc-03`, `mission-control`, `integration`, and `ide-ui`). Fully pushed to remote. Clean. Ready for safe removal.

---

## 4. Lineage & Dependency Graph

```
origin/main (commit dc363d1e89)
 │
 ├── [Lineage 1: Core Backend & IPC Execution Stack]
 │    └── feat/m13-ipc-02-control-plane (ce0396c1f1) [agent-studios-m13]
 │         └── feat/m15-01-per-task-execution-contract (a5dbbfc5eb) [agent-studios-m15]
 │              └── feat/m15-02-multi-runtime-registry (b307b9a6e2 / origin: 61cb7e88e4) [agent-studios-m15-02]
 │                   └── feat/m15-03-artifact-diff-review (cc1c3e8f39) [agent-studios-m15-03]
 │                        └── feat/m15-04-durable-state-recovery (cf1e6c0f98 - M15-05) [agent-studios-m15-04] ★ BACKEND TIP
 │
 ├── [Lineage 2: Frontend & VS Code UI Stack]
 │    └── feat/m14-vscode-extension-host-hardening (ed80b243c9) [agent-studios-vscode-m14-02]
 │         ├── feat/m14-ipc-03-connected-client (73b7298ca3) [agent-studios-vscode-ipc-03]
 │         │    └── feat/m14-mission-control-connected-integration (a13230ee93) [agent-studios-mission-control-integration]
 │         │         └── feat/m15-real-agent-execution-ui (3de7c85a96) [agent-studios-ide-ui] ★ FRONTEND TIP
 │         │
 │         └── [Lineage 5: Divergent Mission Control Workbench]
 │              └── feat/m14-mission-control-workbench (3ee485cacd) [agent-studios-mission-control] ⚠️ HOLD (3 unique commits)
 │
 ├── [Lineage 3: Provider Management & CLI]
 │    └── feat/provider-management-v1 (a98129ef7e) [agent-studios-ags-cli] ★ PROVIDER TIP
 │
 └── [Lineage 4: Claude Code Runtime Adapter & Audit]
      └── feat/m12-claude-code-runtime (3207b7462f) [agent-studios] ★ RUNTIME ROOT (9 uncommitted files)
```

---

## 5. Shallow Repository Impact Analysis

During ancestry analysis, `git merge-base` operations between `feat/m15-04-durable-state-recovery` and `origin/main` initially reported non-zero exit status. Investigation confirmed:
- The repository object store is configured as a **shallow repository** (`git rev-parse --is-shallow-repository` returns `true`).
- File `.git/shallow` contains commit SHA `dcbcb39bf61225a4660be5b533cf1ffd5de14b29` as its boundary commit.
- **Operational Impact**: Git operations that need to traverse history across the shallow cut-off point may report missing ancestor objects if not unshallowed.
- **Consolidation Guidance**: Prior to running automated rebase or multi-branch squashes, run `git fetch --unshallow` or ensure all referenced parent SHAs are loaded locally.

---

## 6. Git Worktree Registry Audit

A full audit of `D:\Projects\agent-studios\.git/worktrees` revealed:
- **Total Worktree Control Directories**: **292**
- **Active Worktrees in `D:\Projects`**: **11**
- **Orphaned / Transient Worktrees in AppData Temp**: **281**
  - Path pattern: `C:/Users/Duwn/AppData/Local/Temp/agent-studios/worktrees/<hash>/agent-studios-ags-cli`
  - Created by: Automated CLI test suites and integration runners executing agent isolation tests.
  - Risk: Bloats `git worktree list` output (over 42 KB), slows Git reference updates, and consumes local drive C: storage.
  - Remediation: Documented in `09_SAFE_WORKTREE_CLEANUP_PLAN.md`.

---
*Report generated by Agent D (Workspace Inventory & Cleanup Auditor) for AGENT STUDIOS M15 CONSOLIDATION.*
