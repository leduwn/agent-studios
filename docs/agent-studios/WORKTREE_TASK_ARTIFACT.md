# Agent Studios — Worktree Isolation, Task Artifacts & Reconciliation

> **Status**: Core Architecture Specification (Milestone M09)
> **Milestone Status**: **UNDER FINAL REVIEW** (Branch `feat/worktree-task-artifact-orchestration` pushed to `origin`, awaiting merge review)
> **Precedence**: Subservient to `MASTER_VISION.md` and `PRODUCT_PRINCIPLES.md`.

---

## 1. Overview & Architectural Motivation

In multi-agent software engineering, allowing parallel agents to execute shell commands and file mutations in the same physical directory causes file contention, clobbered edits, corrupted Git indexes, and race conditions.

Milestone M09 establishes complete physical isolation for mutating workers using **native Git worktrees**, combined with **ephemeral index change capture**, **content-addressed artifact storage**, and **deterministic patch reconciliation**.

```text
               Git Repository Root (`source_root`)
                         │
        ┌────────────────┼────────────────┐
        │                │                │
        ▼                ▼                ▼
Integration WT     Worker WT 1       Worker WT 2
 (Integration)    (Feature Auth)    (Feature UI)
        │                │                │
        │         Ephemeral Index  Ephemeral Index
        │          Change Capture   Change Capture
        │                │                │
        │                ▼                ▼
        │         Patch Artifact   Patch Artifact
        │           (SHA-256)        (SHA-256)
        │                │                │
        └────────────────┼────────────────┘
                         │
                         ▼
             Patch Reconciliation Engine
              (git apply --check / 3way)
                         │
             ┌───────────┴───────────┐
             ▼                       ▼
      Reconciliation           Reconciliation
         Success                  Conflict
             │                       │
             ▼                       ▼
      Clean Integration      TaskState::Blocked
      Artifact Promoted      (Conflict Retained)
```

---

## 2. Worktree Isolation & Physical Containment

1. **Dedicated Worktree Allocation**: Mutating workers are provisioned dedicated Git worktrees under `.git/agent-studios/worktrees/<session>/<task_id>/` via `codex-worktree::WorktreeManager`.
2. **Branch Isolation**: Each worktree operates on a dedicated temporary branch (`agent-studios/<task_id>`).
3. **True Native Tools**: Because agents reside in real Git worktrees, native compilers, test runners, linters, and Git commands execute without mocking or virtual filesystem emulation.

---

## 3. The Thread / CWD / Worktree Invariant

A fundamental architectural rule of Agent Studios:

$$\textbf{Codex Thread Execution Identity} \equiv (\textbf{ThreadId}, \textbf{CWD}, \textbf{Allocated Worktree})$$

- A running Codex thread's working directory (`cwd`) is **inextricably bound** to its allocated worktree path.
- **Threads do not teleport between worktrees**: Dynamically mutating a thread's `cwd` across worktree boundaries during an active turn is strictly forbidden.
- When an agent completes a task or transitions to a different workspace, the old child thread is shut down, the worktree is safely retained or released, and a new child thread is initialized with the target workspace path.

---

## 4. `ExecutionWorkspace`: `source_root` vs. `source_cwd`

To handle sub-projects, monorepos, and nested package directories correctly, the `ExecutionWorkspace` decouples repository root from execution directory:

```rust
pub struct ExecutionWorkspace {
    /// The physical root of the Git repository (.git container)
    pub source_root: PathBuf,
    /// The specific sub-directory where agent commands and tools run
    pub source_cwd: PathBuf,
    /// The allocated isolated worktree (if mutating)
    pub worktree: Option<WorktreeHandle>,
}
```

- **`source_root`**: Used for global Git commands, worktree additions, branch management, and patch reconciliation.
- **`source_cwd`**: Used as the starting directory for tool calls, compiler commands, and language server initialization.
- **Relative Offset Invariant**: When an isolated worktree is created, `source_cwd` is mapped to the identical relative sub-path within the new worktree root.

---

## 5. Safe Worktree Retention (Zero Destructive Cleanup)

Agent Studios enforces strict safety guarantees regarding worker workspaces:

### The Safe Retention Rule
- Managed worktrees containing uncommitted modifications, failed agent attempts, or unmerged artifacts **must never be automatically destroyed**.
- Destructive Git operations are **strictly forbidden** in automated lifecycle cleanup:
  - `git reset --hard` (FORBIDDEN)
  - `git clean -fdx` (FORBIDDEN)
  - `git worktree remove --force` (FORBIDDEN)
- **Retention on Failure**: If a task fails or encounters a reconciliation conflict, the worktree is marked as `Retained`. The developer can open the worktree directory directly to inspect, debug, and manually salvage the code.

---

## 6. Ephemeral Index Change Capture

Capturing changes from a worker worktree must never dirty or disrupt the user's active Git index.

Agent Studios utilizes an **ephemeral Git index**:
1. Sets `GIT_INDEX_FILE` to a temporary, private index file path.
2. Runs `git read-tree HEAD` to seed the ephemeral index from the baseline commit.
3. Runs `git add -A` to stage all modifications, additions, and deletions in the worker worktree into the ephemeral index.
4. Generates a binary-safe patch using `git diff --cached --binary`.
5. Removes the temporary `GIT_INDEX_FILE` on completion.

This captures exact filesystem deltas—including binary files, permission changes, and untracked new files—without touching the working tree's primary `.git/index`.

---

## 7. Content-Addressed Artifact Store & Version Allocation

1. **SHA-256 Content Addressing**: Generated patch diffs and build deliverables are stored by their cryptographic SHA-256 hash in `.git/agent-studios/artifacts/blobs/<sha256>`.
2. **Control Plane Version Allocation**:
   - The Control Plane authoritatively allocates artifact version numbers ($v1, v2, v3\dots$).
   - Individual workers, executors, or external scripts cannot self-assign version numbers.
   - Version metadata records creator agent ID, source task ID, parent artifact SHA, and creation timestamp.

---

## 8. Deterministic Patch Reconciliation

Once a worker produces a patch artifact, the reconciliation engine merges the delta into the integration workspace:

```text
Worker Patch Artifact
        │
        ▼
Validation Pass: `git apply --check --binary`
        │
   ┌────┴────────────────────────┐
   ▼                             ▼
Applies Cleanly              Conflicted
   │                             │
   ▼                             ▼
`git apply --binary`      3-Way Merge Fallback:
   │                      `git apply --3way`
   │                             │
   │                      ┌──────┴──────┐
   │                      ▼             ▼
   │                   Resolved     Unresolved
   │                      │             │
   └──────────┬───────────┘             │
              ▼                         ▼
      ReconciledOutput         TaskState::Blocked
      (Artifact Merged)       (Conflict Recorded)
```

---

## 9. `ReconciledOutput` Contract

When reconciliation succeeds, the engine returns a typed `ReconciledOutput`:

```rust
pub struct ReconciledOutput {
    pub artifact_id: ArtifactId,
    pub content_sha256: Sha256Hash,
    pub target_branch: String,
    pub applied_files: Vec<PathBuf>,
    pub stats: PatchStats,
}
```

This output is staged for final integration and committed to the Control Plane event log via `ControlPlaneEvent::PatchReconciled`.

---

## 10. Invariant: `Blocked != Cancelled` in Reconciliation

If a patch cannot be reconciled due to conflicting changes applied by a preceding task:
1. The task transitions to **`TaskState::Blocked(BlockedReason::ReconciliationConflict)`**.
2. The task is **NEVER** marked as `Cancelled` or `Failed`.
3. The worker worktree is safely retained.
4. The Control Plane queues a reconciliation resolution task for human review or Coordinator re-basing.
