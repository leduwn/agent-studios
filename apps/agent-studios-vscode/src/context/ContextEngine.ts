/**
 * Agent Studios — Context Engine (Wave 4 / Group 3)
 *
 * Collects multi-source developer context attachments (files, folders,
 * selections, diagnostics, git diffs, symbols, terminal snippets), builds a
 * concise workspace repo map, deduplicates identical snippets via SHA-256,
 * and enforces deterministic token budgeting against the active model limit.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import type { ContextAttachment } from '../bridge/types';

export type RichContextSourceKind =
  | 'file'
  | 'folder'
  | 'selection'
  | 'symbol'
  | 'git_diff'
  | 'diagnostic'
  | 'terminal';

export interface RankedContextItem {
  id: string;
  kind: RichContextSourceKind;
  relativePath: string;
  content: string;
  contentHash: string;
  estimatedTokens: number;
  priority: number;
  truncated: boolean;
}

export interface PackedContextResult {
  items: RankedContextItem[];
  attachments: ContextAttachment[];
  repoMapSummary: string;
  totalEstimatedTokens: number;
  budgetTokens: number;
  droppedCount: number;
  deduplicatedCount: number;
}

const SOURCE_PRIORITY: Record<RichContextSourceKind, number> = {
  selection: 100,
  diagnostic: 95,
  git_diff: 85,
  symbol: 80,
  terminal: 75,
  file: 60,
  folder: 40,
};

export function hashSnippet(content: string): string {
  return crypto.createHash('sha256').update(content, 'utf8').digest('hex');
}

export function estimateTokens(text: string): number {
  if (!text) {
    return 0;
  }
  return Math.max(1, Math.ceil(text.length / 4));
}

export class ContextEngine {
  private readonly terminalSnippets: Array<{ label: string; output: string; timestamp: number }> = [];

  public recordTerminalOutput(label: string, output: string): void {
    const trimmed = output.trim();
    if (!trimmed) {
      return;
    }
    this.terminalSnippets.unshift({ label, output: trimmed, timestamp: Date.now() });
    if (this.terminalSnippets.length > 5) {
      this.terminalSnippets.length = 5;
    }
  }

  public async collectWorkspaceRepoMap(maxFiles = 40): Promise<string> {
    try {
      const files = await vscode.workspace.findFiles(
        '**/*',
        '{**/node_modules/**,**/.git/**,**/target/**,**/out/**,**/dist/**}',
        maxFiles
      );
      if (files.length === 0) {
        return '(empty workspace)';
      }
      return files
        .map((uri) => vscode.workspace.asRelativePath(uri))
        .sort()
        .map((rel) => `- ${rel}`)
        .join('\n');
    } catch {
      return '(workspace map unavailable)';
    }
  }

  public collectActiveEditorItems(): RankedContextItem[] {
    const items: RankedContextItem[] = [];
    const editor = vscode.window.activeTextEditor;
    if (editor?.document) {
      const rel = vscode.workspace.asRelativePath(editor.document.uri);
      const selText =
        editor.selection && typeof editor.document.getText === 'function'
          ? editor.document.getText(editor.selection)
          : '';
      if (selText.trim().length > 0) {
        items.push(this.makeItem('selection', rel, selText));
      } else if (typeof editor.document.getText === 'function') {
        const fullText = editor.document.getText();
        if (fullText.trim().length > 0) {
          items.push(this.makeItem('file', rel, fullText));
        }
      }
    }

    const rawDiagnostics = (vscode.languages?.getDiagnostics?.() ?? []) as unknown[];
    const flatDiagnostics: vscode.Diagnostic[] = [];
    for (const entry of rawDiagnostics) {
      if (Array.isArray(entry) && Array.isArray(entry[1])) {
        flatDiagnostics.push(...(entry[1] as vscode.Diagnostic[]));
      } else if (entry && typeof entry === 'object' && 'message' in entry) {
        flatDiagnostics.push(entry as vscode.Diagnostic);
      }
    }
    if (flatDiagnostics.length > 0) {
      const formatted = flatDiagnostics
        .slice(0, 20)
        .map((d) => {
          const line = (d.range?.start?.line ?? 0) + 1;
          const sev = d.severity === 0 ? 'error' : 'warning';
          return `[${sev}] line ${line}: ${d.message}`;
        })
        .join('\n');
      if (formatted) {
        items.push(this.makeItem('diagnostic', 'workspace/diagnostics', formatted));
      }
    }

    for (const term of this.terminalSnippets) {
      items.push(this.makeItem('terminal', `terminal/${term.label}`, term.output));
    }
    return items;
  }

  public makeItem(
    kind: RichContextSourceKind,
    relativePath: string,
    content: string
  ): RankedContextItem {
    const normalized = content.replace(/\r\n/g, '\n');
    const contentHash = hashSnippet(normalized);
    return {
      id: `${kind}:${relativePath}:${contentHash.slice(0, 8)}`,
      kind,
      relativePath,
      content: normalized,
      contentHash,
      estimatedTokens: estimateTokens(normalized),
      priority: SOURCE_PRIORITY[kind],
      truncated: false,
    };
  }

  public packWithinBudget(
    candidates: RankedContextItem[],
    budgetTokens = 16000,
    repoMapSummary = ''
  ): PackedContextResult {
    const seenHashes = new Set<string>();
    let deduplicatedCount = 0;
    const unique: RankedContextItem[] = [];

    const sorted = [...candidates].sort((a, b) => b.priority - a.priority);
    for (const candidate of sorted) {
      if (seenHashes.has(candidate.contentHash)) {
        deduplicatedCount++;
        continue;
      }
      seenHashes.add(candidate.contentHash);
      unique.push({ ...candidate });
    }

    const mapTokens = repoMapSummary ? estimateTokens(repoMapSummary) : 0;
    let remaining = Math.max(0, budgetTokens - mapTokens);
    const packed: RankedContextItem[] = [];
    let droppedCount = 0;

    for (const item of unique) {
      if (remaining <= 0) {
        droppedCount++;
        continue;
      }
      if (item.estimatedTokens <= remaining) {
        packed.push(item);
        remaining -= item.estimatedTokens;
      } else if (remaining >= 64) {
        const maxChars = remaining * 4;
        const truncatedText = `${item.content.slice(0, maxChars)}\n... [truncated to fit context budget]`;
        packed.push({
          ...item,
          content: truncatedText,
          estimatedTokens: remaining,
          truncated: true,
        });
        remaining = 0;
      } else {
        droppedCount++;
      }
    }

    const attachments: ContextAttachment[] = packed.map((item) => ({
      type:
        item.kind === 'selection'
          ? 'selection'
          : item.kind === 'diagnostic'
            ? 'diagnostic'
            : 'file',
      uri: item.relativePath,
      relativePath: item.relativePath,
      contentSnippet: item.content,
    }));

    return {
      items: packed,
      attachments,
      repoMapSummary,
      totalEstimatedTokens:
        mapTokens + packed.reduce((acc, item) => acc + item.estimatedTokens, 0),
      budgetTokens,
      droppedCount,
      deduplicatedCount,
    };
  }
}
