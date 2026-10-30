/**
 * Agent Studios — Webview Message Security Guards & Schema Validation
 * Milestone M14-UI-02: Extension Hardening & Security Assurance
 *
 * Implements runtime message validation, prototype pollution guards,
 * path traversal detection, command schemas, and workspace trust enforcement
 * for the extension-webview bridge.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';

export const ALLOWED_WEBVIEW_COMMANDS = new Set([
  'sendMessage',
  'submitTask',
  'cancelTask',
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
] as const);

export type AllowedWebviewCommand = typeof ALLOWED_WEBVIEW_COMMANDS extends Set<infer T> ? T : never;

/* eslint-disable @typescript-eslint/naming-convention */
export const SECURITY_LIMITS = {
  MAX_MESSAGE_BYTES: 512 * 1024,      // 512 KiB max message payload
  MAX_TEXT_LENGTH: 128 * 1024,        // 128 KiB max prompt / draft
  MAX_PATH_LENGTH: 2048,              // 2048 chars max path
  MAX_ID_LENGTH: 128,                 // 128 chars max ID
  MAX_ATTACHMENTS: 10,                // 10 attachments max
  MAX_OBJECT_DEPTH: 5,                // Max recursion depth for object parsing
} as const;
/* eslint-enable @typescript-eslint/naming-convention */

export interface ValidationResult {
  valid: boolean;
  error?: string;
}

export class ChatSecurityGuards {
  /**
   * Validates incoming raw webview message against prototype pollution,
   * command allowlist, size boundaries, and command-specific schemas.
   */
  public static validateWebviewMessage(message: unknown): ValidationResult {
    if (!message || typeof message !== 'object' || Array.isArray(message)) {
      return { valid: false, error: 'Invalid webview message: must be a non-null object.' };
    }

    // 1. Prototype pollution & depth guard (fails closed on depth > MAX_OBJECT_DEPTH or prohibited keys)
    const pollutionResult = this.checkPrototypePollution(message);
    if (pollutionResult.hasPollution) {
      return { valid: false, error: `Rejected message: ${pollutionResult.reason}` };
    }

    const record = message as Record<string, unknown>;

    // 2. Command allowlist check
    if (typeof record.command !== 'string' || !ALLOWED_WEBVIEW_COMMANDS.has(record.command as AllowedWebviewCommand)) {
      return { valid: false, error: `Disallowed or unknown webview command: '${String(record.command)}'.` };
    }

    // 3. Path traversal checks on generic path-bearing properties
    for (const key of ['path', 'filePath', 'targetPath']) {
      if (typeof record[key] === 'string') {
        const pathVal = record[key] as string;
        if (this.isPathTraversal(pathVal)) {
          return { valid: false, error: `Detected path traversal attempt in '${key}': '${pathVal}'.` };
        }
      }
    }

    // 4. Command-specific schema validation & field bounds
    return this.validateCommandSchema(record.command, record);
  }

  /**
   * Recursively checks object and array structures for prototype pollution keys.
   * Fails closed if nesting depth exceeds MAX_OBJECT_DEPTH.
   */
  public static checkPrototypePollution(
    val: unknown,
    depth = 0
  ): { hasPollution: boolean; reason?: string } {
    if (depth > SECURITY_LIMITS.MAX_OBJECT_DEPTH) {
      return {
        hasPollution: true,
        reason: `Maximum object nesting depth (${SECURITY_LIMITS.MAX_OBJECT_DEPTH}) exceeded: fail-closed.`,
      };
    }

    if (!val || typeof val !== 'object') {
      return { hasPollution: false };
    }

    if (Array.isArray(val)) {
      for (let i = 0; i < val.length; i++) {
        const itemResult = this.checkPrototypePollution(val[i], depth + 1);
        if (itemResult.hasPollution) {
          return itemResult;
        }
      }
      return { hasPollution: false };
    }

    const obj = val as Record<string, unknown>;
    const keys = Object.getOwnPropertyNames(obj);

    for (const key of keys) {
      if (key === '__proto__' || key === 'constructor' || key === 'prototype') {
        return {
          hasPollution: true,
          reason: `Prohibited prototype pollution key detected: '${key}'.`,
        };
      }

      const propVal = obj[key];
      if (propVal && typeof propVal === 'object') {
        const nestedResult = this.checkPrototypePollution(propVal, depth + 1);
        if (nestedResult.hasPollution) {
          return nestedResult;
        }
      }
    }

    return { hasPollution: false };
  }

  /**
   * Backwards compatible helper for existing callers.
   */
  public static hasPrototypePollution(obj: Record<string, unknown>, depth = 0): boolean {
    return this.checkPrototypePollution(obj, depth).hasPollution;
  }

  /**
   * Validates command-specific parameters and bounds.
   */
  public static validateCommandSchema(command: string, record: Record<string, unknown>): ValidationResult {
    switch (command) {
      case 'sendMessage': {
        if (record.text !== undefined) {
          if (typeof record.text !== 'string') {
            return { valid: false, error: "Field 'text' must be a string." };
          }
          if (record.text.length > SECURITY_LIMITS.MAX_TEXT_LENGTH) {
            return { valid: false, error: `Field 'text' exceeds limit of ${SECURITY_LIMITS.MAX_TEXT_LENGTH} chars.` };
          }
        }
        if (record.mode !== undefined) {
          if (record.mode !== 'agent' && record.mode !== 'plan' && record.mode !== 'ask') {
            return { valid: false, error: `Invalid mode '${String(record.mode)}'. Must be 'agent', 'plan', or 'ask'.` };
          }
        }
        if (record.attachments !== undefined) {
          if (!Array.isArray(record.attachments)) {
            return { valid: false, error: "Field 'attachments' must be an array." };
          }
          if (record.attachments.length > SECURITY_LIMITS.MAX_ATTACHMENTS) {
            return { valid: false, error: `Attachments count exceeds maximum limit of ${SECURITY_LIMITS.MAX_ATTACHMENTS}.` };
          }
          for (const att of record.attachments) {
            if (!att || typeof att !== 'object' || Array.isArray(att)) {
              return { valid: false, error: 'Attachment item must be an object.' };
            }
            const attRec = att as Record<string, unknown>;
            if (typeof attRec.path === 'string') {
              if (attRec.path.length > SECURITY_LIMITS.MAX_PATH_LENGTH || this.isPathTraversal(attRec.path)) {
                return { valid: false, error: `Attachment path is invalid or contains traversal: '${attRec.path}'.` };
              }
            }
          }
        }
        if (record.modelId !== undefined && (typeof record.modelId !== 'string' || record.modelId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'modelId' parameter." };
        }
        if (record.providerId !== undefined && (typeof record.providerId !== 'string' || record.providerId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'providerId' parameter." };
        }
        return { valid: true };
      }

      case 'saveDraft': {
        if (record.draft !== undefined) {
          if (typeof record.draft !== 'string') {
            return { valid: false, error: "Field 'draft' must be a string." };
          }
          if (record.draft.length > SECURITY_LIMITS.MAX_TEXT_LENGTH) {
            return { valid: false, error: `Field 'draft' exceeds maximum length of ${SECURITY_LIMITS.MAX_TEXT_LENGTH} chars.` };
          }
        }
        return { valid: true };
      }

      case 'approveAction':
      case 'denyAction': {
        if (typeof record.approvalId !== 'string' || record.approvalId.trim().length === 0 || record.approvalId.length > SECURITY_LIMITS.MAX_ID_LENGTH) {
          return { valid: false, error: `Command '${command}' requires non-empty string 'approvalId' (max ${SECURITY_LIMITS.MAX_ID_LENGTH} chars).` };
        }
        if (!/^[a-zA-Z0-9_\-.:]+$/.test(record.approvalId)) {
          return { valid: false, error: `Command '${command}' approvalId contains invalid characters.` };
        }
        return { valid: true };
      }

      case 'openDiff': {
        if (record.artifactId !== undefined && (typeof record.artifactId !== 'string' || record.artifactId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'artifactId' parameter." };
        }
        if (record.filePath !== undefined) {
          if (typeof record.filePath !== 'string' || record.filePath.length > SECURITY_LIMITS.MAX_PATH_LENGTH) {
            return { valid: false, error: "Invalid 'filePath' parameter." };
          }
          const pathValidation = this.validateWorkspaceOrVirtualPath(record.filePath);
          if (!pathValidation.valid) {
            return pathValidation;
          }
        }
        return { valid: true };
      }

      case 'attachContext': {
        if (record.source !== undefined && record.source !== 'filePicker' && record.source !== 'activeEditor') {
          return { valid: false, error: "Field 'source' must be 'filePicker' or 'activeEditor'." };
        }
        return { valid: true };
      }

      case 'removeAttachment': {
        if (record.path !== undefined) {
          if (typeof record.path !== 'string' || record.path.length > SECURITY_LIMITS.MAX_PATH_LENGTH) {
            return { valid: false, error: "Command 'removeAttachment' requires a valid string 'path'." };
          }
          if (this.isPathTraversal(record.path)) {
            return { valid: false, error: `Command 'removeAttachment' path contains traversal: '${record.path}'.` };
          }
        }
        return { valid: true };
      }

      case 'switchMode': {
        if (record.mode !== undefined && record.mode !== 'agent' && record.mode !== 'plan' && record.mode !== 'ask') {
          return { valid: false, error: `Invalid mode '${String(record.mode)}'. Must be 'agent', 'plan', or 'ask'.` };
        }
        return { valid: true };
      }

      case 'switchProviderModel': {
        if (record.modelId !== undefined && (typeof record.modelId !== 'string' || record.modelId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'modelId' parameter." };
        }
        if (record.providerId !== undefined && (typeof record.providerId !== 'string' || record.providerId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'providerId' parameter." };
        }
        return { valid: true };
      }

      case 'switchSession': {
        if (record.sessionId !== undefined && (typeof record.sessionId !== 'string' || record.sessionId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Command 'switchSession' requires a valid string 'sessionId'." };
        }
        return { valid: true };
      }

      case 'submitTask': {
        if (typeof record.prompt !== 'string' || record.prompt.trim().length === 0) {
          return { valid: false, error: "Command 'submitTask' requires non-empty string 'prompt'." };
        }
        if (record.prompt.length > 4 * 1024 * 1024) {
          return { valid: false, error: "Prompt exceeds 4 MiB wire frame limit." };
        }
        if (record.studioId !== undefined && (typeof record.studioId !== 'string' || record.studioId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'studioId' parameter." };
        }
        if (record.modelId !== undefined && (typeof record.modelId !== 'string' || record.modelId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'modelId' parameter." };
        }
        if (record.providerId !== undefined && (typeof record.providerId !== 'string' || record.providerId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'providerId' parameter." };
        }
        if (record.runtime !== undefined && typeof record.runtime !== 'string') {
          return { valid: false, error: "Invalid 'runtime' parameter." };
        }
        return { valid: true };
      }

      case 'cancelTask': {
        if (record.taskId !== undefined && (typeof record.taskId !== 'string' || record.taskId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'taskId' parameter." };
        }
        if (record.studioId !== undefined && (typeof record.studioId !== 'string' || record.studioId.length > SECURITY_LIMITS.MAX_ID_LENGTH)) {
          return { valid: false, error: "Invalid 'studioId' parameter." };
        }
        return { valid: true };
      }

      default:
        return { valid: true };
    }
  }

  /**
   * Validates path traversal defense and workspace containment.
   */
  public static validateWorkspaceOrVirtualPath(candidatePath: string): ValidationResult {
    if (!candidatePath || typeof candidatePath !== 'string') {
      return { valid: false, error: 'Empty or non-string path.' };
    }

    if (candidatePath.length > SECURITY_LIMITS.MAX_PATH_LENGTH) {
      return { valid: false, error: `Path length exceeds ${SECURITY_LIMITS.MAX_PATH_LENGTH} characters.` };
    }

    // Allow virtual diff protocol
    if (candidatePath.startsWith('agent-studios-diff://')) {
      return { valid: true };
    }

    if (this.isPathTraversal(candidatePath)) {
      return { valid: false, error: `Detected path traversal attempt: '${candidatePath}'.` };
    }

    return this.validateCanonicalWorkspacePath(candidatePath);
  }

  /**
   * Detects path traversal markers such as '..', null bytes, UNC paths, and dangerous schemes.
   */
  public static isPathTraversal(candidatePath: string): boolean {
    if (!candidatePath || typeof candidatePath !== 'string') {
      return false;
    }

    // Null-byte injection check
    if (candidatePath.includes('\0') || candidatePath.includes('%00')) {
      return true;
    }

    // Reject UNC path traversal (e.g. \\host\share or //host/share)
    if (/^[\\/]{2}/.test(candidatePath)) {
      return true;
    }

    // Encoded traversal sequences
    const decoded = decodeURIComponentSafe(candidatePath);
    const normalized = decoded.replace(/\\/g, '/');

    // Relative path breakout check
    const segments = normalized.split('/');
    if (segments.some((segment) => segment === '..')) {
      return true;
    }

    // URI scheme check (exclude single letter Windows drive prefixes like C:)
    const schemeMatch = normalized.match(/^([a-zA-Z][a-zA-Z0-9+.-]*):/);
    if (schemeMatch) {
      const scheme = schemeMatch[1].toLowerCase();
      // Allow single letter Windows drive
      if (scheme.length === 1 && /^[a-z]$/.test(scheme)) {
        return false;
      }
      if (scheme !== 'agent-studios-diff') {
        return true;
      }
    }

    return false;
  }

  /**
   * Enforces that candidatePath is strictly contained inside an open VS Code workspace folder.
   */
  public static validateCanonicalWorkspacePath(
    candidatePath: string,
    workspaceFolders?: readonly vscode.WorkspaceFolder[]
  ): ValidationResult {
    const folders = workspaceFolders ?? (vscode.workspace ? vscode.workspace.workspaceFolders : undefined);

    // If no workspace open, only relative clean paths without escape permitted
    if (!folders || folders.length === 0) {
      if (path.isAbsolute(candidatePath)) {
        return { valid: false, error: `Absolute path '${candidatePath}' disallowed without active workspace.` };
      }
      return { valid: true };
    }

    const isWindows = process.platform === 'win32';

    // If path is relative, verify it does not start with / or \ or contain ..
    if (!path.isAbsolute(candidatePath)) {
      const normalized = candidatePath.replace(/\\/g, '/');
      if (normalized.startsWith('/') || normalized.startsWith('\\') || normalized.split('/').includes('..')) {
        return { valid: false, error: `Relative path '${candidatePath}' contains invalid traversal.` };
      }
      return { valid: true };
    }

    for (const folder of folders) {
      const rootFsPath = folder.uri.fsPath;
      if (!rootFsPath) {
        continue;
      }

      const resolved = path.resolve(candidatePath);

      // Canonicalize case on Windows
      const canonicalRoot = isWindows ? rootFsPath.toLowerCase() : rootFsPath;
      const canonicalTarget = isWindows ? resolved.toLowerCase() : resolved;

      // Ensure target path is within workspace root with exact boundary
      if (canonicalTarget === canonicalRoot || canonicalTarget.startsWith(canonicalRoot + (canonicalRoot.endsWith(path.sep) ? '' : path.sep))) {
        // If file exists on disk, verify symlink target does not escape workspace
        try {
          if (fs.existsSync(resolved)) {
            const realTarget = fs.realpathSync(resolved);
            const realRoot = fs.realpathSync(rootFsPath);
            const canonicalRealRoot = isWindows ? realRoot.toLowerCase() : realRoot;
            const canonicalRealTarget = isWindows ? realTarget.toLowerCase() : realTarget;

            if (canonicalRealTarget !== canonicalRealRoot && !canonicalRealTarget.startsWith(canonicalRealRoot + (canonicalRealRoot.endsWith(path.sep) ? '' : path.sep))) {
              return { valid: false, error: `Symlink target '${realTarget}' escapes workspace root.` };
            }
          }
        } catch {
          // File does not exist yet or fs access restricted; path boundary check passed
        }

        return { valid: true };
      }
    }

    return { valid: false, error: `Path '${candidatePath}' is outside the active workspace folders.` };
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
}

function decodeURIComponentSafe(uri: string): string {
  try {
    return decodeURIComponent(uri);
  } catch {
    return uri;
  }
}
