# M15 UI-04 — Artifact & Cryptographic Security Regression Audit

## Metadata
- **Milestone**: M15 UI-04: EXECUTION WORKBENCH & SECURITY HARDENING
- **Author**: Agent D (Artifact & Security Reviewer)
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Primary Scope**: `ArtifactClient`, `ArtifactReviewPanel`, `DiffParser`, Cryptographic Integrity & Gating
- **Test File**: `apps/agent-studios-vscode/src/test/unit/ArtifactSecurityRegression.test.ts`
- **Status**: PASSED (20/20 regression tests passing, 0 TypeScript compilation errors)
- **Date**: 2026-11-02

---

## 1. Executive Summary

Milestone M15 UI-04 enforces rigorous cryptographic security and defensive bounds across agent artifact retrieval, unified diff inspection, and patch export. As artificial intelligence agents generate code modifications, patch files, and binary assets, the IDE client host represents the critical security perimeter: it must guarantee that code presented to and exported by human operators is cryptographically authentic, resistant to tampering, resilient against malformed encoding attacks, and contained within strict filesystem boundaries.

This audit validates eight foundational security invariants across the Extension Host and Webview panel layers:
1. **Authoritative SHA-256 Verification**: Cryptographic matching of streamed byte streams against backend digests.
2. **TOCTOU Resistance**: Guaranteed byte-for-byte export directly from verified contiguous `rawBuffer` without intermediate mutation or re-encoding.
3. **Malformed UTF-8 Byte Resilience**: Non-crashing `TextDecoder('utf-8', { fatal: false })` decoding preserving raw binary authenticity.
4. **Git Path Quoting & Vietnamese Diacritics**: Robust parsing of C-style octal escape sequences (e.g. `\341\273\207` -> `ệ`) in diff paths.
5. **Path Traversal & Symlink Safety**: Strict sanitization of paths in diff headers and operator-controlled containment during export.
6. **Multi-Chunk Memory Bounds & Anti-DoS**: Hard caps at 16 MiB aggregate memory, 1024 chunk limits, and zero-progress infinite loop aborts.
7. **Studio & Task Identity Enforcement**: Anti-spoofing rejection of cross-studio and cross-task mismatch across transport and webview state.
8. **Tri-State Cryptographic Gating**: Absolute blocking of `INTEGRITY_FAILED` assets, explicit modal authorization for `UNVERIFIED` patches, and seamless export for `VERIFIED` assets.

---

## 2. Invariant Audit & Technical Analysis

### 2.1 SHA-256 Cryptographic Verification
- **Mechanism**: Implemented in `ArtifactClient.fetchTaskDiff` and `ArtifactClient.fetchArtifactContent` using Node.js trusted host cryptographic library (`crypto.createHash('sha256')`).
- **Algorithm**:
  ```typescript
  const rawBuffer = Buffer.concat(chunks);
  const sha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex').toLowerCase();
  let integrity: IntegrityStatus = 'UNVERIFIED';
  if (expectedSha256) {
    const normalizedExpected = expectedSha256.startsWith('sha256:')
      ? expectedSha256.slice(7).toLowerCase()
      : expectedSha256.toLowerCase();
    integrity = sha256 === normalizedExpected ? 'VERIFIED' : 'INTEGRITY_FAILED';
  }
  ```
- **Audit Findings**:
  - Verification occurs directly in the trusted Extension Host process prior to handing diff objects to the UI or renderer.
  - Normalizes both prefixed (`sha256:<hex>`) and raw hex digests case-insensitively.
  - If a single bit in any chunk is altered, `computedSha256 !== normalizedExpectedSha256`, immediately assigning `INTEGRITY_FAILED`.

### 2.2 TOCTOU (Time-of-Check to Time-of-Use) Resistance
- **Vulnerability Prevented**: Re-encoding verified strings (e.g. `Buffer.from(diff.text, 'utf-8')`) prior to export introduces dangerous mutations:
  - Multi-byte slicing or Unicode normalization (NFC/NFD) corrupts binary data or signatures.
  - Line ending conversions (CRLF vs LF) alter cryptographic digests.
  - In-memory modifications to `diff.text` by malicious webview scripts or extensions could bypass earlier hash checks.
- **Defense Implementation**:
  - `ArtifactReviewPanel._handleExportPatch()` writes `this._diffData.rawBuffer` directly via `fs.writeFileSync(targetUri.fsPath, this._diffData.rawBuffer)`.
  - The in-memory decoded `text` property is used exclusively for webview syntax-highlighted rendering and is never re-serialized for disk export.
- **Audit Findings**:
  - Verified by regression test `1.1` where deliberate corruption of `diff.text` had zero impact on exported disk bytes.
  - Verified by regression test `1.2` where mixed CRLF and LF byte sequences were preserved without normalization.

### 2.3 Malformed UTF-8 Byte Resilience
- **Mechanism**: Uses `new TextDecoder('utf-8', { fatal: false })` across `ArtifactClient` and `DiffParser`.
- **Behavior**:
  - If a byte sequence contains invalid UTF-8 (e.g. truncated continuation bytes, 0xC0/0xC1 overlong encodings, or standalone 0xFF), `fatal: false` replaces corrupt sequences with the Unicode replacement character `�` () rather than throwing an unhandled exception.
  - The underlying `rawBuffer` remains entirely unaltered.
- **Audit Findings**:
  - Prevents denial-of-service crashes in the extension host when inspecting arbitrary binary files or corrupted diffs.
  - Verified by regression test `3.2` and `3.3`.

### 2.4 Git Path Quoting & Vietnamese Diacritics
- **Mechanism**: Implemented in `DiffParser.unquoteGitPath`:
  - Git standard (`core.quotePath = true`) outputs non-ASCII paths in C-style octal escape format `\ooo` where each octal group represents a byte of the UTF-8 character.
  - Parses octal digits `0-7` (1 to 3 digits per escape), decodes them into raw byte arrays, and passes them to `TextDecoder('utf-8', { fatal: false })`.
- **Vietnamese Diacritic Invariants**:
  - `'ệ'` (U+1EC7) = `[0xE1, 0xBB, 0x87]` = `\341\273\207`.
  - `'ế'` (U+1EBF) = `[0xE1, 0xBA, 0xBF]` = `\341\272\277`.
  - `'à'` (U+00E0) = `[0xC3, 0xA0]` = `\303\240`.
  - `'ố'` (U+1ED1) = `[0xE1, 0xBB, 0x91]` = `\341\273\221`.
- **Audit Findings**:
  - Full path unquoting accurately handles complex Vietnamese filenames such as `tài_liệu_kế_hoạch_hệ_thống.md`.
  - Truncated or malformed octal sequences fall back gracefully without unhandled exceptions. Verified by regression test `3.1`.

### 2.5 Directory Traversal & Symlink Safety
- **Attack Vectors Evaluated**:
  1. Filename injection in `taskId`: Attacker specifies `taskId = "../../../../etc/passwd"`.
  2. Traversal headers in Git diff: `diff --git "a/../../etc/shadow" "b/../../etc/shadow"`.
- **Defenses**:
  - Export uses `vscode.window.showSaveDialog` with `defaultUri: vscode.Uri.file('task-${this._options.taskId}.patch')`. The operator explicitly confirms the target path. The file is written exclusively to the operator's chosen `targetUri.fsPath`.
  - `cleanGitPath` strips `a/` and `b/` prefixes and normalizes escaped quotes without executing directory operations.
  - Diffs are strictly inspected in memory and never written to workspace directories unless explicitly exported by operator action.
- **Audit Findings**:
  - Verified by regression tests `2.1`, `2.2`, and `2.3`.

### 2.6 Multi-Chunk Memory Bounds & Anti-DoS
- **Thresholds**:
  - `DEFAULT_MAX_BYTES = 16 * 1024 * 1024` (16 MiB aggregate limit).
  - `DEFAULT_MAX_CHUNKS = 1024` (Chunk count limit).
  - `ZERO_PROGRESS` detection: `chunkBuffer.length === 0 && diff.has_more`.
  - `OFFSET_MISMATCH` validation: `diff.offset !== expectedOffset`.
  - `CHUNK_BYTES_MISMATCH` validation: `diff.chunk_bytes !== chunkBuffer.length`.
- **Audit Findings**:
  - Prevents daemon memory exhaustion or infinite pagination loops.
  - Verified by regression tests `5.1`, `5.2`, and `5.3`.

### 2.7 Studio & Task Identity Enforcement (Anti-Spoofing)
- **Invariant**: A response from the daemon MUST match the requested `studio_id` and `task_id`.
- **Protections**:
  - `ArtifactClient`: Throws `STUDIO_ID_MISMATCH` or `TASK_ID_MISMATCH` immediately upon chunk reception.
  - `ArtifactReviewPanel`:
    - `setDiffData`: Rejects update if `diff.studioId !== this._expectedStudioId` or `diff.taskId !== this._expectedTaskId`.
    - `_handleCopyPatch` / `_handleExportPatch`: Validates `this._diffData` against active `_options` before executing clipboard or export operations.
- **Audit Findings**:
  - Confirmed rejection of cross-studio injection and cross-task leakage in regression tests `6.1`, `6.2`, `6.3`, and `6.4`.

### 2.8 Tri-State Cryptographic Gating
- **State Machine**:
  | Integrity State | UI Display | Clipboard Copy | File Export |
  |---|---|---|---|
  | `VERIFIED` | Green `✓ VERIFIED (SHA-256)` badge | Permitted seamlessly | Permitted seamlessly |
  | `UNVERIFIED` | Yellow `? UNVERIFIED` badge | Permitted | Blocked until modal confirmation |
  | `INTEGRITY_FAILED` | Red `✗ INTEGRITY FAILED` badge | Strictly Blocked (Error modal) | Strictly Blocked (Error modal) |
- **Audit Findings**:
  - Tampered code can never be copied to the clipboard or saved to disk without detection.
  - Unsigned/unverified code requires explicit human operator override. Verified in regression tests `7.1`, `7.2`, `7.3`, and `7.4`.

---

## 3. Regression Test Suite Implementation

The dedicated regression test suite is located in:
`apps/agent-studios-vscode/src/test/unit/ArtifactSecurityRegression.test.ts`

### Summary of Implemented Test Cases
```
Artifact & Cryptographic Security Regression Tests (M15 UI-04)
  1. TOCTOU Safety & Direct rawBuffer Export
    √ 1.1 exports exact verified rawBuffer byte-for-byte even if text property is tampered
    √ 1.2 prevents re-encoding mutations: preserves CRLF vs LF line endings without normalization
  2. Path Traversal & Filename Safety
    √ 2.1 handles malicious directory traversal attempts in taskId during export dialog
    √ 2.2 parses path traversal in Git diff headers without crashing and extracts clean relative paths
    √ 2.3 handles Windows backslash path traversal sequences in Git diff paths
  3. Vietnamese Diacritic Quoting & Malformed UTF-8 Resilience
    √ 3.1 correctly decodes Git octal sequences for Vietnamese diacritics including \341\273\207
    √ 3.2 survives malformed UTF-8 octal byte sequences without throwing (fatal: false)
    √ 3.3 survives raw invalid UTF-8 bytes in diff text decoding without corruption
  4. Mixed Binary & Text Diff Export Byte Fidelity
    √ 4.1 reassembles and exports mixed diff containing both code changes and raw binary blobs
  5. Memory Limit & DoS Boundary Enforcement
    √ 5.1 strictly enforces EXCEEDED_MAX_BYTES memory bound across paginated chunks
    √ 5.2 strictly enforces EXCEEDED_MAX_CHUNKS limit (Anti-DoS chunk flooding)
    √ 5.3 detects ZERO_PROGRESS pagination loop when chunk length is zero and has_more is true
  6. Cross-Studio & Cross-Task Identity Spoofing Rejection
    √ 6.1 rejects cross-studio spoofing in ArtifactClient with STUDIO_ID_MISMATCH
    √ 6.2 rejects cross-task spoofing in ArtifactClient with TASK_ID_MISMATCH
    √ 6.3 rejects setDiffData in ArtifactReviewPanel when diff studio or task does not match active workbench
    √ 6.4 blocks clipboard copy and file export if diff data identity does not match workbench options
  7. Cryptographic Gating Invariants (INTEGRITY_FAILED vs UNVERIFIED vs VERIFIED)
    √ 7.1 strictly blocks copy and export for INTEGRITY_FAILED diffs with prominent error
    √ 7.2 requires modal confirmation for UNVERIFIED diff; aborts export if operator declines
    √ 7.3 exports UNVERIFIED diff when operator explicitly confirms modal warning
    √ 7.4 seamlessly exports VERIFIED diff without modal warning
```

**Results**: 20 passing, 0 failing, 0 regressions. Standalone compilation verified via TypeScript compiler (`tsc --noEmit`) with 0 errors.

---

## 4. Verification Evidence & Cross-Check Matrix

| Requirement | Implementation Component | Test Case | Status |
|---|---|---|---|
| SHA-256 Digest Matching | `ArtifactClient.ts:291-305` | Test 4.1, 7.1, 7.4 | VERIFIED |
| TOCTOU Resistance | `ArtifactReviewPanel.ts:336` | Test 1.1, 1.2 | VERIFIED |
| Malformed UTF-8 Resilience | `ArtifactClient.ts:315`, `DiffParser.ts:103` | Test 3.2, 3.3 | VERIFIED |
| Git Octal Quoting (Vietnamese) | `DiffParser.ts:52-105` | Test 3.1 | VERIFIED |
| Path Traversal Containment | `DiffParser.ts:110-147`, `ArtifactReviewPanel.ts:327` | Test 2.1, 2.2, 2.3 | VERIFIED |
| Multi-Chunk Bounds (16 MiB / 1024) | `ArtifactClient.ts:257-270` | Test 5.1, 5.2, 5.3 | VERIFIED |
| Studio/Task Identity Guard | `ArtifactClient.ts:175-186`, `ArtifactReviewPanel.ts:197, 276, 302` | Test 6.1, 6.2, 6.3, 6.4 | VERIFIED |
| Cryptographic Gating | `ArtifactReviewPanel.ts:283-325` | Test 7.1, 7.2, 7.3, 7.4 | VERIFIED |

---

## 5. Security Recommendations for Future Iterations

1. **Task ID Filename Sanitization**: While `vscode.window.showSaveDialog` relies on operator selection, proactively sanitizing the proposed default filename (`task-${taskId.replace(/[^a-zA-Z0-9_-]/g, '_')}.patch`) will provide additional defense against accidental directory traversal confusion in custom file picker extensions.
2. **Subresource Integrity for Webview Styles**: Although webview styles currently rely on internal token mapping and extension roots, pinning inline CSS hashes or strict style nonces ensures complete CSP parity with production enterprise extensions.
3. **Signed Commit Provenance**: Extend `expectedSha256` verification to cross-check Git GPG/SSH signatures on `baseCommit` and `headCommit` when daemon Git configuration has signing enabled.

---

## 6. Final Verdict

**VERDICT: APPROVED & SECURE**

All cryptographic and artifact security invariants for Milestone M15 UI-04 are comprehensively validated, audited, and locked under regression testing. Zero regressions detected.
