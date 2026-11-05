# M15-04 Worktree & Artifact Reconciliation Design

## 1. Context & Motivation

During execution, Agent Studios allocates Git worktrees and content-addressed patch/diff artifacts. When `agsd` restarts, durable records stored in the SQLite database might diverge from the physical filesystem:
- A user or external script may have deleted a worktree directory while the daemon was down.
- A physical worktree may exist without a durable registration (foreign/unmanaged).
- Worktree thread ownership might be inconsistent with durable records.

---

## 2. Worktree Health Diagnostics & State Reconciliation

### 2.1 Missing Worktree Detection
On startup, `ControlPlane::reconcile_missing_worktrees` inspects all non-terminal worktree records:
- Checks if `worktree.root.exists()` on the local filesystem.
- If missing on disk:
  - If `state.can_transition_to(WorktreeState::Failed)`, transitions the record to `WorktreeState::Failed`.
  - Increments `worktrees_missing`.
- If present on disk:
  - Preserves the worktree in its durable state (`Ready`, `InUse`, `Retained`, etc.).
  - Increments `worktrees_reconciled`.

### 2.2 Non-Destructive Reconciliation Report
Using `WorkspaceOrchestrator::generate_recovery_report(source_cwd, durable_worktrees)`:
- Invokes `git worktree list --porcelain` to discover physical checkouts.
- Categorizes worktrees into:
  - `matched`: Durable records matching physical checkouts with matching thread ownership.
  - `missing`: Durable records with missing or deleted directories.
  - `foreign`: Physical checkouts not recognized by the durable store (foreign worktrees are NEVER deleted automatically).
  - `ownership_mismatches`: Worktrees bound to mismatched thread IDs.

---

## 3. Artifact Integrity & Content-Addressed Store

- Artifact files stored in the content-addressed store (`ArtifactStore`) are immutable and keyed by SHA-256 hash.
- Patch artifacts and summary JSON files remain accessible by `ArtifactId`.
- No artifact deletion occurs during restart, preserving forensic execution evidence.
