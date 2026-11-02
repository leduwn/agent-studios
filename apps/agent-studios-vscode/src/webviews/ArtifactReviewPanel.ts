/**
 * Agent Studios — Artifact & Diff Review Workbench Webview Panel
 * Milestone M15 UI-03: Real Read-Only Artifact & Git Diff Inspection
 *
 * Implements a native VS Code panel displaying:
 * - A. Task & Studio Summary (genuine identity, worktree/run state, capture status)
 * - B. Change Summary (total files, additions, deletions, patch size, base commit, SHA-256 integrity)
 * - C. File Explorer (grouped by added/modified/deleted/renamed/binary, filterable, selectable)
 * - D. Unified Diff Viewer (truthful syntax-highlighted hunks, Unicode safe, binary indicator)
 * - Host Actions: Copy Patch to clipboard, Export Patch to file
 *
 * Security: Strictly read-only, strict CSP, no arbitrary file system writes into workspace.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as vscode from 'vscode';
import { ReassembledTaskDiff } from '../bridge/ipc/ArtifactClient';
import { DiffParser, ParsedDiff, ParsedDiffFile } from '../cards/DiffParser';
import { escapeHtml } from './markdownRenderer';

export interface ArtifactReviewOptions {
  studioId: string;
  taskId: string;
  taskTitle?: string;
  taskStatus?: string;
  runId?: string | null;
  worktreeId?: string | null;
  captureStatus?: string;
  retainedReason?: string | null;
}

export class ArtifactReviewPanel implements vscode.Disposable {
  public static readonly viewType = 'agent-studios.artifactReview';
  public static currentPanel: ArtifactReviewPanel | undefined;

  private readonly _panel: vscode.WebviewPanel;
  public readonly extensionUri: vscode.Uri;
  private _disposables: vscode.Disposable[] = [];
  private _options: ArtifactReviewOptions;
  private _expectedStudioId: string;
  private _expectedTaskId: string;
  private _activeRequestId: number = 0;
  private _activeAbortController: AbortController | null = null;
  private _diffData: ReassembledTaskDiff | null = null;
  private _parsedDiff: ParsedDiff | null = null;
  private _selectedFileIndex: number = 0;
  private _filterText: string = '';
  private _errorMessage: string | null = null;
  private _isLoading: boolean = false;
  private _onRefreshCallback?: () => Promise<ReassembledTaskDiff>;

  private constructor(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    options: ArtifactReviewOptions,
    onRefresh?: () => Promise<ReassembledTaskDiff>
  ) {
    this._panel = panel;
    this.extensionUri = extensionUri;
    this._options = options;
    this._expectedStudioId = options.studioId;
    this._expectedTaskId = options.taskId;
    this._onRefreshCallback = onRefresh;

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    this._panel.webview.onDidReceiveMessage(
      async (message) => {
        switch (message.type) {
          case 'selectFile':
            this._selectedFileIndex = message.index;
            this._updateWebview();
            break;
          case 'filter':
            this._filterText = message.text || '';
            this._selectedFileIndex = 0;
            this._updateWebview();
            break;
          case 'copyPatch':
            await this._handleCopyPatch();
            break;
          case 'exportPatch':
            await this._handleExportPatch();
            break;
          case 'refresh':
            await this.refresh();
            break;
        }
      },
      null,
      this._disposables
    );

    this._updateWebview();
  }

  public get isLoading(): boolean {
    return this._isLoading;
  }

  public get activeRequestId(): number {
    return this._activeRequestId;
  }

  public get diffData(): ReassembledTaskDiff | null {
    return this._diffData;
  }

  public get expectedTaskId(): string {
    return this._expectedTaskId;
  }

  public get errorMessage(): string | null {
    return this._errorMessage;
  }

  public static createOrShow(
    extensionUri: vscode.Uri,
    options: ArtifactReviewOptions,
    loadDiffFn?: () => Promise<ReassembledTaskDiff>
  ): ArtifactReviewPanel {
    const column = vscode.window.activeTextEditor
      ? vscode.window.activeTextEditor.viewColumn
      : undefined;

    if (ArtifactReviewPanel.currentPanel) {
      const panel = ArtifactReviewPanel.currentPanel;
      panel._panel.reveal(column || vscode.ViewColumn.Beside);

      const isSwitchingTask =
        panel._options.taskId !== options.taskId ||
        panel._options.studioId !== options.studioId;

      if (isSwitchingTask) {
        // Abort in-flight retrieval for previous task
        if (panel._activeAbortController) {
          panel._activeAbortController.abort();
          panel._activeAbortController = null;
        }
        if (!loadDiffFn) {
          panel._activeRequestId++;
        }

        // Reset diff state immediately so stale diff data is never displayed
        panel._diffData = null;
        panel._parsedDiff = null;
        panel._selectedFileIndex = 0;
        panel._errorMessage = null;
        panel._isLoading = !!loadDiffFn;
      }

      panel._options = options;
      panel._expectedStudioId = options.studioId;
      panel._expectedTaskId = options.taskId;
      panel._onRefreshCallback = loadDiffFn;

      if (loadDiffFn) {
        panel.refresh();
      } else {
        panel._updateWebview();
      }
      return panel;
    }

    const panel = vscode.window.createWebviewPanel(
      ArtifactReviewPanel.viewType,
      `Diff: ${options.taskTitle || options.taskId}`,
      column || vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [extensionUri],
      }
    );

    ArtifactReviewPanel.currentPanel = new ArtifactReviewPanel(
      panel,
      extensionUri,
      options,
      loadDiffFn
    );

    if (loadDiffFn) {
      ArtifactReviewPanel.currentPanel.refresh();
    }

    return ArtifactReviewPanel.currentPanel;
  }

  public setDiffData(diff: ReassembledTaskDiff, requestId?: number): void {
    if (requestId !== undefined && requestId !== this._activeRequestId) {
      return;
    }
    if (diff.studioId !== this._expectedStudioId || diff.taskId !== this._expectedTaskId) {
      return;
    }
    this._diffData = diff;
    this._parsedDiff = DiffParser.parseUnifiedDiff(diff.text);
    this._selectedFileIndex = 0;
    this._errorMessage = null;
    this._isLoading = false;
    this._updateWebview();
  }

  public setError(message: string, requestId?: number): void {
    if (requestId !== undefined && requestId !== this._activeRequestId) {
      return;
    }
    this._errorMessage = message;
    this._isLoading = false;
    this._updateWebview();
  }

  public async refresh(): Promise<void> {
    if (!this._onRefreshCallback) {
      return;
    }

    // Abort any prior in-flight fetch
    if (this._activeAbortController) {
      this._activeAbortController.abort();
    }
    const abortController = new AbortController();
    this._activeAbortController = abortController;
    const currentRequestId = ++this._activeRequestId;
    const targetStudioId = this._options.studioId;
    const targetTaskId = this._options.taskId;

    this._isLoading = true;
    this._errorMessage = null;
    this._updateWebview();

    try {
      const diff = await this._onRefreshCallback();

      // Guard: Ignore stale response if request ID changed or was aborted
      if (currentRequestId !== this._activeRequestId || abortController.signal.aborted) {
        return;
      }

      // Guard: Validate diff identity matches snapshot
      if (diff.studioId !== targetStudioId || diff.taskId !== targetTaskId) {
        this.setError(
          `Received diff for task ${diff.taskId} in studio ${diff.studioId}, but current view is task ${targetTaskId} in studio ${targetStudioId}.`,
          currentRequestId
        );
        return;
      }

      this.setDiffData(diff, currentRequestId);
    } catch (err: unknown) {
      // Guard: Ignore stale errors if request ID changed or was aborted
      if (currentRequestId !== this._activeRequestId || abortController.signal.aborted) {
        return;
      }
      this.setError((err as Error).message || 'Failed to fetch task diff.', currentRequestId);
    } finally {
      if (this._activeAbortController === abortController) {
        this._activeAbortController = null;
      }
    }
  }

  private async _handleCopyPatch(): Promise<void> {
    if (this._isLoading) {
      vscode.window.showWarningMessage('Cannot copy patch while loading.');
      return;
    }
    if (!this._diffData || !this._diffData.text) {
      vscode.window.showWarningMessage('No diff content available to copy.');
      return;
    }
    if (
      this._diffData.studioId !== this._options.studioId ||
      this._diffData.taskId !== this._options.taskId
    ) {
      vscode.window.showErrorMessage('Diff data does not match active task identity.');
      return;
    }
    if (this._diffData.integrity === 'INTEGRITY_FAILED') {
      vscode.window.showErrorMessage(
        'Cannot copy patch: Cryptographic SHA-256 integrity verification failed.'
      );
      return;
    }
    await vscode.env.clipboard.writeText(this._diffData.text);
    vscode.window.showInformationMessage('Unified patch copied to clipboard.');
  }

  private async _handleExportPatch(): Promise<void> {
    if (this._isLoading) {
      vscode.window.showWarningMessage('Cannot export patch while loading.');
      return;
    }
    if (!this._diffData || !this._diffData.rawBuffer) {
      vscode.window.showWarningMessage('No patch data available to export.');
      return;
    }
    if (
      this._diffData.studioId !== this._options.studioId ||
      this._diffData.taskId !== this._options.taskId
    ) {
      vscode.window.showErrorMessage('Diff data does not match active task identity.');
      return;
    }
    if (this._diffData.integrity === 'INTEGRITY_FAILED') {
      vscode.window.showErrorMessage(
        'Cannot export patch: Cryptographic SHA-256 integrity verification failed.'
      );
      return;
    }

    if (this._diffData.integrity === 'UNVERIFIED') {
      const choice = await vscode.window.showWarningMessage(
        'Warning: This patch is UNVERIFIED (daemon did not provide authoritative SHA-256 hash). Export anyway?',
        { modal: true },
        'Export Unverified Patch'
      );
      if (choice !== 'Export Unverified Patch') {
        return;
      }
    }

    const defaultUri = vscode.Uri.file(`task-${this._options.taskId}.patch`);
    const targetUri = await vscode.window.showSaveDialog({
      defaultUri,
      filters: { 'Patch Files': ['patch', 'diff'], 'All Files': ['*'] },
      title: 'Export Read-Only Unified Patch',
    });

    if (targetUri) {
      try {
        fs.writeFileSync(targetUri.fsPath, this._diffData.rawBuffer);
        vscode.window.showInformationMessage(`Patch successfully exported to: ${targetUri.fsPath}`);
      } catch (err: unknown) {
        vscode.window.showErrorMessage(`Failed to export patch: ${(err as Error).message}`);
      }
    }
  }

  private _updateWebview(): void {
    this._panel.title = `Diff: ${this._options.taskTitle || this._options.taskId}`;
    this._panel.webview.html = this._getHtmlForWebview();
  }

  private _getHtmlForWebview(): string {
    const nonce = crypto.randomBytes(16).toString('hex');
    const webview = this._panel.webview;

    const files = this._parsedDiff?.files || [];
    const filteredFiles = this._filterText
      ? files.filter((f) => f.displayPath.toLowerCase().includes(this._filterText.toLowerCase()))
      : files;

    const safeIndex = Math.min(
      Math.max(0, this._selectedFileIndex),
      Math.max(0, filteredFiles.length - 1)
    );
    const selectedFile: ParsedDiffFile | undefined = filteredFiles[safeIndex];

    // Status badge colors and labels
    const integrityBadgeHtml = this._diffData
      ? this._diffData.integrity === 'VERIFIED'
        ? `<span class="badge verified" title="SHA-256 digest matches authoritative backend hash: ${escapeHtml(this._diffData.sha256)}">✓ VERIFIED (SHA-256)</span>`
        : this._diffData.integrity === 'INTEGRITY_FAILED'
        ? `<span class="badge failed" title="SHA-256 mismatch! Expected: ${escapeHtml(this._diffData.expectedSha256 || '')}, got: ${escapeHtml(this._diffData.sha256)}">✗ INTEGRITY FAILED</span>`
        : `<span class="badge unverified" title="Authoritative patch hash was not provided by daemon">? UNVERIFIED</span>`
      : '';

    const captureStatusHtml = this._options.retainedReason
      ? `<div class="warning-banner">⚠️ <strong>Worktree Retained:</strong> ${escapeHtml(this._options.retainedReason)} (Artifact capture incomplete)</div>`
      : '';

    const isActionsDisabled = this._isLoading || !this._diffData;

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios — Artifact & Diff Review</title>
  <style>
    :root {
      --bg: var(--vscode-editor-background);
      --fg: var(--vscode-editor-foreground);
      --sidebar-bg: var(--vscode-sideBar-background);
      --border: var(--vscode-panel-border, #333);
      --add-bg: rgba(46, 160, 67, 0.15);
      --add-fg: #3fb950;
      --del-bg: rgba(248, 81, 73, 0.15);
      --del-fg: #f85149;
      --hunk-bg: rgba(56, 139, 253, 0.12);
      --hunk-fg: #58a6ff;
      --selected-bg: var(--vscode-list-activeSelectionBackground, #094771);
      --hover-bg: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    body {
      margin: 0;
      padding: 0;
      background: var(--bg);
      color: var(--fg);
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      height: 100vh;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    header {
      padding: 12px 16px;
      border-bottom: 1px solid var(--border);
      background: var(--sidebar-bg);
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .header-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    .header-title {
      font-weight: 600;
      font-size: 1.1em;
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .header-actions {
      display: flex;
      gap: 8px;
    }
    .btn {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border: none;
      padding: 4px 10px;
      cursor: pointer;
      font-size: 0.85em;
      border-radius: 2px;
    }
    .btn:hover:not(:disabled) {
      background: var(--vscode-button-hoverBackground);
    }
    .btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .btn.secondary {
      background: var(--vscode-button-secondaryBackground);
      color: var(--vscode-button-secondaryForeground);
    }
    .btn.secondary:hover:not(:disabled) {
      background: var(--vscode-button-secondaryHoverBackground);
    }
    .metadata-bar {
      display: flex;
      gap: 16px;
      font-size: 0.85em;
      color: var(--vscode-descriptionForeground);
      align-items: center;
      flex-wrap: wrap;
    }
    .badge {
      padding: 2px 6px;
      border-radius: 3px;
      font-size: 0.75em;
      font-weight: 600;
      text-transform: uppercase;
    }
    .badge.verified {
      background: rgba(46, 160, 67, 0.2);
      color: var(--add-fg);
      border: 1px solid var(--add-fg);
    }
    .badge.failed {
      background: rgba(248, 81, 73, 0.2);
      color: var(--del-fg);
      border: 1px solid var(--del-fg);
    }
    .badge.unverified {
      background: rgba(187, 128, 9, 0.2);
      color: #d29922;
      border: 1px solid #d29922;
    }
    .badge.added { color: var(--add-fg); }
    .badge.modified { color: #d29922; }
    .badge.deleted { color: var(--del-fg); }
    .badge.renamed { color: #58a6ff; }
    .badge.binary { color: #a371f7; }
    .warning-banner {
      background: rgba(187, 128, 9, 0.15);
      border-left: 3px solid #d29922;
      padding: 6px 10px;
      font-size: 0.85em;
    }
    .main-content {
      display: flex;
      flex: 1;
      overflow: hidden;
    }
    .sidebar {
      width: 300px;
      border-right: 1px solid var(--border);
      background: var(--sidebar-bg);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }
    .filter-box {
      padding: 8px;
      border-bottom: 1px solid var(--border);
    }
    .filter-input {
      width: 100%;
      box-sizing: border-box;
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      border: 1px solid var(--vscode-input-border);
      padding: 4px 8px;
      font-size: 0.85em;
      border-radius: 2px;
    }
    .file-list {
      list-style: none;
      margin: 0;
      padding: 0;
      overflow-y: auto;
      flex: 1;
    }
    .file-item {
      padding: 6px 12px;
      cursor: pointer;
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-bottom: 1px solid rgba(255, 255, 255, 0.03);
    }
    .file-item:hover {
      background: var(--hover-bg);
    }
    .file-item.active {
      background: var(--selected-bg);
      color: #fff;
    }
    .file-name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      margin-right: 8px;
    }
    .file-counts {
      font-size: 0.8em;
      display: flex;
      gap: 4px;
    }
    .file-counts .add { color: var(--add-fg); }
    .file-counts .del { color: var(--del-fg); }
    .diff-container {
      flex: 1;
      overflow-y: auto;
      background: var(--bg);
      display: flex;
      flex-direction: column;
    }
    .diff-header {
      padding: 8px 16px;
      background: var(--sidebar-bg);
      border-bottom: 1px solid var(--border);
      font-weight: 600;
      font-size: 0.9em;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    .diff-body {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: var(--vscode-editor-font-size, 12px);
      line-height: 1.45;
      padding: 8px 0;
      white-space: pre-wrap;
      word-break: break-all;
    }
    .diff-line {
      display: flex;
      padding: 0 12px;
    }
    .diff-line.addition {
      background: var(--add-bg);
      color: var(--add-fg);
    }
    .diff-line.deletion {
      background: var(--del-bg);
      color: var(--del-fg);
    }
    .diff-line.header {
      background: var(--hunk-bg);
      color: var(--hunk-fg);
      font-weight: bold;
    }
    .diff-line.context {
      color: var(--fg);
    }
    .binary-notice {
      padding: 24px;
      text-align: center;
      color: var(--vscode-descriptionForeground);
      font-style: italic;
    }
    .raw-notice {
      padding: 8px 12px;
      font-style: italic;
      color: var(--hunk-fg);
    }
    .raw-patch {
      margin: 8px 12px;
      padding: 12px;
      background: var(--sidebar-bg);
      border: 1px solid var(--border);
      border-radius: 4px;
      overflow-x: auto;
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: var(--vscode-editor-font-size, 12px);
      line-height: 1.45;
    }
    .empty-state {
      padding: 32px;
      text-align: center;
      color: var(--vscode-descriptionForeground);
    }
  </style>
</head>
<body>
  <header>
    <div class="header-row">
      <div class="header-title">
        <span>Task Artifact & Git Diff: ${escapeHtml(this._options.taskTitle || this._options.taskId)}</span>
        ${integrityBadgeHtml}
      </div>
      <div class="header-actions">
        <button class="btn secondary" id="refreshBtn">Refresh</button>
        <button class="btn secondary" id="copyPatchBtn" ${isActionsDisabled ? 'disabled' : ''}>Copy Patch</button>
        <button class="btn primary" id="exportPatchBtn" ${isActionsDisabled ? 'disabled' : ''}>Export Patch</button>
      </div>
    </div>
    <div class="metadata-bar">
      <span><strong>Task ID:</strong> ${escapeHtml(this._options.taskId)}</span>
      <span><strong>Studio ID:</strong> ${escapeHtml(this._options.studioId)}</span>
      ${this._options.taskStatus ? `<span><strong>Status:</strong> ${escapeHtml(this._options.taskStatus)}</span>` : ''}
      ${this._options.runId ? `<span><strong>Run:</strong> ${escapeHtml(this._options.runId)}</span>` : ''}
      ${this._options.worktreeId ? `<span><strong>Worktree:</strong> ${escapeHtml(this._options.worktreeId)}</span>` : ''}
      ${this._parsedDiff ? `<span><strong>Changes:</strong> ${this._parsedDiff.totalFiles} files (+${this._parsedDiff.totalAdditions} -${this._parsedDiff.totalDeletions})</span>` : ''}
      ${this._diffData?.totalBytes ? `<span><strong>Size:</strong> ${(this._diffData.totalBytes / 1024).toFixed(1)} KB</span>` : ''}
    </div>
    ${captureStatusHtml}
  </header>

  <div class="main-content">
    <div class="sidebar">
      <div class="filter-box">
        <input type="text" id="filterInput" class="filter-input" placeholder="Filter changed files..." value="${escapeHtml(this._filterText)}">
      </div>
      <ul class="file-list">
        ${
          filteredFiles.length > 0
            ? filteredFiles
                .map((file, idx) => {
                  const isActive = file === selectedFile;
                  const kindBadge =
                    file.changeKind === 'added'
                      ? '<span class="badge added">[A]</span>'
                      : file.changeKind === 'deleted'
                      ? '<span class="badge deleted">[D]</span>'
                      : file.changeKind === 'renamed'
                      ? '<span class="badge renamed">[R]</span>'
                      : file.changeKind === 'binary'
                      ? '<span class="badge binary">[BIN]</span>'
                      : file.changeKind === 'unknown'
                      ? '<span class="badge unverified">[RAW]</span>'
                      : '<span class="badge modified">[M]</span>';

                  return `<li class="file-item ${isActive ? 'active' : ''}" data-index="${idx}">
                    <span class="file-name" title="${escapeHtml(file.displayPath)}">${kindBadge} ${escapeHtml(file.displayPath)}</span>
                    <span class="file-counts">
                      ${file.additions > 0 ? `<span class="add">+${file.additions}</span>` : ''}
                      ${file.deletions > 0 ? `<span class="del">-${file.deletions}</span>` : ''}
                    </span>
                  </li>`;
                })
                .join('')
            : '<li class="empty-state" style="padding: 16px;">No changed files found</li>'
        }
      </ul>
    </div>

    <div class="diff-container">
      ${
        this._isLoading
          ? '<div class="empty-state">Loading artifact diff from daemon...</div>'
          : this._errorMessage
          ? `<div class="empty-state" style="color: var(--del-fg);">⚠️ ${escapeHtml(this._errorMessage)}</div>`
          : selectedFile
          ? `<div class="diff-header">
              <span>${escapeHtml(selectedFile.displayPath)}</span>
              <span>+${selectedFile.additions} -${selectedFile.deletions}</span>
            </div>
            <div class="diff-body">
              ${
                selectedFile.isBinary
                  ? '<div class="binary-notice">📦 Binary Git Patch — Raw byte modifications captured. Text diff not displayed.</div>'
                  : selectedFile.changeKind === 'unknown' || (selectedFile.hunks.length === 0 && selectedFile.rawPatch)
                  ? `<div class="raw-notice">📄 Raw Unclassified Patch Content:</div><pre class="raw-patch"><code>${escapeHtml(selectedFile.rawPatch)}</code></pre>`
                  : selectedFile.hunks.length > 0
                  ? selectedFile.hunks
                      .map((hunk) => {
                        const hunkHeader = `<div class="diff-line header">${escapeHtml(hunk.header)}</div>`;
                        const linesHtml = hunk.lines
                          .map((l) => {
                            if (l.type === 'header') {
                              return '';
                            }
                            const lineClass = l.type;
                            return `<div class="diff-line ${lineClass}">${escapeHtml(l.text)}</div>`;
                          })
                          .join('');
                        return hunkHeader + linesHtml;
                      })
                      .join('')
                  : '<div class="empty-state">No line modifications recorded for this file.</div>'
              }
            </div>`
          : '<div class="empty-state">Select a file from the list to inspect its unified diff.</div>'
      }
    </div>
  </div>

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();

    document.getElementById('copyPatchBtn')?.addEventListener('click', () => {
      vscode.postMessage({ type: 'copyPatch' });
    });

    document.getElementById('exportPatchBtn')?.addEventListener('click', () => {
      vscode.postMessage({ type: 'exportPatch' });
    });

    document.getElementById('refreshBtn')?.addEventListener('click', () => {
      vscode.postMessage({ type: 'refresh' });
    });

    const filterInput = document.getElementById('filterInput');
    filterInput?.addEventListener('input', (e) => {
      vscode.postMessage({ type: 'filter', text: e.target.value });
    });

    document.querySelectorAll('.file-item').forEach((item) => {
      item.addEventListener('click', () => {
        const idx = parseInt(item.getAttribute('data-index') || '0', 10);
        vscode.postMessage({ type: 'selectFile', index: idx });
      });
    });
  </script>
</body>
</html>`;
  }

  public dispose(): void {
    if (this._activeAbortController) {
      this._activeAbortController.abort();
      this._activeAbortController = null;
    }
    this._activeRequestId++;
    this._diffData = null;
    this._parsedDiff = null;
    this._onRefreshCallback = undefined;
    ArtifactReviewPanel.currentPanel = undefined;
    this._panel.dispose();
    while (this._disposables.length) {
      const d = this._disposables.pop();
      if (d) {
        d.dispose();
      }
    }
  }
}
