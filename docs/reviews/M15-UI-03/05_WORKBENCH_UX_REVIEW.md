# M15 UI-03 — Workbench UX, Layout & Accessibility Review

## Metadata
- **Milestone**: M15 UI-03
- **Topic**: Artifact & Diff Review UX, Status Badges, Unified Diff Viewer & Entry Points
- **Date**: 2026-11-02

---

## 1. Visual Layout Architecture

The `ArtifactReviewPanel` provides a native VS Code two-pane workbench experience:

```
+----------------------------------------------------------------------------------------------------+
|  Agent Studios — Artifact & Diff Review: [Task Title]                                              |
+----------------------------------------------------------------------------------------------------+
|  Task: Analyze workspace (task-1) | Studio: default-studio | Status: SUCCEEDED                     |
|  Integrity: [VERIFIED (SHA-256: d559...)] | 2 Files Changed (+14, -3) | Size: 4.2 KiB              |
+----------------------------------+-----------------------------------------------------------------+
|  [Filter files...              ] |  Viewing: src/app.ts [MODIFIED] (+12, -2)   [Copy] [Export]     |
|                                  +-----------------------------------------------------------------+
|  [M] src/app.ts        (+12, -2) |  @@ -10,6 +10,16 @@ function bootstrap() {                     |
|  [A] src/types.ts       (+2, -0) |    const app = new App();                                          |
|  [BIN] media/logo.png    (1.4KB) |  - app.initialize();                                               |
|                                  |  + app.initialize({                                                |
|                                  |  +   strictMode: true,                                             |
|                                  |  + });                                                             |
|                                  |    return app;                                                     |
|                                  |  }                                                                 |
+----------------------------------+-----------------------------------------------------------------+
```

---

## 2. Key UX Components

1. **Header & Metadata Summary**:
   - Task title, Task ID, Studio ID, Task Status pill (`SUCCEEDED`, `RUNNING`, `FAILED`).
   - Cryptographic integrity badge:
     - `VERIFIED`: Green badge with truncated SHA-256 fingerprint.
     - `INTEGRITY_FAILED`: Red warning badge indicating hash mismatch.
     - `UNVERIFIED`: Neutral badge when backend hash is omitted.
   - Aggregate change metrics: Total files changed, total additions (`+`), total deletions (`-`), and patch size in KiB.

2. **File Explorer (Sidebar)**:
   - Instant search/filter box filtering files by path.
   - Status badges:
     - `[A]` (Green): Added file
     - `[M]` (Yellow/Orange): Modified file
     - `[D]` (Red): Deleted file
     - `[R]` (Blue): Renamed file
     - `[BIN]` (Purple): Binary Git patch
   - Per-file addition and deletion counters.
   - Active file selection state.

3. **Unified Diff Viewer**:
   - Syntax-highlighted diff rendering with line-by-line addition (`+`), deletion (`-`), context (` `), and hunk header (`@@ ... @@`) rows.
   - Non-monospace code box with native VS Code theme CSS variables (`--vscode-editor-background`, `--vscode-editor-foreground`, `--vscode-list-activeSelectionBackground`).
   - Dedicated Binary File indicator when files contain `GIT binary patch` or null bytes, displaying total byte size and notice that raw binary bytes cannot be displayed as text.

4. **Operator Host Actions**:
   - **Copy Raw Patch**: Copies the unified diff of the currently selected file into the system clipboard via `vscode.env.clipboard.writeText`.
   - **Export Patch**: Opens native save dialog to export the complete unified patch to disk (`vscode.window.showSaveDialog`).

---

## 3. Seamless Integration Entry Points

1. **Command Palette**: `Agent Studios: Review Task Artifacts & Diff` (`agent-studios.openArtifactReview`).
2. **Chat & Assistant**: Quick actions and messages can dispatch `viewTaskDiff` to inspect artifacts for the active or completed task.
3. **Mission Control Workbench**:
   - Task list table includes dedicated `Diff` action button next to `Details`.
   - Task inspector drawer includes prominent `Review Artifacts & Diff` button.
