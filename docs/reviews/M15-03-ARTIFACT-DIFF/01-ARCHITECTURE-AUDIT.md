# M15-03 Architecture Audit: Artifact Capture, Diff Review & Execution Evidence

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Principal Rust Systems Architect, Agent Runtime Engineer, Git Worktree Engineer, IPC Protocol Architect  

---

## 1. Executive Overview

Milestone M15-03 closes the verification loop between autonomous agent execution in managed Git worktrees and human-in-the-loop developer inspection in the Agent Studios VS Code extension. 

Prior to M15-03, tasks could be dispatched to managed Git worktrees across multiple agent runtimes (`ClaudeCodeRuntime`, `MultiRuntimeMock`), but filesystem mutations produced during task execution were not systematically captured, preserved as immutable evidence, or queryable over authenticated IPC channels.

M15-03 establishes:
1. **Zero-Pollution Ephemeral Index Capture**: Ephemeral `GIT_INDEX_FILE` capture mechanism (`capture_changes`) computing unified and binary diffs without mutating the worktree's real `.git/index`.
2. **Content-Addressed Artifact Storage**: SHA-256 keyed immutable blob storage (`ArtifactStore`) storing patches and execution statistics under `<root>/blobs/sha256/<hash>`.
3. **Control Plane Integration & State Transitions**: Automatic registration of patch and report artifacts, and transition of `WorktreeRecord` state to `WorktreeState::ChangeCaptured`.
4. **Authenticated Read-Only IPC Contract**: Frame-bounded IPC commands (`ListTaskArtifacts`, `GetArtifact`, `GetTaskDiff`) with strict tenant isolation, 2 MiB chunk ceilings, and truthful `DiffSummary` parsing.

---

## 2. End-to-End Execution Flow

```
[VS Code Client / Human Developer]
         │
         │ 1. SubmitTask (spec, repo_path, runtime)
         ▼
[agsd IPC Server & Daemon Engine]
         │
         │ 2. Allocate Managed Worktree & Run Record
         ▼
[agent-studios-workspace / Git Engine]
         │ (Isolated worktree created: .git/worktrees/wt-<id>)
         ▼
[AgentRuntime Session Execution]
         │ (Runtime mutates, adds, and removes files in worktree)
         │ Emits: SessionStarted -> ... -> SessionCompleted / Failed / Stopped
         ▼
[Pre-State-Transition Capture Barrier]
         │ 1. capture_changes(wt_root, base_sha) via ephemeral GIT_INDEX_FILE
         │ 2. store_patch_artifact(...) -> Content-addressed write & SHA-256 check
         │ 3. ControlPlane::register_artifact_record(patch_art)
         │ 4. ControlPlane::record_worktree_change_captured(wt_id, patch_art.id)
         │ 5. ControlPlane::transition_task_state(task_id, TerminalState)
         ▼
[Client Inspection via Authenticated IPC]
         │
         ├──> ListTaskArtifacts(studio_id, task_id) -> [PatchRecord, StatsRecord]
         ├──> GetArtifact(studio_id, patch_id) -> Raw patch blob
         └──> GetTaskDiff(studio_id, task_id, offset, limit) -> TaskDiffResponse (DiffSummary + Chunk)
```

---

## 3. Subsystem Breakdown

### 3.1 Ephemeral Index Capture (`agent-studios-workspace`)
- Uses `tempfile::tempdir()` to isolate `GIT_INDEX_FILE`.
- Invokes `git read-tree <base_sha>` to populate the ephemeral index with the base commit tree.
- Invokes `git add -A -- .` inside the worktree against the ephemeral index.
- Computes `git diff --cached --binary --full-index --no-ext-diff <base_sha> -- .`.
- Computes `git diff --cached --numstat <base_sha> -- .`.
- The real `.git/index` is untouched. Dropping `temp_dir` automatically deletes the index file from disk.

### 3.2 Content-Addressed Artifact Storage (`ArtifactStore`)
- Blobs are stored at `<root>/blobs/sha256/<hash>`.
- Storage is atomic: writes are performed to a temporary file in `<root>/tmp/` and atomically renamed to destination.
- Verification: Every read computes SHA-256 and rejects corrupt blobs.

### 3.3 Control Plane Registration (`agent-studios-control-plane`)
- `ArtifactRecord` is registered with metadata: `id`, `studio_id`, `task_id`, `agent_id`, `run_id`, `worktree_id`, `kind`, `filename`, `content_hash`, `size_bytes`.
- `record_worktree_change_captured` updates `WorktreeRecord`:
  - Transitions `state` from `InUse` to `ChangeCaptured`.
  - Records `patch_artifact_id`, `stats_artifact_id`, and `files_changed`.
- All operations record committed events for IPC event stream subscribers.

### 3.4 IPC Transport Handlers (`agent-studios-ipc-transport`)
- `IpcCommand::ListTaskArtifacts`: Queries Control Plane for artifacts matching `task_id`, verifying ownership by `studio_id`.
- `IpcCommand::GetArtifact`: Queries Control Plane for `artifact_id`, validates `studio_id`, and streams content from `ArtifactStore`.
- `IpcCommand::GetTaskDiff`: Locates task patch artifact, reads bytes from `ArtifactStore`, computes truthful `DiffSummary`, and paginates diff chunks using `offset` and `limit`.

---

## 4. Key Invariants Preserved
1. **Read-tree Isolation**: Working directory unstaged changes are not polluted.
2. **Pre-Transition Ordering**: Capture always finishes before terminal task state transition is published.
3. **No Unbounded Memory Allocation**: 2 MiB maximum chunk size within 4 MiB frame limit.
4. **Tenant Isolation**: Tasks and artifacts belonging to studio $A$ cannot be read by studio $B$.
