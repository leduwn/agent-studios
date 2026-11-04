/**
 * Agent Studios — Real Cross-Process E2E Daemon Tests
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Verifies real end-to-end communication across independent OS processes.
 * Spawns an out-of-process standalone daemon with real Named Pipe or Unix Domain Socket,
 * writes agsd.json bootstrap file, and establishes authenticated two-phase handshake
 * from VS Code DaemonConnectedBridge.
 */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { TimelineEvent } from '../../bridge/types';

describe('Cross-Process Real Daemon E2E Tests', () => {
  let tempDir: string;
  let bootstrapPath: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let daemonPid: number = 0;
  let daemonEndpoint: string = '';

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-crossproc-'));
    bootstrapPath = path.join(tempDir, 'agsd.json');

    const daemonScript = path.resolve(__dirname, '..', 'e2e', 'StandaloneMockDaemon.js');
    assert.ok(fs.existsSync(daemonScript), `Daemon script must exist at ${daemonScript}`);

    const keyId = 'e2e-client-key';
    const clientKey = 'synthetic-key-32bytes-hex-abcdef0123456789';

    // Spawn daemon as a separate, isolated OS child process
    await new Promise<void>((resolve, reject) => {
      const child = childProcess.spawn(
        process.execPath,
        [
          daemonScript,
          '--bootstrap-path',
          bootstrapPath,
          '--key-id',
          keyId,
          '--client-key',
          clientKey,
        ],
        {
          stdio: ['pipe', 'pipe', 'pipe'],
          env: { ...process.env },
        }
      );

      daemonProcess = child;

      let started = false;
      const timeout = setTimeout(() => {
        if (!started) {
          child.kill();
          reject(new Error('Standalone daemon failed to start within 5000ms.'));
        }
      }, 5000);

      child.stdout?.on('data', (chunk: Buffer) => {
        const text = chunk.toString('utf8');
        const match = text.match(/DAEMON_READY:(\d+):(.+)/);
        if (match) {
          started = true;
          clearTimeout(timeout);
          daemonPid = parseInt(match[1], 10);
          daemonEndpoint = match[2].trim();
          resolve();
        }
      });

      child.stderr?.on('data', (errChunk: Buffer) => {
        console.error('[Daemon Process Stderr]:', errChunk.toString('utf8'));
      });

      child.on('error', (err) => {
        clearTimeout(timeout);
        reject(err);
      });

      child.on('exit', (code) => {
        if (!started) {
          clearTimeout(timeout);
          reject(new Error(`Daemon process exited prematurely with code ${code}`));
        }
      });
    });
  });

  afterEach(async () => {
    if (daemonProcess && !daemonProcess.killed) {
      daemonProcess.kill('SIGTERM');
      daemonProcess = null;
    }
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  });

  it('verifies real cross-process discovery, authenticated connection, and queries', async () => {
    assert.ok(daemonPid > 0, 'Daemon PID should be recorded');
    assert.ok(daemonEndpoint.length > 0, 'Daemon endpoint should be recorded');
    assert.ok(fs.existsSync(bootstrapPath), 'agsd.json should exist on disk');

    const bridge = new DaemonConnectedBridge({ bootstrapPath });
    assert.strictEqual(bridge.connectionState, 'disconnected');

    const receivedEvents: TimelineEvent[] = [];
    bridge.onEvent((evt) => {
      receivedEvents.push(evt);
    });

    // 1. Connect via real OS IPC socket
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    assert.strictEqual(bridge.activeStudioId, 'studio-e2e-01');

    // 2. Health check over cross-process socket
    const health = await bridge.getHealth();
    assert.strictEqual(health.status, 'ok');
    assert.strictEqual(health.active_clients, 1);
    assert.ok(health.uptime_secs >= 0);

    // 3. Ping check
    const latency = await bridge.ping('e2e-ping');
    assert.ok(typeof latency === 'number' && latency >= 0);

    // 4. Session info query
    const session = await bridge.getCurrentSession();
    assert.ok(session !== null);
    assert.strictEqual(session.id, 'studio-e2e-01');
    assert.strictEqual(session.goalSummary, 'E2E Cross-Process Studio');
    assert.strictEqual(session.status, 'completed');
    assert.strictEqual(session.completedTasksCount, 1);
    assert.strictEqual(session.totalTasksCount, 1);

    // 5. Tasks query
    const tasks = await bridge.listTasks();
    assert.strictEqual(tasks.length, 1);
    assert.strictEqual(tasks[0].id, 'task-e2e-01');
    assert.strictEqual(tasks[0].status, 'succeeded');
    assert.strictEqual(tasks[0].progress, 100);

    // 6. Provider summaries query
    const providers = await bridge.listProviderSummaries();
    assert.strictEqual(providers.length, 1);
    assert.strictEqual(providers[0].displayName, 'Mock Test Provider');
    assert.strictEqual(providers[0].models.length, 1);
    assert.strictEqual(providers[0].models[0].name, 'Mock Model v1');

    // 7. Event streaming verification: wait for subscribed event
    await new Promise((r) => setTimeout(r, 60));
    assert.ok(receivedEvents.length >= 1, 'Should have received at least 1 stream event');
    assert.strictEqual(receivedEvents[0].type, 'task_started');
    assert.strictEqual(receivedEvents[0].taskId, 'task-e2e-01');

    // 8. Clean disconnection
    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected');

    bridge.dispose();
  });
});
