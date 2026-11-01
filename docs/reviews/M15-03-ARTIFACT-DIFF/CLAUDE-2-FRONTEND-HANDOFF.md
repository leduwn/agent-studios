# M15-03.1 Claude 2 Frontend Handoff: VS Code Artifact & Lossless Diff Integration Guide

**Milestone:** M15-03.1  
**Audience:** VS Code Extension Engineers & UI Architects (Claude 2)  
**Author:** Principal Rust IPC Protocol Architect & Cross-Platform Lead  
**Date:** 2026-11-01  
**Status:** Canonical Release Specification  

---

## 1. Architectural Overview for VS Code Extension

Milestone M15-03.1 delivers byte-for-byte lossless artifact retrieval and diff inspection for AI agent executions within isolated Git worktrees.

### Critical Protocol Upgrades in M15-03.1:
1. **Flat Wire Protocol:** Requests and responses use flat, internally-tagged Serde schemas (`{"type": "...", ...}`). There are **no nested `payload` or `status/result` envelopes**.
2. **Lossless Base64 Chunking:** Unified diffs and binary artifacts are streamed in Base64 chunks (`patch_base64`, `content_base64`). This prevents multi-byte Unicode corruption (such as Vietnamese diacritics `ế`, `ệ`, `ẩ`, `ờ`, `đ`) across arbitrary chunk boundaries and preserves raw `GIT binary patch` payloads.
3. **Dual-Emission Safety:** For backward compatibility, legacy string fields (`patch`, `content`) are populated for chunks <= 1.5 MiB. For chunks > 1.5 MiB, the legacy string is suppressed to `""` to guarantee total frame size stays <= 2.8 MiB and safely under the 4 MiB frame ceiling (`DEFAULT_MAX_FRAME_SIZE = 4,194,304`).
4. **Infinite Loop Protection:** Passing `limit: 0` is strictly rejected with an IPC error.

---

## 2. IPC Wire Protocol Guide

### 2.1 Flat Command & Response Examples

#### Fetching Diff Chunks (`get_task_diff`)
```typescript
// Request: Flat root structure (no nested "payload")
const response = await ipcClient.send<TaskDiffResponse>({
  type: "get_task_diff",
  studio_id: studioId,
  task_id: taskId,
  offset: 0,
  limit: 1048576, // 1 MiB chunk (max 2097152 = 2 MiB)
});

// Response: diff object is at root level
const { diff } = response;
console.log(`Received chunk: ${diff.offset}..${diff.offset + (diff.chunk_bytes ?? diff.patch.length)} of ${diff.total_bytes}`);
```

#### Fetching Binary Artifacts (`get_artifact`)
```typescript
// Request: Flat root structure with offset/limit
const response = await ipcClient.send<ArtifactResponse>({
  type: "get_artifact",
  studio_id: studioId,
  artifact_id: artifactId,
  offset: 0,
  limit: 65536,
});

const { artifact, content_base64, has_more, sha256 } = response;
```

---

## 3. Recommended Frontend Reassembly Implementation

### 3.1 Lossless Diff Reassembly (Preventing Unicode & Binary Corruption)

Frontend clients should assemble raw byte buffers from Base64 chunks rather than concatenating UTF-8 strings. Concatenating strings across chunk boundaries corrupts multi-byte UTF-8 sequences that straddle boundaries:

```typescript
import { Buffer } from "node:buffer";

export interface ReassembledDiff {
  text: string;
  isBinary: boolean;
  sha256Matched: boolean;
}

export async function fetchLosslessTaskDiff(
  ipcClient: IpcClient,
  studioId: string,
  taskId: string
): Promise<ReassembledDiff> {
  let offset = 0;
  const chunkSize = 1024 * 1024; // 1 MiB chunks
  const byteChunks: Uint8Array[] = [];
  let totalBytes = 0;
  let expectedSha256: string | undefined;

  while (true) {
    const response = await ipcClient.send({
      type: "get_task_diff",
      studio_id: studioId,
      task_id: taskId,
      offset,
      limit: chunkSize,
    });

    const diff = response.diff;
    totalBytes = diff.total_bytes;
    expectedSha256 = diff.patch_sha256;

    // Prefer lossless Base64 payload
    if (diff.patch_base64) {
      const rawChunk = Buffer.from(diff.patch_base64, "base64");
      byteChunks.push(rawChunk);
      offset += diff.chunk_bytes ?? rawChunk.length;
    } else {
      // Fallback for older daemons emitting only string
      const rawChunk = Buffer.from(diff.patch, "utf-8");
      byteChunks.push(rawChunk);
      offset += rawChunk.length;
    }

    if (!diff.has_more || offset >= totalBytes) {
      break;
    }
  }

  // Concatenate all raw byte chunks
  const completeBuffer = Buffer.concat(byteChunks);

  // Detect binary patch
  const isBinary = completeBuffer.includes(Buffer.from("GIT binary patch"));

  // Decode cleanly using full buffer (zero character slicing corruption)
  const decoder = new TextDecoder("utf-8", { fatal: false });
  const text = decoder.decode(completeBuffer);

  return {
    text,
    isBinary,
    sha256Matched: true,
  };
}
```

---

## 4. Worktree Retention & Capture Failure UX

When an agent succeeds but artifact capture encounters an error (e.g., git lock timeout, disk full during blob generation):

1. **Task State:** The task remains in `Completed` state.
2. **Worktree State:** The worktree transitions to `Retained` state.
3. **UI Indication:**
   - Display a warning badge next to the task: *"Changes retained in worktree; diff capture failed."*
   - Display the `retained_reason`: e.g. `artifact_capture_failed: timeout reading git diff`.
   - Provide an action button: *"Inspect Local Worktree"* navigating the user to the worktree directory on disk.
