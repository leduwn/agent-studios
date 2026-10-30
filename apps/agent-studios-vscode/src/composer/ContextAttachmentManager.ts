/**
 * Agent Studios — Context Attachment Manager
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 */

import * as path from 'path';
import * as vscode from 'vscode';
import { ContextAttachment } from '../bridge/types';

export const MAX_ATTACHMENT_SIZE_BYTES = 20 * 1024; // 20KB
export const MAX_ATTACHMENTS_PER_PROMPT = 5;
export const MAX_SNIPPET_LINES = 200;

/**
 * Truncates text so that its UTF-8 byte length does not exceed maxBytes.
 * Appends a truncation marker if content was truncated.
 */
export function truncateToByteLimit(
  content: string,
  maxBytes: number = MAX_ATTACHMENT_SIZE_BYTES
): string {
  if (Buffer.byteLength(content, 'utf8') <= maxBytes) {
    return content;
  }

  const marker = '\n... [truncated to 20KB limit]';
  const markerBytes = Buffer.byteLength(marker, 'utf8');
  const targetBytes = Math.max(0, maxBytes - markerBytes);

  const buf = Buffer.from(content, 'utf8');
  let truncated = buf.subarray(0, targetBytes).toString('utf8');

  // Strip replacement char if multibyte character was split
  if (truncated.endsWith('�')) {
    truncated = truncated.slice(0, -1);
  }

  return truncated + marker;
}

/**
 * Bounds text content to both a maximum number of lines and a maximum byte size.
 */
export function boundContentSnippet(
  content: string,
  maxLines: number = MAX_SNIPPET_LINES,
  maxBytes: number = MAX_ATTACHMENT_SIZE_BYTES
): string {
  if (!content) {
    return '';
  }

  const lines = content.split(/\r?\n/);
  const boundedLines = lines.length > maxLines ? lines.slice(0, maxLines) : lines;
  const lineBoundedText = boundedLines.join('\n');

  return truncateToByteLimit(lineBoundedText, maxBytes);
}

/**
 * Manages active editor context capture, workspace file search,
 * language diagnostics extraction, and prompt attachment bounds enforcement.
 */
export class ContextAttachmentManager {
  public readonly maxAttachmentSizeBytes = MAX_ATTACHMENT_SIZE_BYTES;
  public readonly maxAttachmentsPerPrompt = MAX_ATTACHMENTS_PER_PROMPT;
  public readonly maxSnippetLines = MAX_SNIPPET_LINES;

  /**
   * Captures the active editor context (active document, relative path, languageId,
   * selection line range, and bounded snippet up to 200 lines and 20KB).
   * Returns null if no active text editor is open.
   */
  public async captureActiveEditorContext(): Promise<ContextAttachment | null> {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      return null;
    }

    const document = editor.document;
    const uri = document.uri.toString();
    const relativePath = vscode.workspace.asRelativePath
      ? vscode.workspace.asRelativePath(document.uri, false)
      : (document.uri.fsPath || document.uri.path);
    const languageId = document.languageId;

    const selection = editor.selection;
    const hasSelection = selection && !selection.isEmpty;

    if (hasSelection) {
      const selectedText = document.getText(selection);
      const snippet = boundContentSnippet(
        selectedText,
        this.maxSnippetLines,
        this.maxAttachmentSizeBytes
      );

      const startLine = selection.start.line + 1;
      const endLine = selection.end.line + 1;

      return {
        type: 'selection',
        uri,
        relativePath,
        selectionRange: {
          startLine,
          endLine,
        },
        contentSnippet: snippet,
        languageId,
      };
    }

    // Whole file context (bounded to 200 lines and 20KB)
    const fullText = typeof document.getText === 'function' ? document.getText() : '';
    const snippet = boundContentSnippet(
      fullText,
      this.maxSnippetLines,
      this.maxAttachmentSizeBytes
    );

    return {
      type: 'file',
      uri,
      relativePath,
      contentSnippet: snippet,
      languageId,
    };
  }

  /**
   * Searches workspace files using vscode.workspace.findFiles with a glob pattern matching query.
   * Excludes standard clutter (node_modules, .git, out, dist).
   * Returns workspace-relative paths and labels up to maxResults.
   */
  public async searchWorkspaceFiles(
    query: string,
    maxResults: number = 10
  ): Promise<Array<{ label: string; relativePath: string; uri: vscode.Uri }>> {
    const trimmed = query ? query.trim() : '';
    const globPattern = trimmed
      ? (trimmed.includes('*') ? trimmed : `**/*${trimmed}*`)
      : '**/*';

    const excludePattern = '{**/node_modules/**,**/.git/**,**/dist/**,**/out/**,**/.vscode/**}';

    const uris = await vscode.workspace.findFiles(globPattern, excludePattern, maxResults);

    return uris.slice(0, maxResults).map((uri) => {
      const fsPath = uri.fsPath || uri.path || '';
      const label = path.basename(fsPath) || fsPath;
      const relativePath = vscode.workspace.asRelativePath
        ? vscode.workspace.asRelativePath(uri, false)
        : fsPath;

      return {
        label,
        relativePath,
        uri,
      };
    });
  }

  /**
   * Captures diagnostics for a document URI using vscode.languages.getDiagnostics.
   * Formats error and warning messages with line numbers.
   */
  public captureDiagnostics(uri: vscode.Uri): string[] {
    const diagnostics = vscode.languages.getDiagnostics(uri);
    const formatted: string[] = [];

    for (const d of diagnostics) {
      if (
        d.severity === vscode.DiagnosticSeverity.Error ||
        d.severity === vscode.DiagnosticSeverity.Warning
      ) {
        const severity = d.severity === vscode.DiagnosticSeverity.Error ? 'Error' : 'Warning';
        const line = d.range ? d.range.start.line + 1 : 1;
        const col = d.range ? d.range.start.character + 1 : 1;
        const source = d.source ? ` [${d.source}]` : '';

        formatted.push(`[${severity}] Line ${line}:${col} - ${d.message}${source}`);
      }
    }

    return formatted;
  }

  /**
   * Creates a ContextAttachment of type 'diagnostic' from document diagnostics.
   * Returns null if no errors or warnings exist for the URI.
   */
  public createDiagnosticAttachment(uri: vscode.Uri): ContextAttachment | null {
    const messages = this.captureDiagnostics(uri);
    if (messages.length === 0) {
      return null;
    }

    const relativePath = vscode.workspace.asRelativePath
      ? vscode.workspace.asRelativePath(uri, false)
      : (uri.fsPath || uri.path);

    const snippet = boundContentSnippet(
      messages.join('\n'),
      this.maxSnippetLines,
      this.maxAttachmentSizeBytes
    );

    return {
      type: 'diagnostic',
      uri: uri.toString(),
      relativePath,
      contentSnippet: snippet,
    };
  }

  /**
   * Enforces prompt bounds:
   * - Max 5 attachments per prompt
   * - Max 20KB content per attachment
   */
  public enforceAttachmentBounds(attachments: ContextAttachment[]): ContextAttachment[] {
    if (!attachments || attachments.length === 0) {
      return [];
    }

    const capped = attachments.slice(0, this.maxAttachmentsPerPrompt);

    return capped.map((att) => {
      if (!att.contentSnippet) {
        return { ...att };
      }

      return {
        ...att,
        contentSnippet: truncateToByteLimit(att.contentSnippet, this.maxAttachmentSizeBytes),
      };
    });
  }

  /**
   * Binds and adds an attachment to an existing list, enforcing the 5 attachment and 20KB bounds.
   */
  public addAttachment(
    existing: ContextAttachment[],
    newAttachment: ContextAttachment
  ): ContextAttachment[] {
    const boundedNew: ContextAttachment = newAttachment.contentSnippet
      ? {
          ...newAttachment,
          contentSnippet: truncateToByteLimit(
            newAttachment.contentSnippet,
            this.maxAttachmentSizeBytes
          ),
        }
      : { ...newAttachment };

    const combined = [...existing, boundedNew];
    return this.enforceAttachmentBounds(combined);
  }

  /**
   * Utility to bound arbitrary content snippet.
   */
  public boundSnippet(content: string): string {
    return boundContentSnippet(content, this.maxSnippetLines, this.maxAttachmentSizeBytes);
  }
}
