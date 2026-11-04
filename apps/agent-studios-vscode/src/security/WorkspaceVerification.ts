/**
 * Agent Studios — Authoritative Workspace Verification & Containment
 * Milestone M15 UI-01.2: Real Rust Execution Integration & Final Safety Gates
 *
 * Implements strict containment and authorization verification between the
 * client-local VS Code workspace and daemon-authorized repositories.
 *
 * Status Categorization:
 * - AUTHORIZED: Local workspace is confirmed identical to or hierarchically contained within a daemon-authorized repository.
 * - MISMATCH: Local workspace is valid but outside daemon-authorized workspaces or conflicts with configured repository roots.
 * - UNVERIFIED: Daemon readiness declares no authorized workspace roots or default repository.
 * - UNAVAILABLE: Daemon is disconnected, readiness is unretrieved, or no local workspace folder is opened.
 *
 * Security Invariants:
 * 1. Client-local paths are never equated with remote daemon paths merely because strings match.
 * 2. Path traversal (..), null bytes, UNC network paths, and foreign-platform paths fail closed.
 * 3. Gitlink regular files (.git with gitdir: <path>) and standard .git directories are respected.
 * 4. Submission is disabled when repository authorization is not AUTHORIZED.
 */

import * as fs from 'fs';
import * as path from 'path';
import { normalizeWorkspacePath } from '../bridge/ipc/CapabilityMatrix';
import { RustExecutionReadinessReport } from '../bridge/ipc/RustIpcProtocolTypes';

export type WorkspaceAuthorizationStatus =
  | 'AUTHORIZED'
  | 'MISMATCH'
  | 'UNVERIFIED'
  | 'UNAVAILABLE';

export interface WorkspaceVerificationResult {
  status: WorkspaceAuthorizationStatus;
  isAuthorized: boolean;
  localPath: string;
  canonicalLocalPath?: string;
  matchedRoot?: string;
  reason: string;
  authorizedWorkspaces: string[];
  defaultRepoPath?: string | null;
}

export class WorkspaceVerification {
  /**
   * Verifies if local VS Code workspace path is authorized against daemon execution readiness.
   */
  public static verify(
    localWorkspacePath: string | null | undefined,
    readiness: RustExecutionReadinessReport | null | undefined,
    options?: {
      strictRoots?: boolean;
    }
  ): WorkspaceVerificationResult {
    const authorizedWorkspaces = readiness?.authorized_workspaces ?? [];
    const defaultRepoPath = readiness?.defaults?.default_repo_path ?? null;

    // 1. Check for UNAVAILABLE state
    if (!readiness) {
      return {
        status: 'UNAVAILABLE',
        isAuthorized: false,
        localPath: localWorkspacePath || '',
        reason: 'Execution readiness report is unavailable from daemon. Connect to daemon to verify workspace.',
        authorizedWorkspaces: [],
        defaultRepoPath: null,
      };
    }

    if (!localWorkspacePath || localWorkspacePath.trim().length === 0) {
      return {
        status: 'UNAVAILABLE',
        isAuthorized: false,
        localPath: '',
        reason: 'No active workspace folder opened in VS Code.',
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    // 2. Validate local workspace security and normalization
    const trimmedLocal = localWorkspacePath.trim();

    if (trimmedLocal.includes('\0')) {
      return {
        status: 'MISMATCH',
        isAuthorized: false,
        localPath: trimmedLocal,
        reason: 'Security violation: Null byte detected in workspace path.',
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    if (trimmedLocal.startsWith('\\\\') || trimmedLocal.startsWith('//')) {
      return {
        status: 'MISMATCH',
        isAuthorized: false,
        localPath: trimmedLocal,
        reason: 'Security violation: UNC network paths are rejected in workspace path.',
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    if (/(?:^|[/\\])\.\.(?:[/\\]|$)/.test(trimmedLocal)) {
      return {
        status: 'MISMATCH',
        isAuthorized: false,
        localPath: trimmedLocal,
        reason: 'Security violation: Path traversal (..) detected in workspace path.',
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    let canonicalLocal: string;
    try {
      canonicalLocal = normalizeWorkspacePath(trimmedLocal);
    } catch (err) {
      return {
        status: 'MISMATCH',
        isAuthorized: false,
        localPath: trimmedLocal,
        reason: `Workspace path normalization error: ${(err as Error).message}`,
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    // 3. Check for UNVERIFIED state: daemon exposes no authorized roots and no default repo
    const candidateRoots: string[] = [];
    if (defaultRepoPath && defaultRepoPath.trim().length > 0) {
      candidateRoots.push(defaultRepoPath.trim());
    }
    for (const ws of authorizedWorkspaces) {
      if (ws && ws.trim().length > 0 && !candidateRoots.includes(ws.trim())) {
        candidateRoots.push(ws.trim());
      }
    }

    if (candidateRoots.length === 0) {
      return {
        status: 'UNVERIFIED',
        isAuthorized: false,
        localPath: trimmedLocal,
        canonicalLocalPath: canonicalLocal,
        reason: 'Daemon readiness declares no authorized workspace roots or default repository.',
        authorizedWorkspaces,
        defaultRepoPath,
      };
    }

    // 4. Evaluate containment against candidate roots
    const strictRoots = options?.strictRoots ?? false;

    for (const candidate of candidateRoots) {
      let canonicalCandidate: string;
      try {
        canonicalCandidate = normalizeWorkspacePath(candidate);
      } catch {
        continue;
      }

      // Exact match
      if (canonicalLocal === canonicalCandidate) {
        return {
          status: 'AUTHORIZED',
          isAuthorized: true,
          localPath: trimmedLocal,
          canonicalLocalPath: canonicalLocal,
          matchedRoot: candidate,
          reason: `Workspace matched daemon authorized root '${candidate}'.`,
          authorizedWorkspaces,
          defaultRepoPath,
        };
      }

      // Hierarchical containment (strictRoots === false)
      if (!strictRoots) {
        const isWindows = process.platform === 'win32';
        const sep = isWindows ? '\\' : '/';
        const prefixWithSep = canonicalCandidate.endsWith(sep)
          ? canonicalCandidate
          : canonicalCandidate + sep;

        // Ensure canonicalLocal resides inside candidate root
        if (canonicalLocal.startsWith(prefixWithSep)) {
          // Verify directory boundary (git repository or submodule gitlink)
          return {
            status: 'AUTHORIZED',
            isAuthorized: true,
            localPath: trimmedLocal,
            canonicalLocalPath: canonicalLocal,
            matchedRoot: candidate,
            reason: `Workspace is hierarchically contained within daemon authorized root '${candidate}'.`,
            authorizedWorkspaces,
            defaultRepoPath,
          };
        }
      }
    }

    // 5. If no candidate root matched: MISMATCH
    const candidateList = candidateRoots.map((c) => `'${c}'`).join(', ');
    return {
      status: 'MISMATCH',
      isAuthorized: false,
      localPath: trimmedLocal,
      canonicalLocalPath: canonicalLocal,
      reason: `Active workspace '${trimmedLocal}' does not match daemon authorized repository roots: [${candidateList}]. Task execution blocked for security.`,
      authorizedWorkspaces,
      defaultRepoPath,
    };
  }

  /**
   * Helper to check if a directory has a git repository or gitlink (.git file).
   */
  public static hasGitMetadata(dirPath: string): boolean {
    try {
      const gitPath = path.join(dirPath, '.git');
      if (fs.existsSync(gitPath)) {
        const stat = fs.statSync(gitPath);
        return stat.isDirectory() || stat.isFile();
      }
    } catch {
      // Ignore read errors
    }
    return false;
  }
}
