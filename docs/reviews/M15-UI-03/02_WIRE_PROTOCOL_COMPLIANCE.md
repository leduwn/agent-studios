# M15 UI-03 — Wire Protocol Compliance & Golden Wire Fixture Audit

## Metadata
- **Milestone**: M15 UI-03
- **Topic**: Rust IPC Protocol Wire Serialization & Flat Serde Schema Alignment
- **Pinned Backend Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29 / M15-03.1)
- **Date**: 2026-11-02

---

## 1. Serde Wire Serialization Model

The authoritative Rust daemon uses internally tagged, flat JSON serialization (`#[serde(tag = "type")]`) without nested generic `status`/`result` wrappers.

### Implemented Command Envelopes
1. `get_task_diff`:
   ```json
   {
     "type": "get_task_diff",
     "studio_id": "studio-m15-001",
     "task_id": "task-m15-001",
     "offset": 0,
     "limit": 1048576
   }
   ```
2. `get_artifact`:
   ```json
   {
     "type": "get_artifact",
     "studio_id": "studio-m15-001",
     "artifact_id": "art-001",
     "offset": 0,
     "limit": 1048576
   }
   ```
3. `list_task_artifacts`:
   ```json
   {
     "type": "list_task_artifacts",
     "studio_id": "studio-m15-001",
     "task_id": "task-m15-001"
   }
   ```

### Implemented Response Envelopes
1. `task_diff`:
   ```json
   {
     "type": "task_diff",
     "studio_id": "studio-m15-001",
     "task_id": "task-m15-001",
     "artifact_id": "art-diff-001",
     "base_commit": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
     "head_commit": "4b825dc642cb6eb9a060e54bf8d69288fbee4904ce714c330c6a8f6d7fd9b7f5",
     "summary": {
       "files_changed": 1,
       "added_files": [],
       "modified_files": ["src/main.rs"],
       "deleted_files": [],
       "renamed_files": [],
       "binary_files": [],
       "patch_size_bytes": 128,
       "base_commit": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
     },
     "patch": "diff --git a/src/main.rs b/src/main.rs...",
     "patch_base64": "ZGlmZiAtLWdpdCBhL3NyYy9tYWluLnJzIGIvc3JjL21haW4ucnMuLi4=",
     "encoding": "utf-8",
     "offset": 0,
     "limit": 1048576,
     "total_bytes": 128,
     "chunk_bytes": 128,
     "has_more": false,
     "patch_sha256": "sha256:d559868bf03043232152bd6cc15dbb9cfb7efec26986fa808797f259021e1279"
   }
   ```
2. `artifact`:
   ```json
   {
     "type": "artifact",
     "studio_id": "studio-m15-001",
     "artifact": {
       "id": "art-001",
       "studio_id": "studio-m15-001",
       "task_id": "task-m15-001",
       "producer_agent_id": "agent-coder-01",
       "kind": "patch",
       "logical_name": "unified.patch",
       "content_hash": "sha256:d559868bf03043232152bd6cc15dbb9cfb7efec26986fa808797f259021e1279",
       "location": "/tmp/worktrees/wt-01/artifacts/unified.patch",
       "created_at": "2026-11-02T00:00:00Z",
       "size_bytes": 128
     },
     "content": "diff --git a/src/main.rs b/src/main.rs...",
     "content_base64": "ZGlmZiAtLWdpdCBhL3NyYy9tYWluLnJzIGIvc3JjL21haW4ucnMuLi4=",
     "encoding": "utf-8",
     "offset": 0,
     "limit": 1048576,
     "total_bytes": 128,
     "chunk_bytes": 128,
     "has_more": false,
     "is_truncated": false
   }
   ```
3. `task_artifacts`:
   ```json
   {
     "type": "task_artifacts",
     "studio_id": "studio-m15-001",
     "task_id": "task-m15-001",
     "artifacts": [
       {
         "id": "art-001",
         "studio_id": "studio-m15-001",
         "task_id": "task-m15-001",
         "producer_agent_id": "agent-coder-01",
         "kind": "patch",
         "logical_name": "unified.patch",
         "content_hash": "sha256:d559868bf03043232152bd6cc15dbb9cfb7efec26986fa808797f259021e1279",
         "location": "/tmp/worktrees/wt-01/artifacts/unified.patch",
         "created_at": "2026-11-02T00:00:00Z",
         "size_bytes": 128
       }
     ]
   }
   ```

---

## 2. Golden Wire Fixture Verification

The unit test suite `src/test/unit/ArtifactProtocol.test.ts` executes round-trip framing, encoding, and decoding against the 6 canonical golden wire fixtures extracted directly from backend commit `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b`. All 6 tests pass cleanly with byte-exact property parity.
