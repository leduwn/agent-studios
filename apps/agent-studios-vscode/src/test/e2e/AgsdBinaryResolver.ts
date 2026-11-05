/**
 * Agent Studios — agsd Binary Path & Provenance Resolver
 * Milestone M15 UI-06: Release Hardening & CI Binary Provenance Gate
 *
 * PROVENANCE BOUNDARY & THREAT MODEL SPECIFICATION:
 * 1. Path validation PROVES that the resolved `agsd` daemon binary is located strictly within
 *    the expected compiled target outputs of the authorized repository checkout, proactively
 *    rejecting obsolete historical worktrees (m15-02, m15-03, m15-04) and untrusted external binaries.
 * 2. Path validation DOES NOT, by itself, cryptographically prove that the binary was compiled from
 *    the exact source commit currently checked out. Complete source-to-binary build provenance
 *    requires the CI workflow pipeline to compile `agsd` directly from the checked-out commit
 *    (e.g., via `cargo build --bin agsd`) within the same isolated, immutable runner workspace.
 * 3. Never fall back silently to an alternative daemon path after any provenance verification failure.
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
  realpathSync?: (filePath: string) => string;
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
 * Normalizes a path for safe cross-platform comparison.
 * On Windows, handles case-insensitivity (drive letters, folders) and forward/backward slashes.
 */
export function normalizePathForComparison(filePath: string, isWindows: boolean): string {
  const resolved = path.resolve(filePath);
  const normalized = path.normalize(resolved);
  return isWindows ? normalized.toLowerCase() : normalized;
}

/**
 * Checks whether `targetPath` is identical to or located inside `parentDir`.
 */
export function isSubpathOrEqual(targetPath: string, parentDir: string, isWindows: boolean): boolean {
  const normTarget = normalizePathForComparison(targetPath, isWindows);
  const normParent = normalizePathForComparison(parentDir, isWindows);

  if (normTarget === normParent) {
    return true;
  }

  const relative = path.relative(normParent, normTarget);
  return !relative.startsWith('..') && !path.isAbsolute(relative);
}

/**
 * Safely resolves symlinks / reparse points when supported, falling back to original path.
 */
function safeRealpath(filePath: string, realpathFn?: (p: string) => string): string {
  try {
    const fn = realpathFn ?? fs.realpathSync;
    return fn(filePath);
  } catch {
    return filePath;
  }
}

/**
 * Resolves git repository or worktree root by climbing directories.
 */
export function findWorkspaceRoot(startDir: string, fileExists: (p: string) => boolean = fs.existsSync): string {
  let current = path.resolve(startDir);
  while (true) {
    if (
      fileExists(path.join(current, 'agent-studios-rs')) ||
      fileExists(path.join(current, '.git'))
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
function findMainRepoRoot(startDir: string, fileExists: (p: string) => boolean = fs.existsSync): string | null {
  try {
    const gitDirCandidate = path.join(startDir, '.git');
    if (fileExists(gitDirCandidate)) {
      const stat = fs.statSync(gitDirCandidate);
      if (stat.isFile()) {
        const content = fs.readFileSync(gitDirCandidate, 'utf8').trim();
        const match = content.match(/^gitdir:\s*(.+)$/i);
        if (match) {
          const gitCommonDir = path.resolve(startDir, match[1]);
          const mainRepo = path.resolve(gitCommonDir, '../../..');
          if (fileExists(path.join(mainRepo, 'agent-studios-rs'))) {
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
  const realpathFn = options?.realpathSync;
  const platform = options?.platform ?? process.platform;

  const isWindows = platform === 'win32';
  const binaryName = isWindows ? 'agsd.exe' : 'agsd';

  // 1. Determine CI Mode first to enforce strict checkout boundaries
  const isCI = env.CI === 'true' || env.GITHUB_ACTIONS === 'true';

  if (isCI) {
    // Resolve authoritative checked-out workspace root
    const ciWorkspace = env.GITHUB_WORKSPACE
      ? path.resolve(env.GITHUB_WORKSPACE)
      : findWorkspaceRoot(cwd, fileExists);

    const explicitPath = env.AGSD_BIN_PATH?.trim();

    if (explicitPath && explicitPath.length > 0) {
      const resolvedExplicit = path.resolve(explicitPath);
      const canonicalExplicit = safeRealpath(resolvedExplicit, realpathFn);

      // Check stale historical worktree patterns
      if (isStaleHistoricalPath(resolvedExplicit) || isStaleHistoricalPath(canonicalExplicit)) {
        throw new Error(
          `[AgsdBinaryResolver] Explicit AGSD_BIN_PATH is rejected because it points to stale historical worktree: '${resolvedExplicit}'. ` +
            `Please update AGSD_BIN_PATH to point to current unified repository binary.`
        );
      }

      // Check existence on disk
      if (!fileExists(resolvedExplicit)) {
        throw new Error(
          `[AgsdBinaryResolver] Explicit AGSD_BIN_PATH does not exist on disk: '${resolvedExplicit}'.`
        );
      }

      // Require binary to reside within authoritative checked-out workspace
      if (!isSubpathOrEqual(canonicalExplicit, ciWorkspace, isWindows)) {
        throw new Error(
          `[AgsdBinaryResolver] Strict CI provenance error: Explicit AGSD_BIN_PATH '${resolvedExplicit}' is outside the checked-out repository root '${ciWorkspace}'. ` +
            `Arbitrary external paths are prohibited in CI mode.`
        );
      }

      // Require binary to be within expected compiled daemon target locations
      const expectedLocations = [
        path.join(ciWorkspace, 'agent-studios-rs', 'target'),
        path.join(ciWorkspace, 'target'),
      ];
      const isInExpectedLocation = expectedLocations.some((loc) =>
        isSubpathOrEqual(canonicalExplicit, loc, isWindows)
      );

      if (!isInExpectedLocation) {
        throw new Error(
          `[AgsdBinaryResolver] Strict CI provenance error: Explicit AGSD_BIN_PATH '${resolvedExplicit}' is not located in an expected compiled daemon target directory under '${ciWorkspace}'.`
        );
      }

      return resolvedExplicit;
    }

    // CI Default Search: Look strictly within the checked-out repository
    const ciCandidates = [
      path.join(ciWorkspace, 'agent-studios-rs', 'target', 'debug', binaryName),
      path.join(ciWorkspace, 'agent-studios-rs', 'target', 'release', binaryName),
      path.join(ciWorkspace, 'target', 'debug', binaryName),
      path.join(ciWorkspace, 'target', 'release', binaryName),
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

  // 2. Local Development Mode: Support explicit developer override with sanity checks
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

  // 3. Local Workspace Candidate Search
  const currentWorkspace = findWorkspaceRoot(cwd, fileExists);
  const mainRepo = findMainRepoRoot(currentWorkspace, fileExists);

  const localCandidates: string[] = [
    path.join(currentWorkspace, 'agent-studios-rs', 'target', 'debug', binaryName),
    path.join(currentWorkspace, 'agent-studios-rs', 'target', 'release', binaryName),
    path.join(currentWorkspace, 'target', 'debug', binaryName),
    path.join(currentWorkspace, 'target', 'release', binaryName),
  ];

  if (mainRepo && mainRepo !== currentWorkspace) {
    localCandidates.push(
      path.join(mainRepo, 'agent-studios-rs', 'target', 'debug', binaryName),
      path.join(mainRepo, 'agent-studios-rs', 'target', 'release', binaryName)
    );
  }

  // Canonical workspace fallback for local Windows developer workstations
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
