/**
 * Agent Studios — Artifact Review Workbench Async Safety & State Isolation Tests
 * Milestone M15 UI-03.1: Task Switching, Request Generation Isolation & Cryptographic Gating
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { ReassembledTaskDiff } from '../../bridge/ipc/ArtifactClient';
import {
  ArtifactReviewOptions,
  ArtifactReviewPanel,
} from '../../webviews/ArtifactReviewPanel';
import {
  mockWindowMessages,
  resetMockState,
} from './vscodeMock';

describe('ArtifactReviewWorkbench Async Safety & State Isolation Tests', () => {
  const extensionUri = vscode.Uri.file('/mock/extension');
  let tempDir: string;

  beforeEach(() => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    resetMockState();
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-workbench-test-'));
  });

  afterEach(() => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    resetMockState();
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  });

  function createMockDiff(options: {
    studioId: string;
    taskId: string;
    rawText: string;
    integrity?: 'VERIFIED' | 'UNVERIFIED' | 'INTEGRITY_FAILED';
    isBinary?: boolean;
  }): ReassembledTaskDiff {
    const rawBuffer = Buffer.from(options.rawText, 'utf8');
    const sha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex');

    return {
      studioId: options.studioId,
      taskId: options.taskId,
      summary: {
        files_changed: 1,
        added_files: ['file.txt'],
        modified_files: [],
        deleted_files: [],
        renamed_files: [],
        binary_files: options.isBinary ? ['file.txt'] : [],
        patch_size_bytes: rawBuffer.length,
        base_commit: 'abc1234',
      },
      rawBuffer,
      text: options.rawText,
      isBinary: !!options.isBinary,
      sha256,
      expectedSha256: options.integrity === 'UNVERIFIED' ? null : sha256,
      integrity: options.integrity || 'VERIFIED',
      totalBytes: rawBuffer.length,
      chunkCount: 1,
    };
  }

  it('1. increments active request generation ID and resets state when switching tasks', async () => {
    const options1: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-1',
      taskTitle: 'Task One',
    };

    let resolveTask1: ((diff: ReassembledTaskDiff) => void) | null = null;
    const loadTask1 = () =>
      new Promise<ReassembledTaskDiff>((resolve) => {
        resolveTask1 = resolve;
      });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options1, loadTask1);
    const req1 = panel.activeRequestId;
    assert.strictEqual(panel.isLoading, true);
    assert.ok(req1 >= 1);

    // Switch to Task 2 before Task 1 finishes
    const options2: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-2',
      taskTitle: 'Task Two',
    };

    let resolveTask2: ((diff: ReassembledTaskDiff) => void) | null = null;
    const loadTask2 = () =>
      new Promise<ReassembledTaskDiff>((resolve) => {
        resolveTask2 = resolve;
      });

    ArtifactReviewPanel.createOrShow(extensionUri, options2, loadTask2);

    // Request ID must have incremented, old diff state reset immediately
    assert.strictEqual(panel.activeRequestId, req1 + 1);
    assert.strictEqual(panel.isLoading, true);
    assert.strictEqual(panel.diffData, null);
    assert.strictEqual(panel.expectedTaskId, 'task-2');

    // If Task 1 now finishes out of order, it must be discarded
    const diff1 = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-1',
      rawText: 'diff --git a/t1.txt b/t1.txt\n+t1\n',
    });
    resolveTask1!(diff1);
    await new Promise((r) => setTimeout(r, 10));

    // Stale Task 1 response must NOT be set
    assert.strictEqual(panel.diffData, null);

    // Task 2 resolves
    const diff2 = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-2',
      rawText: 'diff --git a/t2.txt b/t2.txt\n+t2\n',
    });
    resolveTask2!(diff2);
    await new Promise((r) => setTimeout(r, 10));

    assert.strictEqual(panel.isLoading, false);
    const resolvedData = panel.diffData as ReassembledTaskDiff | null;
    assert.strictEqual(resolvedData?.taskId, 'task-2');
    assert.strictEqual(resolvedData?.text, diff2.text);
  });

  it('2. discards stale error message if request generation has moved forward', async () => {
    const options1: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-1',
      taskTitle: 'Task One',
    };

    let rejectTask1: ((err: Error) => void) | null = null;
    const loadTask1 = () =>
      new Promise<ReassembledTaskDiff>((_resolve, reject) => {
        rejectTask1 = reject;
      });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options1, loadTask1);
    assert.strictEqual(panel.activeRequestId, 1);

    // Switch to Task 2
    const options2: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-2',
      taskTitle: 'Task Two',
    };
    const diff2 = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-2',
      rawText: 'diff --git a/ok.txt b/ok.txt\n+ok\n',
    });
    ArtifactReviewPanel.createOrShow(extensionUri, options2, async () => diff2);

    // Stale error rejected from Task 1
    rejectTask1!(new Error('Task 1 IPC timeout'));
    await new Promise((r) => setTimeout(r, 10));

    // Error from old generation must NOT be displayed
    assert.strictEqual(panel.errorMessage, null);
    const resolvedData2 = panel.diffData as ReassembledTaskDiff | null;
    assert.strictEqual(resolvedData2?.taskId, 'task-2');
  });

  it('3. rejects setDiffData if studio or task ID does not match snapshot', () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-expected',
      taskId: 'task-expected',
    };

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
    const mismatchedDiff = createMockDiff({
      studioId: 'studio-rogue',
      taskId: 'task-expected',
      rawText: 'diff --git a/a.txt b/a.txt\n+bad\n',
    });

    panel.setDiffData(mismatchedDiff, panel.activeRequestId);
    assert.strictEqual(panel.diffData, null, 'Must reject diff with mismatched studio identity');
  });

  it('4. strictly blocks copy and export when diff integrity is INTEGRITY_FAILED', async () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-tampered',
    };

    const tamperedDiff = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-tampered',
      rawText: 'diff --git a/a.txt b/a.txt\n+tampered\n',
      integrity: 'INTEGRITY_FAILED',
    });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
    panel.setDiffData(tamperedDiff, panel.activeRequestId);

    // Attempt copy
    await (panel as unknown as { _handleCopyPatch: () => Promise<void> })._handleCopyPatch();
    assert.ok(mockWindowMessages.error.some((e) => e.includes('Cryptographic SHA-256 integrity verification failed')));
    assert.strictEqual(mockWindowMessages.clipboardText, '');

    // Attempt export
    await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();
    assert.ok(mockWindowMessages.error.some((e) => e.includes('Cryptographic SHA-256 integrity verification failed')));
  });

  it('5. prompts modal warning for UNVERIFIED patch and aborts export when operator cancels', async () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-unverified',
    };

    const unverifiedDiff = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-unverified',
      rawText: 'diff --git a/unverified.txt b/unverified.txt\n+unverified\n',
      integrity: 'UNVERIFIED',
    });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
    panel.setDiffData(unverifiedDiff, panel.activeRequestId);

    // Operator declines confirmation
    mockWindowMessages.modalWarningResponse = 'Cancel';
    mockWindowMessages.saveDialogResult = vscode.Uri.file(path.join(tempDir, 'export.patch'));

    await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

    assert.ok(mockWindowMessages.warning.some((w) => w.includes('UNVERIFIED')));
    // File must NOT have been written
    assert.strictEqual(fs.existsSync(path.join(tempDir, 'export.patch')), false);
  });

  it('6. exports UNVERIFIED patch byte-for-byte when operator explicitly confirms modal warning', async () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-unverified',
    };

    const unverifiedDiff = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-unverified',
      rawText: 'diff --git a/unverified.txt b/unverified.txt\n+unverified\n',
      integrity: 'UNVERIFIED',
    });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
    panel.setDiffData(unverifiedDiff, panel.activeRequestId);

    const exportPath = path.join(tempDir, 'confirmed-unverified.patch');
    mockWindowMessages.modalWarningResponse = 'Export Unverified Patch';
    mockWindowMessages.saveDialogResult = vscode.Uri.file(exportPath);

    await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

    assert.strictEqual(fs.existsSync(exportPath), true);
    const exportedBytes = fs.readFileSync(exportPath);
    assert.deepStrictEqual(exportedBytes, unverifiedDiff.rawBuffer);
  });

  it('7. exports VERIFIED patch byte-for-byte without warning banner', async () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-verified',
    };

    const verifiedDiff = createMockDiff({
      studioId: 'studio-1',
      taskId: 'task-verified',
      rawText: 'diff --git a/v.txt b/v.txt\n+verified line\n',
      integrity: 'VERIFIED',
    });

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
    panel.setDiffData(verifiedDiff, panel.activeRequestId);

    const exportPath = path.join(tempDir, 'verified.patch');
    mockWindowMessages.saveDialogResult = vscode.Uri.file(exportPath);

    await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

    // No warning message for verified diff
    assert.strictEqual(mockWindowMessages.warning.length, 0);
    assert.strictEqual(fs.existsSync(exportPath), true);
    const exportedBytes = fs.readFileSync(exportPath);
    assert.deepStrictEqual(exportedBytes, verifiedDiff.rawBuffer);
  });

  it('8. blocks copy and export while panel is loading', async () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-loading',
    };

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options, () => new Promise(() => {}));
    assert.strictEqual(panel.isLoading, true);

    await (panel as unknown as { _handleCopyPatch: () => Promise<void> })._handleCopyPatch();
    assert.ok(mockWindowMessages.warning.some((w) => w.includes('Cannot copy patch while loading')));

    await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();
    assert.ok(mockWindowMessages.warning.some((w) => w.includes('Cannot export patch while loading')));
  });

  it('9. aborts in-flight request on panel disposal', () => {
    const options: ArtifactReviewOptions = {
      studioId: 'studio-1',
      taskId: 'task-dispose',
    };

    const panel = ArtifactReviewPanel.createOrShow(extensionUri, options, () => new Promise(() => {}));
    const abortSignalCaptured = (panel as unknown as { _activeAbortController: AbortController | null })._activeAbortController?.signal;

    assert.ok(abortSignalCaptured !== undefined);
    assert.strictEqual(abortSignalCaptured.aborted, false);

    panel.dispose();
    assert.strictEqual(abortSignalCaptured.aborted, true);
    assert.strictEqual(ArtifactReviewPanel.currentPanel, undefined);
  });
});
