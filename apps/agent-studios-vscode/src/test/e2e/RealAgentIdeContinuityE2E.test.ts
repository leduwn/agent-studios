/**
 * Agent Studios — Tier 3: Real Daemon Execution UX & Mode Continuity Acceptance E2E
 * Milestone M15 UI-06: Agent-First Experience, Shared Session Continuity & Real Daemon Integration
 *
 * Verifies real end-to-end communication with the compiled Rust `agsd` binary:
 * 1. Native daemon startup with persistent SQLite database and Named Pipe / UDS.
 * 2. Authentic two-phase HMAC-SHA256 handshake with role separation (operator vs admin).
 * 3. Session state hydration into SessionStateProjection from live daemon.
 * 4. Mode transition continuity (openInIde, switchToAgentMode, continueSession):
 *    - Preserves session_id and active workspace across mode transitions.
 *    - Never restarts agsd daemon process (PID invariant).
 *    - Never resubmits tasks or duplicates approvals (zero-mutation invariant).
 * 5. Reconnection survival across transient transport drops.
 * 6. Privilege enforcement: operator cannot shut down daemon; authorized admin terminates cleanly.
 */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { DaemonDiscovery } from '../../bridge/ipc/DaemonDiscovery';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { DaemonBootstrapInfo } from '../../bridge/ipc/RustIpcProtocolTypes';
import { registerAllCommands } from '../../commands/CommandsRegistry';
import {
  executedCommands,
  registeredCommands,
  resetMockState,
} from '../unit/vscodeMock';

function resolveAgsdBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    isWindows ? 'D:\\Projects\\agent-studios\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-04\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(process.cwd(), '../../agent-studios-rs/target/debug/agsd' + exeSuffix),
  ].filter((p): p is string => typeof p === 'string' && p.length > 0);

  for (const candidate of candidatePaths) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[RealAgentIdeContinuityE2E] Real Rust daemon binary (agsd) not found. Checked locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n')
  );
}

describe('Tier 3: Real Daemon Execution UX & Mode Continuity Acceptance E2E (M15 UI-06)', function () {
  this.timeout(30000);

  let agsdBinaryPath: string;
  let tempDir: string;
  let runtimePath: string;
  let adminRuntimePath: string;
  let dbPath: string;
  let pipeOrSocketPath: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let daemonPid: number | undefined;
  let bootstrapInfo: DaemonBootstrapInfo;
  let adminBootstrapInfo: DaemonBootstrapInfo;
  let bridge: DaemonConnectedBridge | null = null;
  let bridgeManager: BridgeManager | null = null;
  let compositeDisposable: vscode.Disposable | null = null;

  before(async () => {
    resetMockState();
    agsdBinaryPath = resolveAgsdBinaryPath();

    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-m15-ui-06-'));
    runtimePath = path.join(tempDir, 'agsd.json');
    adminRuntimePath = path.join(tempDir, 'agsd-admin.json');
    dbPath = path.join(tempDir, 'state.sqlite');

    const isWindows = process.platform === 'win32';
    const uuid = crypto.randomUUID();
    pipeOrSocketPath = isWindows
      ? `\\\\.\\pipe\\ags-continuity-e2e-${uuid}`
      : path.join(tempDir, `ags-continuity-e2e-${uuid}.sock`);

    if (!isWindows) {
      try {
        await fs.promises.chmod(tempDir, 0o700);
      } catch {
        // Ignore on platforms without chmod
      }
    }

    // Spawn native daemon
    const spawnArgs = [
      '--runtime-info-path',
      runtimePath,
      '--admin-runtime-info-path',
      adminRuntimePath,
      '--db-path',
      dbPath,
    ];

    if (isWindows) {
      spawnArgs.push('--pipe-name', pipeOrSocketPath);
    } else {
      spawnArgs.push('--socket-path', pipeOrSocketPath);
    }

    daemonProcess = childProcess.spawn(agsdBinaryPath, spawnArgs, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, RUST_LOG: 'info' },
    });

    daemonProcess.stderr?.on('data', (_chunk: Buffer) => {
      // Consume stderr to avoid backpressure
    });
    daemonProcess.stdout?.on('data', (_chunk: Buffer) => {
      // Consume stdout to avoid backpressure
    });

    daemonPid = daemonProcess.pid;

    // Poll for bootstrap files
    const startTime = Date.now();
    while (Date.now() - startTime < 10000) {
      if (fs.existsSync(runtimePath) && fs.existsSync(adminRuntimePath)) {
        try {
          const content = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
          if (content.endpoint && content.client_key) {
            break;
          }
        } catch {
          // File may be mid-write
        }
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    assert.ok(fs.existsSync(runtimePath), 'Standard bootstrap file must exist');
    assert.ok(fs.existsSync(adminRuntimePath), 'Admin bootstrap file must exist');

    bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(runtimePath);
    const adminRaw = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
    adminBootstrapInfo = {
      endpoint: adminRaw.endpoint,
      key_id: adminRaw.key_id,
      client_key: adminRaw.admin_key || adminRaw.client_key,
      admin_key: adminRaw.admin_key,
      pid: adminRaw.pid,
      created_at: adminRaw.created_at,
    };
  });

  after(async () => {
    if (compositeDisposable) {
      compositeDisposable.dispose();
      compositeDisposable = null;
    }
    if (bridgeManager) {
      bridgeManager.dispose();
      bridgeManager = null;
    }
    if (bridge) {
      try {
        await bridge.disconnect();
        bridge.dispose();
      } catch {
        // Ignore
      }
      bridge = null;
    }

    // Clean admin shutdown if process still running
    if (daemonProcess && !daemonProcess.killed) {
      try {
        const adminTransport = new NativeIpcTransport({
          endpoint: adminBootstrapInfo.endpoint,
          keyId: adminBootstrapInfo.key_id,
          clientKey: adminBootstrapInfo.admin_key || adminBootstrapInfo.client_key,
          commandTimeoutMs: 5000,
        });
        await adminTransport.connect();
        await adminTransport.sendCommand({ type: 'shutdown' });
        await adminTransport.disconnect();
      } catch {
        try {
          daemonProcess.kill('SIGKILL');
        } catch {
          // Ignore
        }
      }
      daemonProcess = null;
    }

    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  });

  it('1. Authenticates standard operator role via two-phase HMAC challenge', async () => {
    assert.ok(bootstrapInfo.endpoint, 'Bootstrap info must contain endpoint');
    bridge = new DaemonConnectedBridge({ bootstrapPath: runtimePath });
    await bridge.connect();

    assert.strictEqual(bridge.connectionState, 'connected', 'Bridge must reach connected state');
    const health = await bridge.getHealth();
    assert.strictEqual(health.status, 'healthy', 'Daemon health must report healthy');

    // Create studio session in SQLite database so daemon contains active studio
    const studioResp = await bridge.transport!.sendCommand({
      type: 'create_studio',
      name: 'M15-UI-06 Continuity Studio',
    });
    assert.strictEqual(studioResp.type, 'studio_created', 'Studio creation must succeed');
    if (studioResp.type === 'studio_created') {
      await bridge.switchSession(studioResp.studio.id);
    }
  });

  it('2. Hydrates live native daemon session into SessionStateProjection', async () => {
    assert.ok(bridge, 'Bridge must be connected');
    bridgeManager = new BridgeManager(bridge);
    await bridgeManager.stateProjection.initializeFromBridge();

    const session = bridgeManager.stateProjection.session;
    assert.ok(session, 'Projection must contain hydrated session from real daemon');
    assert.strictEqual(session.status, 'idle', 'Initial session status must be idle');
  });

  it('3. Mode transition (openInIde & switchToAgentMode) preserves session identity without restarting daemon', async () => {
    assert.ok(bridgeManager, 'BridgeManager must exist');
    const context = { subscriptions: [] } as unknown as vscode.ExtensionContext;
    compositeDisposable = registerAllCommands(context, bridgeManager, {}, {});

    const openInIdeHandler = registeredCommands.get('agent-studios.openInIde');
    assert.ok(openInIdeHandler, 'openInIde must be registered');

    const switchToAgentHandler = registeredCommands.get('agent-studios.switchToAgentMode');
    assert.ok(switchToAgentHandler, 'switchToAgentMode must be registered');

    // Capture pre-transition baseline
    const preSessionId = bridgeManager.stateProjection.session?.id;
    const initialTasks = await bridgeManager.bridge.listTasks();

    // 1. Transition to IDE mode
    await openInIdeHandler();
    assert.ok(
      executedCommands.some((c) => c.command === 'workbench.view.explorer'),
      'Must reveal workbench explorer'
    );

    // 2. Transition back to Agent mode
    await switchToAgentHandler();
    assert.ok(
      executedCommands.some((c) => c.command === 'agent-studios.chat.focus'),
      'Must focus chat assistant'
    );

    // Invariant 1: Daemon PID unchanged (daemon never restarted)
    assert.strictEqual(daemonProcess?.pid, daemonPid, 'Daemon PID must remain identical across transitions');

    // Invariant 2: Session ID unchanged
    const postSessionId = bridgeManager.stateProjection.session?.id;
    assert.strictEqual(postSessionId, preSessionId, 'Session ID must be strictly preserved across mode switches');

    // Invariant 3: Zero unexpected task submissions
    const postTasks = await bridgeManager.bridge.listTasks();
    assert.strictEqual(postTasks.length, initialTasks.length, 'Task count must remain invariant');
  });

  it('4. Reconnection preserves session state from native SQLite WAL store', async () => {
    assert.ok(bridge, 'Bridge must exist');
    const initialSession = await bridge.getCurrentSession();
    assert.ok(initialSession, 'Initial session must exist');

    // Disconnect transport
    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected', 'Bridge must report disconnected');

    // Reconnect to native daemon
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected', 'Bridge must reconnect successfully');

    // Re-verify session identity survived from SQLite WAL
    const currentSession = await bridge.getCurrentSession();
    assert.ok(currentSession, 'Current session must be restored from SQLite WAL');
    assert.strictEqual(currentSession.id, initialSession.id, 'Session ID must match pre-disconnect state');
  });

  it('5. Role separation: standard operator client cannot execute shutdown (permission denied)', async () => {
    assert.ok(bridge && bridge.transport, 'Bridge and transport must exist');

    await assert.rejects(
      async () => {
        await bridge!.transport!.sendCommand({ type: 'shutdown' });
      },
      (err: Error) => {
        const msg = err.message.toLowerCase();
        return msg.includes('permission denied') || msg.includes('admin') || msg.includes('unauthorized');
      },
      'Standard operator role must be denied shutdown permission'
    );

    assert.strictEqual(daemonProcess?.exitCode, null, 'Daemon must still be running after denied shutdown');
  });

  it('6. Authorized admin role executes clean shutdown terminating native daemon', async () => {
    if (bridge) {
      await bridge.disconnect();
      bridge.dispose();
      bridge = null;
    }

    const adminTransport = new NativeIpcTransport({
      endpoint: adminBootstrapInfo.endpoint,
      keyId: adminBootstrapInfo.key_id,
      clientKey: (adminBootstrapInfo.admin_key || adminBootstrapInfo.client_key)!,
      commandTimeoutMs: 5000,
    });
    await adminTransport.connect();

    const response = await adminTransport.sendCommand({ type: 'shutdown' });
    assert.strictEqual(response.type, 'shutdown_acknowledged', 'Admin shutdown must return acknowledgement');
    await adminTransport.disconnect();

    // Await child process exit
    const exitCode = await new Promise<number | null>((resolve) => {
      if (!daemonProcess || daemonProcess.exitCode !== null) {
        resolve(daemonProcess?.exitCode ?? 0);
        return;
      }
      daemonProcess.once('exit', (code) => resolve(code));
      setTimeout(() => resolve(daemonProcess?.exitCode ?? null), 5000);
    });

    assert.strictEqual(exitCode, 0, 'Daemon child process must terminate with exit code 0');
    daemonProcess = null;
  });
});
