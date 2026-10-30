/**
 * Agent Studios — Virtual Diff Document Content Provider
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as path from 'path';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from './AgentStudiosBridge';
import { Artifact, TimelineEvent } from './types';

export const DIFF_SCHEME = 'agent-studios-diff';

/**
 * Applies a unified diff patch to original text in memory without touching workspace files.
 * Returns the modified target text, or the diffText if patching is not applicable.
 */
export function applyPatch(originalContent: string, diffText: string): string {
  if (!diffText || typeof diffText !== 'string') {
    return originalContent;
  }

  // Not a unified diff patch; return verbatim
  if (!diffText.includes('@@') || (!diffText.includes('---') && !diffText.includes('+++'))) {
    return diffText;
  }

  // New file creation diff: --- /dev/null
  if (/^---\s+(?:\/dev\/null|""|''|\(empty\))/m.test(diffText)) {
    const lines = diffText.split(/\r?\n/);
    const addedLines: string[] = [];
    let inHunk = false;
    for (const line of lines) {
      if (line.startsWith('@@')) {
        inHunk = true;
        continue;
      }
      if (inHunk) {
        if (line.startsWith('+') && !line.startsWith('+++')) {
          addedLines.push(line.slice(1));
        } else if (line.startsWith(' ')) {
          addedLines.push(line.slice(1));
        }
      }
    }
    return addedLines.join('\n');
  }

  try {
    const origLines = originalContent.split(/\r?\n/);
    const diffLines = diffText.split(/\r?\n/);

    interface Hunk {
      oldStart: number;
      oldLength: number;
      newStart: number;
      newLength: number;
      lines: string[];
    }

    const hunks: Hunk[] = [];
    let currentHunk: Hunk | null = null;

    for (const line of diffLines) {
      const hunkMatch = line.match(/^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/);
      if (hunkMatch) {
        currentHunk = {
          oldStart: parseInt(hunkMatch[1], 10),
          oldLength: hunkMatch[2] !== undefined ? parseInt(hunkMatch[2], 10) : 1,
          newStart: parseInt(hunkMatch[3], 10),
          newLength: hunkMatch[4] !== undefined ? parseInt(hunkMatch[4], 10) : 1,
          lines: [],
        };
        hunks.push(currentHunk);
        continue;
      }
      if (currentHunk) {
        if (line.startsWith('+') || line.startsWith('-') || line.startsWith(' ') || line.startsWith('\\')) {
          currentHunk.lines.push(line);
        }
      }
    }

    if (hunks.length === 0) {
      return diffText;
    }

    const resultLines: string[] = [];
    let origIndex = 0;

    for (const hunk of hunks) {
      const hunkOldStart0 = Math.max(0, hunk.oldStart === 0 ? 0 : hunk.oldStart - 1);

      while (origIndex < hunkOldStart0 && origIndex < origLines.length) {
        resultLines.push(origLines[origIndex]);
        origIndex++;
      }

      for (const hLine of hunk.lines) {
        if (hLine.startsWith('+')) {
          resultLines.push(hLine.slice(1));
        } else if (hLine.startsWith('-')) {
          origIndex++;
        } else if (hLine.startsWith(' ')) {
          resultLines.push(hLine.slice(1));
          origIndex++;
        }
      }
    }

    while (origIndex < origLines.length) {
      resultLines.push(origLines[origIndex]);
      origIndex++;
    }

    return resultLines.join('\n');
  } catch {
    // If patch parsing or application fails, safely fallback to raw diff
    return diffText;
  }
}

/**
 * Extracts affected relative file paths from unified diff headers.
 * Prioritizes target paths (+++ b/) before source paths (--- a/).
 */
export function extractAffectedPaths(diffContent: string, fallbackPath?: string): string[] {
  const targetPaths: string[] = [];
  const sourcePaths: string[] = [];
  const lines = diffContent.split(/\r?\n/);

  for (const line of lines) {
    if (line.startsWith('+++ b/')) {
      targetPaths.push(line.substring(6).trim());
    } else if (line.startsWith('+++ ') && !line.startsWith('+++ /dev/null')) {
      targetPaths.push(line.substring(4).trim());
    } else if (line.startsWith('--- a/')) {
      sourcePaths.push(line.substring(6).trim());
    } else if (line.startsWith('--- ') && !line.startsWith('--- /dev/null')) {
      sourcePaths.push(line.substring(4).trim());
    }
  }

  if (fallbackPath && !fallbackPath.endsWith('.patch') && !fallbackPath.endsWith('.diff')) {
    targetPaths.push(fallbackPath);
  }

  return Array.from(new Set([...targetPaths, ...sourcePaths].filter(Boolean)));
}

/**
 * Resolves an existing workspace file matching the artifact's target path.
 */
export async function findWorkspaceFileForArtifact(
  artifact: Artifact,
  diffContent: string
): Promise<vscode.Uri | undefined> {
  const candidateRelativePaths = extractAffectedPaths(diffContent, artifact.path);
  const folders = vscode.workspace.workspaceFolders;

  if (folders) {
    for (const candidatePath of candidateRelativePaths) {
      for (const folder of folders) {
        const candidateUri = vscode.Uri.joinPath(folder.uri, candidatePath);
        try {
          const stat = await vscode.workspace.fs.stat(candidateUri);
          if (stat.type !== vscode.FileType.Directory) {
            return candidateUri;
          }
        } catch {
          // File does not exist in folder, try next
        }
      }
    }
  }

  if (artifact.path && path.isAbsolute(artifact.path)) {
    try {
      const absUri = vscode.Uri.file(artifact.path);
      const stat = await vscode.workspace.fs.stat(absUri);
      if (stat.type !== vscode.FileType.Directory) {
        return absUri;
      }
    } catch {
      // Ignore
    }
  }

  return undefined;
}

/**
 * Implements vscode.TextDocumentContentProvider for scheme `agent-studios-diff`.
 * Serves virtual read-only document content for artifact diffs and original/target comparisons.
 *
 * Guarantees safe virtual documents: NEVER overwrites real workspace files just to display a diff!
 * Fires onDidChange event when artifacts update.
 */
export class DiffDocumentProvider implements vscode.TextDocumentContentProvider, vscode.Disposable {
  private readonly _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  public readonly onDidChange: vscode.Event<vscode.Uri> = this._onDidChange.event;

  private readonly _virtualDocuments = new Map<string, string>();
  private readonly _disposables: vscode.Disposable[] = [];

  constructor(private readonly _bridge?: AgentStudiosBridge) {
    this._disposables.push(this._onDidChange);

    if (this._bridge) {
      this._disposables.push(
        this._bridge.onEvent((event: TimelineEvent) => {
          this.handleBridgeEvent(event);
        })
      );
    }
  }

  /**
   * Sets virtual document content directly in-memory and triggers change event.
   */
  public setDocumentContent(uri: vscode.Uri, content: string): void {
    this._virtualDocuments.set(uri.toString(), content);
    this._onDidChange.fire(uri);
  }

  /**
   * Fires the onDidChange event for a given URI.
   */
  public fireDidChange(uri: vscode.Uri): void {
    this._onDidChange.fire(uri);
  }

  /**
   * Alias for fireDidChange.
   */
  public update(uri: vscode.Uri): void {
    this._onDidChange.fire(uri);
  }

  /**
   * Provides virtual read-only document content for a given URI under `agent-studios-diff`.
   */
  public async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    // 1. In-memory virtual document cache
    if (this._virtualDocuments.has(uri.toString())) {
      return this._virtualDocuments.get(uri.toString())!;
    }
    if (this._virtualDocuments.has(uri.path)) {
      return this._virtualDocuments.get(uri.path)!;
    }

    // 2. Parse URI to extract artifact identification
    // Supports:
    //   agent-studios-diff://artifact/<id>/<name>
    //   agent-studios-diff://target/<id>/<name>
    //   agent-studios-diff://original/<id>/<name>
    //   agent-studios-diff:/artifact/<id>/<name>
    const authority = uri.authority || '';
    const segments = uri.path.split('/').filter(Boolean);

    let role = authority;
    let artifactId: string | undefined;
    let artifactName: string | undefined;

    if (authority === 'artifact' || authority === 'target' || authority === 'original' || authority === 'diff') {
      artifactId = segments[0];
      artifactName = segments[1];
    } else if (segments.length > 0) {
      if (segments[0] === 'artifact' || segments[0] === 'target' || segments[0] === 'original' || segments[0] === 'diff') {
        role = segments[0];
        artifactId = segments[1];
        artifactName = segments[2];
      } else {
        artifactId = segments[0];
        artifactName = segments[1];
      }
    }

    if (!artifactId) {
      return '';
    }

    // 3. Look up artifact from bridge
    let diffContent: string | null = null;
    let artifact: Artifact | undefined;

    if (this._bridge) {
      try {
        diffContent = await this._bridge.getArtifactDiff(artifactId);
      } catch {
        diffContent = null;
      }

      try {
        const artifacts = await this._bridge.listArtifacts();
        artifact = artifacts.find(
          a => a.id === artifactId || a.name === artifactId || (artifactName && a.name === artifactName)
        );
      } catch {
        artifact = undefined;
      }
    }

    if (!diffContent && artifact) {
      diffContent = artifact.diffContent;
    }

    if (!diffContent && !artifact) {
      return '';
    }

    const rawDiff = diffContent ?? artifact?.diffContent ?? '';

    // 4. Handle query parameters or role specifications
    const query = uri.query || '';
    const wantsDiff = role === 'diff' || query.includes('view=diff');
    const wantsOriginal = role === 'original' || query.includes('view=original');
    const wantsTarget = role === 'target' || query.includes('view=target');

    if (wantsDiff) {
      return rawDiff;
    }

    // Try finding corresponding workspace file
    const originalUri = artifact ? await findWorkspaceFileForArtifact(artifact, rawDiff) : undefined;

    if (wantsOriginal) {
      if (originalUri) {
        try {
          const rawBytes = await vscode.workspace.fs.readFile(originalUri);
          return Buffer.from(rawBytes).toString('utf8');
        } catch {
          return '';
        }
      }
      return '';
    }

    if (wantsTarget) {
      if (originalUri) {
        try {
          const rawBytes = await vscode.workspace.fs.readFile(originalUri);
          const origText = Buffer.from(rawBytes).toString('utf8');
          return applyPatch(origText, rawDiff);
        } catch {
          return rawDiff;
        }
      }
      return applyPatch('', rawDiff);
    }

    // Default behavior for `agent-studios-diff://artifact/<id>/<name>`:
    // If the diff affects an existing workspace file and is a patch, serve the patched target content
    // so vscode.diff(originalUri, virtualUri, title) renders the side-by-side diff cleanly.
    // If it does not affect an existing workspace file, serve rawDiff for standalone viewing.
    if (originalUri && rawDiff.includes('@@')) {
      try {
        const rawBytes = await vscode.workspace.fs.readFile(originalUri);
        const origText = Buffer.from(rawBytes).toString('utf8');
        return applyPatch(origText, rawDiff);
      } catch {
        return rawDiff;
      }
    }

    return rawDiff;
  }

  private handleBridgeEvent(event: TimelineEvent): void {
    if (event.type === 'artifact_created') {
      const details = event.details as Record<string, unknown> | undefined;
      const artifact = details?.artifact as Artifact | undefined;
      if (artifact) {
        const diffUri = vscode.Uri.parse(`${DIFF_SCHEME}://artifact/${artifact.id}/${artifact.name}`);
        const targetUri = vscode.Uri.parse(`${DIFF_SCHEME}://target/${artifact.id}/${artifact.name}`);
        this._onDidChange.fire(diffUri);
        this._onDidChange.fire(targetUri);
      }
    }
  }

  public dispose(): void {
    while (this._disposables.length > 0) {
      const disposable = this._disposables.pop();
      disposable?.dispose();
    }
    this._virtualDocuments.clear();
  }
}
