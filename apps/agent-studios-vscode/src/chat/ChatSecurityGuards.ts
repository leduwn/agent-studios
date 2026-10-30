/**
 * Agent Studios — Webview Message Security Guards & Schema Validation
 * Milestone M14-UI-02: Extension Hardening & Security Assurance
 *
 * Implements runtime message validation, prototype pollution guards,
 * path traversal detection, and workspace trust enforcement for extension-webview bridge.
 */

import * as vscode from 'vscode';

export const ALLOWED_WEBVIEW_COMMANDS = new Set([
  'sendMessage',
  'cancelRun',
  'approveAction',
  'denyAction',
  'openDiff',
  'attachContext',
  'removeAttachment',
  'switchMode',
  'switchProviderModel',
  'switchSession',
  'newSession',
  'openTaskGraph',
  'saveDraft',
  'enablePreviewMode',
  'connectDaemon',
  'dismissOnboarding',
]);

export interface ValidationResult {
  valid: boolean;
  error?: string;
}

export class ChatSecurityGuards {
  /**
   * Validates incoming raw webview message against prototype pollution, command allowlist,
   * and illegal payload properties.
   */
  public static validateWebviewMessage(message: unknown): ValidationResult {
    if (!message || typeof message !== 'object' || Array.isArray(message)) {
      return { valid: false, error: 'Invalid webview message: must be a non-null object.' };
    }

    const record = message as Record<string, unknown>;

    // 1. Prototype pollution guard
    if (this.hasPrototypePollution(record)) {
      return { valid: false, error: 'Rejected message with prototype pollution payload.' };
    }

    // 2. Command allowlist check
    if (typeof record.command !== 'string' || !ALLOWED_WEBVIEW_COMMANDS.has(record.command)) {
      return { valid: false, error: `Disallowed or unknown webview command: '${String(record.command)}'.` };
    }

    // 3. Path traversal checks on path-bearing properties
    for (const key of ['path', 'filePath', 'targetPath']) {
      if (typeof record[key] === 'string') {
        const pathVal = record[key] as string;
        if (this.isPathTraversal(pathVal)) {
          return { valid: false, error: `Detected path traversal attempt in '${key}': '${pathVal}'.` };
        }
      }
    }

    return { valid: true };
  }

  /**
   * Checks whether the current workspace is trusted.
   * If untrusted, mutating actions (sending messages, executing approvals, attaching files) are blocked.
   */
  public static verifyWorkspaceTrust(command: string): boolean {
    const mutatingCommands = new Set([
      'sendMessage',
      'approveAction',
      'denyAction',
      'attachContext',
      'newSession',
    ]);

    if (!mutatingCommands.has(command)) {
      return true;
    }

    // vscode.workspace.isTrusted defaults to true if workspace trust is not supported or disabled
    if (vscode.workspace && vscode.workspace.isTrusted === false) {
      return false;
    }

    return true;
  }

  /**
   * Detects path traversal markers such as '..', null bytes, or dangerous protocols.
   */
  public static isPathTraversal(candidatePath: string): boolean {
    if (!candidatePath || typeof candidatePath !== 'string') {
      return false;
    }

    // Null-byte injection check
    if (candidatePath.includes('\0') || candidatePath.includes('%00')) {
      return true;
    }

    // Relative path breakout check
    const normalized = candidatePath.replace(/\\/g, '/');
    const segments = normalized.split('/');
    if (segments.some((segment) => segment === '..')) {
      return true;
    }

    // Scheme traversal check (e.g. javascript:, data:, file:///)
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(candidatePath) && !candidatePath.startsWith('agent-studios-diff://')) {
      return true;
    }

    return false;
  }

  /**
   * Recursively checks if an object or any nested property contains forbidden prototype keys.
   */
  public static hasPrototypePollution(obj: Record<string, unknown>, depth = 0): boolean {
    if (depth > 5) {
      return false;
    }

    for (const key of Object.keys(obj)) {
      if (key === '__proto__' || key === 'constructor' || key === 'prototype') {
        return true;
      }

      const val = obj[key];
      if (val && typeof val === 'object' && !Array.isArray(val)) {
        if (this.hasPrototypePollution(val as Record<string, unknown>, depth + 1)) {
          return true;
        }
      }
    }

    return false;
  }
}
