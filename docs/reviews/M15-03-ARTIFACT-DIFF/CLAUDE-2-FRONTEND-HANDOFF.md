# M15-03 Claude 2 Frontend Handoff: VS Code Artifact & Diff Integration Guide

**Milestone:** M15-03  
**Audience:** VS Code Extension Engineers & UI Architects  
**Author:** Backend Systems Architect & IPC Protocol Architect  
**Date:** 2026-11-01  

---

## 1. Architectural Overview for VS Code

Milestone M15-03 provides the backend capability to capture, persist, and retrieve the exact code modifications produced by an AI Agent running inside an isolated Git worktree.

### The Developer Experience
1. Developer submits a task from VS Code.
2. The agent executes in an isolated managed worktree (never modifying the user's active editor workspace).
3. The backend captures all changes (added files, modified files, deleted files, renames, binary files) as an immutable patch in `ArtifactStore`.
4. The VS Code extension receives a `task_state_changed` event and queries `GetTaskDiff`.
5. VS Code renders:
   - A diff summary badge (e.g. `+1 ~2 -0`).
   - A tree view of modified files.
   - Rich side-by-side or unified diffs for review before applying/merging.

---

## 2. IPC Wire Protocol Guide

### 2.1 Subscribing to Completion Events
Listen for `task_state_changed` to automatically trigger diff fetching:

```typescript
client.onEvent((event: IpcEvent) => {
  if (event.type === "control_plane_event") {
    const cpEvent = event.payload;
    if (cpEvent.type === "task_state_changed") {
      const { task_id, state } = cpEvent;
      if (state === "succeeded" || state === "failed" || state === "cancelled") {
        // Trigger diff review fetch
        loadTaskDiff(studioId, task_id);
      }
    }
  }
});
```

*Guarantee: Changes are captured and registered in Control Plane BEFORE `task_state_changed` is emitted. There is zero race condition—`GetTaskDiff` will return the diff immediately.*

---

### 2.2 Querying `GetTaskDiff`

```typescript
interface GetTaskDiffRequest {
  type: "get_task_diff";
  payload: {
    studio_id: string;
    task_id: string;
    offset?: number;  // Default: 0
    limit?: number;   // Default: 1048576 (1 MiB), Max: 2097152 (2 MiB)
  };
}

interface DiffSummary {
  files_changed: number;
  added_files: string[];
  modified_files: string[];
  deleted_files: string[];
  renamed_files: string[];
  binary_files: string[];
  patch_size_bytes: number;
  base_commit: string;
}

interface TaskDiffResponse {
  studio_id: string;
  task_id: string;
  artifact_id?: string;
  base_commit?: string;
  head_commit?: string;
  summary: DiffSummary;
  patch: string;
  offset: number;
  limit: number;
  total_bytes: number;
  has_more: boolean;
}
```

#### Large Patch Streaming Flow
If `has_more === true`:
```typescript
async function fetchCompletePatch(studioId: string, taskId: string): Promise<string> {
  let patch = "";
  let offset = 0;
  const chunkSize = 1024 * 1024; // 1 MiB

  while (true) {
    const response = await ipcClient.send<TaskDiffResponse>({
      type: "get_task_diff",
      payload: { studio_id: studioId, task_id: taskId, offset, limit: chunkSize }
    });
    patch += response.patch;
    if (!response.has_more) {
      break;
    }
    offset += response.patch.length;
  }
  return patch;
}
```

---

### 2.3 Querying `ListTaskArtifacts` and `GetArtifact`

To list all artifacts (patch, stats reports, etc.):
```typescript
const { artifacts } = await ipcClient.send({
  type: "list_task_artifacts",
  payload: { studio_id: studioId, task_id: taskId }
});
```

To fetch a specific artifact blob:
```typescript
const { artifact, content } = await ipcClient.send({
  type: "get_artifact",
  payload: { studio_id: studioId, artifact_id: patchArtifactId }
});
```

---

## 3. UI/UX Recommendations

### 3.1 Review Sidebar Tree View
Use `response.summary` directly to populate the tree view without parsing the unified patch:
- Added files (`added_files`): Green `[A]` icon.
- Modified files (`modified_files`): Yellow `[M]` icon.
- Deleted files (`deleted_files`): Red `[D]` icon.
- Renamed files (`renamed_files`): Blue `[R]` icon showing `old -> new`.
- Binary files (`binary_files`): Gray `[B]` icon with notice: *"Binary file changed"*.

### 3.2 In-Memory Diff Document Provider
To render side-by-side diffs in VS Code:
1. Register a virtual document scheme: `agent-studios-diff://`.
2. When the user clicks a file in the tree, open:
   `vscode.diff(Uri.parse('agent-studios-base://...'), Uri.parse('agent-studios-patch://...'), 'Review Changes')`
3. Zero host files are modified during review.

---

## 4. Error Handling & Edge Cases

| Error Code | Meaning | Recommended UI Action |
|---|---|---|
| `not_found` | Task has no captured diff (e.g. task failed before starting, or task made zero filesystem changes). | Show message: *"No changes recorded for this task."* |
| `unauthorized` / `forbidden` | Mismatched studio ID or revoked session token. | Prompt operator to reconnect or select matching workspace. |
| `FrameTooLarge` | Chunk size > 2 MiB requested. | Use default chunk size (1 MiB). |
