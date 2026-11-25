/**
 * Agent Studios — Windows-First Canonical OS Paths & Containment Guards
 * Milestone M15.1: Canonical Windows & POSIX Configuration, State DB, and Artifact Paths
 *
 * Canonical Layout:
 * - Windows (win32):
 *   - configPath:   %APPDATA%\AgentStudios\config.toml
 *   - stateDbPath:  %LOCALAPPDATA%\AgentStudios\state.db
 *   - artifactsDir: %LOCALAPPDATA%\AgentStudios\artifacts\
 * - POSIX (linux / darwin):
 *   - configPath:   ~/.config/AgentStudios/config.toml
 *   - stateDbPath:  ~/.local/share/AgentStudios/state.db
 *   - artifactsDir: ~/.local/share/AgentStudios/artifacts/
 */

import * as os from 'os';
import * as path from 'path';

export const AGENT_STUDIOS_DIR_NAME = 'AgentStudios' as const;
export const CONFIG_FILE_NAME = 'config.toml' as const;
export const STATE_DB_FILE_NAME = 'state.db' as const;
export const ARTIFACTS_SUBDIR_NAME = 'artifacts' as const;

export interface CanonicalPaths {
  readonly platform: NodeJS.Platform;
  readonly configPath: string;
  readonly stateDbPath: string;
  readonly artifactsDir: string;
}

/**
 * Checks whether a path string contains null bytes, raw `..` traversal segments,
 * or URL-encoded `..` sequences.
 */
export function containsPathTraversal(candidatePath: string): boolean {
  if (!candidatePath || candidatePath.includes('\0')) {
    return true;
  }

  const decoded = candidatePath.toLowerCase();
  if (decoded.includes('%2e%2e') || decoded.includes('%2f') || decoded.includes('%5c')) {
    return true;
  }

  const segments = candidatePath.split(/[\\/]+/);
  return segments.some((segment) => segment === '..');
}

/**
 * Verifies that `candidatePath` is strictly contained within `baseDir` without
 * any `..` traversal segments or cross-drive escapes.
 */
export function isPathContainedIn(
  baseDir: string,
  candidatePath: string,
  platform: NodeJS.Platform = process.platform
): boolean {
  if (!baseDir || !candidatePath) {
    return false;
  }
  if (containsPathTraversal(baseDir) || containsPathTraversal(candidatePath)) {
    return false;
  }

  if (platform === 'win32') {
    const resolvedBase = path.win32.resolve(baseDir).replace(/\\+$/, '');
    const resolvedCandidate = path.win32.resolve(resolvedBase, candidatePath);

    const normalizedBaseLower = resolvedBase.toLowerCase();
    const normalizedCandidateLower = resolvedCandidate.toLowerCase();

    if (normalizedCandidateLower === normalizedBaseLower) {
      return true;
    }
    return normalizedCandidateLower.startsWith(`${normalizedBaseLower}\\`);
  }

  const resolvedBase = path.posix.resolve(baseDir).replace(/\/+$/, '');
  const resolvedCandidate = path.posix.resolve(resolvedBase, candidatePath);

  if (resolvedCandidate === resolvedBase) {
    return true;
  }
  return resolvedCandidate.startsWith(`${resolvedBase}/`);
}

/**
 * Safely resolves a relative path inside `baseDir`, throwing an error if the
 * target attempts path traversal (`..`), is absolute, or escapes `baseDir`.
 */
export function resolveSafeContainedPath(
  baseDir: string,
  relativeTarget: string,
  platform: NodeJS.Platform = process.platform
): string {
  if (!relativeTarget || relativeTarget.trim().length === 0) {
    throw new Error('Target path must not be empty');
  }
  if (containsPathTraversal(relativeTarget)) {
    throw new Error(`Path traversal detected in target path: ${relativeTarget}`);
  }

  const pathImpl = platform === 'win32' ? path.win32 : path.posix;
  if (pathImpl.isAbsolute(relativeTarget) || /^[a-zA-Z]:/.test(relativeTarget)) {
    throw new Error(`Relative path required within canonical directory: ${relativeTarget}`);
  }

  const joined = pathImpl.join(baseDir, relativeTarget);
  if (!isPathContainedIn(baseDir, joined, platform)) {
    throw new Error(`Resolved path escapes canonical base directory: ${relativeTarget}`);
  }

  return joined;
}

/**
 * Resolves the canonical Agent Studios file system locations for the specified
 * environment and OS platform (defaults to `process.env` and `process.platform`).
 */
export function resolveCanonicalPaths(
  env: NodeJS.ProcessEnv = process.env,
  platform: NodeJS.Platform = process.platform
): CanonicalPaths {
  if (platform === 'win32') {
    const userProfile = env.USERPROFILE?.trim() || os.homedir() || 'C:\\Users\\Default';
    const appData =
      env.APPDATA?.trim() || path.win32.join(userProfile, 'AppData', 'Roaming');
    const localAppData =
      env.LOCALAPPDATA?.trim() || path.win32.join(userProfile, 'AppData', 'Local');

    if (containsPathTraversal(appData) || containsPathTraversal(localAppData)) {
      throw new Error('Invalid path traversal segment detected in Windows APPDATA/LOCALAPPDATA');
    }

    const configPath = path.win32.join(appData, AGENT_STUDIOS_DIR_NAME, CONFIG_FILE_NAME);
    const stateDbPath = path.win32.join(localAppData, AGENT_STUDIOS_DIR_NAME, STATE_DB_FILE_NAME);
    const artifactsBase = path.win32.join(
      localAppData,
      AGENT_STUDIOS_DIR_NAME,
      ARTIFACTS_SUBDIR_NAME
    );
    const artifactsDir = artifactsBase.endsWith('\\') ? artifactsBase : `${artifactsBase}\\`;

    return {
      platform,
      configPath,
      stateDbPath,
      artifactsDir,
    };
  }

  const homeDir = env.HOME?.trim() || os.homedir() || '/home/default';
  const configHome = env.XDG_CONFIG_HOME?.trim() || path.posix.join(homeDir, '.config');
  const dataHome = env.XDG_DATA_HOME?.trim() || path.posix.join(homeDir, '.local', 'share');

  if (containsPathTraversal(configHome) || containsPathTraversal(dataHome)) {
    throw new Error('Invalid path traversal segment detected in POSIX config/data home');
  }

  const configPath = path.posix.join(configHome, AGENT_STUDIOS_DIR_NAME, CONFIG_FILE_NAME);
  const stateDbPath = path.posix.join(dataHome, AGENT_STUDIOS_DIR_NAME, STATE_DB_FILE_NAME);
  const artifactsBase = path.posix.join(dataHome, AGENT_STUDIOS_DIR_NAME, ARTIFACTS_SUBDIR_NAME);
  const artifactsDir = artifactsBase.endsWith('/') ? artifactsBase : `${artifactsBase}/`;

  return {
    platform,
    configPath,
    stateDbPath,
    artifactsDir,
  };
}
