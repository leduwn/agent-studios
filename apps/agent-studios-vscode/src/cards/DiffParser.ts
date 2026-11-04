/**
 * Agent Studios — Unified Diff Parser
 * Milestone M15 UI-03: Artifact & Diff Review Workbench
 *
 * Parses unified Git diff strings into structured files, hunks, and line annotations.
 * Detects file additions, modifications, deletions, renames, and binary files.
 * Handles Git C-style quoted paths with octal UTF-8 escapes (including Vietnamese diacritics).
 */

/* eslint-disable @typescript-eslint/naming-convention */

export type FileChangeKind = 'added' | 'modified' | 'deleted' | 'renamed' | 'binary' | 'unknown';

export interface DiffHunkLine {
  type: 'addition' | 'deletion' | 'context' | 'header';
  text: string;
  oldLineNumber?: number;
  newLineNumber?: number;
}

export interface DiffHunk {
  header: string;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffHunkLine[];
}

export interface ParsedDiffFile {
  oldPath?: string;
  newPath: string;
  displayPath: string;
  changeKind: FileChangeKind;
  isBinary: boolean;
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
  rawPatch: string;
  hasPathTraversal?: boolean;
}

export interface ParsedDiff {
  files: ParsedDiffFile[];
  totalAdditions: number;
  totalDeletions: number;
  totalFiles: number;
}

/**
 * Decodes Git C-style quoted path string containing octal UTF-8 escapes (e.g. "\341\273\207").
 */
export function unquoteGitPath(str: string): string {
  if (str.length < 2 || !str.startsWith('"') || !str.endsWith('"')) {
    return str;
  }
  const inner = str.slice(1, -1);
  const bytes: number[] = [];
  let i = 0;
  while (i < inner.length) {
    if (inner[i] === '\\') {
      i++;
      if (i >= inner.length) {
        bytes.push('\\'.charCodeAt(0));
        break;
      }
      const c = inner[i];
      if (c >= '0' && c <= '7') {
        let octalStr = c;
        for (let j = 0; j < 2 && i + 1 < inner.length; j++) {
          const nextC = inner[i + 1];
          if (nextC >= '0' && nextC <= '7') {
            octalStr += nextC;
            i++;
          } else {
            break;
          }
        }
        bytes.push(parseInt(octalStr, 8));
      } else if (c === 'n') {
        bytes.push(10);
      } else if (c === 'r') {
        bytes.push(13);
      } else if (c === 't') {
        bytes.push(9);
      } else if (c === 'b') {
        bytes.push(8);
      } else if (c === 'f') {
        bytes.push(12);
      } else if (c === 'v') {
        bytes.push(11);
      } else if (c === 'a') {
        bytes.push(7);
      } else if (c === '\\' || c === '"') {
        bytes.push(c.charCodeAt(0));
      } else {
        const codePoint = c.codePointAt(0)!;
        const char = String.fromCodePoint(codePoint);
        const encoded = Buffer.from(char, 'utf8');
        for (let b = 0; b < encoded.length; b++) {
          bytes.push(encoded[b]);
        }
      }
    } else {
      const codePoint = inner.codePointAt(i)!;
      const char = String.fromCodePoint(codePoint);
      const encoded = Buffer.from(char, 'utf8');
      for (let b = 0; b < encoded.length; b++) {
        bytes.push(encoded[b]);
      }
      if (char.length > 1) {
        i += char.length - 1;
      }
    }
    i++;
  }
  const decoder = new TextDecoder('utf-8', { fatal: false });
  return decoder.decode(new Uint8Array(bytes));
}

/**
 * Detects whether a path contains path traversal sequences, absolute paths, or control characters.
 */
export function isPathTraversal(rawPath?: string | null): boolean {
  if (!rawPath || typeof rawPath !== 'string') {
    return false;
  }
  // Check for control characters (ASCII 0-31 and 127)
  if (/[\x00-\x1f\x7f]/.test(rawPath)) {
    return true;
  }
  // Check for directory traversal sequences (.. or ../ or ..\ or /.. or \..)
  if (/(?:^|[/\\])\.\.(?:[/\\]|$)/.test(rawPath)) {
    return true;
  }
  // Check for Windows drive letter prefix (e.g. C:\, D:/)
  if (/^[a-zA-Z]:[/\\]?/.test(rawPath)) {
    return true;
  }
  // Check for UNC paths (\\server\share or //server/share)
  if (/^[/\\]{2}/.test(rawPath)) {
    return true;
  }
  // Check for Unix root absolute path (starts with / or \)
  if (/^[/\\]/.test(rawPath)) {
    return true;
  }
  // Check for sensitive system paths like /etc/
  if (/(?:^|[/\\])etc(?:[/\\]|$)/i.test(rawPath)) {
    return true;
  }
  return false;
}

/**
 * Sanitizes an artifact or task ID by removing directory traversal,
 * absolute path prefixes, drive letters, and control characters.
 */
export function sanitizeArtifactId(rawId: string): string {
  if (!rawId || typeof rawId !== 'string') {
    return 'unknown';
  }
  // Strip control characters
  let sanitized = rawId.replace(/[\x00-\x1f\x7f]/g, '');
  // Strip Windows drive letters
  sanitized = sanitized.replace(/^[a-zA-Z]:[/\\]*/, '');
  // Strip traversal segments
  sanitized = sanitized.replace(/(?:^|[/\\])\.\.(?=[/\\]|$)/g, '');
  // Strip leading slashes
  sanitized = sanitized.replace(/^[/\\]+/, '');
  // Strip /etc/ or etc/ prefix
  sanitized = sanitized.replace(/^(?:etc[/\\]+)+/i, '');
  // Strip leading slashes again if any left
  sanitized = sanitized.replace(/^[/\\]+/, '');
  // Normalize multiple slashes to underscore
  sanitized = sanitized.replace(/[/\\]+/g, '_');
  sanitized = sanitized.replace(/^_+|_+$/g, '');
  sanitized = sanitized.trim();
  if (sanitized.length === 0 || sanitized === '.' || sanitized === '..') {
    return 'safe_artifact';
  }
  return sanitized;
}

/**
 * Sanitizes a file path from a diff header:
 * - Unquotes Git C-style quotes
 * - Strips control characters
 * - Strips Windows drive letters and UNC prefixes
 * - Strips leading absolute slashes
 * - Safely eliminates traversal segments (..)
 */
export function sanitizePath(rawPath?: string | null): string {
  if (!rawPath || typeof rawPath !== 'string') {
    return '';
  }
  let pathStr = unquoteGitPath(rawPath.trim());
  // Strip control characters
  pathStr = pathStr.replace(/[\x00-\x1f\x7f]/g, '');
  // Strip Windows drive letters (e.g. C:\)
  pathStr = pathStr.replace(/^[a-zA-Z]:[/\\]*/, '');
  // Strip UNC / leading slashes
  pathStr = pathStr.replace(/^[/\\]+/, '');
  // Normalize backslashes to forward slashes
  pathStr = pathStr.replace(/\\/g, '/');
  // Strip leading etc/ prefix
  pathStr = pathStr.replace(/^(?:etc\/)+/i, '');
  // Split segments and filter out '.' and resolve '..' safely
  const segments = pathStr.split('/').filter(Boolean);
  const safeSegments: string[] = [];
  for (const seg of segments) {
    if (seg === '.') {
      continue;
    }
    if (seg === '..') {
      if (safeSegments.length > 0) {
        safeSegments.pop();
      }
      continue;
    }
    safeSegments.push(seg);
  }
  while (safeSegments.length > 0 && safeSegments[0].toLowerCase() === 'etc') {
    safeSegments.shift();
  }
  return safeSegments.join('/');
}

/**
 * Strips Git a/ and b/ diff prefixes and unquotes path.
 * Optionally sanitizes path traversal sequences if sanitize is true.
 */
export function cleanGitPath(raw: string, sanitize = false): string {
  let unquoted = unquoteGitPath(raw.trim());
  if (unquoted.startsWith('a/')) {
    unquoted = unquoted.substring(2);
  } else if (unquoted.startsWith('b/')) {
    unquoted = unquoted.substring(2);
  }
  if (sanitize) {
    return sanitizePath(unquoted);
  }
  return unquoted;
}

export interface ParseDiffGitHeaderOptions {
  rejectTraversal?: boolean;
}

/**
 * Parses a `diff --git ...` header line supporting quoted and unquoted paths with spaces.
 */
export function parseDiffGitHeader(
  line: string,
  options?: ParseDiffGitHeaderOptions
): { oldPath: string; newPath: string } | null {
  if (!line.startsWith('diff --git ')) {
    return null;
  }
  const rest = line.substring(11).trim();
  const match = rest.match(/^(?:"((?:\\.|[^"\\])*)"|(\S+))\s+(?:"((?:\\.|[^"\\])*)"|(\S+))$/);
  let oldPath = '';
  let newPath = '';

  if (match) {
    const rawOld = match[1] !== undefined ? `"${match[1]}"` : match[2];
    const rawNew = match[3] !== undefined ? `"${match[3]}"` : match[4];
    oldPath = cleanGitPath(rawOld);
    newPath = cleanGitPath(rawNew);
  } else {
    const unquotedMatch = rest.match(/^a\/(.+?)\s+b\/(.+)$/);
    if (unquotedMatch) {
      oldPath = unquotedMatch[1];
      newPath = unquotedMatch[2];
    } else {
      return null;
    }
  }

  if (options?.rejectTraversal) {
    if (isPathTraversal(oldPath) || isPathTraversal(newPath)) {
      return null;
    }
  }

  return { oldPath, newPath };
}

export interface ParseUnifiedDiffOptions {
  rejectTraversal?: boolean;
  sanitizePaths?: boolean;
}

export class DiffParser {
  /**
   * Parses a raw unified diff string into structured file entries and hunks.
   */
  public static parseUnifiedDiff(rawDiff: string, options?: ParseUnifiedDiffOptions): ParsedDiff {
    if (!rawDiff || rawDiff.trim().length === 0) {
      return {
        files: [],
        totalAdditions: 0,
        totalDeletions: 0,
        totalFiles: 0,
      };
    }

    const lines = rawDiff.split(/\r?\n/);
    const files: ParsedDiffFile[] = [];
    let currentFile: Partial<ParsedDiffFile> | null = null;
    let currentHunk: DiffHunk | null = null;
    let fileLines: string[] = [];

    const finalizeFile = () => {
      if (currentFile && currentFile.newPath) {
        if (currentHunk) {
          currentFile.hunks!.push(currentHunk);
          currentHunk = null;
        }
        currentFile.rawPatch = fileLines.join('\n');

        const hasTraversal =
          isPathTraversal(currentFile.newPath) ||
          (currentFile.oldPath ? isPathTraversal(currentFile.oldPath) : false);
        currentFile.hasPathTraversal = hasTraversal;

        if (options?.rejectTraversal && hasTraversal) {
          currentFile = null;
          fileLines = [];
          return;
        }

        if (options?.sanitizePaths) {
          if (currentFile.displayPath) {
            currentFile.displayPath = sanitizePath(currentFile.displayPath);
          }
          if (currentFile.newPath) {
            currentFile.newPath = sanitizePath(currentFile.newPath);
          }
          if (currentFile.oldPath) {
            currentFile.oldPath = sanitizePath(currentFile.oldPath);
          }
        }

        files.push(currentFile as ParsedDiffFile);
        currentFile = null;
        fileLines = [];
      }
    };

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      if (line.startsWith('diff --git ')) {
        finalizeFile();

        const paths = parseDiffGitHeader(line, options);
        if (options?.rejectTraversal && !paths) {
          currentFile = null;
          fileLines = [];
          continue;
        }

        const oldP = paths ? paths.oldPath : '';
        const newP = paths ? paths.newPath : '';
        const hasTraversal = isPathTraversal(oldP) || isPathTraversal(newP);

        currentFile = {
          oldPath: oldP,
          newPath: newP || oldP || 'unknown',
          displayPath: newP || oldP || 'unknown',
          changeKind: 'modified',
          isBinary: false,
          hunks: [],
          additions: 0,
          deletions: 0,
          rawPatch: '',
          hasPathTraversal: hasTraversal,
        };
        fileLines = [line];
        continue;
      }

      if (currentFile) {
        fileLines.push(line);

        if (line.startsWith('new file mode ')) {
          currentFile.changeKind = 'added';
        } else if (line.startsWith('deleted file mode ')) {
          currentFile.changeKind = 'deleted';
        } else if (line.startsWith('rename from ')) {
          currentFile.changeKind = 'renamed';
          currentFile.oldPath = cleanGitPath(line.substring(12));
        } else if (line.startsWith('rename to ')) {
          currentFile.changeKind = 'renamed';
          const toPath = cleanGitPath(line.substring(10));
          currentFile.newPath = toPath;
          currentFile.displayPath = toPath;
        } else if (line.startsWith('Binary files ') || line.startsWith('GIT binary patch')) {
          currentFile.isBinary = true;
          currentFile.changeKind = 'binary';
        } else if (line.startsWith('@@ ')) {
          if (currentHunk) {
            currentFile.hunks!.push(currentHunk);
          }

          const hunkMatch = line.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$/);
          if (hunkMatch) {
            const oldStart = parseInt(hunkMatch[1], 10);
            const oldLines = hunkMatch[2] ? parseInt(hunkMatch[2], 10) : 1;
            const newStart = parseInt(hunkMatch[3], 10);
            const newLines = hunkMatch[4] ? parseInt(hunkMatch[4], 10) : 1;

            currentHunk = {
              header: line,
              oldStart,
              oldLines,
              newStart,
              newLines,
              lines: [
                {
                  type: 'header',
                  text: line,
                },
              ],
            };
          } else {
            currentHunk = {
              header: line,
              oldStart: 0,
              oldLines: 0,
              newStart: 0,
              newLines: 0,
              lines: [{ type: 'header', text: line }],
            };
          }
        } else if (currentHunk) {
          if (line.startsWith('+') && !line.startsWith('+++')) {
            currentFile.additions!++;
            currentHunk.lines.push({
              type: 'addition',
              text: line,
            });
          } else if (line.startsWith('-') && !line.startsWith('---')) {
            currentFile.deletions!++;
            currentHunk.lines.push({
              type: 'deletion',
              text: line,
            });
          } else if (line.startsWith(' ') || line === '') {
            currentHunk.lines.push({
              type: 'context',
              text: line,
            });
          }
        }
      }
    }

    finalizeFile();

    if (files.length === 0 && rawDiff.trim().length > 0) {
      if (options?.rejectTraversal) {
        return {
          files: [],
          totalAdditions: 0,
          totalDeletions: 0,
          totalFiles: 0,
        };
      }
      // Unclassified / unparsed raw patch fallback
      return {
        files: [
          {
            oldPath: undefined,
            newPath: 'RAW-DIFF',
            displayPath: 'RAW-DIFF (Unclassified)',
            changeKind: 'unknown',
            isBinary: rawDiff.includes('GIT binary patch') || rawDiff.includes('\0'),
            hunks: [],
            additions: 0,
            deletions: 0,
            rawPatch: rawDiff,
          },
        ],
        totalAdditions: 0,
        totalDeletions: 0,
        totalFiles: 1,
      };
    }

    let totalAdditions = 0;
    let totalDeletions = 0;
    for (const f of files) {
      totalAdditions += f.additions;
      totalDeletions += f.deletions;
    }

    return {
      files,
      totalAdditions,
      totalDeletions,
      totalFiles: files.length,
    };
  }
}
