# Agent Studios — Worktree Isolation, Task Artifacts & Reconciliation

> **Status**: Core Architecture Specification (Milestone M09)
>
> **Milestone Status**: **UNDER FINAL REVIEW** (Branch `feat/worktree-task-artifact-orchestration` pushed to `origin`, awaiting merge review)
>
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

### The Full Destructive-Cleanup Prohibition
Automated lifecycle cleanup must **never** perform destructive Git operations equivalent to any of the following three:
1. `git reset --hard` (FORBIDDEN in automated lifecycle)
2. `git clean -fdx` (FORBIDDEN in automated lifecycle)
3. `git worktree remove --force` (FORBIDDEN in automated lifecycle)

Normal managed-worktree cleanup must **never destroy dirty contents**.

### Canonical Worktree Lifecycle Behavior
- **Dirty or Unsafe Worktree**:
  $$\text{dirty / conflicted / failed worktree} \longrightarrow \textbf{Retain Safely}$$
  If a task fails, times out, or encounters a reconciliation conflict, the worktree is marked as `Retained`. The developer can inspect, debug, and manually salvage code directly from the worktree folder.
- **Safe Explicit Cleanup**:
  $$\text{no active turn} \longrightarrow \text{retire owner thread} \longrightarrow \text{wait for termination} \longrightarrow \textbf{safe upstream WorktreeManager removal}$$
  Worktrees are removed only when cleanly committed or reconciled, with no active threads running.
- **Explicit User Intent for Discard**:
  Any future destructive discard feature requires explicit, confirmed user intent/approval and is **not** part of M09 normal automated cleanup.

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

### Canonical Architectural Requirements
1. **Content-Addressed SHA-256 Storage**:
   Artifacts (unified patch diffs, build outputs) are identified and stored by their cryptographic SHA-256 hash:
   $$\text{Concept: } \langle\text{artifact-root}\rangle\text{/blobs/sha256/}\langle\text{hash}\rangle$$
   - Deduplication: Identical patch contents share the same underlying storage blob.
   - Atomic Persistence: Blobs are written atomically (write to temp file, flush, rename).
   - Provenance & Lineage: Artifact metadata records source task ID, creator agent ID, parent artifact SHA, and creation timestamp.
2. **Current Implementation Path**:
   In the current M09 implementation, artifact blobs are stored under `.git/agent-studios/artifacts/blobs/<sha256>`. This path represents a current implementation detail, **not** a permanent architectural constraint that limits future storage layout evolution.
3. **Control Plane Authoritative Version Allocation**:
   - The Control Plane authoritatively allocates artifact version numbers ($v1, v2, v3\dots$).
   - Individual workers, executors, or external scripts cannot self-assign version numbers.

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
