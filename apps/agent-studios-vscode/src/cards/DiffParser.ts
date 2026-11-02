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
  if (!str.startsWith('"') || !str.endsWith('"')) {
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
        bytes.push(c.charCodeAt(0));
      }
    } else {
      bytes.push(inner.charCodeAt(i));
    }
    i++;
  }
  const decoder = new TextDecoder('utf-8', { fatal: false });
  return decoder.decode(new Uint8Array(bytes));
}

/**
 * Strips Git a/ and b/ diff prefixes and unquotes path.
 */
export function cleanGitPath(raw: string): string {
  let unquoted = unquoteGitPath(raw.trim());
  if (unquoted.startsWith('a/')) {
    unquoted = unquoted.substring(2);
  } else if (unquoted.startsWith('b/')) {
    unquoted = unquoted.substring(2);
  }
  return unquoted;
}

/**
 * Parses a `diff --git ...` header line supporting quoted and unquoted paths with spaces.
 */
export function parseDiffGitHeader(line: string): { oldPath: string; newPath: string } | null {
  if (!line.startsWith('diff --git ')) {
    return null;
  }
  const rest = line.substring(11).trim();
  const match = rest.match(/^(?:"((?:\\.|[^"\\])*)"|(\S+))\s+(?:"((?:\\.|[^"\\])*)"|(\S+))$/);
  if (match) {
    const rawOld = match[1] !== undefined ? `"${match[1]}"` : match[2];
    const rawNew = match[3] !== undefined ? `"${match[3]}"` : match[4];
    return {
      oldPath: cleanGitPath(rawOld),
      newPath: cleanGitPath(rawNew),
    };
  }

  const unquotedMatch = rest.match(/^a\/(.+?)\s+b\/(.+)$/);
  if (unquotedMatch) {
    return {
      oldPath: unquotedMatch[1],
      newPath: unquotedMatch[2],
    };
  }

  return null;
}

export class DiffParser {
  /**
   * Parses a raw unified diff string into structured file entries and hunks.
   */
  public static parseUnifiedDiff(rawDiff: string): ParsedDiff {
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
        files.push(currentFile as ParsedDiffFile);
        currentFile = null;
        fileLines = [];
      }
    };

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      if (line.startsWith('diff --git ')) {
        finalizeFile();

        const paths = parseDiffGitHeader(line);
        const oldP = paths ? paths.oldPath : '';
        const newP = paths ? paths.newPath : '';

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
