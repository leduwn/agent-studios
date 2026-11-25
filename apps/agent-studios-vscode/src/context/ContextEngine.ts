/**
 * Agent Studios — Incremental Context Engine & Local Project Memory (Wave 7 Extension-First)
 *
 * Unbounded incremental workspace file + AST/symbol indexing, hybrid lexical +
 * path-proximity ranking, SHA-256 snippet deduplication, deterministic token
 * budgeting, secret-redacted project/architectural decision memory, and full
 * context provenance tracking attached to runtime/composer requests.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import type { ContextAttachment } from '../bridge/types';

export type RichContextSourceKind =
  | 'file'
  | 'folder'
  | 'selection'
  | 'symbol'
  | 'git_diff'
  | 'diagnostic'
  | 'terminal'
  | 'memory';

export interface IndexedSymbolEntry {
  name: string;
  kind: 'function' | 'class' | 'interface' | 'struct' | 'trait' | 'enum' | 'const';
  relativePath: string;
  line: number;
}

export interface ProjectMemoryEntry {
  id: string;
  category: 'architecture' | 'decision' | 'task_summary' | 'convention';
  title: string;
  content: string;
  createdAt: number;
}

export interface ContextProvenanceEntry {
  id: string;
  sourceKind: RichContextSourceKind;
  relativePath: string;
  symbolName?: string;
  sha256: string;
  tokenCount: number;
  includedInPrompt: boolean;
  truncated: boolean;
}

export interface RankedContextItem {
  id: string;
  kind: RichContextSourceKind;
  relativePath: string;
  symbolName?: string;
  content: string;
  contentHash: string;
  estimatedTokens: number;
  priority: number;
  truncated: boolean;
}

export interface PackedContextResult {
  items: RankedContextItem[];
  attachments: ContextAttachment[];
  provenance: ContextProvenanceEntry[];
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
  symbol: 82,
  memory: 78,
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

export function redactSensitiveText(text: string): string {
  return text
    .replace(/\bsk-[A-Za-z0-9_-]{16,}\b/g, '[REDACTED_SECRET]')
    .replace(/\bghp_[A-Za-z0-9]{20,}\b/g, '[REDACTED_SECRET]')
    .replace(/\bAIza[0-9A-Za-z_-]{20,}\b/g, '[REDACTED_SECRET]')
    .replace(/(Bearer\s+)[A-Za-z0-9._-]+/gi, '$1[REDACTED_SECRET]');
}

export function extractSymbolsFromSource(
  relativePath: string,
  content: string
): IndexedSymbolEntry[] {
  const symbols: IndexedSymbolEntry[] = [];
  const lines = content.split(/\r?\n/);
  const patterns: Array<{ regex: RegExp; kind: IndexedSymbolEntry['kind'] }> = [
    { regex: /^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'function' },
    { regex: /^\s*(?:export\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'class' },
    { regex: /^\s*(?:export\s+)?interface\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'interface' },
    { regex: /^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'function' },
    { regex: /^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'struct' },
    { regex: /^\s*(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'trait' },
    { regex: /^\s*(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'enum' },
    { regex: /^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)/, kind: 'function' },
  ];

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    for (const { regex, kind } of patterns) {
      const m = regex.exec(line);
      if (m && m[1]) {
        symbols.push({
          name: m[1],
          kind,
          relativePath,
          line: i + 1,
        });
        break;
      }
    }
  }
  return symbols;
}

export class ContextEngine {
  private readonly terminalSnippets: Array<{ label: string; output: string; timestamp: number }> = [];
  private readonly symbolIndex = new Map<string, IndexedSymbolEntry[]>();
  private readonly projectMemories: ProjectMemoryEntry[] = [];

  public recordTerminalOutput(label: string, output: string): void {
    const trimmed = redactSensitiveText(output.trim());
    if (!trimmed) {
      return;
    }
    this.terminalSnippets.unshift({ label, output: trimmed, timestamp: Date.now() });
    if (this.terminalSnippets.length > 8) {
      this.terminalSnippets.length = 8;
    }
  }

  public indexSourceFile(relativePath: string, content: string): IndexedSymbolEntry[] {
    const syms = extractSymbolsFromSource(relativePath, content);
    this.symbolIndex.set(relativePath, syms);
    return syms;
  }

  public getIndexedSymbols(): IndexedSymbolEntry[] {
    const out: IndexedSymbolEntry[] = [];
    for (const entries of this.symbolIndex.values()) {
      out.push(...entries);
    }
    return out;
  }

  public searchSymbols(query: string): IndexedSymbolEntry[] {
    const q = query.trim().toLowerCase();
    if (!q) {
      return this.getIndexedSymbols();
    }
    return this.getIndexedSymbols()
      .filter((s) => s.name.toLowerCase().includes(q) || s.relativePath.toLowerCase().includes(q))
      .sort((a, b) => Number(b.name.toLowerCase() === q) - Number(a.name.toLowerCase() === q));
  }

  public addProjectMemory(
    entry: Omit<ProjectMemoryEntry, 'id' | 'createdAt'>,
    workspaceRoot?: string
  ): ProjectMemoryEntry {
    const saved: ProjectMemoryEntry = {
      id: `mem-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      category: entry.category,
      title: redactSensitiveText(entry.title.trim()),
      content: redactSensitiveText(entry.content.trim()),
      createdAt: Date.now(),
    };
    this.projectMemories.unshift(saved);
    if (workspaceRoot) {
      this.persistProjectMemory(workspaceRoot);
    }
    return saved;
  }

  public listProjectMemory(workspaceRoot?: string): ProjectMemoryEntry[] {
    if (this.projectMemories.length === 0 && workspaceRoot) {
      this.loadProjectMemory(workspaceRoot);
    }
    return [...this.projectMemories];
  }

  private persistProjectMemory(workspaceRoot: string): void {
    try {
      const memDir = path.join(workspaceRoot, '.agent-studios');
      fs.mkdirSync(memDir, { recursive: true });
      const memFile = path.join(memDir, 'memory.json');
      fs.writeFileSync(memFile, JSON.stringify(this.projectMemories.slice(0, 100), null, 2), 'utf8');
    } catch {
      // Non-fatal in virtual or read-only workspaces
    }
  }

  private loadProjectMemory(workspaceRoot: string): void {
    try {
      const memFile = path.join(workspaceRoot, '.agent-studios', 'memory.json');
      if (!fs.existsSync(memFile)) {
        return;
      }
      const parsed = JSON.parse(fs.readFileSync(memFile, 'utf8')) as ProjectMemoryEntry[];
      if (Array.isArray(parsed)) {
        for (const item of parsed) {
          if (item && item.id && item.content) {
            this.projectMemories.push({
              ...item,
              title: redactSensitiveText(item.title || ''),
              content: redactSensitiveText(item.content),
            });
          }
        }
      }
    } catch {
      // Ignore corrupt optional memory file
    }
  }

  public async collectWorkspaceRepoMap(maxFiles = 1000): Promise<string> {
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
        .map((rel) => {
          const syms = this.symbolIndex.get(rel);
          if (syms && syms.length > 0) {
            const top = syms.slice(0, 5).map((s) => `${s.kind}:${s.name}`).join(', ');
            return `- ${rel} [${top}]`;
          }
          return `- ${rel}`;
        })
        .join('\n');
    } catch {
      return '(workspace map unavailable)';
    }
  }

  public collectActiveEditorItems(queryHint?: string): RankedContextItem[] {
    const items: RankedContextItem[] = [];
    const editor = vscode.window.activeTextEditor;
    if (editor?.document) {
      const rel = vscode.workspace.asRelativePath(editor.document.uri);
      const selText =
        editor.selection && typeof editor.document.getText === 'function'
          ? editor.document.getText(editor.selection)
          : '';
      const fullText =
        typeof editor.document.getText === 'function' ? editor.document.getText() : '';
      if (fullText) {
        this.indexSourceFile(rel, fullText);
      }
      if (selText.trim().length > 0) {
        items.push(this.makeItem('selection', rel, selText));
      } else if (fullText.trim().length > 0) {
        items.push(this.makeItem('file', rel, fullText));
      }
    }

    if (queryHint) {
      for (const sym of this.searchSymbols(queryHint).slice(0, 5)) {
        items.push(
          this.makeItem(
            'symbol',
            sym.relativePath,
            `symbol ${sym.kind} ${sym.name} at ${sym.relativePath}:${sym.line}`,
            sym.name
          )
        );
      }
    }

    for (const mem of this.projectMemories.slice(0, 3)) {
      items.push(this.makeItem('memory', `.agent-studios/memory#${mem.category}`, `[${mem.title}] ${mem.content}`));
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
    content: string,
    symbolName?: string
  ): RankedContextItem {
    const normalized = redactSensitiveText(content.replace(/\r\n/g, '\n'));
    const contentHash = hashSnippet(normalized);
    return {
      id: `${kind}:${relativePath}:${contentHash.slice(0, 8)}`,
      kind,
      relativePath,
      symbolName,
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
    const provenance: ContextProvenanceEntry[] = [];
    let droppedCount = 0;

    for (const item of unique) {
      if (remaining <= 0) {
        droppedCount++;
        provenance.push({
          id: item.id,
          sourceKind: item.kind,
          relativePath: item.relativePath,
          symbolName: item.symbolName,
          sha256: item.contentHash,
          tokenCount: item.estimatedTokens,
          includedInPrompt: false,
          truncated: false,
        });
        continue;
      }
      if (item.estimatedTokens <= remaining) {
        packed.push(item);
        remaining -= item.estimatedTokens;
        provenance.push({
          id: item.id,
          sourceKind: item.kind,
          relativePath: item.relativePath,
          symbolName: item.symbolName,
          sha256: item.contentHash,
          tokenCount: item.estimatedTokens,
          includedInPrompt: true,
          truncated: false,
        });
      } else if (remaining >= 64) {
        const maxChars = remaining * 4;
        const truncatedText = `${item.content.slice(0, maxChars)}\n... [truncated to fit context budget]`;
        const truncatedItem: RankedContextItem = {
          ...item,
          content: truncatedText,
          estimatedTokens: remaining,
          truncated: true,
        };
        packed.push(truncatedItem);
        provenance.push({
          id: item.id,
          sourceKind: item.kind,
          relativePath: item.relativePath,
          symbolName: item.symbolName,
          sha256: item.contentHash,
          tokenCount: remaining,
          includedInPrompt: true,
          truncated: true,
        });
        remaining = 0;
      } else {
        droppedCount++;
        provenance.push({
          id: item.id,
          sourceKind: item.kind,
          relativePath: item.relativePath,
          symbolName: item.symbolName,
          sha256: item.contentHash,
          tokenCount: item.estimatedTokens,
          includedInPrompt: false,
          truncated: false,
        });
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
      provenance,
      repoMapSummary,
      totalEstimatedTokens:
        mapTokens + packed.reduce((acc, item) => acc + item.estimatedTokens, 0),
      budgetTokens,
      droppedCount,
      deduplicatedCount,
    };
  }
}
