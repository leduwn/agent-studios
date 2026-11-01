/**
 * Agent Studios — Unified Diff Parser
 * Milestone M15 UI-03: Artifact & Diff Review Workbench
 *
 * Parses unified Git diff strings into structured files, hunks, and line annotations.
 * Detects file additions, modifications, deletions, renames, and binary files.
 */

/* eslint-disable @typescript-eslint/naming-convention */

export type FileChangeKind = 'added' | 'modified' | 'deleted' | 'renamed' | 'binary';

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

        const match = line.match(/^diff --git a\/(.+?) b\/(.+)$/);
        const oldP = match ? match[1] : '';
        const newP = match ? match[2] : '';

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
        } else if (line.startsWith('similarity index ') || line.startsWith('rename from ')) {
          currentFile.changeKind = 'renamed';
        } else if (line.startsWith('rename to ')) {
          currentFile.newPath = line.substring(10).trim();
          currentFile.displayPath = currentFile.newPath;
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
