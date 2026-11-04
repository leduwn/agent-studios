# M15 UI-03 — Multi-byte Unicode & Binary Safety Audit

## Metadata
- **Milestone**: M15 UI-03
- **Topic**: Preservation of Multi-byte UTF-8 Sequences and Git Binary Patches across Streaming Chunk Boundaries
- **Date**: 2026-11-02

---

## 1. Problem Statement: UTF-8 Slicing Vulnerability

When streaming text chunks over IPC or network frames, arbitrary byte boundaries (e.g., 64 KiB, 1 MiB) can split multi-byte UTF-8 codepoints across chunk edges:
- Vietnamese diacritics (`ế` = `0xC3 0xAA 0xCC 0x81`, `ệ` = `0xE1 0xBB 0x87`, `ẩ` = `0xE1 0xBA 0xA9`, `ờ` = `0xE1 0xBB 0x9D`, `đ` = `0xC4 0x91`).
- If an intermediate layer performs string conversion (`chunk.toString('utf8')`) on individual chunks before concatenation, incomplete leading or trailing byte sequences are replaced by the Unicode replacement character (`�`), permanently corrupting the text.
- If raw binary Git patches (`GIT binary patch`) are coerced into JavaScript strings, null bytes (`\0`) and arbitrary binary sequences cause truncation or corruption.

---

## 2. Implemented Defense in ArtifactClient

`ArtifactClient` eliminates this vulnerability through byte-level raw buffer reassembly:

```typescript
// 1. Decode Base64 chunk to raw binary Buffer
const chunkBuf = Buffer.from(chunkB64, 'base64');

// 2. Validate chunk byte length
if (chunk_bytes !== null && chunk_bytes !== undefined && chunkBuf.byteLength !== chunk_bytes) {
  throw new ArtifactRetrievalError('Chunk byte count mismatch', 'CHUNK_BYTES_MISMATCH');
}

// 3. Accumulate raw binary chunks
chunks.push(chunkBuf);

// 4. Concatenate raw buffers only AFTER all chunks are received
const fullBuffer = Buffer.concat(chunks, totalBytes);

// 5. Compute SHA-256 over raw binary buffer
const computedHash = crypto.createHash('sha256').update(fullBuffer).digest('hex');

// 6. Decode to UTF-8 string safely after complete reassembly
const text = isBinary ? '' : fullBuffer.toString('utf8');
```

---

## 3. Test Verification

`src/test/unit/ArtifactClient.test.ts` exercises:
1. `preserves multi-byte Vietnamese Unicode split across chunk boundaries`:
   - Text: `"Tiếng Việt có dấu: Việt Nam đất nước tôi yêu, hòa bình và phát triển bền vững."`
   - Split exactly across the 3-byte boundary of `'ệ'` (`0xE1 0xBB 0x87`).
   - Asserts assembled text matches original string without corruption or `�`.
2. `reassembles binary Git patch faithfully`:
   - Binary Git patch containing `GIT binary patch` header, null bytes (`\0`), and binary sequences.
   - Asserts `isBinary: true`, binary buffer matches original bytes, and SHA-256 matches.
