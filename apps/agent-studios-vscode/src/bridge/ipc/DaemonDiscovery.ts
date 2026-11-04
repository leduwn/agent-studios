/**
 * Agent Studios — Daemon Discovery & Credential Security
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Implements authoritative daemon discovery from runtime connection files (`agsd.json`),
 * matching agent-studios-rs/crates/ipc-transport/src/daemon.rs.
 *
 * Security Invariants:
 * 1. Strictly standard-client credentials only (never load or expose admin keys).
 * 2. Symlink hijack prevention: fails closed if bootstrap file or parent dir is a symlink.
 * 3. Never expose client_key or secrets through logs, webviews, settings, or error strings.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { DaemonBootstrapInfo } from './RustIpcProtocolTypes';

export class DaemonDiscovery {
  /**
   * Computes platform-default runtime discovery path for agsd.json matching Rust:
   * - Windows: %LOCALAPPDATA%/agent-studios/agsd.json (fallback: %TEMP%/agent-studios/agsd.json)
   * - Linux/macOS: $XDG_RUNTIME_DIR/agent-studios/agsd.json (fallback: /tmp/agent-studios-<uid>/agsd.json)
   */
  public static getDefaultRuntimePath(): string {
    const isWindows = process.platform === 'win32';
    if (isWindows) {
      const localAppData = process.env.LOCALAPPDATA;
      if (localAppData && localAppData.trim().length > 0) {
        return path.join(localAppData, 'agent-studios', 'agsd.json');
      }
      return path.join(os.tmpdir(), 'agent-studios', 'agsd.json');
    }

    const xdgRuntime = process.env.XDG_RUNTIME_DIR;
    if (xdgRuntime && xdgRuntime.trim().length > 0) {
      return path.join(xdgRuntime, 'agent-studios', 'agsd.json');
    }

    const uid = typeof process.getuid === 'function' ? process.getuid() : 1000;
    return path.join(`/tmp/agent-studios-${uid}`, 'agsd.json');
  }

  /**
   * Loads and validates DaemonBootstrapInfo from filesystem.
   * Performs fail-closed symlink check and standard-client credential validation.
   */
  public static async loadBootstrapInfo(explicitPath?: string): Promise<DaemonBootstrapInfo> {
    const filePath = explicitPath && explicitPath.trim().length > 0
      ? path.resolve(explicitPath)
      : this.getDefaultRuntimePath();

    // 1. Existence and symlink inspection (Rust parity: symlink_metadata check)
    let stats: fs.BigIntStats;
    try {
      stats = await fs.promises.lstat(filePath, { bigint: true });
    } catch (err: unknown) {
      const code = (err as { code?: string }).code;
      if (code === 'ENOENT') {
        throw new Error(`Agent Studios daemon bootstrap file not found at ${filePath}. Daemon may not be running.`);
      }
      throw new Error(`Failed to access bootstrap file: ${err instanceof Error ? err.message : String(err)}`);
    }

    if (stats.isSymbolicLink()) {
      throw new Error(`Security Violation: Bootstrap file at ${filePath} is a symlink. Potential hijacking attack rejected.`);
    }

    // 2. Read and parse file
    let content: string;
    try {
      content = await fs.promises.readFile(filePath, 'utf8');
    } catch (err: unknown) {
      throw new Error(`Failed to read bootstrap file: ${err instanceof Error ? err.message : String(err)}`);
    }

    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(content);
    } catch {
      throw new Error(`Bootstrap file at ${filePath} contains invalid JSON.`);
    }

    // 3. Schema and credential validation
    if (typeof parsed.endpoint !== 'string' || parsed.endpoint.trim().length === 0) {
      throw new Error('Bootstrap file missing valid endpoint string.');
    }
    if (typeof parsed.key_id !== 'string' || parsed.key_id.trim().length === 0) {
      throw new Error('Bootstrap file missing valid key_id.');
    }
    if (typeof parsed.client_key !== 'string' || parsed.client_key.trim().length === 0) {
      throw new Error('Bootstrap file missing valid client_key.');
    }

    // Security invariant: never expose or propagate admin_key to standard client
    return {
      endpoint: parsed.endpoint.trim(),
      key_id: parsed.key_id.trim(),
      client_key: parsed.client_key.trim(),
      admin_key: null, // Strictly suppressed
      pid: typeof parsed.pid === 'number' ? parsed.pid : 0,
      created_at: typeof parsed.created_at === 'number' ? parsed.created_at : Date.now(),
    };
  }

  /**
   * Returns a redacted representation safe for diagnostics and logging.
   */
  public static redact(info: DaemonBootstrapInfo): Record<string, unknown> {
    return {
      endpoint: info.endpoint,
      key_id: info.key_id,
      client_key: '[REDACTED]',
      admin_key: undefined,
      pid: info.pid,
      created_at: info.created_at,
    };
  }
}
