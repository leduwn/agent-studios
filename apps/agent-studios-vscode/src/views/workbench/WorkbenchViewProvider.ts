/**
 * Agent Studios — Workbench View Provider
 * Milestone M15 UI-05A: Real Agent Execution UI & Artifact Review Workbench
 *
 * Coordinates artifact and diff review in the workbench:
 * - Active studio and task lifecycle with automatic cache invalidation
 * - In-flight async cancellation via AbortController
 * - Cryptographic SHA-256 integrity export gating (blocks on INTEGRITY_FAILED)
 * - Lossless byte buffer exports preserving exact line endings (CRLF/LF)
 * - Path traversal sanitization for export filenames and diffs
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as fs from 'fs';
import * as vscode from 'vscode';
import {
  ArtifactClient,
  FetchOptions,
  IntegrityStatus,
  IpcTransportSender,
  ReassembledTaskDiff,
} from '../../bridge/ipc/ArtifactClient';
import {
  DiffParser,
  isPathTraversal,
  ParsedDiff,
  sanitizeArtifactId,
} from './DiffParser';

export interface WorkbenchState {
  activeStudioId: string | null;
  activeTaskId: string | null;
  activeDiff: ReassembledTaskDiff | null;
  parsedDiff: ParsedDiff | null;
  isLoading: boolean;
  errorMessage: string | null;
}

export class WorkbenchViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  public static readonly viewType = 'agent-studios.workbench';

  private _view?: vscode.WebviewView;
  private _disposables: vscode.Disposable[] = [];
  private _activeStudioId: string | null = null;
  private _activeTaskId: string | null = null;
  private _activeAbortController: AbortController | null = null;
  private _activeDiff: ReassembledTaskDiff | null = null;
  private _parsedDiff: ParsedDiff | null = null;
  private _isLoading = false;
  private _errorMessage: string | null = null;

  constructor(private readonly _extensionUri?: vscode.Uri) {}

  public get activeStudioId(): string | null {
    return this._activeStudioId;
  }

  public get activeTaskId(): string | null {
    return this._activeTaskId;
  }

  public get activeDiff(): ReassembledTaskDiff | null {
    return this._activeDiff;
  }

  public get parsedDiff(): ParsedDiff | null {
    return this._parsedDiff;
  }

  public get isLoading(): boolean {
    return this._isLoading;
  }

  public get errorMessage(): string | null {
    return this._errorMessage;
  }

  /**
   * Sets the active studio and invalidates cached artifacts when studio changes.
   */
  public setStudio(studioId: string): void {
    if (this._activeStudioId && this._activeStudioId !== studioId) {
      // Invalidate artifact client cache for previous studio
      ArtifactClient.invalidateCache(this._activeStudioId);
    }
    this._abortInFlight();
    this._activeStudioId = studioId;
    this._activeTaskId = null;
    this._activeDiff = null;
    this._parsedDiff = null;
    this._errorMessage = null;
    this._updateWebview();
  }

  /**
   * Sets active task and aborts any in-flight download for previous task.
   */
  public setTask(taskId: string): void {
    this._abortInFlight();
    this._activeTaskId = taskId;
    this._activeDiff = null;
    this._parsedDiff = null;
    this._errorMessage = null;
    this._updateWebview();
  }

  /**
   * Checks whether the given diff can be exported to disk or clipboard.
   * Strictly blocks export when SHA-256 integrity status is INTEGRITY_FAILED.
   */
  public canExport(diff: ReassembledTaskDiff | { integrity: IntegrityStatus } | null): boolean {
    if (!diff) {
      return false;
    }
    if (diff.integrity === 'INTEGRITY_FAILED') {
      return false;
    }
    return true;
  }

  /**
   * Exports raw unmutated Buffer to target file path.
   * Guarantees:
   * - Strict export block on INTEGRITY_FAILED
   * - Rejection on path traversal in target file path
   * - Preserves CRLF/LF line endings byte-for-byte
   * - Never re-encodes through UTF-8
   */
  public async exportPatch(
    diff: ReassembledTaskDiff,
    targetFilePath: string
  ): Promise<void> {
    if (!this.canExport(diff)) {
      throw new Error(
        'Export blocked: Cryptographic SHA-256 integrity verification failed.'
      );
    }

    if (
      /[\x00-\x1f\x7f]/.test(targetFilePath) ||
      /(?:^|[/\\])\.\.(?:[/\\]|$)/.test(targetFilePath)
    ) {
      throw new Error(
        `Export blocked: Target path contains directory traversal sequence: '${targetFilePath}'`
      );
    }

    if (!diff.rawBuffer || diff.rawBuffer.length === 0) {
      throw new Error('Export failed: Empty or missing raw buffer.');
    }

    await fs.promises.writeFile(targetFilePath, diff.rawBuffer);
  }

  /**
   * Generates a safe default export filename for a task patch.
   */
  public getDefaultExportFilename(taskId: string): string {
    const safeId = sanitizeArtifactId(taskId);
    return `task-${safeId}.patch`;
  }

  /**
   * Loads task diff with clean in-flight cancellation and cache validation.
   */
  public async loadTaskDiff(
    transport: IpcTransportSender,
    studioId: string,
    taskId: string,
    options?: FetchOptions
  ): Promise<ReassembledTaskDiff> {
    if (isPathTraversal(studioId)) {
      throw new Error(`Path traversal detected in studioId: '${studioId}'`);
    }
    if (isPathTraversal(taskId)) {
      throw new Error(`Path traversal detected in taskId: '${taskId}'`);
    }

    // Abort prior in-flight request
    this._abortInFlight();
    const abortController = new AbortController();
    this._activeAbortController = abortController;

    this._isLoading = true;
    this._errorMessage = null;
    this._activeStudioId = studioId;
    this._activeTaskId = taskId;
    this._updateWebview();

    try {
      const diff = await ArtifactClient.fetchTaskDiff(transport, studioId, taskId, {
        ...options,
        signal: abortController.signal,
      });

      if (abortController.signal.aborted) {
        throw new Error('Task diff download was aborted by client.');
      }

      this._activeDiff = diff;
      this._parsedDiff = DiffParser.parseUnifiedDiff(diff.text, {
        rejectTraversal: true,
        sanitizePaths: true,
      });
      this._isLoading = false;
      this._updateWebview();
      return diff;
    } catch (err: unknown) {
      if (abortController.signal.aborted) {
        throw new Error('Task diff download was aborted by client.');
      }
      const message = err instanceof Error ? err.message : String(err);
      this._errorMessage = message;
      this._isLoading = false;
      this._updateWebview();
      throw err;
    } finally {
      if (this._activeAbortController === abortController) {
        this._activeAbortController = null;
      }
    }
  }

  /**
   * Resolves WebviewView from VS Code.
   */
  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): void {
    this._view = webviewView;
    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: this._extensionUri ? [this._extensionUri] : [],
    };
    this._updateWebview();
  }

  private _abortInFlight(): void {
    if (this._activeAbortController) {
      this._activeAbortController.abort();
      this._activeAbortController = null;
    }
  }

  private _updateWebview(): void {
    if (!this._view) {
      return;
    }
    this._view.webview.html = this._getHtmlForWebview();
  }

  private _getHtmlForWebview(): string {
    const diff = this._activeDiff;
    const status = diff ? diff.integrity : 'NO_DIFF';
    const statusColor =
      status === 'VERIFIED' ? '#4ec9b0' : status === 'INTEGRITY_FAILED' ? '#f14c4c' : '#cca700';

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Workbench</title>
  <style>
    body { font-family: var(--vscode-font-family); padding: 10px; color: var(--vscode-foreground); }
    .badge { padding: 2px 6px; border-radius: 3px; font-weight: bold; background: ${statusColor}; color: #000; }
  </style>
</head>
<body>
  <div>
    <h3>Workbench Review</h3>
    <p>Studio: ${this._activeStudioId || 'None'}</p>
    <p>Task: ${this._activeTaskId || 'None'}</p>
    <p>Status: <span class="badge">${status}</span></p>
    ${this._isLoading ? '<p>Loading diff...</p>' : ''}
    ${this._errorMessage ? `<p style="color: #f14c4c;">${this._errorMessage}</p>` : ''}
  </div>
</body>
</html>`;
  }

  public dispose(): void {
    this._abortInFlight();
    while (this._disposables.length > 0) {
      const disposable = this._disposables.pop();
      disposable?.dispose();
    }
  }
}
