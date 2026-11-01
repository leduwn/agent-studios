# M15-03 IPC Contract Specification: Artifact & Diff Review

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** IPC Protocol Architect & Frontend Integration Lead  

---

## 1. Overview

This document specifies the exact JSON wire format, framing constraints, pagination parameters, error codes, and TypeScript interfaces for the three read-only IPC commands introduced in Milestone M15-03:
1. `ListTaskArtifacts`: Enumerate all artifacts generated during a task run.
2. `GetArtifact`: Retrieve artifact metadata and raw content blob.
3. `GetTaskDiff`: Retrieve structured diff summary and paginated unified patch.

---

## 2. Framing & Transport Envelopes

All commands and responses are transmitted across the authenticated duplex IPC stream using `DEFAULT_MAX_FRAME_SIZE = 4,194,304` bytes (4 MiB) with 4-byte big-endian length prefix framing.

### 2.1 Request Envelope (`IpcCommand`)
```json
{
  "type": "<command_name>",
  "payload": { ... }
}
```

### 2.2 Response Envelope (`IpcResponse`)
```json
{
  "status": "success",
  "result": { ... }
}
```
Or on error:
```json
{
  "status": "error",
  "code": "not_found",
  "message": "Task task_01J... not found in studio studio_01J...",
  "retryable": false
}
```

---

## 3. Command Specifications

### 3.1 `ListTaskArtifacts`
Enumerates all artifacts associated with an authorized task execution.

#### Request
```json
{
  "type": "list_task_artifacts",
  "payload": {
    "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
    "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H"
  }
}
```

#### Response (`IpcResponse::TaskArtifacts`)
```json
{
  "status": "success",
  "result": {
    "artifacts": [
      {
        "id": "art_01JBMX9Q3Y4E1A2B3C4D5E6F7J",
        "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
        "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H",
        "agent_id": "agent_01JBMX9Q3Y4E1A2B3C4D5E6F7K",
        "run_id": "run_01JBMX9Q3Y4E1A2B3C4D5E6F7L",
        "worktree_id": "wt_01JBMX9Q3Y4E1A2B3C4D5E6F7M",
        "kind": "patch",
        "filename": "task-task_01J...-diff.patch",
        "content_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "size_bytes": 1024,
        "created_at": "2026-11-01T12:00:00Z"
      },
      {
        "id": "art_01JBMX9Q3Y4E1A2B3C4D5E6F7N",
        "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
        "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H",
        "agent_id": "agent_01JBMX9Q3Y4E1A2B3C4D5E6F7K",
        "run_id": "run_01JBMX9Q3Y4E1A2B3C4D5E6F7L",
        "worktree_id": "wt_01JBMX9Q3Y4E1A2B3C4D5E6F7M",
        "kind": "report",
        "filename": "task-task_01J...-stats.txt",
        "content_hash": "f2ca1bb6c7e907d06dafe4687e579fce76b37e4e93b7605022da52e6ccc26fd2",
        "size_bytes": 142,
        "created_at": "2026-11-01T12:00:00Z"
      }
    ]
  }
}
```

---

### 3.2 `GetArtifact`
Retrieves artifact metadata and the raw string/blob content.

#### Request
```json
{
  "type": "get_artifact",
  "payload": {
    "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
    "artifact_id": "art_01JBMX9Q3Y4E1A2B3C4D5E6F7J"
  }
}
```

#### Response (`IpcResponse::Artifact`)
```json
{
  "status": "success",
  "result": {
    "artifact": {
      "id": "art_01JBMX9Q3Y4E1A2B3C4D5E6F7J",
      "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
      "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H",
      "agent_id": "agent_01JBMX9Q3Y4E1A2B3C4D5E6F7K",
      "run_id": "run_01JBMX9Q3Y4E1A2B3C4D5E6F7L",
      "worktree_id": "wt_01JBMX9Q3Y4E1A2B3C4D5E6F7M",
      "kind": "patch",
      "filename": "task-task_01J...-diff.patch",
      "content_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "size_bytes": 1024,
      "created_at": "2026-11-01T12:00:00Z"
    },
    "content": "diff --git a/src/lib.rs b/src/lib.rs\nindex ...\n"
  }
}
```

---

### 3.3 `GetTaskDiff`
Retrieves a paginated patch chunk and the truthful structured diff summary.

#### Request
```json
{
  "type": "get_task_diff",
  "payload": {
    "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
    "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H",
    "offset": 0,
    "limit": 1048576
  }
}
```
*Note: If `limit` is omitted, defaults to 1 MiB (1,048,576 bytes). If `limit` exceeds 2 MiB (2,097,152 bytes), it is clamped to 2 MiB.*

#### Response (`IpcResponse::TaskDiff`)
```json
{
  "status": "success",
  "result": {
    "diff": {
      "studio_id": "studio_01JBMX9Q3Y4E1A2B3C4D5E6F7G",
      "task_id": "task_01JBMX9Q3Y4E1A2B3C4D5E6F7H",
      "artifact_id": "art_01JBMX9Q3Y4E1A2B3C4D5E6F7J",
      "base_commit": "61cb7e88e4ae0e3a0432134b77eee0c30f56c302",
      "head_commit": "3207b7462fa90b9f5e1f76d9bc5612f0e43d9b89",
      "summary": {
        "files_changed": 3,
        "added_files": ["src/new_module.rs"],
        "modified_files": ["src/main.rs"],
        "deleted_files": ["src/deprecated.rs"],
        "renamed_files": ["src/old_name.rs -> src/new_name.rs"],
        "binary_files": [],
        "patch_size_bytes": 45120,
        "base_commit": "61cb7e88e4ae0e3a0432134b77eee0c30f56c302"
      },
      "patch": "diff --git a/src/new_module.rs b/src/new_module.rs\n...",
      "offset": 0,
      "limit": 1048576,
      "total_bytes": 45120,
      "has_more": false
    }
  }
}
```

---

## 4. TypeScript Interface Definitions

```typescript
export type ArtifactKind = "patch" | "report" | "log" | "binary" | "custom";

export interface ArtifactRecord {
  id: string;
  studio_id: string;
  task_id: string;
  agent_id: string;
  run_id?: string;
  worktree_id?: string;
  kind: ArtifactKind;
  filename: string;
  content_hash?: string;
  size_bytes: number;
  created_at: string;
}

export interface DiffSummary {
  files_changed: number;
  added_files: string[];
  modified_files: string[];
  deleted_files: string[];
  renamed_files: string[];
  binary_files: string[];
  patch_size_bytes: number;
  base_commit: string;
}

export interface TaskDiffResponse {
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
