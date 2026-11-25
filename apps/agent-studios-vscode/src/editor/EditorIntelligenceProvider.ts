/**
 * Agent Studios — Editor Intelligence Provider (Wave 7 Extension-First)
 *
 * Implements Code-OSS native inline completions with prefix/suffix windowing,
 * debounce, CancellationToken propagation, and monotonic epoch rejection for stale
 * responses (never returns hardcoded fake code stubs when offline), plus Quick Fix
 * diagnostic CodeActions (`fixDiagnostics`) and inline selection edit (`inlineEdit`).
 */

import * as vscode from 'vscode';
import type { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';

export interface InlineSuggestionOptions {
  languageId: string;
  prefix: string;
  suffix: string;
  relativePath: string;
  debounceMs?: number;
}

export class AgentStudiosInlineCompletionProvider {
  private requestEpoch = 0;

  constructor(private readonly bridgeProvider: () => AgentStudiosBridge) {}

  public async suggestCompletion(
    options: InlineSuggestionOptions,
    cancellationToken?: { isCancellationRequested?: boolean }
  ): Promise<string[]> {
    const myEpoch = ++this.requestEpoch;
    if (cancellationToken?.isCancellationRequested) {
      return [];
    }

    const prefixLines = options.prefix.split(/\r?\n/);
    const trimmedTail = (prefixLines[prefixLines.length - 1] ?? '').trim();
    if (!trimmedTail) {
      return [];
    }

    if ((options.debounceMs ?? 0) > 0) {
      await new Promise((resolve) => setTimeout(resolve, options.debounceMs));
      if (cancellationToken?.isCancellationRequested || myEpoch !== this.requestEpoch) {
        return [];
      }
    }

    const bridge = this.bridgeProvider();
    if (bridge.connectionState !== 'connected' || vscode.workspace.isTrusted === false) {
      return [];
    }

    const boundedPrefix = prefixLines.slice(-120).join('\n');
    const boundedSuffix = options.suffix.split(/\r?\n/).slice(0, 40).join('\n');

    try {
      const chunks: string[] = [];
      await bridge.sendMessage(
        {
          message:
            `Complete only the cursor gap for ${options.relativePath} (${options.languageId}). ` +
            `Do not include markdown fences or prose.\n<prefix>\n${boundedPrefix}\n</prefix>\n<suffix>\n${boundedSuffix}\n</suffix>`,
          mode: 'ask',
        },
        (chunk) => {
          if (myEpoch === this.requestEpoch && !cancellationToken?.isCancellationRequested) {
            if (chunk.type === 'text_delta' && chunk.text) {
              chunks.push(chunk.text);
            }
          }
        }
      );

      if (cancellationToken?.isCancellationRequested || myEpoch !== this.requestEpoch) {
        return [];
      }

      const combined = chunks.join('').trim();
      return combined ? [combined] : [];
    } catch {
      return [];
    }
  }

  public async provideInlineCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position,
    _context: unknown,
    token: vscode.CancellationToken
  ): Promise<Array<{ insertText: string }>> {
    const startLine = Math.max(0, position.line - 120);
    const prefixRange = new vscode.Range(new vscode.Position(startLine, 0), position);
    const prefix = typeof document.getText === 'function'
      ? document.getText(prefixRange)
      : document.lineAt(position.line).text.slice(0, position.character);

    const endLine = Math.min((document.lineCount ?? position.line + 1) - 1, position.line + 40);
    const suffixRange = new vscode.Range(position, new vscode.Position(endLine, 250));
    const suffix = typeof document.getText === 'function' ? document.getText(suffixRange) : '';

    const relativePath = vscode.workspace.asRelativePath(document.uri);
    const suggestions = await this.suggestCompletion(
      {
        languageId: document.languageId,
        prefix,
        suffix,
        relativePath,
        debounceMs: 40,
      },
      token
    );
    return suggestions.map((text) => ({ insertText: text }));
  }
}

export class AgentStudiosDiagnosticsCodeActionProvider {
  public provideCodeActions(
    document: vscode.TextDocument,
    range: vscode.Range,
    context: { diagnostics: readonly vscode.Diagnostic[] }
  ): Array<{
    title: string;
    kind?: unknown;
    command: { command: string; title: string; arguments: unknown[] };
  }> {
    if (!context.diagnostics || context.diagnostics.length === 0) {
      return [];
    }
    const messages = context.diagnostics.map((d) => d.message);
    return [
      {
        title: `Ask Agent Studios to Fix (${context.diagnostics.length} diagnostic${context.diagnostics.length === 1 ? '' : 's'})`,
        command: {
          command: 'agent-studios.fixDiagnostics',
          title: 'Fix Diagnostics with Agent Studios',
          arguments: [document.uri.toString(), range, messages],
        },
      },
    ];
  }
}

export function registerEditorIntelligence(
  context: vscode.ExtensionContext,
  bridgeProvider: () => AgentStudiosBridge
): vscode.Disposable {
  const disposables: vscode.Disposable[] = [];
  const inlineProvider = new AgentStudiosInlineCompletionProvider(bridgeProvider);
  const codeActionProvider = new AgentStudiosDiagnosticsCodeActionProvider();

  const languagesApi = vscode.languages as unknown as {
    registerInlineCompletionItemProvider?: (
      selector: unknown,
      provider: unknown
    ) => vscode.Disposable;
    registerCodeActionsProvider?: (selector: unknown, provider: unknown) => vscode.Disposable;
  };

  if (typeof languagesApi.registerInlineCompletionItemProvider === 'function') {
    disposables.push(
      languagesApi.registerInlineCompletionItemProvider({ pattern: '**/*' }, inlineProvider)
    );
  }

  if (typeof languagesApi.registerCodeActionsProvider === 'function') {
    disposables.push(
      languagesApi.registerCodeActionsProvider({ pattern: '**/*' }, codeActionProvider)
    );
  }

  disposables.push(
    vscode.commands.registerCommand(
      'agent-studios.fixDiagnostics',
      async (uriArg?: string, _range?: unknown, diagnosticMessages?: string[]) => {
        const rawDiags = (vscode.languages.getDiagnostics?.() ?? []) as unknown[];
        const resolvedMessages: string[] = [];
        for (const entry of rawDiags) {
          if (Array.isArray(entry) && Array.isArray(entry[1])) {
            for (const d of entry[1] as vscode.Diagnostic[]) {
              if (d?.message) {
                resolvedMessages.push(d.message);
              }
            }
          } else if (entry && typeof entry === 'object' && 'message' in entry) {
            resolvedMessages.push((entry as vscode.Diagnostic).message);
          }
        }
        const messages =
          Array.isArray(diagnosticMessages) && diagnosticMessages.length > 0
            ? diagnosticMessages
            : resolvedMessages;
        if (messages.length === 0) {
          vscode.window.showInformationMessage('No active diagnostics found for Agent Studios to fix.');
          return { dispatched: false, count: 0 };
        }
        const bridge = bridgeProvider();
        const prompt = `Fix diagnostics${uriArg ? ` in ${uriArg}` : ''}:\n- ${messages.join('\n- ')}`;
        if (bridge.connectionState === 'connected') {
          await bridge.submitEditorAction({
            action: 'refactor',
            documentUri: uriArg || 'workspace://active',
            languageId: vscode.window.activeTextEditor?.document.languageId || 'plaintext',
            selectedText: prompt,
            selectionRange: { startLine: 0, startCol: 0, endLine: 0, endCol: 0 },
          });
        }
        vscode.window.showInformationMessage(
          `Agent Studios queued fix for ${messages.length} diagnostic(s).`
        );
        return { dispatched: true, count: messages.length, prompt };
      }
    ),
    vscode.commands.registerCommand('agent-studios.inlineEdit', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showWarningMessage('Open an editor before requesting an inline edit.');
        return undefined;
      }
      const instruction = await vscode.window.showInputBox?.({
        prompt: 'Describe the inline edit to perform on the selection',
        placeHolder: 'e.g. Add input validation and explicit return type',
      });
      if (!instruction) {
        return undefined;
      }
      const selectedText = editor.document.getText(editor.selection);
      const bridge = bridgeProvider();
      if (bridge.connectionState !== 'connected') {
        vscode.window.showInformationMessage(
          `Inline edit instruction captured (daemon disconnected): ${instruction}`
        );
        return { dispatched: false, instruction };
      }
      const result = await bridge.submitEditorAction({
        action: 'refactor',
        documentUri: editor.document.uri.toString(),
        languageId: editor.document.languageId,
        selectedText: `${instruction}\n\n${selectedText}`,
        selectionRange: {
          startLine: editor.selection.start.line,
          startCol: editor.selection.start.character,
          endLine: editor.selection.end.line,
          endCol: editor.selection.end.character,
        },
      });
      vscode.window.showInformationMessage(result.message);
      return result;
    })
  );

  const composite = vscode.Disposable.from(...disposables);
  context.subscriptions.push(composite);
  return composite;
}
