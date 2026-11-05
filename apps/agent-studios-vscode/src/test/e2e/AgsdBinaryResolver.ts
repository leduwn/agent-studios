/**
 * Agent Studios — agsd Binary Path & Provenance Resolver
 * Milestone M15 UI-06: Release Hardening & Test Provenance
 *
 * Enforces strict binary resolution policies:
 * 1. Honors explicit `AGSD_BIN_PATH` environment variable if valid and non-stale.
 * 2. In CI, strictly requires the compiled daemon from the current checked-out workspace root.
 * 3. Proactively rejects stale historical worktree paths (e.g. m15-02, m15-03, m15-04).
 * 4. Fails clearly with actionable remediation instructions if no verified binary is found.
 * 5. Full Windows (.exe) and POSIX compatibility.
 */

import * as fs from 'fs';
import * as path from 'path';

export const STALE_WORKTREE_PATTERNS: readonly RegExp[] = [
  /[\\/]agent-studios-m15-04[\\/]/i,
  /[\\/]agent-studios-m15-03[\\/]/i,
  /[\\/]agent-studios-m15-02[\\/]/i,
  /[\\/]agent-studios-m15[\\/]/i,
];

export interface AgsdResolveOptions {
  env?: NodeJS.ProcessEnv;
  cwd?: string;
  existsSync?: (filePath: string) => boolean;
  platform?: NodeJS.Platform;
}

/**
 * Checks whether a candidate path targets an obsolete historical worktree.
 */
export function isStaleHistoricalPath(filePath: string): boolean {
  const normalized = path.normalize(filePath);
  return STALE_WORKTREE_PATTERNS.some((pattern) => pattern.test(normalized));
}

/**
 * Resolves git repository or worktree root by climbing directories.
 */
function findWorkspaceRoot(startDir: string): string {
  let current = path.resolve(startDir);
  while (true) {
    if (
      fs.existsSync(path.join(current, 'agent-studios-rs')) ||
      fs.existsSync(path.join(current, '.git'))
    ) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      break;
    }
    current = parent;
  }
  return startDir;
}

/**
 * Resolves main git checkout root if currently inside a linked worktree.
 */
function findMainRepoRoot(startDir: string): string | null {
  try {
    const gitDirCandidate = path.join(startDir, '.git');
    if (fs.existsSync(gitDirCandidate)) {
      const stat = fs.statSync(gitDirCandidate);
      if (stat.isFile()) {
        const content = fs.readFileSync(gitDirCandidate, 'utf8').trim();
        const match = content.match(/^gitdir:\s*(.+)$/i);
        if (match) {
          const gitCommonDir = path.resolve(startDir, match[1]);
          // gitCommonDir is typically <main-repo>/.git/worktrees/<name>
          // The main repo root is 3 levels up from the worktree admin dir
          const mainRepo = path.resolve(gitCommonDir, '../../..');
          if (fs.existsSync(path.join(mainRepo, 'agent-studios-rs'))) {
            return mainRepo;
          }
        }
      }
    }
  } catch {
    // Non-fatal if worktree structure cannot be parsed
  }
  return null;
}

/**
 * Resolves authoritative path to compiled `agsd` binary with provenance verification.
 */
export function resolveAgsdBinaryPath(options?: AgsdResolveOptions): string {
  const env = options?.env ?? process.env;
  const cwd = options?.cwd ?? process.cwd();
  const fileExists = options?.existsSync ?? fs.existsSync;
  const platform = options?.platform ?? process.platform;

  const isWindows = platform === 'win32';
  const binaryName = isWindows ? 'agsd.exe' : 'agsd';

  // 1. Explicit AGSD_BIN_PATH policy
  const explicitPath = env.AGSD_BIN_PATH?.trim();
  if (explicitPath && explicitPath.length > 0) {
    const resolvedExplicit = path.resolve(explicitPath);
    if (isStaleHistoricalPath(resolvedExplicit)) {
      throw new Error(
        `[AgsdBinaryResolver] Explicit AGSD_BIN_PATH is rejected because it points to stale historical worktree: '${resolvedExplicit}'. ` +
          `Please update AGSD_BIN_PATH to point to current unified repository binary.`
      );
    }
    if (!fileExists(resolvedExplicit)) {
      throw new Error(
        `[AgsdBinaryResolver] Explicit AGSD_BIN_PATH does not exist on disk: '${resolvedExplicit}'.`
      );
    }
    return resolvedExplicit;
  }

  const isCI = env.CI === 'true' || env.GITHUB_ACTIONS === 'true';

  // 2. CI Strict Provenance Policy: Must come from current checked-out repository
  if (isCI) {
    const ciWorkspace = env.GITHUB_WORKSPACE
      ? path.resolve(env.GITHUB_WORKSPACE)
      : findWorkspaceRoot(cwd);

    const ciCandidates = [
      path.join(ciWorkspace, 'agent-studios-rs', 'target', 'debug', binaryName),
      path.join(ciWorkspace, 'agent-studios-rs', 'target', 'release', binaryName),
    ];

    for (const candidate of ciCandidates) {
      if (!isStaleHistoricalPath(candidate) && fileExists(candidate)) {
        return candidate;
      }
    }

    throw new Error(
      `[AgsdBinaryResolver] CI provenance error: agsd daemon binary for checked-out commit was not found in workspace '${ciWorkspace}'. ` +
        `Checked locations:\n` +
        ciCandidates.map((p) => `  - ${p}`).join('\n') +
        `\nEnsure cargo build --bin agsd was executed prior to running E2E test suites.`
    );
  }

  // 3. Local Development Provenance Policy
  const currentWorkspace = findWorkspaceRoot(cwd);
  const mainRepo = findMainRepoRoot(currentWorkspace);

  const localCandidates: string[] = [
    // Current workspace / worktree target dirs
    path.join(currentWorkspace, 'agent-studios-rs', 'target', 'debug', binaryName),
    path.join(currentWorkspace, 'agent-studios-rs', 'target', 'release', binaryName),
  ];

  if (mainRepo && mainRepo !== currentWorkspace) {
    localCandidates.push(
      path.join(mainRepo, 'agent-studios-rs', 'target', 'debug', binaryName),
      path.join(mainRepo, 'agent-studios-rs', 'target', 'release', binaryName)
    );
  }

  // Canonical workspace fallback
  const canonicalRoot = isWindows ? 'D:\\Projects\\agent-studios' : undefined;
  if (canonicalRoot && canonicalRoot !== currentWorkspace && canonicalRoot !== mainRepo) {
    localCandidates.push(
      path.join(canonicalRoot, 'agent-studios-rs', 'target', 'debug', binaryName),
      path.join(canonicalRoot, 'agent-studios-rs', 'target', 'release', binaryName)
    );
  }

  for (const candidate of localCandidates) {
    if (!isStaleHistoricalPath(candidate) && fileExists(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[AgsdBinaryResolver] Real Rust daemon binary (agsd) not found. Checked locations:\n` +
      localCandidates.map((p) => `  - ${p}`).join('\n') +
      `\n\nTo build the unified native daemon, run:\n` +
      `  cd agent-studios-rs && cargo build --bin agsd -p agent-studios-ipc-transport\n` +
      `Or export AGSD_BIN_PATH=/path/to/compiled/${binaryName}`
  );
}
