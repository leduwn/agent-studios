# Agent Studios — Unified Repository & Workspace Management

> **Status**: Authoritative Workspace Inventory & Migration Specification  
> **Milestone**: M15 Consolidation & Canonical Master Reconstruction  
> **Target Branch**: `integration/m15-unified-codebase`  
> **Reference**: Implements `01_INVENTORY_RECONCILIATION.md`, `04_CANONICAL_WORKSPACE_MIGRATION.md`, and `05_FINAL_CLEANUP_APPROVAL_MATRIX.md`

---

## 1. Executive Summary: The Monorepo Target

`[EXISTING_CANONICAL_DECISION]`
The ultimate architectural goal of Milestone M15 is to establish `D:\Projects\agent-studios` as the single canonical repository root containing the entire unified Agent Studios platform (Rust backend, Claude Code runtime, Code-OSS extension, and CI automation).

During previous milestone sprints (M10–M15), development diverged across 12 distinct directories under `D:\Projects`. Under the strict **Phase 0 Safety Gate**, zero worktrees were deleted, no uncommitted files were overwritten, and zero paid API calls were made.

---

## 2. Inventory & Classification of All 12 Local Directories

```text
D:\Projects/
├── agent-studios                         [CANONICAL_CANDIDATE / ACTIVE]
│   └── Retain: Repository root holding .git database and uncommitted provider work
│
├── agent-studios-unified-staging         [ACTIVE_WORKTREE]
│   └── Retain: Active staging workspace for integration/m15-unified-codebase
│
├── agent-studios-m15-04                  [HOLD]
│   └── Retain: Authoritative backend tip (cf1e6c0f) pending PR #30 merge
│
├── agent-studios-ide-ui                  [HOLD]
│   └── Retain: Authoritative frontend tip (3de7c85a) pending PR #27 merge
│
├── agent-studios-mission-control         [HOLD]
│   └── Retain: Commits (3ee485ca) not in DAG; code rebased into PR #25
│
├── agent-studios-ags-cli                 [INDEPENDENT_PROJECT]
│   └── Retain: Standalone CLI, TUI, and provider management (a98129ef, PR #20)
│
├── agent-studios-m13                     [APPROVED_CANDIDATE] -> Phase 2 Cleanup
├── agent-studios-m15                     [APPROVED_CANDIDATE] -> Phase 2 Cleanup
├── agent-studios-m15-02                  [APPROVED_CANDIDATE] -> Phase 2 Cleanup
├── agent-studios-m15-03                  [APPROVED_CANDIDATE] -> Phase 2 Cleanup
├── agent-studios-vscode-m14-02           [APPROVED_CANDIDATE] -> Phase 2 Cleanup
├── agent-studios-vscode-ipc-03           [APPROVED_CANDIDATE] -> Phase 2 Cleanup
└── agent-studios-mission-control-integration [APPROVED_CANDIDATE] -> Phase 2 Cleanup
```

### Complete Classification Table

| # | Directory Path | Branch / SHA | Classification | Disk Size | Preflight Verdict |
|:---:|---|---|:---:|:---:|:---:|
| 1 | `agent-studios` | `feat/m12-claude-code-runtime` (`3207b746`) | `CANONICAL_CANDIDATE` | 1.01 GB | **RETAIN (CANONICAL ROOT)** |
| 2 | `agent-studios-unified-staging` | `integration/m15-unified-codebase` (`661daaa9`) | `ACTIVE_WORKTREE` | 306 MB | **RETAIN (ACTIVE STAGING)** |
| 3 | `agent-studios-m15-04` | `feat/m15-04-durable-state-recovery` (`cf1e6c0f`) | `HOLD` | 4.64 GB | **HOLD (PENDING PR #30 MERGE)** |
| 4 | `agent-studios-ide-ui` | `feat/m15-real-agent-execution-ui` (`3de7c85a`) | `HOLD` | 461 MB | **HOLD (PENDING PR #27 MERGE)** |
| 5 | `agent-studios-mission-control` | `feat/m14-mission-control-workbench` (`3ee485ca`) | `HOLD` | 431 MB | **HOLD (DIVERGENT COMMITS)** |
| 6 | `agent-studios-ags-cli` | `feat/provider-management-v1` (`a98129ef`) | `INDEPENDENT_PROJECT`| 4.57 GB | **RETAIN (PR #20 CLI WORK)** |
| 7 | `agent-studios-m13` | `feat/m13-ipc-02-control-plane` (`ce0396c1`) | `APPROVED_CANDIDATE` | 4.46 GB | **APPROVED FOR REMOVAL** |
| 8 | `agent-studios-m15` | `feat/m15-01-per-task-execution-contract` (`a5dbbfc5`)| `APPROVED_CANDIDATE` | 4.47 GB | **APPROVED FOR REMOVAL** |
| 9 | `agent-studios-m15-02` | `feat/m15-02-multi-runtime-registry` (`b307b9a6`) | `APPROVED_CANDIDATE` | 4.48 GB | **APPROVED FOR REMOVAL** |
| 10| `agent-studios-m15-03` | `feat/m15-03-artifact-diff-review` (`cc1c3e8f`) | `APPROVED_CANDIDATE` | 4.50 GB | **APPROVED FOR REMOVAL** |
| 11| `agent-studios-vscode-m14-02` | `feat/m14-vscode-extension-host-hardening` (`ed80b243`)| `APPROVED_CANDIDATE` | 421 MB | **APPROVED FOR REMOVAL** |
| 12| `agent-studios-vscode-ipc-03` | `feat/m14-ipc-03-connected-client` (`73b7298c`) | `APPROVED_CANDIDATE` | 427 MB | **APPROVED FOR REMOVAL** |
| 13| `agent-studios-mission-control-integration` | `feat/m14-mission-control-connected-integration` (`a13230ee`) | `APPROVED_CANDIDATE` | 434 MB | **APPROVED FOR REMOVAL** |

---

## 3. Resolution of `agent-studios-mission-control` Conflict

`[EXISTING_CANONICAL_DECISION]`
Initial automated scans classified `agent-studios-mission-control` ambiguously between `APPROVED_CANDIDATE` and `HOLD`.

**Cryptographic Analysis Outcome**:
- Commits `e988e785ac`, `4693c63486`, and `3ee485cacd` in `agent-studios-mission-control` diverged from parent `ed80b243c9`.
- When checked via `git merge-base --is-ancestor 3ee485cacd HEAD`, Git returns `FALSE` (non-zero exit code).
- Although their source code and documentation were rebased into PR #25 (`38b47d39` -> `eec37f9b` -> `86bb1e7f5b`) and integrated into unified HEAD, the raw commit objects themselves are not DAG ancestors.
- **Final Binding Verdict**: Retained on **`HOLD`** indefinitely. Remote tracking branch `origin/feat/m14-mission-control-workbench` is preserved.

---

## 4. Host Storage Volumes Verification

Explicit verification on the host machine disproved past assumptions:
- **`C:\` Drive**: Exists. 281.17 GB free space.
- **`D:\` Drive**: Exists. 30.15 GB free space (hosting all project worktrees).
- **`E:\` Drive**: **DOES NOT EXIST** (`Test-Path E:\` returned `False`). Proved conclusively that no split-volume repository exists on the host.

---

## 5. Canonical Workspace Migration Plan (`D:\Projects\agent-studios`)

`[EXISTING_CANONICAL_DECISION]`
The canonical repository root `D:\Projects\agent-studios` currently contains **uncommitted provider model discovery feature code**:
- `agent-studios-rs/crates/provider/src/discovery.rs`
- `agent-studios-rs/crates/provider/tests/discovery_tests.rs`
- `agent-studios-rs/crates/provider/src/secret.rs`
- `agent-studios-rs/crates/provider/Cargo.toml`
- `agent-studios-rs/Cargo.lock`

To transition the canonical root to the unified codebase without data loss:

### Step 1: Preserve Uncommitted Work on Backup Branch
```bash
cd D:/Projects/agent-studios
git checkout -b feat/provider-discovery-v1.2-wip
git add agent-studios-rs/crates/provider/ agent-studios-rs/Cargo.lock
git commit -m "wip(provider): preserve uncommitted dynamic model discovery implementation"
```

### Step 2: Checkout and Fast-Forward Unified Branch
```bash
cd D:/Projects/agent-studios
git fetch origin
git checkout integration/m15-unified-codebase
git pull origin integration/m15-unified-codebase
```

### Step 3: Run Baseline Quality Verification
```bash
cd D:/Projects/agent-studios/agent-studios-rs
cargo test --workspace
cd D:/Projects/agent-studios/apps/agent-studios-vscode
npm run test:unit
```

### Step 4: Decommission Temporary Staging Worktree
```bash
git worktree remove D:/Projects/agent-studios-unified-staging
git worktree prune
```

---

## 6. Post-Approval Disk Space Reclamation Plan

Once the operator signs off on the cleanup manifest (`final_cleanup_manifest.json`):

| Cleanup Phase | Targeted Directories | Action | Space Recovered |
|---|---|---|:---:|
| **Phase 1: Build Cache Cleanup** | `m13`, `m15`, `m15-02`, `m15-03` | `cargo clean` | **~21.27 GB** |
| **Phase 2: Subsumed Worktree Prune**| 7 `APPROVED_CANDIDATE` trees | `git worktree remove` | **~2.22 GB** |
| **Phase 3: Post-Merge Prune** | `m15-04`, `ide-ui` (post-merge) | `git worktree remove` | **~5.10 GB** |
| **Total Disk Space Reclaimed** | | | **~28.59 GB** |

*Result*: Projected free space on drive `D:\` will increase from **30.15 GB to ~58.74 GB** (+94.8% increase).
