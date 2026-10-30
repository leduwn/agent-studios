/**
 * Agent Studios — Real Rust Daemon E2E Test Suite
 * Milestone M14 Integration Quality Gate: Mission Control + ConnectedClient + Real Rust Daemon
 *
 * Verifies real OS cross-process communication against the actual compiled Rust
 * `agsd.exe` binary over Windows Named Pipes (and Unix Domain Sockets on POSIX):
 * - Process spawning and bootstrap discovery (`agsd.json`)
 * - Two-phase HMAC-SHA256 handshake
 * - Health check telemetry
 * - Round-trip ping latency
 * - Control plane queries (studios, tasks)
 * - Provider and model catalog queries
 * - Fail-closed mutating operation boundaries
 * - Role-based authorization enforcement (standard client cannot shutdown)
 * - Clean shutdown via admin authentication
 */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { DaemonDiscovery } from '../../bridge/ipc/DaemonDiscovery';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';

describe('Real Rust Daemon (agsd) E2E Test Suite', function () {
  // Allow time for OS process spawn, Named Pipe binding, and handshake
  this.timeout(15000);

  let tempDir: string;
  let runtimePath: string;
  let adminRuntimePath: string;
  let pipeName: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let agsdBinaryPath: string;

  before(function () {
    const candidatePaths = [
      process.env.AGSD_BIN_PATH,
      'D:\\Projects\\agent-studios-m13\\agent-studios-rs\\target\\debug\\agsd.exe',
      path.resolve(__dirname, '../../../../../../agent-studios-m13/agent-studios-rs/target/debug/agsd.exe'),
      path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd.exe'),
      path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd'),
    ].filter((p): p is string => typeof p === 'string' && p.length > 0);

    let foundPath: string | null = null;
    for (const p of candidatePaths) {
      if (fs.existsSync(p)) {
        foundPath = p;
        break;
      }
    }

    if (!foundPath) {
      console.warn('Real Rust daemon binary (agsd) not found. Skipping real daemon E2E tests.');
      this.skip();
      return;
    }

    agsdBinaryPath = foundPath;
  });

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-real-e2e-'));
    runtimePath = path.join(tempDir, 'agsd.json');
    adminRuntimePath = path.join(tempDir, 'agsd-admin.json');

    const isWindows = process.platform === 'win32';
    const id = Math.random().toString(36).substring(2, 10);
    pipeName = isWindows
      ? `\\\\.\\pipe\\ags-e2e-${id}`
      : path.join(tempDir, `ags-e2e-${id}.sock`);

    const spawnArgs = [
      '--pipe-name',
      pipeName,
      '--runtime-info-path',
      runtimePath,
      '--admin-runtime-info-path',
      adminRuntimePath,
    ];

    if (!isWindows) {
      spawnArgs.push('--socket-path', pipeName);
    }

    // Spawn real compiled Rust daemon process
    daemonProcess = childProcess.spawn(agsdBinaryPath, spawnArgs, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, RUST_LOG: 'info' },
    });

    daemonProcess.stderr?.on('data', (_chunk: Buffer) => {
      // Consume stderr to prevent pipe buffer blockage
    });

    // Await bootstrap runtime discovery file generation
    const startTime = Date.now();
    let ready = false;
    while (Date.now() - startTime < 8000) {
      if (fs.existsSync(runtimePath) && fs.existsSync(adminRuntimePath)) {
        try {
          const content = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
          if (content.endpoint && content.client_key) {
            ready = true;
            break;
          }
        } catch {
          // File write may still be in progress
        }
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    assert.ok(ready, `agsd.exe failed to write runtime discovery files within 8000ms at ${runtimePath}`);
  });

  afterEach(async () => {
    if (daemonProcess && !daemonProcess.killed) {
      try {
        daemonProcess.kill('SIGKILL');
      } catch {
        // Ignore
      }
      daemonProcess = null;
    }

    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  });

  it('performs bootstrap discovery, HMAC handshake, health check, queries, and security enforcement', async () => {
    // 1. Validate bootstrap discovery file parsing and symlink check
    const bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(runtimePath);
    assert.ok(bootstrapInfo.endpoint.length > 0, 'Discovered endpoint must not be empty');
    assert.strictEqual(bootstrapInfo.key_id, 'ags-client');
    assert.ok(bootstrapInfo.client_key.length > 0, 'Discovered client_key must be present');
    assert.strictEqual(bootstrapInfo.admin_key, null, 'Security invariant: admin_key must never be exposed');
    assert.ok(bootstrapInfo.pid > 0, 'Discovered daemon PID must be valid');

    // 2. Validate credential redaction
    const redacted = DaemonDiscovery.redact(bootstrapInfo);
    assert.strictEqual(redacted.client_key, '[REDACTED]');
    assert.strictEqual(redacted.admin_key, undefined);

    // 3. Connect via DaemonConnectedBridge
    const bridge = new DaemonConnectedBridge({ bootstrapPath: runtimePath });
    assert.strictEqual(bridge.connectionState, 'disconnected');

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    assert.ok(bridge.endpoint !== null, 'Bridge should expose connected endpoint');

    // 4. Verify health check from real Rust daemon
    const health = await bridge.getHealth();
    assert.ok(health !== null, 'Health response must not be null');
    assert.strictEqual(health.status, 'healthy');
    assert.strictEqual(health.active_clients, 1);
    assert.ok(typeof health.uptime_secs === 'number' && health.uptime_secs >= 0);

    // 5. Verify round-trip ping latency
    const pingLatency = await bridge.ping('m14-real-rust-ping');
    assert.ok(typeof pingLatency === 'number' && pingLatency >= 0, 'Ping latency should be non-negative');

    // 6. Verify task listing over real IPC
    const tasks = await bridge.listTasks();
    assert.ok(Array.isArray(tasks), 'Tasks must be an array');
    assert.strictEqual(tasks.length, 0, 'Fresh daemon control plane starts with 0 tasks');

    // 7. Verify provider summaries over real IPC
    const providers = await bridge.listProviderSummaries();
    assert.ok(Array.isArray(providers), 'Providers must be an array');

    // 8. Verify security fail-closed boundary for mutating operations
    const approvalRes = await bridge.requestApprovalDecision('appr-real-01', 'approved');
    assert.strictEqual(approvalRes.applied, false, 'Mutating approvals must fail closed in read-only slice');

    const editorRes = await bridge.submitEditorAction({
      action: 'refactor',
      documentUri: 'file:///workspace/test.ts',
      languageId: 'typescript',
      selectedText: 'fn()',
      selectionRange: { startLine: 1, startCol: 0, endLine: 1, endCol: 4 },
    });
    assert.strictEqual(editorRes.accepted, false, 'Editor actions must fail closed');

    // 9. Verify role authorization: standard client cannot invoke shutdown
    if (bridge.transport) {
      await assert.rejects(async () => {
        await bridge.transport!.sendCommand({ type: 'shutdown' });
      }, (err: Error) => {
        return err.message.toLowerCase().includes('permission denied') ||
               err.message.toLowerCase().includes('admin');
      });
    }

    // 10. Clean disconnect
    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected');
    bridge.dispose();

    // 11. Verify clean daemon shutdown using admin credentials
    const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
    assert.ok(adminContent.admin_key, 'Admin bootstrap file must contain admin_key');

    const adminTransport = new NativeIpcTransport({
      endpoint: adminContent.endpoint,
      keyId: adminContent.key_id,
      clientKey: adminContent.admin_key,
    });

    await adminTransport.connect();
    const shutdownResp = await adminTransport.sendCommand({ type: 'shutdown' });
    assert.strictEqual(shutdownResp.type, 'shutdown_acknowledged');

    // Wait for daemon process to exit cleanly with code 0
    const exitCode = await new Promise<number | null>((resolve) => {
      if (!daemonProcess || daemonProcess.exitCode !== null) {
        resolve(daemonProcess?.exitCode ?? 0);
        return;
      }
      const timer = setTimeout(() => resolve(-1), 3000);
      daemonProcess.on('exit', (code) => {
        clearTimeout(timer);
        resolve(code);
      });
    });

    assert.strictEqual(exitCode, 0, 'agsd.exe process must exit cleanly with code 0 upon shutdown command');
    daemonProcess = null;
  });
});
