# M15 UI-03.1 — Report 11: Artifact Integrity & Export Safety

## Metadata
- **Milestone**: M15 UI-03.1: DIFF REVIEW CORRECTNESS, ASYNC SAFETY & REAL IPC VERIFICATION
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Backend Pinned Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29 / M15-03.1)
- **Status**: VERIFIED & COMPLETE
- **Date**: 2026-11-02

---

## 1. Problem Statement & Root Cause

1. **Unprotected Tampered Patches**: If patch bytes did not match `patch_sha256` digest, previous implementation warned visually but allowed copy-to-clipboard and disk export, exposing developers to compromised code.
2. **Unconfirmed Export of Unsigned Patches**: Patches missing expected SHA-256 (`UNVERIFIED`) were exported without operator authorization.
3. **Invalid `vscode.diff` Usage**: Prior commands attempted `vscode.diff` between physical source files and unified patch text streams, causing garbled split diffs.
4. **Fabricated Studio IDs**: Missing active studio defaulted to `'default-studio'`, violating strict studio UUID validation in `agent-studios-rs`.

---

## 2. Technical Implementation

### A. Tri-State Cryptographic Gating
In `src/bridge/ipc/ArtifactClient.ts` & `src/webviews/ArtifactReviewPanel.ts`:
- Calculated true SHA-256 digest using Node.js `crypto.createHash('sha256')` over contiguous raw byte buffer.
- Assigned integrity state:
  - `VERIFIED`: `computedSha256 === normalizedExpectedSha256`.
  - `INTEGRITY_FAILED`: Expected SHA-256 present but computed digest does not match.
  - `UNVERIFIED`: No expected SHA-256 provided by backend daemon.

### B. Strict Export and Clipboard Controls
In `ArtifactReviewPanel.ts`:
- **`INTEGRITY_FAILED`**:
  - `_handleCopyPatch`: Blocks clipboard write and shows `vscode.window.showErrorMessage('Cryptographic SHA-256 integrity verification failed: patch has been tampered with or corrupted in transit. Export and clipboard copy are blocked.')`.
  - `_handleExportPatch`: Blocks file save dialog and displays identical security error.
- **`UNVERIFIED`**:
  - `_handleExportPatch`: Triggers modal confirmation prompt (`showWarningMessage` with `{ modal: true }`).
  - Halts export if operator selects `'Cancel'` or dismisses dialog.
  - Only writes to disk when operator explicitly selects `'Export Unverified Patch'`.
- **`VERIFIED`**:
  - Allows direct clipboard copy and file export.

### C. Byte-for-Byte Raw Buffer Export
Export writes `this._diffData.rawBuffer` directly via `fs.writeFileSync(saveUri.fsPath, this._diffData.rawBuffer)`.
Avoids string re-encoding, stripping trailing newlines, or converting CRLF/LF line endings.

### D. Corrected Diff Viewer & Studio Routing
In `src/commands/ArtifactCommands.ts`:
- Replaced invalid `vscode.diff` invocations.
- Routed task unified diffs to authoritative `agent-studios.openArtifactReview`.
- Routed standalone artifacts to virtual read-only document provider (`agent-studios-artifact://`).
- Eliminated `'default-studio'` fallback; rejects execution when no studio resolved.

---

## 3. Verification & Evidence

- `ArtifactReviewWorkbenchAsync.test.ts`:
  - `4. strictly blocks copy and export when diff integrity is INTEGRITY_FAILED` -> PASS
  - `5. prompts modal warning for UNVERIFIED patch and aborts export when operator cancels` -> PASS
  - `6. exports UNVERIFIED patch byte-for-byte when operator explicitly confirms modal warning` -> PASS
  - `7. exports VERIFIED patch byte-for-byte without warning banner` -> PASS
- `artifactCommands.test.ts`:
  - `7. fails closed when no studio is resolved` -> PASS
  - `8. routes task diff review directly to openArtifactReview` -> PASS
  - `9. opens standalone artifact in virtual document viewer` -> PASS
