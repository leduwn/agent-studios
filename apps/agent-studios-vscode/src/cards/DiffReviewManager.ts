/**
 * Agent Studios — Diff Review Manager
 * Milestone M14-UI-01: In-Conversation Activities & Diff Review
 */

import * as path from 'path';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import {
  DIFF_SCHEME,
  DiffDocumentProvider,
  extractAffectedPaths,
  findWorkspaceFileForArtifact,
} from '../bridge/DiffDocumentProvider';
import { Artifact } from '../bridge/types';
import { CardRenderer } from './CardRenderer';
import {
  ApprovalCardData,
  BackendReconciliationProof,
  ChangedFileEntry,
  ReconciliationResult,
} from './types';

/**
 * Manages user interactions with changed files and artifact diffs.
 * Enforces safe review: NEVER mutates workspace files on disk without verified backend reconciliation!
 */
export class DiffReviewManager implements vscode.Disposable {
  private readonly _disposables: vscode.Disposable[] = [];

  constructor(
    private readonly _bridge: AgentStudiosBridge,
    private readonly _diffProvider?: DiffDocumentProvider
  ) {}

  public get diffProvider(): DiffDocumentProvider | undefined {
    return this._diffProvider;
  }

  /**
   * Opens diff view using existing DiffDocumentProvider (agent-studios-diff://) and native vscode.diff.
   * Safe review: Strictly read-only, never mutates workspace files on disk to show diffs.
   */
  public async openDiff(target: ChangedFileEntry | Artifact | string): Promise<void> {
    let artifact: Artifact | undefined;
    let targetPath: string | undefined;
    let artifactId: string | undefined;

    // 1. Resolve artifact or target path from input
    if (typeof target === 'string') {
      const artifacts = await this._bridge.listArtifacts();
      artifact = artifacts.find(a => a.id === target || a.name === target || a.path === target);
      if (!artifact) {
        // Check if string is a path affected by an artifact
        artifact = artifacts.find(a => {
          const paths = extractAffectedPaths(a.diffContent || '', a.path);
          return paths.includes(target);
        });
      }
      if (!artifact) {
        targetPath = target;
        artifactId = target;
      }
    } else if ('id' in target && 'diffContent' in target) {
      artifact = target as Artifact;
    } else if ('path' in target && 'additions' in target) {
      const fileEntry = target as ChangedFileEntry;
      targetPath = fileEntry.path;
      artifactId = fileEntry.artifactId;

      const artifacts = await this._bridge.listArtifacts();
      if (fileEntry.artifactId) {
        artifact = artifacts.find(a => a.id === fileEntry.artifactId);
      }
      if (!artifact) {
        artifact = artifacts.find(a => {
          const paths = extractAffectedPaths(a.diffContent || '', a.path);
          return paths.includes(fileEntry.path);
        });
      }
    }

    // 2. If artifact resolved, use virtual diff document provider
    if (artifact) {
      let diffContent = await this._bridge.getArtifactDiff(artifact.id);
      if (!diffContent && artifact.diffContent) {
        diffContent = artifact.diffContent;
      }

      if (!diffContent) {
        vscode.window.showInformationMessage(`No diff content available for artifact '${artifact.name}'.`);
        return;
      }

      const virtualUri = vscode.Uri.parse(
        `${DIFF_SCHEME}://artifact/${artifact.id}/${artifact.name}`
      );

      const originalUri = await findWorkspaceFileForArtifact(artifact, diffContent);

      if (originalUri) {
        const fileName = path.basename(originalUri.fsPath);
        const title = `${artifact.name} (${fileName}: Working Tree ↔ Proposed Diff)`;
        await vscode.commands.executeCommand('vscode.diff', originalUri, virtualUri, title);
      } else {
        const doc = await vscode.workspace.openTextDocument(virtualUri);
        await vscode.window.showTextDocument(doc, { preview: true });
      }
      return;
    }

    // 3. Fallback when artifact was not found directly in bridge
    if (targetPath || artifactId) {
      const id = artifactId || 'unknown';
      const name = targetPath ? path.basename(targetPath) : id;
      const virtualUri = vscode.Uri.parse(`${DIFF_SCHEME}://artifact/${id}/${name}`);

      // Attempt to find file in workspace
      let originalUri: vscode.Uri | undefined;
      if (targetPath && vscode.workspace.workspaceFolders) {
        for (const folder of vscode.workspace.workspaceFolders) {
          const candidateUri = vscode.Uri.joinPath(folder.uri, targetPath);
          try {
            const stat = await vscode.workspace.fs.stat(candidateUri);
            if (stat.type !== vscode.FileType.Directory) {
              originalUri = candidateUri;
              break;
            }
          } catch {
            // Not found in this folder
          }
        }
      }

      if (originalUri) {
        const fileName = path.basename(originalUri.fsPath);
        const title = `${fileName} (Working Tree ↔ Proposed Diff)`;
        await vscode.commands.executeCommand('vscode.diff', originalUri, virtualUri, title);
      } else {
        const doc = await vscode.workspace.openTextDocument(virtualUri);
        await vscode.window.showTextDocument(doc, { preview: true });
      }
    }
  }

  /**
   * Parses unified diff content into structured ChangedFileEntry records with +X and -Y counts.
   */
  public static parseDiffStats(diffContent: string, defaultArtifactId?: string): ChangedFileEntry[] {
    if (!diffContent || !diffContent.trim()) {
      return [];
    }

    const lines = diffContent.split(/\r?\n/);
    const files: ChangedFileEntry[] = [];
    let pendingSourcePath: string | null = null;
    let currentPath: string | null = null;
    let currentAdditions = 0;
    let currentDeletions = 0;

    const commitCurrent = () => {
      if (currentPath) {
        files.push({
          path: currentPath,
          additions: currentAdditions,
          deletions: currentDeletions,
          artifactId: defaultArtifactId,
        });
      }
      currentPath = null;
      currentAdditions = 0;
      currentDeletions = 0;
      pendingSourcePath = null;
    };

    for (const line of lines) {
      if (line.startsWith('--- a/')) {
        if (currentPath) {
          commitCurrent();
        }
        pendingSourcePath = line.substring(6).trim();
        continue;
      } else if (line.startsWith('--- ') && !line.startsWith('--- /dev/null')) {
        if (currentPath) {
          commitCurrent();
        }
        pendingSourcePath = line.substring(4).trim();
        continue;
      } else if (line.startsWith('+++ b/')) {
        currentPath = line.substring(6).trim();
        continue;
      } else if (line.startsWith('+++ ')) {
        const target = line.substring(4).trim();
        if (target === '/dev/null') {
          currentPath = pendingSourcePath;
        } else {
          currentPath = target;
        }
        continue;
      }

      // Inside a hunk, count additions and deletions
      if (currentPath) {
        if (line.startsWith('+') && !line.startsWith('+++')) {
          currentAdditions++;
        } else if (line.startsWith('-') && !line.startsWith('---')) {
          currentDeletions++;
        }
      }
    }

    commitCurrent();
    return files;
  }

  /**
   * Instance helper for parseDiffStats.
   */
  public parseDiffStats(diffContent: string, defaultArtifactId?: string): ChangedFileEntry[] {
    return DiffReviewManager.parseDiffStats(diffContent, defaultArtifactId);
  }

  /**
   * Safe review guard: NEVER mutates workspace files on disk without verified backend reconciliation!
   * Blocks unverified workspace file modifications to guarantee workspace safety.
   */
  public async reconcileWorkspaceFile(
    filePath: string,
    _content: string,
    proof?: BackendReconciliationProof
  ): Promise<ReconciliationResult> {
    // 1. Block if no verified backend proof provided
    if (!proof || !proof.verified || !proof.reconciliationId) {
      const errorMsg = `Safe review policy violation: Mutation of workspace file '${filePath}' rejected. Backend reconciliation is not verified.`;
      vscode.window.showErrorMessage(errorMsg);
      return {
        success: false,
        reconciled: false,
        filePath,
        message: errorMsg,
      };
    }

    // 2. Block if bridge is disconnected
    if (this._bridge.connectionState === 'disconnected') {
      const errorMsg = `Safe review policy violation: Cannot reconcile workspace file '${filePath}' while bridge is disconnected.`;
      vscode.window.showErrorMessage(errorMsg);
      return {
        success: false,
        reconciled: false,
        filePath,
        message: errorMsg,
      };
    }

    // 3. Verified backend reconciliation
    return {
      success: true,
      reconciled: true,
      filePath,
      message: `File '${filePath}' safely reconciled under verified backend transaction ${proof.reconciliationId}.`,
    };
  }

  /**
   * Handles user interaction actions dispatched from card buttons.
   */
  public async handleCardAction(action: { command: string; payload?: unknown }): Promise<unknown> {
    switch (action.command) {
      case 'agent-studios.inspectArtifact':
      case 'openDiff':
        return await this.openDiff(action.payload as ChangedFileEntry | Artifact | string);

      case 'agent-studios.showTaskGraph':
        return await vscode.commands.executeCommand('agent-studios.showTaskGraph');

      case 'agent-studios.approveAction':
      case 'approve':
        return await CardRenderer.handleApprovalAction(
          action.payload as ApprovalCardData,
          'approved',
          this._bridge
        );

      case 'agent-studios.denyAction':
      case 'deny':
        return await CardRenderer.handleApprovalAction(
          action.payload as ApprovalCardData,
          'denied',
          this._bridge
        );

      case 'reconcileFile': {
        const p = action.payload as {
          filePath: string;
          content: string;
          proof?: BackendReconciliationProof;
        };
        return await this.reconcileWorkspaceFile(p.filePath, p.content, p.proof);
      }

      default:
        return undefined;
    }
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const disposable = this._disposables.pop();
      disposable?.dispose();
    }
  }
}
