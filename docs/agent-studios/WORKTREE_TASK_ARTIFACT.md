# Agent Studios: Worktree, Task & Artifact Orchestration

This document details the architecture, lifecycle state machines, and operational invariants for
isolated workspace execution, ephemeral change-set capture, content-addressed artifact persistence,
and safe patch reconciliation (Milestone M09).

---

## 1. Architecture & Design Principles

Milestone M09 extends Agent Studios multi-agent orchestration with isolated Git worktree execution,
thread affinity, ephemeral change capture, and atomic artifact storage without duplicating upstream
Git worktree infrastructure or circumventing the Control Plane.

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        AgentStudiosSupervisor                          │
│                                                                        │
│   ┌──────────────────────┐              ┌──────────────────────────┐   │
│   │ Coordinator Planning │              │   Workspace Arbitrator   │   │
│   │  (ReadOnly / Mutate) │              │  (worktree-{id} Leases)  │   │
│   └──────────┬───────────┘              └────────────┬─────────────┘   │
│              │                                       │                 │
│              ▼                                       ▼                 │
│   ┌──────────────────────┐              ┌──────────────────────────┐   │
│   │  ControlPlaneActor   │              │   WorkspaceOrchestrator  │   │
│   │ (Worktrees/Artifacts)│              │  (codex_worktree::Manager│   │
│   └──────────┬───────────┘              └────────────┬─────────────┘   │
└──────────────┼───────────────────────────────────────┼─────────────────┘
               │                                       │
               ▼                                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│                       CodexAgentExecutor Backend                       │
│                                                                        │
│   - Thread-Worktree 1:1 Affinity (codex-thread.json via bind_thread)   │
│   - Non-Teleporting Worker Invariant (stable ThreadId + Config.cwd)    │
│   - Parallel Mutating Workers on Dedicated Isolated Worktrees          │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│               Lifecycle Completion & Post-Task Integration             │
│                                                                        │
│   1. Ephemeral Index Change Capture (GIT_INDEX_FILE, git read-tree)    │
│   2. Content-Addressed ArtifactStore (SHA-256 blobs, atomic rename)    │
│   3. Safe Patch Reconciliation (git apply --check --binary dry-run)   │
│   4. Integration Worktree Merge or Conflict (Conflict != Failure)      │
│   5. Worktree Retention Policy (Preserve dirty worktree on crash)      │
└────────────────────────────────────────────────────────────────────────┘
```

### Core Invariants

1. **Upstream Reuse**: Wraps `codex_worktree::WorktreeManager`, `codex_worktree::ManagedWorktree`,
   and `codex_worktree::bind_thread`. Does NOT shell out directly to `git worktree add` or maintain
   a bespoke worktree implementation.
2. **Worktree-Thread 1:1 Affinity**: A worktree is owned by at most one Codex thread, verified via
   `codex-thread.json` metadata. Re-binding the same thread is idempotent; binding a different thread
   fails with conflict.
3. **Non-Teleporting Worker Invariant**: A Codex thread's working directory (`Config.cwd`) is immutable
   once spawned. A worker cannot be teleported between worktrees. Reusing worker threads is permitted
   strictly when `existing.bound_workspace == requested.workspace_path`. If workspace differs, old
   worker is retired and a new thread is spawned with fresh `ThreadId` and new worktree.
4. **Per-Workspace Concurrency Arbitration**: Replaces M08 global single-writer lock with per-worktree
   leases (`format!("worktree-{}", wt_id)`). Multiple mutating tasks execute concurrently in parallel
   when allocated distinct dedicated worktrees.
5. **Ephemeral Change Capture**: Staged changes are computed using isolated temporary Git index files
   (`GIT_INDEX_FILE`), leaving the active worktree `.git/index` pristine.
6. **Conflict Isolation**: Reconciliation applies exclusively to a dedicated integration worktree.
   Reconciliation conflict does NOT fail the task: `TaskState = Succeeded`, `ReconciliationState = Conflicted`.
7. **Dirty Worktree Retention**: Worktrees with uncommitted changes or task failures are retained on
   disk with `retained = true` for developer inspection and never deleted automatically.

---

## 2. Worktree & Reconciliation State Machines

### Worktree State Machine (`WorktreeState`)

```text
               ┌──────────┐
               │ Creating │
               └────┬─────┘
                    │
                    ▼
               ┌──────────┐ ◄────────────────┐
               │  Ready   │                  │
               └────┬─────┘                  │
                    │                        │
                    ▼                        │
               ┌──────────┐                  │
               │  InUse   │                  │
               └────┬─────┘                  │
                    │                        │
                    ▼                        │
          ┌───────────────────┐              │
          │  ChangeCaptured   │              │
          └─────────┬─────────┘              │
                    │                        │
                    ▼                        │
          ┌───────────────────┐              │
          │ ReconcilePending  │              │
          └─────────┬─────────┘              │
                    │                        │
         ┌──────────┴──────────┐             │
         ▼                     ▼             │
   ┌───────────┐         ┌────────────┐      │
   │Reconciled │         │ Conflicted │      │
   └─────┬─────┘         └─────┬──────┘      │
         │                     │             │
         └──────────┬──────────┴─────────────┘
                    │
                    ▼
               ┌──────────┐ (Failure / Cancel)
               │ Retained │
               └────┬─────┘
                    │
                    ▼
               ┌──────────┐
               │ Removing │
               └────┬─────┘
                    │
                    ▼
               ┌──────────┐
               │ Removed  │ (Terminal)
               └──────────┘
```

- **Active States**: `Ready`, `InUse`, `ChangeCaptured`, `ReconcilePending`, `Reconciled`, `Conflicted`, `Retained`.
- **Terminal States**: `Removed`, `Failed`.

### Reconciliation State Machine (`ReconciliationState`)

```text
               ┌──────────┐
               │ Pending  │
               └────┬─────┘
                    │
                    ▼
               ┌──────────┐
               │ Checking │ (git apply --check --binary)
               └────┬─────┘
                    │
          ┌─────────┴─────────┐
          │ (Pass)            │ (Fail)
          ▼                   ▼
    ┌──────────┐        ┌────────────┐
    │ Applying │        │ Conflicted │ (Terminal)
    └─────┬────┘        └────────────┘
          │
          ▼
    ┌──────────┐
    │ Applied  │ (Terminal, Merge Commit Recorded)
    └──────────┘
```

- **Terminal States**: `Applied`, `Conflicted`, `Failed`, `Cancelled`.

---

## 3. Ephemeral Git Index Change-Set Capture

To guarantee that computing patches never mutates the worktree's live `.git/index` or races with
active agent tools:

1. Creates an isolated temporary directory containing a transient `temp_git_index` file.
2. Sets `GIT_INDEX_FILE` environment variable pointing to the temporary index file.
3. Initializes the index from base commit tree: `git read-tree <base_sha>`.
4. Stages all current untracked and modified files: `git add -A -- .`.
5. Computes a binary-safe patch:
   ```bash
   git diff --cached --binary --full-index --no-ext-diff <base_sha> -- .
   ```
6. Discards temporary index directory.

---

## 4. Content-Addressed Artifact Store

The `ArtifactStore` provides content-addressed, deduplicated, and crash-safe storage:

- **Directory Layout**: `<artifact-root>/blobs/sha256/<hash>`.
- **Atomic Writes**: Blobs are written to a temporary sibling file in `<artifact-root>/tmp/`,
  flushed to disk, and atomically renamed to final destination.
- **Deduplication**: If the destination hash already exists, writing is skipped and the existing
  blob is referenced.
- **Artifact Records**: Rich metadata tracks `studio_id`, `task_id`, `producer_agent_id`,
  `kind` (`File`, `Patch`, `Log`, `Report`, `Plan`, `Other`), `version`, `worktree_id`, `run_id`,
  and `size_bytes`.

---

## 5. Safe Patch Reconciliation

Reconciliation applies captured patches to a dedicated integration worktree:

1. **Two-Phase Application**:
   - Phase 1 (Dry-Run): `git apply --check --binary <patch>` checks compatibility without writing.
   - Phase 2 (Apply): If check succeeds, `git apply --binary <patch>` modifies files, followed by
     `git add -A` and `git commit` to seal merge commit.
2. **Conflict Detection**:
   - If `git apply --check` fails, stderr is parsed to extract conflicted files.
   - Reconciliation state is updated to `Conflicted`.
   - Workspace files remain completely untouched.
   - Task execution remains `Succeeded` (conflict is an integration state, not worker failure).

---

## 6. Verification & Test Evidence

| Test Suite | Location | Verification Highlights |
| :--- | :--- | :--- |
| `workspace_tests` | `crates/workspace/tests/` | Path validation escaping managed root, atomic artifact write and deduplication, end-to-end worktree lifecycle with capture, thread binding, clean reconciliation, and conflicting reconciliation. |
| `worktree_orchestration_e2e_tests` | `crates/internal-agent/tests/` | Parallel mutating workers with peak concurrency 2 executing simultaneously on distinct worktrees, dirty worktree retention on crash, and conflict does not fail task invariant. |
| `read_model_tests` | `crates/orchestration/tests/` | Projections for `ArtifactIndex`, `WorktreeSnapshot`, `TaskTimelineProjection`, and enriched `TaskGraphSnapshot`. |
