# M15 UI-03.1 — Report 10: Async Task Switching & State Isolation

## Metadata
- **Milestone**: M15 UI-03.1: DIFF REVIEW CORRECTNESS, ASYNC SAFETY & REAL IPC VERIFICATION
- **Target PR**: Draft PR #27 (`feat/m15-real-agent-execution-ui`)
- **Backend Pinned Commit**: `cc1c3e8f398a5acfa24cf643cfcb2b043f6e163b` (PR #29 / M15-03.1)
- **Status**: VERIFIED & COMPLETE
- **Date**: 2026-11-02

---

## 1. Problem Statement & Root Cause

Rapid operator task switching in `ArtifactReviewPanel` caused out-of-order resolution bugs:
1. **Generation Collisions**: In-flight IPC diff retrieval for Task A could resolve after operator switched to Task B, overwriting Task B state with Task A diff.
2. **Ghost Errors**: Failed background fetch for discarded task displayed error banner over newly selected task.
3. **Cross-Task/Studio Pollution**: Reused webview panels retained stale file selections, scroll offsets, or mismatched diff payloads.
4. **Leaked In-Flight Requests**: Disposed review panels left active daemon IPC fetch promises uncancelled.

---

## 2. Technical Implementation

### A. Monotonic Generation Tracking & AbortController
In `src/webviews/ArtifactReviewPanel.ts`:
- Maintained private `_activeRequestId: number = 0`.
- Maintained `_activeAbortController: AbortController | null = null`.
- On task switch or refresh:
  1. Aborted active `_activeAbortController`.
  2. Replaced with new `AbortController`.
  3. Incremented `_activeRequestId` monotonically.
  4. Snapshot `_expectedStudioId` and `_expectedTaskId`.

### B. Immediate State Reset
When switching tasks (`createOrShow` with differing `taskId`):
- Immediately set `_diffData = null`, `_parsedDiff = null`, `_errorMessage = null`, `_selectedFileIndex = 0`.
- Set `_isLoading = true` if fetch loader function supplied.
- Rendered immediate loading spinner to prevent displaying stale diff.

### C. Generation & Identity Verification Gating
In `setDiffData(diff, requestId)` and `refresh()`:
- Silently discarded any payload or error whose `requestId !== this._activeRequestId`.
- Validated `diff.studioId === this._expectedStudioId` and `diff.taskId === this._expectedTaskId`.
- Rejected rogue or mismatched task diffs before webview message posting.

### D. Disposal Cleanup
In `dispose()`:
- Triggered `_activeAbortController?.abort()`.
- Cleared timer references and set `ArtifactReviewPanel.currentPanel = undefined`.

---

## 3. Verification & Evidence

`ArtifactReviewWorkbenchAsync.test.ts`:
- `1. increments active request generation ID and resets state when switching tasks` -> PASS
- `2. discards stale error message if request generation has moved forward` -> PASS
- `3. rejects setDiffData if studio or task ID does not match snapshot` -> PASS
- `8. blocks copy and export while panel is loading` -> PASS
- `9. aborts in-flight request on panel disposal` -> PASS
