/**
 * Agent Studios — Embedded Daemon Supervisor (Wave 11 Extension-First)
 *
 * Locates the native `agsd` daemon binary (env override, user setting, bundled
 * VSIX `bin/win32-x64/agsd.exe`, or local dev target) and starts/attaches cleanly
 * without requiring users to clone the repo or run `cargo build`.
 */

import * as childProcess from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import { DaemonDiscovery } from './ipc/DaemonDiscovery';

export interface EnsureDaemonResult {
  running: boolean;
  startedBySupervisor: boolean;
  binaryPath: string | null;
  bootstrapPath: string;
  pid?: number;
  endpoint?: string;
}

export class EmbeddedDaemonSupervisor {
  public static resolveAgsdBinary(extensionPath?: string): string | null {
    const exeSuffix = process.platform === 'win32' ? '.exe' : '';
    const binaryName = `agsd${exeSuffix}`;
    const config = vscode.workspace.getConfiguration?.('agentStudios');
    const configuredPath = config?.get<string>('daemonBinaryPath', '')?.trim();

    const platformDir = `${process.platform}-${process.arch}`;
    const candidates: Array<string | undefined> = [
      process.env.AGSD_BIN_PATH?.trim(),
      configuredPath || undefined,
      extensionPath ? path.join(extensionPath, 'bin', platformDir, binaryName) : undefined,
      extensionPath ? path.join(extensionPath, 'bin', 'win32-x64', binaryName) : undefined,
      extensionPath ? path.join(extensionPath, 'bin', binaryName) : undefined,
      process.platform === 'win32'
        ? `C:\\AgentStudiosBuild\\cargo-target-rc2\\release\\${binaryName}`
        : undefined,
      process.platform === 'win32'
        ? `C:\\AgentStudiosBuild\\cargo-target-rc2\\debug\\${binaryName}`
        : undefined,
    ];

    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (workspaceRoot) {
      candidates.push(
        path.join(workspaceRoot, 'agent-studios-rs', 'target', 'release', binaryName),
        path.join(workspaceRoot, 'agent-studios-rs', 'target', 'debug', binaryName)
      );
    }

    for (const candidate of candidates) {
      if (candidate && fs.existsSync(candidate)) {
        return candidate;
      }
    }
    return null;
  }

  public static isProcessAlive(pid: number): boolean {
    if (!Number.isInteger(pid) || pid <= 0) {
      return false;
    }
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  }

  public static async ensureDaemonRunning(options?: {
    extensionPath?: string;
    bootstrapPath?: string;
    adminBootstrapPath?: string;
    pipeName?: string;
    socketPath?: string;
    stateDir?: string;
    workspacePath?: string;
    startupTimeoutMs?: number;
  }): Promise<EnsureDaemonResult> {
    const bootstrapPath =
      options?.bootstrapPath || DaemonDiscovery.getDefaultRuntimePath();

    if (fs.existsSync(bootstrapPath)) {
      try {
        const discovered = await DaemonDiscovery.loadBootstrapInfo(bootstrapPath);
        if (this.isProcessAlive(discovered.pid)) {
          return {
            running: true,
            startedBySupervisor: false,
            binaryPath: this.resolveAgsdBinary(options?.extensionPath),
            bootstrapPath,
            pid: discovered.pid,
            endpoint: discovered.endpoint,
          };
        }
      } catch {
        // Stale or corrupt bootstrap file; fall through to spawn fresh daemon
      }
    }

    const binaryPath = this.resolveAgsdBinary(options?.extensionPath);
    if (!binaryPath) {
      return {
        running: false,
        startedBySupervisor: false,
        binaryPath: null,
        bootstrapPath,
      };
    }

    const args: string[] = ['--runtime-info-path', bootstrapPath];
    if (options?.adminBootstrapPath) {
      args.push('--admin-runtime-info-path', options.adminBootstrapPath);
    }
    if (options?.pipeName) {
      args.push('--pipe-name', options.pipeName);
    }
    if (options?.socketPath) {
      args.push('--socket-path', options.socketPath);
    }
    if (options?.stateDir) {
      args.push('--state-dir', options.stateDir);
    }

    const child = childProcess.spawn(binaryPath, args, {
      detached: true,
      stdio: 'ignore',
      windowsHide: true,
      cwd: options?.workspacePath || path.dirname(binaryPath),
    });
    child.unref();

    const deadline = Date.now() + (options?.startupTimeoutMs ?? 5000);
    while (Date.now() < deadline) {
      if (fs.existsSync(bootstrapPath)) {
        try {
          const discovered = await DaemonDiscovery.loadBootstrapInfo(bootstrapPath);
          if (discovered.endpoint) {
            return {
              running: true,
              startedBySupervisor: true,
              binaryPath,
              bootstrapPath,
              pid: discovered.pid || child.pid,
              endpoint: discovered.endpoint,
            };
          }
        } catch {
          // Wait until daemon finishes writing JSON atomically
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    return {
      running: false,
      startedBySupervisor: true,
      binaryPath,
      bootstrapPath,
      pid: child.pid,
    };
  }
}
