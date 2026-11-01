# M15-03.1 Golden Wire Fixtures & Protocol Schema Documentation

**Milestone:** M15-03.1  
**Status:** Canonical Reference  
**Date:** 2026-11-01  
**Author:** Principal Rust IPC & Frontend Protocol Architect  
**Target Repository:** `agent-studios-rs`  
**Test Harness:** [golden_fixture_tests.rs](agent-studios-rs/crates/ipc-transport/tests/golden_fixture_tests.rs)  
**Disk Fixtures Directory:** `crates/ipc-transport/tests/fixtures/golden_wire/`

---

## 1. Overview & Architectural Purpose

Milestone M15-03.1 establishes canonical golden wire JSON fixtures for all active daemon IPC endpoints governing task artifacts and git diffs.

Historically, protocol evolution suffered from discrepancies between Rust's flat internally-tagged Serde serialization (`{"type": "...", ...}`) and nested frontend envelopes (`{"type": "...", "payload": { ... }}` or `{"status": "ok", "result": { ... }}`).

These fixtures serve as the binding contract:
- Guaranteed flat root structure: `"type"` discriminator lives alongside data fields.
- Guaranteed lossless Base64 representation for binary diffs and artifacts.
- Exact byte offset pagination metadata (`offset`, `limit`, `total_bytes`, `has_more`, `sha256`).

---

## 2. Canonical Golden Fixtures

### 2.1 `list_task_artifacts` Request & Response

#### Request: `list_task_artifacts_request.json`
```json
{
  "type": "list_task_artifacts",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "c0a80101-0000-0000-0000-000000000003"
}
```

#### Response: `task_artifacts_response.json`
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

### 2.2 `get_task_diff` Request & Response

#### Request: `get_task_diff_request.json`
```json
{
  "type": "get_task_diff",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "task_id": "c0a80101-0000-0000-0000-000000000003",
  "offset": 0,
  "limit": 1048576
}
```

#### Response: `task_diff_response.json`
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

### 2.3 `get_artifact` Request & Response

#### Request: `get_artifact_request.json`
```json
{
  "type": "get_artifact",
  "studio_id": "c0a80101-0000-0000-0000-000000000001",
  "artifact_id": "c0a80101-0000-0000-0000-000000000002",
  "offset": 0,
  "limit": 65536
}
```

#### Response: `artifact_response.json`
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

## 3. Frontend Decoder Guidelines (Claude 2 Guidance)

Frontend engineers and TypeScript/Tauri developers should configure their deserialization logic according to these specifications:

1. **Root Discriminated Union:**
   ```typescript
   export type DaemonMessage =
     | { type: 'artifact'; artifact?: ArtifactRecord; content_base64?: string; ... }
     | { type: 'task_diff'; diff: TaskDiffResponse }
     | { type: 'task_artifacts'; artifacts: ArtifactRecord[] }
     | { type: 'error'; message: string };
   ```
2. **Lossless Base64 Decoding:**
   - Always prefer `patch_base64` or `content_base64` over legacy `patch` / `content` strings.
   - For UTF-8 text, concatenate raw binary `Uint8Array` chunks before decoding using `TextDecoder('utf-8')`. This prevents multi-byte split replacement errors.
3. **Integrity Verification:**
   - When all chunks have been received (`has_more === false`), compute SHA-256 over concatenated bytes and assert equality with `patch_sha256` / `sha256`.
