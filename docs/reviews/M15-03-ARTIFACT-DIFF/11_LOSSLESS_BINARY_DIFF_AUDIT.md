# M15-03.1 Lossless Binary Diff & Artifact Streaming Audit

**Milestone:** M15-03.1  
**Status:** Complete  
**Date:** 2026-11-01  
**Author:** Subagent B (Protocol & Binary Serialization Architect)  
**Target Repository:** `agent-studios-rs`  
**Referenced Components:**
- `agent-studios-protocol::artifact` (`crates/protocol/src/artifact.rs`)
- `agent-studios-ipc-transport::connection` (`crates/ipc-transport/src/connection.rs`)
- `agent-studios-ipc-transport::client` (`crates/ipc-transport/src/client.rs`)
- `agent-studios-workspace::artifact_store` (`crates/workspace/src/artifact_store.rs`)
- `agent-studios-ipc-protocol::framing` (`crates/ipc-protocol/src/framing.rs`)

---

## 1. Executive Summary

Milestone M15-03 introduced the initial read-only IPC endpoints for artifact and patch inspection: `ListTaskArtifacts`, `GetArtifact`, and `GetTaskDiff`. While functional for standard ASCII text files under 2 MiB, rigorous architectural inspection reveals critical corruption and denial-of-service vulnerabilities in the current byte-slicing and UTF-8 handling implementations:

1. **UTF-8 Boundary Corruption in `GetTaskDiff` (connection.rs:1500–1540):** Patch chunking slices raw byte buffers at arbitrary byte offsets and converts slices using `String::from_utf8_lossy`. Any multi-byte UTF-8 character (such as Vietnamese diacritics `ế`, `ệ`, `ẩ`, `ờ`, `đ`) straddling a chunk boundary is replaced with Unicode replacement characters (`\u{FFFD}`). When chunks are concatenated by clients, the original text is irreversibly corrupted.
2. **Binary Git Patch Destruction (`GIT binary patch`):** `git diff --binary` emits binary delta blobs containing arbitrary byte values `0x00..0xFF`. Passing arbitrary byte slices through `String::from_utf8_lossy` replaces non-UTF-8 bytes with `0xEF 0xBF 0xBD` (`\u{FFFD}`), corrupting the binary patch and causing downstream `git apply --binary` failures.
3. **Hard 2 MiB Ceiling & Rejection in `GetArtifact` (connection.rs:1400–1425):** `GetArtifact` rejects any artifact blob larger than 2 MiB with an IPC error because pagination is completely absent in `IpcCommand::GetArtifact`. Furthermore, binary artifacts (e.g., compiled WASM, SQLite databases, PNGs) under 2 MiB are converted to lossy strings, corrupting binary assets.

This audit specifies the comprehensive **M15-03.1 Lossless Binary Architecture**:
- **Additive Base64 Encoding:** Non-breaking additive fields `patch_base64`, `encoding`, `patch_sha256`, and `chunk_bytes` in `TaskDiffResponse`.
- **Exact Byte-Level Slicing & Preservation:** Guarantees 100% byte-for-byte fidelity across arbitrary chunk boundaries, preserving Vietnamese Unicode text and `GIT binary patch` data.
- **Paginated Binary Retrieval for `GetArtifact`:** Adds `offset` and `limit` to `IpcCommand::GetArtifact`, and additive `content_base64`, `encoding`, `offset`, `limit`, `total_bytes`, `has_more`, and `sha256` to `IpcResponse::Artifact`.
- **Framing Safety Budget:** Strict 2 MiB chunk bound ensuring Base64 payloads (max 2.67 MiB) stay well inside the 4 MiB frame limit (`DEFAULT_MAX_FRAME_SIZE = 4,194,304` bytes) without frame overflow.

---

## 2. Root Cause Analysis: Current Implementation Flaws

### 2.1 Patch Chunking in `connection.rs` (Lines 1500–1540)

The current implementation in `connection.rs` executes:

```rust
// Current connection.rs lines 1510-1534
let total_bytes = patch_bytes.len();
let offset_val = offset.unwrap_or(0);
const MAX_CHUNK_LIMIT: usize = 2 * 1024 * 1024;
let limit_val = limit.unwrap_or(1024 * 1024).min(MAX_CHUNK_LIMIT);

let start = offset_val.min(total_bytes);
let end = offset_val.saturating_add(limit_val).min(total_bytes);
let chunk_slice = &patch_bytes[start..end];
let patch = String::from_utf8_lossy(chunk_slice).to_string();
let has_more = end < total_bytes;

TaskDiffResponse {
    studio_id,
    task_id,
    artifact_id: Some(patch_record.id),
    base_commit,
    head_commit,
    summary,
    patch,
    offset: start,
    limit: limit_val,
    total_bytes,
    has_more,
}
```

#### Defect 1: Multi-Byte UTF-8 Boundary Splitting
UTF-8 encodes Unicode code points into sequences of 1 to 4 bytes:
- 1 byte: Standard ASCII (`0x00..0x7F`)
- 2 bytes: Latin Extended, Greek, Cyrillic (`0xC2..0xDF 0x80..0xBF`)
- 3 bytes: Basic Multilingual Plane, including Vietnamese tone marks and CJK (`0xE0..0xEF 0x80..0xBF 0x80..0xBF`)
- 4 bytes: Supplementary planes, including emoji (`0xF0..0xF4 0x80..0xBF 0x80..0xBF 0x80..0xBF`)

When `limit_val` cuts across a multi-byte sequence:
1. `chunk_slice` for Chunk $N$ terminates with an incomplete prefix (e.g., bytes 1 and 2 of a 3-byte sequence).
2. `String::from_utf8_lossy` cannot decode the incomplete sequence and substitutes Unicode Replacement Character `\u{FFFD}` (3 bytes: `0xEF 0xBF 0xBD`).
3. Chunk $N+1$ begins with the orphan continuation byte (byte 3). `String::from_utf8_lossy` cannot decode an orphan continuation byte and substitutes another `\u{FFFD}`.
4. Client string concatenation:
   $$\text{reconstructed} = \text{chunk}_N\text{.patch} + \text{chunk}_{N+1}\text{.patch}$$
   The target character is replaced by `\u{FFFD}\u{FFFD}`. The original code point is irretrievably destroyed.

#### Defect 2: Vietnamese Unicode Degradation
Vietnamese text extensively uses composite and precomposed diacritics located in the Latin Extended Additional block (`U+1EA0` to `U+1EF9`). All these glyphs require 3 UTF-8 bytes:

| Character | Code Point | UTF-8 Bytes (Hex) | Description |
|-----------|------------|-------------------|-------------|
| **ế** | `U+1EBF` | `E1 BA BF` | e with circumflex and acute |
| **ệ** | `U+1EC7` | `E1 BB 87` | e with circumflex and dot below |
| **ẩ** | `U+1EA9` | `E1 BA A9` | a with circumflex and hook above |
| **ờ** | `U+1EDD` | `E1 BB 9D` | o with horn and grave |
| **đ** | `U+0111` | `C4 91` | d with stroke (2 bytes) |

*Failure Scenario:* If a chunk boundary occurs between `0xBA` and `0xBF` for the character `ế`:
- Chunk 1 receives `[... 0xE1, 0xBA]` $\rightarrow$ `from_utf8_lossy` emits `\u{FFFD}`
- Chunk 2 receives `[0xBF, ...]` $\rightarrow$ `from_utf8_lossy` emits `\u{FFFD}`
- Client receives: `Ti\u{FFFD}\u{FFFD}ng Vi\u{FFFD}\u{FFFD}t` instead of `Tiếng Việt`.

#### Defect 3: Corruption of Git Binary Patches (`GIT binary patch`)
In `agent-studios-workspace::capture::capture_changes`, diffs are generated via:
```bash
git diff --cached --binary --full-index --no-ext-diff <base_sha> -- .
```
When files contain binary data (images, compiled artifacts, serialized protobufs, fonts), Git emits binary patch format headers followed by zlib-deflated base85 or raw literal chunks. If this data is sliced into UTF-8 lossy strings:
- Arbitrary byte sequences outside valid UTF-8 are replaced with `\u{FFFD}`.
- Reconstructed patch strings fail Git header validation and checksum tests.
- Execution of `git apply --binary` fails with `fatal: corrupt patch at line ...`.

---

### 2.2 `GetArtifact` Handling in `connection.rs` (Lines 1400–1425)

The current implementation in `connection.rs` executes:

```rust
// Current connection.rs lines 1406-1421
match orchestrator.artifact_store().read_bytes(hash_str) {
    Ok(bytes) => {
        const MAX_CONTENT_SIZE: usize = 2 * 1024 * 1024;
        if bytes.len() > MAX_CONTENT_SIZE {
            tx.send(IpcResponse::Error {
                message: format!(
                    "Artifact {artifact_id} content size ({} bytes) exceeds frame limit ({MAX_CONTENT_SIZE} bytes)",
                    bytes.len()
                ),
            })
            .await
            .map_err(|_| TransportError::ConnectionClosed)?;
            continue;
        }
        Some(String::from_utf8_lossy(&bytes).to_string())
    }
    Err(e) => { ... }
}
```

#### Defect 1: Hard 2 MiB Failure Barrier
Any artifact stored in `ArtifactStore` exceeding 2 MiB (e.g., large diff patches, test logs, coverage files, compiler output) cannot be read via IPC. Instead of paginating, the server returns `IpcResponse::Error` and terminates the request.

#### Defect 2: Destruction of Binary Artifacts
Even for artifacts $\le 2$ MiB, `String::from_utf8_lossy` is unconditionally applied to the raw bytes. If an artifact is a binary file (such as a compiled WASM module or PNG asset recorded as `ArtifactKind::File`), every non-ASCII byte is corrupted into `0xEF 0xBF 0xBD`.

#### Defect 3: Lack of Pagination Controls
`IpcCommand::GetArtifact` only accepts `{ studio_id, artifact_id }`. Clients cannot request specific byte ranges or stream large artifacts.

---

## 3. Lossless Patch Representation in `TaskDiffResponse`

To preserve 100% byte fidelity without breaking existing callers that expect string patches, four additive fields are introduced into `TaskDiffResponse`.

### 3.1 Updated Rust Struct Definition

```rust
// File: agent-studios-rs/crates/protocol/src/artifact.rs

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDiffResponse {
    pub studio_id: StudioId,
    pub task_id: TaskId,
    pub artifact_id: Option<ArtifactId>,
    pub base_commit: Option<String>,
    pub head_commit: Option<String>,
    pub summary: DiffSummary,
    
    /// Legacy UTF-8 lossy representation of the chunk.
    /// Preserved for backward compatibility with older clients.
    pub patch: String,
    
    pub offset: usize,
    pub limit: usize,
    pub total_bytes: usize,
    pub has_more: bool,

    // --- Additive Lossless Binary Diff Extensions (M15-03.1) ---

    /// RFC 4648 standard Base64-encoded chunk slice bytes.
    /// Guaranteed exact byte-for-byte representation of &patch_bytes[start..end].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_base64: Option<String>,

    /// Encoding format of `patch_base64` (always "base64" when present).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,

    /// Cryptographic SHA-256 hex digest of the entire raw patch artifact blob.
    /// Allows the client to verify reassembled stream integrity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_sha256: Option<String>,

    /// Exact byte length of the raw chunk slice (&patch_bytes[start..end].len()).
    /// Must match decoded base64 byte count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_bytes: Option<usize>,
}
```

### 3.2 Server-Side Chunking & Dual-Emission Algorithm

```rust
// Slicing and encoding algorithm in connection.rs:
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;

let total_bytes = patch_bytes.len();
let offset_val = offset.unwrap_or(0);

// Maximum raw chunk limit clamped to 2 MiB to ensure frame safety
const MAX_CHUNK_LIMIT: usize = 2 * 1024 * 1024;
let limit_val = limit.unwrap_or(1024 * 1024).min(MAX_CHUNK_LIMIT);

let start = offset_val.min(total_bytes);
let end = offset_val.saturating_add(limit_val).min(total_bytes);
let chunk_slice = &patch_bytes[start..end];
let chunk_len = chunk_slice.len();

// 1. Lossless RFC 4648 Base64 encoding of exact byte slice
let patch_base64_str = BASE64_STANDARD.encode(chunk_slice);

// 2. Full artifact SHA-256 for cryptographic end-to-end verification
let full_patch_sha256 = hash_str
    .strip_prefix("sha256:")
    .unwrap_or(hash_str)
    .to_string();

// 3. Frame Budget Optimization for Dual-Emission:
// If chunk_len > 1.5 MiB, dual-emitting full lossy string + base64 would exceed 4 MiB frame.
// In that case, suppress legacy `patch` field with empty string to protect transport.
let legacy_patch = if chunk_len > 1_500_000 {
    String::new()
} else {
    String::from_utf8_lossy(chunk_slice).to_string()
};

let has_more = end < total_bytes;

let response = TaskDiffResponse {
    studio_id,
    task_id,
    artifact_id: Some(patch_record.id),
    base_commit,
    head_commit,
    summary,
    patch: legacy_patch,
    offset: start,
    limit: limit_val,
    total_bytes,
    has_more,
    patch_base64: Some(patch_base64_str),
    encoding: Some("base64".to_string()),
    patch_sha256: Some(full_patch_sha256),
    chunk_bytes: Some(chunk_len),
};
```

### 3.3 Exact Byte-for-Byte Client Reassembly Algorithm

```typescript
/**
 * Assembles paginated TaskDiffResponse chunks into an exact byte buffer.
 * Fully preserves Vietnamese Unicode and binary Git patch deltas.
 */
export async function fetchFullTaskPatch(
  client: IpcClient,
  studioId: string,
  taskId: string
): Promise<Uint8Array> {
  const CHUNK_SIZE = 1024 * 1024; // 1 MiB chunk requests
  let offset = 0;
  let hasMore = true;
  const byteChunks: Uint8Array[] = [];
  let expectedSha256: string | undefined;
  let totalBytes = 0;

  while (hasMore) {
    const diffResp: TaskDiffResponse = await client.getTaskDiff(
      studioId,
      taskId,
      offset,
      CHUNK_SIZE
    );

    expectedSha256 = diffResp.patch_sha256 ?? expectedSha256;
    totalBytes = diffResp.total_bytes;

    let chunkBytes: Uint8Array;
    if (diffResp.patch_base64) {
      // Modern lossless path
      const binaryString = atob(diffResp.patch_base64);
      chunkBytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        chunkBytes[i] = binaryString.charCodeAt(i);
      }
      if (diffResp.chunk_bytes !== undefined && chunkBytes.length !== diffResp.chunk_bytes) {
        throw new Error(
          `Chunk byte count mismatch: decoded ${chunkBytes.length}, expected ${diffResp.chunk_bytes}`
        );
      }
    } else {
      // Legacy fallback
      chunkBytes = new TextEncoder().encode(diffResp.patch);
    }

    byteChunks.push(chunkBytes);
    offset += chunkBytes.length;
    hasMore = diffResp.has_more && offset < totalBytes;
  }

  // Concatenate byte chunks
  const assembled = new Uint8Array(offset);
  let writePtr = 0;
  for (const chunk of byteChunks) {
    assembled.set(chunk, writePtr);
    writePtr += chunk.length;
  }

  // Validate complete SHA-256 hash if provided
  if (expectedSha256) {
    const digestBuffer = await crypto.subtle.digest("SHA-256", assembled);
    const actualSha256 = Array.from(new Uint8Array(digestBuffer))
      .map((b) => b.toString(16).padStart(2, "0"))
      .join("");
    if (actualSha256 !== expectedSha256) {
      throw new Error(
        `Integrity failure: patch SHA-256 mismatch (expected ${expectedSha256}, got ${actualSha256})`
      );
    }
  }

  return assembled;
}
```

---

## 4. Paginated Binary Retrieval for `GetArtifact`

`GetArtifact` is extended to support pagination, streaming, and binary encoding.

### 4.1 Extended `IpcCommand::GetArtifact`

```rust
// File: agent-studios-rs/crates/ipc-transport/src/connection.rs

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum IpcCommand {
    // ...
    GetArtifact {
        studio_id: StudioId,
        artifact_id: ArtifactId,
        /// Starting byte offset (defaults to 0)
        #[serde(default)]
        offset: Option<usize>,
        /// Maximum bytes to retrieve in this chunk (defaults to 1 MiB, max 2 MiB)
        #[serde(default)]
        limit: Option<usize>,
    },
    // ...
}
```

### 4.2 Extended `IpcResponse::Artifact`

```rust
// File: agent-studios-rs/crates/ipc-transport/src/connection.rs

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum IpcResponse {
    // ...
    Artifact {
        artifact: Option<ArtifactRecord>,
        /// Legacy string content (only populated for text artifacts <= 1 MiB)
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        /// RFC 4648 Base64-encoded chunk slice bytes
        #[serde(default, skip_serializing_if = "Option::is_none")]
        content_base64: Option<String>,
        /// Encoding format (always "base64" when content_base64 is present)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encoding: Option<String>,
        /// Chunk slice start offset
        #[serde(default, skip_serializing_if = "Option::is_none")]
        offset: Option<usize>,
        /// Chunk slice limit parameter
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        /// Total artifact size in bytes
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total_bytes: Option<usize>,
        /// True if additional chunks remain after this slice
        #[serde(default, skip_serializing_if = "Option::is_none")]
        has_more: Option<bool>,
        /// Cryptographic SHA-256 hex digest of the full artifact blob
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
    },
    // ...
}
```

### 4.3 Handler Implementation in `connection.rs`

```rust
IpcCommand::GetArtifact {
    studio_id,
    artifact_id,
    offset,
    limit,
} => {
    let (artifact_rec, content_hash) = {
        let cp = server_state.control_plane.read().await;
        match cp.get_artifact(artifact_id) {
            Some(art) => {
                if art.studio_id != studio_id {
                    tx.send(IpcResponse::Error {
                        message: format!(
                            "Artifact {artifact_id} does not belong to studio {studio_id}"
                        ),
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                    continue;
                }
                (Some(art.clone()), art.content_hash.clone())
            }
            None => (None, None),
        }
    };

    if let (Some(ref rec), Some(ref hash_str)) = (&artifact_rec, &content_hash) {
        if let Some(ref orchestrator) = server_state.workspace_orchestrator {
            match orchestrator.artifact_store().read_bytes(hash_str) {
                Ok(raw_bytes) => {
                    let total_bytes = raw_bytes.len();
                    let offset_val = offset.unwrap_or(0);
                    const MAX_ARTIFACT_CHUNK: usize = 2 * 1024 * 1024;
                    let limit_val = limit.unwrap_or(1024 * 1024).min(MAX_ARTIFACT_CHUNK);

                    let start = offset_val.min(total_bytes);
                    let end = offset_val.saturating_add(limit_val).min(total_bytes);
                    let chunk = &raw_bytes[start..end];
                    let chunk_len = chunk.len();
                    let has_more = end < total_bytes;

                    let base64_payload = BASE64_STANDARD.encode(chunk);
                    let clean_sha256 = hash_str
                        .strip_prefix("sha256:")
                        .unwrap_or(hash_str)
                        .to_string();

                    // Legacy content: only populate if chunk <= 1 MiB to prevent frame explosion
                    let legacy_content = if chunk_len <= 1_048_576 {
                        Some(String::from_utf8_lossy(chunk).to_string())
                    } else {
                        None
                    };

                    tx.send(IpcResponse::Artifact {
                        artifact: artifact_rec,
                        content: legacy_content,
                        content_base64: Some(base64_payload),
                        encoding: Some("base64".to_string()),
                        offset: Some(start),
                        limit: Some(limit_val),
                        total_bytes: Some(total_bytes),
                        has_more: Some(has_more),
                        sha256: Some(clean_sha256),
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
                Err(e) => {
                    tx.send(IpcResponse::Error {
                        message: format!("Failed to read artifact blob: {e}"),
                    })
                    .await
                    .map_err(|_| TransportError::ConnectionClosed)?;
                }
            }
        } else {
            tx.send(IpcResponse::Artifact {
                artifact: artifact_rec,
                content: None,
                content_base64: None,
                encoding: None,
                offset: None,
                limit: None,
                total_bytes: None,
                has_more: None,
                sha256: None,
            })
            .await
            .map_err(|_| TransportError::ConnectionClosed)?;
        }
    } else {
        tx.send(IpcResponse::Artifact {
            artifact: None,
            content: None,
            content_base64: None,
            encoding: None,
            offset: None,
            limit: None,
            total_bytes: None,
            has_more: None,
            sha256: None,
        })
        .await
        .map_err(|_| TransportError::ConnectionClosed)?;
    }
}
```

---

## 5. Mathematical Framing & Chunk Budget Verification

The IPC transport layer enforces a strict frame limit defined in `agent-studios-ipc-protocol::framing`:
$$\text{DEFAULT\_MAX\_FRAME\_SIZE} = 4 \times 1024 \times 1024 = 4,194,304 \text{ bytes (4 MiB)}$$

### 5.1 Base64 Encoding Expansion
Base64 encodes every 3 raw bytes into 4 ASCII characters:
$$\text{Base64 Length} = 4 \times \left\lceil \frac{N}{3} \right\rceil$$

For a maximum raw chunk size of $N = 2 \text{ MiB} = 2,097,152 \text{ bytes}$:
$$\left\lceil \frac{2,097,152}{3} \right\rceil = 699,051 \text{ triplets}$$
$$\text{Base64 Size} = 699,051 \times 4 = 2,796,204 \text{ bytes } (\approx 2.6667 \text{ MiB})$$

### 5.2 Dual-Emission Frame Budget Analysis

When both legacy `patch: String` and `patch_base64: String` are serialized in the same JSON frame:

| Component | 1.0 MiB Chunk | 1.5 MiB Chunk | 2.0 MiB Chunk (Unmitigated) | 2.0 MiB Chunk (Mitigated) |
|-----------|---------------|---------------|-----------------------------|---------------------------|
| Raw Slice | 1,048,576 B | 1,572,864 B | 2,097,152 B | 2,097,152 B |
| `patch_base64` | 1,398,104 B | 2,097,152 B | 2,796,204 B | 2,796,204 B |
| `patch` (lossy) | 1,048,576 B | 1,572,864 B | 2,097,152 B | 0 B (Suppressed) |
| JSON Overhead | ~600 B | ~600 B | ~600 B | ~600 B |
| **Total Frame** | **2,447,280 B** | **3,670,616 B** | **4,893,956 B** | **2,796,804 B** |
| **vs 4 MiB Cap** | **58.3% (SAFE)** | **87.5% (SAFE)** | **116.7% (EXCEEDS!)** | **66.7% (SAFE)** |

**Mitigation Rule:**
If a requested raw slice exceeds 1.5 MiB (`1,500,000` bytes), the server suppresses the legacy `patch` / `content` string (setting it to `String::new()` or `None`). This guarantees the total encoded JSON frame never exceeds 2.8 MiB, remaining well under the 4,194,304-byte limit under all conditions.

---

## 6. Vietnamese Unicode & Multi-Byte Boundary Proof

### 6.1 Concrete Failure Demonstration (Current Code)
Given the Vietnamese string `"Tập tin được lưu tại Agent Studios"`:
- Character `ậ` (`U+1EAD`): bytes `[0xE1, 0xBA, 0xAD]`
- Character `đ` (`U+0111`): bytes `[0xC4, 0x91]`
- Character `ư` (`U+01B0`): bytes `[0xC6, 0xB0]`
- Character `ợ` (`U+1EE3`): bytes `[0xE1, 0xBB, 0xA3]`

Suppose a chunk boundary splits `ợ` between byte 2 (`0xBB`) and byte 3 (`0xA3`):
- Chunk 1 ends with: `[..., 0xE1, 0xBB]`
  - `String::from_utf8_lossy(&[0xE1, 0xBB])` produces `"\u{FFFD}"`
- Chunk 2 begins with: `[0xA3, ...]`
  - `String::from_utf8_lossy(&[0xA3])` produces `"\u{FFFD}"`
- Concatenation: `\u{FFFD}\u{FFFD}` (Permanent data corruption).

### 6.2 Lossless Proof (M15-03.1 Architecture)
1. Chunk 1 extracts byte slice `&patch_bytes[0..end]` ending in `0xE1, 0xBB`.
2. Base64 encodes exact octets `0xE1, 0xBB`.
3. Chunk 2 extracts byte slice `&patch_bytes[end..next]` beginning with `0xA3`.
4. Base64 encodes exact octet `0xA3`.
5. Client decodes Chunk 1 Base64 $\rightarrow$ `Uint8Array` ending in `0xE1, 0xBB`.
6. Client decodes Chunk 2 Base64 $\rightarrow$ `Uint8Array` beginning with `0xA3`.
7. Client concatenates raw byte arrays $\rightarrow$ octet sequence `[0xE1, 0xBB, 0xA3]` is perfectly restored.
8. Client passes combined buffer to `new TextDecoder("utf-8").decode(buffer)` $\rightarrow$ yields exact glyph `ợ`.

---

## 7. Git Binary Patch Verification Lifecycle

The diagram below illustrates the end-to-end byte preservation pipeline for binary assets:

```
[Worktree Mutation: PNG / WASM / Binary File]
                     │
                     ▼
[capture_changes: git diff --cached --binary --full-index]
                     │ Produces raw patch bytes:
                     │ "GIT binary patch\nliteral 1024\nzc...<raw binary bytes>"
                     ▼
[ArtifactStore: store_bytes]
                     │ Keyed by SHA-256 hash
                     │ Stored on disk under blobs/sha256/<hash>
                     ▼
[IPC Transport: GetTaskDiff]
                     │ Slices raw byte buffer at exact byte boundaries
                     │ Encodes slice via RFC 4648 Base64
                     │ Computes full blob SHA-256
                     ▼
[VS Code Client / IPC Receiver]
                     │ Decodes Base64 chunks into Uint8Array
                     │ Concatenates byte arrays
                     │ Verifies SHA-256 checksum against patch_sha256
                     ▼
[Disk / Apply Pipeline: git apply --binary]
                     │ Exact byte-for-byte fidelity restored
                     │ git apply succeeds without corruption
```

---

## 8. TypeScript Contract Specifications

### 8.1 TypeScript Types

```typescript
// Additions to agent-studios frontend client types:

export interface TaskDiffResponse {
  studio_id: string;
  task_id: string;
  artifact_id?: string;
  base_commit?: string;
  head_commit?: string;
  summary: DiffSummary;
  patch: string; // Legacy lossy representation
  offset: number;
  limit: number;
  total_bytes: number;
  has_more: boolean;
  // M15-03.1 Additions
  patch_base64?: string;
  encoding?: "base64";
  patch_sha256?: string;
  chunk_bytes?: number;
}

export interface ArtifactResponse {
  artifact?: ArtifactRecord;
  content?: string; // Legacy string content
  // M15-03.1 Additions
  content_base64?: string;
  encoding?: "base64";
  offset?: number;
  limit?: number;
  total_bytes?: number;
  has_more?: boolean;
  sha256?: string;
}
```

---

## 9. Edge Cases & Defensive Invariants

1. **Zero-Byte Artifacts / Empty Diffs:**
   - When `patch_bytes.len() == 0`: `total_bytes = 0`, `offset = 0`, `limit = limit_val`, `has_more = false`.
   - `patch_base64 = Some("")`, `chunk_bytes = Some(0)`, `patch = ""`.
   - `patch_sha256 = Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")` (SHA-256 of empty byte array).
2. **Out-of-Bounds Offsets:**
   - If client passes `offset >= total_bytes`: `start = total_bytes`, `end = total_bytes`, `chunk_slice = &[]`.
   - Returns empty base64 string `""`, `has_more = false`, `chunk_bytes = Some(0)`.
3. **Integer Overflow in Slicing:**
   - Uses `offset_val.saturating_add(limit_val).min(total_bytes)` to prevent arithmetic overflow on 32-bit/64-bit bounds.
4. **Tenant Isolation Enforcement:**
   - Cross-studio artifact access is rejected *before* reading disk blobs:
     `if art.studio_id != studio_id { return Error; }`
5. **Frame Overrun Defense:**
   - Any client request specifying `limit > 2 * 1024 * 1024` is clamped to 2 MiB.
   - Dual emission string suppression prevents frames exceeding 2.8 MiB.

---

## 10. Audit Conclusion & Implementation Recommendations

1. **Protocol Update:** Add `patch_base64`, `encoding`, `patch_sha256`, and `chunk_bytes` to `TaskDiffResponse` in `agent-studios-protocol::artifact`.
2. **IPC Command Extension:** Add `offset` and `limit` to `IpcCommand::GetArtifact` in `agent-studios-ipc-transport`.
3. **IPC Response Extension:** Add `content_base64`, `encoding`, `offset`, `limit`, `total_bytes`, `has_more`, `sha256` to `IpcResponse::Artifact`.
4. **Transport Layer Update:** Replace UTF-8 lossy chunking in `connection.rs` with byte-level Base64 slicing with legacy string fallback.
5. **Client Helper Update:** Update `IpcClient::get_artifact` and `IpcClient::get_task_diff` in `client.rs` to expose paginated lossless methods.
