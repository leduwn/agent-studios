/* eslint-disable @typescript-eslint/naming-convention */
/**
 * Agent Studios — IPC & Lifecycle Reliability Unit Tests
 * Milestone M15 UI-05A: Phase B IPC Reconnection & Lifecycle Testing
 *
 * Requirements Tested:
 * 1. Connection retry on transient ENOENT/ECONNREFUSED eliminating pipe binding race
 * 2. Sequence gap recovery triggering full hydration without connection drop
 * 3. Epoch/generation invalidation on rapid reconnect preventing stale hydration overwrite
 * 4. Clean disposal during active reconnection stopping loops and clearing handles
 * 5. Bounded retries on resubscription preventing tight retry loops
 */

import * as assert from 'assert';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { RustIpcCommand } from '../../bridge/ipc/RustIpcProtocolTypes';

describe('IpcReconnectLifecycle Unit Tests (M15 UI-05A)', () => {
  let server: net.Server | null = null;
  let activeSocket: net.Socket | null = null;
  let endpoint: string;
  let tempDir: string;
  const keyId = 'test-reconnect-key';
  const clientKey = 'secret-reconnect-key-0123456789abcdef';

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-lifecycle-test-'));
    if (process.platform === 'win32') {
      const pipeId = Math.random().toString(36).substring(2, 10);
      endpoint = `\\\\.\\pipe\\ags-lifecycle-${pipeId}`;
    } else {
      endpoint = path.join(tempDir, 'lifecycle.sock');
    }
  });

  afterEach(async () => {
    if (activeSocket) {
      activeSocket.destroy();
      activeSocket = null;
    }
    if (server) {
      await new Promise<void>((resolve) => server!.close(() => resolve()));
      server = null;
    }
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore directory cleanup error
    }
  });

  function sendFrame(socket: net.Socket, obj: unknown): void {
    if (!socket || socket.destroyed || !socket.writable) {
      return;
    }
    try {
      const jsonBytes = Buffer.from(JSON.stringify(obj), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(jsonBytes.length, 0);
      socket.write(Buffer.concat([header, jsonBytes]));
    } catch {
      // Ignore write errors if socket closed concurrently
    }
  }

  function startMockDaemon(
    onRequest?: (cmd: RustIpcCommand, rawSocket: net.Socket) => void
  ): Promise<net.Server> {
    return new Promise((resolve) => {
      const srv = net.createServer((socket) => {
        activeSocket = socket;
        let buffer = Buffer.alloc(0);
        socket.on('data', (chunk) => {
          buffer = Buffer.concat([buffer, chunk]);
          while (buffer.length >= 4) {
            const length = buffer.readUInt32BE(0);
            if (buffer.length < 4 + length) {
              break;
            }
            const jsonStr = buffer.toString('utf8', 4, 4 + length);
            buffer = buffer.subarray(4 + length);
            const cmd = JSON.parse(jsonStr) as RustIpcCommand;

            // Default handshake handling
            if (cmd.type === 'negotiate_capabilities') {
              sendFrame(socket, {
                type: 'capabilities_negotiated',
                selected_version: { major: 1, minor: 0, patch: 0 },
                agreed_capabilities: ['worktree_isolation', 'event_streaming'],
              });
            } else if (cmd.type === 'authenticate') {
              sendFrame(socket, {
                type: 'authenticated',
                session_id: 'sess-lifecycle-test',
              });
            } else if (cmd.type === 'list_studios') {
              sendFrame(socket, {
                type: 'studios',
                studios: [
                  {
                    id: 'studio-reconnect-01',
                    name: 'Lifecycle Studio',
                    created_at: new Date().toISOString(),
                  },
                ],
              });
            } else if (cmd.type === 'get_studio') {
              sendFrame(socket, {
                type: 'studio',
                studio: {
                  id: cmd.studio_id,
                  name: 'Lifecycle Studio',
                  created_at: new Date().toISOString(),
                },
              });
            } else if (cmd.type === 'list_tasks') {
              sendFrame(socket, {
                type: 'tasks',
                tasks: [],
              });
            } else if (cmd.type === 'get_execution_readiness') {
              sendFrame(socket, {
                type: 'execution_readiness',
                readiness: {
                  status: 'ready',
                  blocking_reasons: [],
                  authorized_workspaces: [],
                  defaults: {},
                },
              });
            }

            if (onRequest) {
              onRequest(cmd, socket);
            }
          }
        });
      });

      srv.listen(endpoint, () => {
        server = srv;
        resolve(srv);
      });
    });
  }

  it('1. Connection retry on transient ENOENT/ECONNREFUSED succeeds when daemon binds pipe late', async () => {
    const transport = new NativeIpcTransport({
      endpoint,
      keyId,
      clientKey,
      connectTimeoutMs: 2000,
      connectRetryDelayMs: 40,
    });

    // Start connection attempt BEFORE daemon is listening
    assert.strictEqual(transport.getState(), 'disconnected');
    const connectPromise = transport.connect();

    // Verify state transitioned to 'connecting'
    assert.strictEqual(transport.getState(), 'connecting');

    // Simulate startup delay (pipe binding race condition: 150ms before daemon becomes available)
    await new Promise((r) => setTimeout(r, 150));

    // Now start the daemon server
    await startMockDaemon();

    // Connect promise should succeed via retry
    await connectPromise;
    assert.strictEqual(transport.getState(), 'ready');

    transport.disconnect();
    assert.strictEqual(transport.getState(), 'disconnected');
  });

  it('2. Connection retry fails with error when timeout is exceeded', async () => {
    const transport = new NativeIpcTransport({
      endpoint: process.platform === 'win32'
        ? `\\\\.\\pipe\\ags-nonexistent-${Date.now()}`
        : path.join(tempDir, 'nonexistent.sock'),
      keyId,
      clientKey,
      connectTimeoutMs: 150,
      connectRetryDelayMs: 30,
    });

    let failed = false;
    try {
      await transport.connect();
    } catch {
      failed = true;
    }

    assert.strictEqual(failed, true, 'connect() must reject when endpoint does not become available');
    assert.strictEqual(transport.getState(), 'error');
    transport.disconnect();
  });

  it('3. Sequence gap recovery triggers full rehydration without dropping connection', async () => {
    let subscribeCount = 0;
    let listStudiosCount = 0;

    await startMockDaemon((cmd) => {
      if (cmd.type === 'subscribe_events') {
        subscribeCount++;
      } else if (cmd.type === 'list_studios') {
        listStudiosCount++;
      }
    });

    const transport = new NativeIpcTransport({
      endpoint,
      keyId,
      clientKey,
    });

    const bridge = new DaemonConnectedBridge({
      transport,
      autoSubscribeEvents: true,
    });

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    assert.strictEqual(subscribeCount, 1);
    assert.strictEqual(listStudiosCount, 1);

    // Send an initial event advancing watermark
    sendFrame(activeSocket!, {
      type: 'event',
      event: {
        sequence: 15,
        timestamp: new Date().toISOString(),
        event_type: 'task_created',
        payload: { task_id: 'task-1' },
      },
    });

    await new Promise((r) => setTimeout(r, 50));
    assert.strictEqual(bridge.lastWatermark, 15);

    // Simulate daemon emitting unprompted SequenceGap error frame
    let errorFired = false;
    let subscriptionErrorFired = false;
    transport.on('error', () => {
      errorFired = true;
    });
    transport.on('subscription_error', () => {
      subscriptionErrorFired = true;
    });

    sendFrame(activeSocket!, {
      type: 'error',
      message: 'Sequence gap detected: requested 16 but lowest available in replay buffer is 40 (SequenceGapRecoverable)',
    });

    // Wait for recovery to process
    await new Promise((r) => setTimeout(r, 100));

    // Invariants verified:
    // a. subscription_error fired
    assert.strictEqual(subscriptionErrorFired, true, 'subscription_error event must be emitted');
    // b. Fatal generic error did NOT fire
    assert.strictEqual(errorFired, false, 'Fatal generic error event must NOT fire for sequence gap');
    // c. Bridge connection state remains 'connected'
    assert.strictEqual(bridge.connectionState, 'connected', 'Bridge must remain connected during recovery');
    // d. Re-subscription was triggered from watermark 0
    assert.ok(subscribeCount >= 2, 'subscribe_events must be invoked to recover from gap');
    assert.strictEqual(bridge.lastWatermark, 0, 'Watermark must be reset to 0');
    // e. State was re-hydrated via syncDomainState -> list_studios
    assert.ok(listStudiosCount >= 2, 'Domain state hydration (list_studios) must be performed');

    bridge.dispose();
  });

  it('4. Epoch/generation tracking invalidates stale hydration responses on rapid reconnect', async () => {
    let listStudiosDelayMs = 0;

    await startMockDaemon((cmd, socket) => {
      if (cmd.type === 'list_studios') {
        const delay = listStudiosDelayMs;
        if (delay > 0) {
          setTimeout(() => {
            sendFrame(socket, {
              type: 'studios',
              studios: [
                {
                  id: 'stale-studio-from-past-epoch',
                  name: 'Stale Studio',
                  created_at: new Date().toISOString(),
                },
              ],
            });
          }, delay);
        }
      }
    });

    const transport = new NativeIpcTransport({
      endpoint,
      keyId,
      clientKey,
    });

    const bridge = new DaemonConnectedBridge({
      transport,
      autoSubscribeEvents: false,
    });

    const initialEpoch = bridge.epoch;
    assert.strictEqual(bridge.generation, initialEpoch);

    await bridge.connect();
    const connectedEpoch = bridge.epoch;
    assert.ok(connectedEpoch > initialEpoch, 'Connect must advance epoch');
    assert.strictEqual(bridge.generation, connectedEpoch);

    // Set delay for next list_studios query
    listStudiosDelayMs = 60;

    // Trigger domain sync in background (captured at connectedEpoch)
    const backgroundSyncPromise = bridge.syncDomainState();

    // Advance epoch by switching session while syncDomainState is in flight
    await bridge.switchSession('studio-reconnect-01');
    const newEpoch = bridge.epoch;
    assert.ok(newEpoch > connectedEpoch, 'Session switch must advance epoch');

    // Wait for the delayed background response from the past epoch to complete
    await backgroundSyncPromise;
    await new Promise((r) => setTimeout(r, 100));

    // Active studio should remain 'studio-reconnect-01' and NOT be overwritten by stale response
    assert.strictEqual(
      bridge.activeStudioId,
      'studio-reconnect-01',
      'Stale hydration response from prior epoch must be discarded'
    );
    assert.notStrictEqual(
      bridge.activeStudioId,
      'stale-studio-from-past-epoch',
      'Stale studio must not overwrite active studio'
    );

    // Disconnect also advances epoch
    await bridge.disconnect();
    assert.ok(bridge.epoch > newEpoch, 'Disconnect must advance epoch');

    bridge.dispose();
  });

  it('5. Clean disposal during active reconnection stops retry loop and frees resources', async () => {
    const nonexistentEndpoint = process.platform === 'win32'
      ? `\\\\.\\pipe\\ags-disposal-${Date.now()}`
      : path.join(tempDir, 'disposal.sock');

    const transport = new NativeIpcTransport({
      endpoint: nonexistentEndpoint,
      keyId,
      clientKey,
      connectTimeoutMs: 5000,
      connectRetryDelayMs: 40,
    });

    const bridge = new DaemonConnectedBridge({
      transport,
      autoSubscribeEvents: false,
    });

    // Start connecting (starts retry loop)
    const connectPromise = bridge.connect().catch((err: unknown) => err);
    assert.strictEqual(bridge.connectionState, 'connecting');

    // Dispose bridge while connection retry is active
    await new Promise((r) => setTimeout(r, 60));
    bridge.dispose();

    assert.strictEqual(bridge.isDisposed, true, 'Bridge must report isDisposed: true');
    assert.strictEqual(bridge.connectionState, 'disconnected', 'Bridge state must be disconnected after dispose');

    // Wait for connection promise to resolve/reject
    await connectPromise;

    // Verify transport is cleanly disconnected
    assert.strictEqual(transport.getState(), 'disconnected');

    // Disposing a second time must be safe and idempotent
    bridge.dispose();
    assert.strictEqual(bridge.isDisposed, true);
  });

  it('6. Bounded retries prevent infinite loop on continuous subscription error', async () => {
    let subscribeAttempts = 0;

    await startMockDaemon((cmd, socket) => {
      if (cmd.type === 'subscribe_events') {
        subscribeAttempts++;
        // Daemon continuously reports sequence gap
        setTimeout(() => {
          sendFrame(socket, {
            type: 'error',
            message: 'Sequence gap detected: SequenceGapRecoverable',
          });
        }, 10);
      }
    });

    const transport = new NativeIpcTransport({
      endpoint,
      keyId,
      clientKey,
    });

    const bridge = new DaemonConnectedBridge({
      transport,
      autoSubscribeEvents: false,
    });

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');

    // Trigger initial subscription error
    sendFrame(activeSocket!, {
      type: 'error',
      message: 'Sequence gap detected: SequenceGapRecoverable',
    });

    // Allow bounded retries to exhaust
    await new Promise((r) => setTimeout(r, 300));

    // Resubscription must be bounded at maxResubscribeAttempts (3)
    assert.ok(
      subscribeAttempts <= 4,
      `Resubscription attempts (${subscribeAttempts}) must be bounded by maxResubscribeAttempts`
    );
    assert.strictEqual(
      bridge.connectionState,
      'degraded',
      'Bridge connection state must degrade when retry bounds are exceeded'
    );

    bridge.dispose();
  });
});
