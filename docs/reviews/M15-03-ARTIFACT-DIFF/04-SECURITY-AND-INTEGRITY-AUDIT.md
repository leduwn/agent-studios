# M15-03 Security and Integrity Audit

**Milestone:** M15-03  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Security Engineer & Systems Architect  

---

## 1. Threat Model & Boundaries

M15-03 introduces artifact persistence and IPC retrieval for filesystem diffs. The primary threat vectors analyzed:
1. **Cross-Tenant Information Disclosure**: Client associated with Studio $A$ attempts to read artifacts or diffs belonging to Studio $B$.
2. **Buffer Overflow / OOM via Malicious or Massive Diffs**: Agent runtime produces a massive diff (e.g. 100 MB generated asset or log) causing IPC transport frame exhaustion or memory exhaustion.
3. **Path Traversal via Manipulated Artifact Filenames / Hashes**: Exploitation of content hashes or artifact filenames to write outside `<artifact_root>` or read arbitrary files on the host filesystem.
4. **Data Corruption or Tampering in Blob Storage**: In-flight modification or corrupted bits in cached diff patches.

---

## 2. Security Controls & Mitigations

### 2.1 Multi-Tenant Studio Validation
Every IPC handler enforcing artifact queries performs rigorous studio ownership checks before disclosing metadata or blob contents:

```rust
// ListTaskArtifacts ownership check
if task.studio_id != studio_id {
    tx.send(IpcResponse::Error {
        message: format!("Task {task_id} not found in studio {studio_id}"),
    }).await;
    continue;
}

// GetArtifact ownership check
if art.studio_id != studio_id {
    tx.send(IpcResponse::Error {
        message: format!("Artifact {artifact_id} not found in studio {studio_id}"),
    }).await;
    continue;
}

// GetTaskDiff ownership check
if task.studio_id != studio_id {
    tx.send(IpcResponse::Error {
        message: format!("Task {task_id} not found in studio {studio_id}"),
    }).await;
    continue;
}
```
**Audit Result**: PASS. Mismatched studio requests return clean errors with zero data leakage.

### 2.2 Strict Frame-Bounded Chunking (2 MiB / 4 MiB)
- The IPC transport frame codec enforces `DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024` (4 MiB).
- `GetTaskDiff` enforces:
  ```rust
  const MAX_CHUNK_LIMIT: usize = 2 * 1024 * 1024;
  let limit_val = limit.unwrap_or(1024 * 1024).min(MAX_CHUNK_LIMIT);
  ```
  Any client request exceeding 2 MiB is silently clamped to the 2 MiB ceiling.
- `GetArtifact` enforces:
  ```rust
  const MAX_CONTENT_SIZE: usize = 2 * 1024 * 1024;
  if bytes.len() > MAX_CONTENT_SIZE {
      tx.send(IpcResponse::Error {
          message: format!("Artifact {artifact_id} content size exceeds frame limit"),
      }).await;
  }
  ```
**Audit Result**: PASS. IPC frames are mathematically bounded to $\le 2$ MiB of payload, leaving $\ge 2$ MiB of safety margin within the 4 MiB frame limit.

### 2.3 Path Traversal Prevention
`ArtifactStore` locates blobs strictly using their 64-character hexadecimal SHA-256 string:
```
<artifact_root>/blobs/sha256/<hash>
```
1. `validate_hash` verifies that `<hash>` contains exactly 64 hexadecimal ASCII characters (`[0-9a-f]`).
2. Directory separators (`/`, `\`), path traversal sequences (`..`), and null bytes (`\0`) are strictly rejected with `ArtifactStoreError::InvalidContentHash`.
3. Temporary write files reside in `<artifact_root>/tmp/<uuid>`.
**Audit Result**: PASS. Arbitrary file read/write through artifact IDs or content hashes is impossible.

### 2.4 Cryptographic Integrity Verification
On every read, `ArtifactStore::read_bytes` computes the SHA-256 digest of the read file bytes:
```rust
let actual_hash = hex::encode(Sha256::digest(&bytes));
if actual_hash != hash {
    return Err(ArtifactStoreError::HashMismatch {
        expected: hash.to_string(),
        actual: actual_hash,
    });
}
```
**Audit Result**: PASS. Tampered, partially written, or corrupted blobs are rejected immediately.
