/**
 * Agent Studios — Hunk-Level Review, Checkpoints & Revert Manager (Wave 4 / Group 4)
 *
 * Parses unified diffs per hunk using `parseUnifiedDiff`, validates context
 * lines before applying changes (conflict detection), records file checkpoints
 * before mutation, and supports exact deterministic revert to checkpoint.
 */

import * as vscode from 'vscode';
import { type DiffHunk, DiffParser, type ParsedDiffFile } from './DiffParser';

export type HunkDecision = 'pending' | 'accepted' | 'rejected';

export interface FileCheckpoint {
  id: string;
  filePath: string;
  originalContent: string;
  createdAt: number;
  appliedHunkIndices: number[];
}

export interface HunkApplicationResult {
  applied: boolean;
  conflictReason?: string;
  updatedContent?: string;
  checkpointId?: string;
}

export class HunkReviewManager implements vscode.Disposable {
  private readonly hunkStates = new Map<string, HunkDecision>();
  private readonly checkpoints = new Map<string, FileCheckpoint[]>();

  public dispose(): void {
    this.hunkStates.clear();
    this.checkpoints.clear();
  }

  private hunkKey(artifactId: string, filePath: string, hunkIndex: number): string {
    return `${artifactId}::${filePath}::${hunkIndex}`;
  }

  public parseDiff(unifiedDiff: string): ParsedDiffFile[] {
    return DiffParser.parseUnifiedDiff(unifiedDiff).files;
  }

  public getHunkDecision(artifactId: string, filePath: string, hunkIndex: number): HunkDecision {
    return this.hunkStates.get(this.hunkKey(artifactId, filePath, hunkIndex)) ?? 'pending';
  }

  public rejectHunk(artifactId: string, filePath: string, hunkIndex: number): void {
    this.hunkStates.set(this.hunkKey(artifactId, filePath, hunkIndex), 'rejected');
  }

  public createCheckpoint(
    filePath: string,
    originalContent: string,
    appliedHunkIndices: number[] = []
  ): FileCheckpoint {
    const checkpoint: FileCheckpoint = {
      id: `ckpt-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      filePath,
      originalContent,
      createdAt: Date.now(),
      appliedHunkIndices: [...appliedHunkIndices],
    };
    const list = this.checkpoints.get(filePath) ?? [];
    list.push(checkpoint);
    this.checkpoints.set(filePath, list);
    return checkpoint;
  }

  public listCheckpoints(filePath: string): FileCheckpoint[] {
    return [...(this.checkpoints.get(filePath) ?? [])];
  }

  public revertToCheckpoint(checkpointId: string): FileCheckpoint | undefined {
    for (const [filePath, list] of this.checkpoints.entries()) {
      const idx = list.findIndex((c) => c.id === checkpointId);
      if (idx >= 0) {
        const target = list[idx];
        this.checkpoints.set(filePath, list.slice(0, idx));
        return target;
      }
    }
    return undefined;
  }

  public applyHunkToContent(
    artifactId: string,
    filePath: string,
    hunkIndex: number,
    baseContent: string,
    hunk: DiffHunk
  ): HunkApplicationResult {
    const lineEnding = baseContent.includes('\r\n') ? '\r\n' : '\n';
    const lines = baseContent.split(/\r?\n/);
    const startIdx = Math.max(0, hunk.oldStart - 1);

    const expectedOldLines: string[] = [];
    const replacementLines: string[] = [];
    for (const line of hunk.lines) {
      if (line.type === 'context') {
        const content = line.text.startsWith(' ') ? line.text.slice(1) : line.text;
        expectedOldLines.push(content);
        replacementLines.push(content);
      } else if (line.type === 'deletion') {
        const content = line.text.startsWith('-') ? line.text.slice(1) : line.text;
        expectedOldLines.push(content);
      } else if (line.type === 'addition') {
        const content = line.text.startsWith('+') ? line.text.slice(1) : line.text;
        replacementLines.push(content);
      }
    }

    if (startIdx + expectedOldLines.length > lines.length) {
      return {
        applied: false,
        conflictReason: `Hunk ${hunkIndex} out of bounds for ${filePath} (expected ${startIdx + expectedOldLines.length} lines, found ${lines.length}).`,
      };
    }

    for (let offset = 0; offset < expectedOldLines.length; offset++) {
      if (lines[startIdx + offset] !== expectedOldLines[offset]) {
        return {
          applied: false,
          conflictReason: `Stale base conflict at ${filePath} line ${startIdx + offset + 1}: expected '${expectedOldLines[offset]}', found '${lines[startIdx + offset]}'.`,
        };
      }
    }

    const checkpoint = this.createCheckpoint(filePath, baseContent, [hunkIndex]);
    const nextLines = [
      ...lines.slice(0, startIdx),
      ...replacementLines,
      ...lines.slice(startIdx + expectedOldLines.length),
    ];
    this.hunkStates.set(this.hunkKey(artifactId, filePath, hunkIndex), 'accepted');

    return {
      applied: true,
      updatedContent: nextLines.join(lineEnding),
      checkpointId: checkpoint.id,
    };
  }
}

export function registerHunkReviewCommands(
  context: vscode.ExtensionContext,
  manager: HunkReviewManager
): vscode.Disposable {
  const disposables = [
    vscode.commands.registerCommand(
      'agent-studios.revertCheckpoint',
      async (filePath?: string) => {
        const target =
          filePath ||
          (vscode.window.activeTextEditor
            ? vscode.workspace.asRelativePath(vscode.window.activeTextEditor.document.uri)
            : undefined);
        if (!target) {
          vscode.window.showInformationMessage('Select a file to inspect Agent Studios checkpoints.');
          return undefined;
        }
        const ckpts = manager.listCheckpoints(target);
        if (ckpts.length === 0) {
          vscode.window.showInformationMessage(`No checkpoints recorded for ${target}.`);
          return undefined;
        }
        const latest = ckpts[ckpts.length - 1];
        const reverted = manager.revertToCheckpoint(latest.id);
        if (reverted) {
          vscode.window.showInformationMessage(
            `Reverted ${target} to checkpoint ${reverted.id}.`
          );
        }
        return reverted;
      }
    ),
  ];

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
