# M15-03.1 IPC Contract Specification: Artifact & Lossless Diff Review

**Milestone:** M15-03.1  
**Status:** Canonical Spec (Updated for Flat Wire Protocol & Lossless Base64 Slicing)  
**Date:** 2026-11-01  
**Author:** Principal IPC Protocol Architect & Frontend Integration Lead  

---

## 1. Overview

This document specifies the exact flat JSON wire format, framing constraints, pagination parameters, error representations, and TypeScript interfaces for the three read-only IPC commands in Milestone M15-03.1:
1. `list_task_artifacts`: Enumerate all artifacts generated during a task run.
2. `get_artifact`: Retrieve paginated binary/text artifact chunk and metadata.
3. `get_task_diff`: Retrieve structured diff summary and paginated lossless Base64 unified patch.

---

## 2. Framing & Transport Envelopes

All commands and responses are transmitted across the authenticated duplex IPC stream using `DEFAULT_MAX_FRAME_SIZE = 4,194,304` bytes (4 MiB) with 4-byte big-endian length prefix framing.

### 2.1 Request Protocol (`IpcCommand`)
Rust Serde serializes `IpcCommand` as a **flat, internally tagged enum** with `tag = "type"` and `rename_all = "snake_case"`. There are **no nested `payload` envelopes**. All parameters exist directly at the JSON root:

```json
{
  "type": "<command_name>",
  "studio_id": "...",
  ...
}
```

### 2.2 Response Protocol (`IpcResponse`)
Rust Serde serializes `IpcResponse` as a **flat, internally tagged enum** with `tag = "type"` and `rename_all = "snake_case"`. There are **no nested `status` or `result` envelopes**:

```json
{
  "type": "<response_type>",
  ...
}
```

Or on error:
```json
{
  "type": "error",
  "message": "Task 01J... not found in studio 01J..."
}
```

---

## 3. Command Specifications

### 3.1 `list_task_artifacts`
Enumerates all artifacts associated with an authorized task execution.

#### Request
```json
{
  "type": "list_task_artifacts",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "c0a80101-0000-0000-0000-000000000003"
}
```

#### Response (`IpcResponse::TaskArtifacts`)
```json
{
  "type": "task_artifacts",
  "artifacts": [
    {
      "id": "c0a80101-0000-0000-0000-000000000002",
      "studio_id": "c0a80101-0000-0000-0000-000000000001",
      "task_id": "c0a80101-0000-0000-0000-000000000003",
      "producer_agent_id": "c0a80101-0000-0000-0000-000000000004",
      "kind": "patch",
      "logical_name": "task-patch.diff",
      "content_hash": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "location": "blobs/sha256/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "created_at": "2026-11-01T00:00:00Z",
      "run_id": "c0a80101-0000-0000-0000-000000000005",
      "worktree_id": "c0a80101-0000-0000-0000-000000000006",
      "version": 1,
      "supersedes": null,
      "relative_path": null,
      "size_bytes": 1024
    }
  ]
}
```

---

### 3.2 `get_artifact`
Retrieves artifact metadata and paginated binary/text chunk.

#### Request
```json
{
  "type": "get_artifact",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "artifact_id": "c0a80101-0000-0000-0000-000000000002",
  "offset": 0,
  "limit": 65536
}
```
*Note: `limit == 0` is rejected with an error to prevent infinite pagination loops. Default chunk size is 1 MiB (1,048,576 bytes); max chunk size is 2 MiB (2,097,152 bytes).*

#### Response (`IpcResponse::Artifact`)
```json
{
  "type": "artifact",
  "artifact": {
    "id": "c0a80101-0000-0000-0000-000000000002",
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "task_id": "c0a80101-0000-0000-0000-000000000003",
    "producer_agent_id": "c0a80101-0000-0000-0000-000000000004",
    "kind": "patch",
    "logical_name": "task-patch.diff",
    "content_hash": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "location": "blobs/sha256/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "created_at": "2026-11-01T00:00:00Z",
    "run_id": "c0a80101-0000-0000-0000-000000000005",
    "worktree_id": "c0a80101-0000-0000-0000-000000000006",
    "version": 1,
    "supersedes": null,
    "relative_path": null,
    "size_bytes": 1024
  },
  "content": null,
  "content_base64": "ZGlmZiAtLWdpdCBhL2ZpbGUudHh0IGIvZmlsZS50eHQK",
  "encoding": "base64",
  "offset": 0,
  "limit": 65536,
  "total_bytes": 33,
  "has_more": false,
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

---

### 3.3 `get_task_diff`
Retrieves a paginated patch chunk and the truthful structured diff summary.

#### Request
```json
{
  "type": "get_task_diff",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "c0a80101-0000-0000-0000-000000000003",
  "offset": 0,
  "limit": 1048576
}
```
*Note: `limit == 0` is rejected with an error. Max chunk ceiling is 2 MiB. When raw chunk size exceeds 1.5 MiB, legacy UTF-8 `patch` is suppressed to `""` to prevent exceeding 4 MiB frame limits.*

#### Response (`IpcResponse::TaskDiff`)
```json
{
  "type": "task_diff",
  "diff": {
    "studio_id": "c0a80101-0000-0000-0000-000000000001",
    "task_id": "c0a80101-0000-0000-0000-000000000003",
    "artifact_id": "c0a80101-0000-0000-0000-000000000002",
    "base_commit": "0123456789abcdef0123456789abcdef01234567",
    "head_commit": "fedcba9876543210fedcba9876543210fedcba98",
    "summary": {
      "files_changed": 1,
      "added_files": [
        "file.txt"
      ],
      "modified_files": [],
      "deleted_files": [],
      "renamed_files": [],
      "binary_files": [],
      "patch_size_bytes": 33,
      "base_commit": "0123456789abcdef0123456789abcdef01234567"
    },
    "patch": "diff --git a/file.txt b/file.txt\n",
    "offset": 0,
    "limit": 1048576,
    "total_bytes": 33,
    "has_more": false,
    "patch_base64": "ZGlmZiAtLWdpdCBhL2ZpbGUudHh0IGIvZmlsZS50eHQK",
    "encoding": "base64",
    "patch_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "chunk_bytes": 33
  }
}
```

---

## 4. TypeScript Interface Definitions

```typescript
export type ArtifactKind = "patch" | "report" | "log" | "binary" | "file";

export interface ArtifactRecord {
  id: string;
  studio_id: string;
  task_id: string;
  producer_agent_id: string;
  kind: ArtifactKind;
  logical_name: string;
  content_hash?: string;
  location: string;
  created_at: string;
  run_id?: string;
  worktree_id?: string;
  version: number;
  supersedes?: string | null;
  relative_path?: string | null;
  size_bytes: number;
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
  patch_base64?: string;
  encoding?: string;
  patch_sha256?: string;
  chunk_bytes?: number;
}

export type IpcCommand =
  | { type: "list_task_artifacts"; studio_id: string; task_id: string }
  | { type: "get_artifact"; studio_id: string; artifact_id: string; offset?: number; limit?: number }
  | { type: "get_task_diff"; studio_id: string; task_id: string; offset?: number; limit?: number };

export type IpcResponse =
  | { type: "task_artifacts"; artifacts: ArtifactRecord[] }
  | {
      type: "artifact";
      artifact?: ArtifactRecord;
      content?: string | null;
      content_base64?: string;
      encoding?: string;
      offset?: number;
      limit?: number;
      total_bytes?: number;
      has_more?: boolean;
      sha256?: string;
    }
  | { type: "task_diff"; diff: TaskDiffResponse }
  | { type: "error"; message: string };
```
