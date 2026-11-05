# M15 UI-05A — Report 06: Artifact Security & Diff Verification Architecture

## Metadata
- **Milestone**: M15 UI-05A: Frontend Reliability, Shared State Consistency & Integration Readiness
- **Focus**: Contiguous Raw Buffer Export, Tri-State SHA-256 Gating, Path Traversal Sanitization & Vietnamese UTF-8
- **Target Worktree**: `D:\Projects\agent-studios-ide-ui\apps\agent-studios-vscode`
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Author**: Claude 2 (Frontend / VSCode Extension Integration Lead)
- **Date**: 2026-11-02

---

## 1. Problem Statement & Artifact Vulnerability Surface

Artifacts produced by agents (diff patches, source files, binary outputs) introduce critical security attack surfaces:
1. **Truncated / Discontiguous Buffer Assembly**: In multi-chunk streaming, dropped or out-of-order chunks could corrupt source files or create invalid executable patches.
2. **Hash Tampering & Integrity Degradation**: Allowing artifacts with mismatched hashes or unverified statuses to be exported to disk risks code injection.
3. **Path Traversal Attacks**: Malicious or malformed file paths inside Git patches (e.g. `../../etc/shadow` or `C:\Windows\System32\...`) could write outside the workspace.
4. **Multi-byte Unicode Splitting**: Splitting UTF-8 multi-byte characters (such as Vietnamese diacritics `ệ`, `ắ`, `ồ`) across chunk boundaries without byte-level assembly leads to character corruption.

---

## 2. Security Invariants & Implementation Details

### A. Contiguous Raw Buffer Export (`ArtifactClient.ts`)
When assembling chunks for export:
- Each chunk carries an explicit 0-indexed sequence and byte length:
  ```typescript
  export interface ArtifactChunk {
      chunkIndex: number;
      totalChunks: number;
      data: Uint8Array;
      expectedSha256: string;
  }
  ```
- Before returning the assembled buffer, `ArtifactClient` verifies:
  1. `chunks.length === totalChunks`
  2. All indices `0 .. totalChunks - 1` are present contiguously with zero gaps.
  3. Total concatenated byte length matches the sum of chunk lengths.
  4. Final SHA-256 computed over the contiguous raw buffer matches `expectedSha256`.

### B. Tri-State Integrity Gating
Artifacts are evaluated against a tri-state status:
- **`VERIFIED`**: Full SHA-256 match. Permitted for diff display and disk export.
- **`UNVERIFIED`**: Incomplete stream or pending verification. Read-only inspection allowed; export blocked.
- **`INTEGRITY_FAILED`**: Hash mismatch or missing chunks. Quarantined immediately. Diff rendering blocked with security banner; export strictly disabled.

### C. Path Traversal Sanitization (`DiffParser.ts` & `WorkbenchViewProvider.ts`)
All file paths extracted from diff headers or artifact manifests undergo rigorous path normalization:
```typescript
export function sanitizeArtifactPath(rawPath: string): string {
    // 1. Reject null bytes
    if (rawPath.includes('\0')) {
        throw new SecurityError('Path contains null bytes');
    }
    // 2. Normalize separators to POSIX
    const normalized = path.posix.normalize(rawPath.replace(/\\/g, '/'));
    // 3. Reject absolute paths or traversal sequences
    if (path.posix.isAbsolute(normalized) || normalized.startsWith('../') || normalized === '..') {
        throw new SecurityError(`Path traversal attempt detected: ${rawPath}`);
    }
    return normalized;
}
```

### D. Vietnamese Multi-byte UTF-8 Streaming
- Binary chunks are concatenated as raw `Buffer` / `Uint8Array` instances before UTF-8 decoding.
- Multi-byte sequences split across chunk boundaries (e.g. byte 1 in chunk N, bytes 2–3 in chunk N+1) are reassembled intact, preserving characters like `ệ` (`0xEA 0xBB 0x87`) without replacement characters (`U+FFFD`).

---

## 3. Verification & Regressions Handled

- **Test Suites**:
  - `src/test/unit/ArtifactSecurityRegression.test.ts`
  - `src/test/e2e/RealArtifactIpcE2E.test.ts` (Tests 6–11)
- **Verified Invariants**:
  - Tampered chunk data produces `INTEGRITY_FAILED` and blocks export.
  - Path traversal inputs (`../../../foo.ts`) throw `SecurityError`.
  - Binary Git patches stream without corruption.
  - Multi-byte Vietnamese diacritics maintain byte-for-byte fidelity.
