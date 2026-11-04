/**
 * Agent Studios — Native IPC Transport Unit Tests
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 */

import * as assert from 'assert';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { RustHmacSigner } from '../../bridge/ipc/RustHmacSigner';
import { RustIpcCommand, RustIpcEvent } from '../../bridge/ipc/RustIpcProtocolTypes';

describe('NativeIpcTransport Unit Tests', () => {
  let server: net.Server;
  let endpoint: string;
  const keyId = 'test-key-01';
  const clientKey = 'secret-test-key-abcdef1234567890';
  let tempDir: string;

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-transport-test-'));
    if (process.platform === 'win32') {
      const pipeId = Math.random().toString(36).substring(2, 10);
      endpoint = `\\\\.\\pipe\\ags-test-${pipeId}`;
    } else {
      endpoint = path.join(tempDir, 'test.sock');
    }
  });

  afterEach(async () => {
    if (server) {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  function startMockDaemon(
    onRequest: (cmd: RustIpcCommand, sendResponse: (resp: unknown) => void, rawSocket: net.Socket) => void
  ): Promise<net.Server> {
    return new Promise((resolve) => {
      server = net.createServer((socket) => {
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

            const sendResponse = (resp: unknown) => {
              const respBytes = Buffer.from(JSON.stringify(resp), 'utf8');
              const header = Buffer.alloc(4);
              header.writeUInt32BE(respBytes.length, 0);
              socket.write(Buffer.concat([header, respBytes]));
            };

            onRequest(cmd, sendResponse, socket);
          }
        });
      });

      server.listen(endpoint, () => {
        resolve(server);
      });
    });
  }

  it('performs full 2-phase handshake (negotiate_capabilities -> authenticate)', async () => {
    let handshakeState = 'init';

    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        handshakeState = 'negotiated';
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: ['worktree_isolation'],
        });
      } else if (cmd.type === 'authenticate') {
        assert.strictEqual(handshakeState, 'negotiated');
        assert.strictEqual(cmd.header.key_id, keyId);

        // Verify HMAC signature
        const verifyRes = RustHmacSigner.verify(
          clientKey,
          cmd.header,
          null,
          ['worktree_isolation']
        );
        assert.strictEqual(verifyRes.valid, true);

        handshakeState = 'authenticated';
        sendResponse({
          type: 'authenticated',
          session_id: 'sess-daemon-001',
        });
      }
    });

    const transport = new NativeIpcTransport({
      endpoint,
      keyId,
      clientKey,
      capabilities: ['worktree_isolation'],
    });

    assert.strictEqual(transport.getState(), 'disconnected');
    await transport.connect();
    assert.strictEqual(transport.getState(), 'ready');
    assert.strictEqual(transport.getSessionId(), 'sess-daemon-001');
    assert.deepStrictEqual(transport.getAgreedCapabilities(), ['worktree_isolation']);

    transport.disconnect();
    assert.strictEqual(transport.getState(), 'disconnected');
  });

  it('executes ping -> pong and health commands over connected transport', async () => {
    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({
          type: 'authenticated',
          session_id: 'sess-01',
        });
      } else if (cmd.type === 'ping') {
        sendResponse({
          type: 'pong',
          payload: cmd.payload,
          client_timestamp: cmd.timestamp,
          server_timestamp: Date.now(),
        });
      } else if (cmd.type === 'get_health') {
        sendResponse({
          type: 'health',
          status: 'ok',
          active_clients: 1,
          uptime_secs: 50,
          memory_events_count: 10,
        });
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    await transport.connect();

    const pong = await transport.sendCommand({
      type: 'ping',
      payload: 'hello',
      timestamp: 12345,
    });
    assert.strictEqual(pong.type, 'pong');
    if (pong.type === 'pong') {
      assert.strictEqual(pong.payload, 'hello');
      assert.strictEqual(pong.client_timestamp, 12345);
    }

    const health = await transport.sendCommand({ type: 'get_health' });
    assert.strictEqual(health.type, 'health');
    if (health.type === 'health') {
      assert.strictEqual(health.status, 'ok');
      assert.strictEqual(health.uptime_secs, 50);
    }

    transport.disconnect();
  });

  it('serializes single-flight commands: queued commands execute strictly in order', async () => {
    const receivedOrder: string[] = [];

    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-01' });
      } else if (cmd.type === 'ping') {
        receivedOrder.push(cmd.payload);
        // Add small asynchronous delay before responding to simulate work
        setTimeout(() => {
          sendResponse({
            type: 'pong',
            payload: cmd.payload,
            client_timestamp: cmd.timestamp,
            server_timestamp: Date.now(),
          });
        }, 10);
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    await transport.connect();

    // Fire 3 concurrent ping requests without awaiting sequentially
    const [p1, p2, p3] = await Promise.all([
      transport.sendCommand({ type: 'ping', payload: 'req-1', timestamp: 1 }),
      transport.sendCommand({ type: 'ping', payload: 'req-2', timestamp: 2 }),
      transport.sendCommand({ type: 'ping', payload: 'req-3', timestamp: 3 }),
    ]);

    assert.strictEqual(p1.type, 'pong');
    assert.strictEqual(p2.type, 'pong');
    assert.strictEqual(p3.type, 'pong');

    // Server must have received commands strictly one by one in order
    assert.deepStrictEqual(receivedOrder, ['req-1', 'req-2', 'req-3']);

    transport.disconnect();
  });

  it('routes unsolicited asynchronous event frames out-of-band without completing command', async () => {
    await startMockDaemon((cmd, sendResponse, socket) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-01' });
      } else if (cmd.type === 'ping') {
        // Send an event BEFORE sending the command's pong
        const eventFrame = {
          type: 'event',
          event: {
            sequence: 1,
            timestamp: new Date().toISOString(),
            event_type: 'task_started',
            payload: { task_id: 'task-abc' },
          },
        };
        const bytes = Buffer.from(JSON.stringify(eventFrame), 'utf8');
        const header = Buffer.alloc(4);
        header.writeUInt32BE(bytes.length, 0);
        socket.write(Buffer.concat([header, bytes]));

        // Then send pong
        setTimeout(() => {
          sendResponse({
            type: 'pong',
            payload: cmd.payload,
            client_timestamp: cmd.timestamp,
            server_timestamp: Date.now(),
          });
        }, 15);
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    await transport.connect();

    const receivedEvents: RustIpcEvent[] = [];
    transport.on('event', (evt: RustIpcEvent) => {
      receivedEvents.push(evt);
    });

    const pong = await transport.sendCommand({ type: 'ping', payload: 'ping-with-event', timestamp: 100 });
    assert.strictEqual(pong.type, 'pong');
    assert.strictEqual(receivedEvents.length, 1);
    assert.strictEqual(receivedEvents[0].event_type, 'task_started');
    assert.strictEqual(transport.getLastReceivedSequence(), 1);

    transport.disconnect();
  });

  it('fails all pending commands when disconnected or socket errors', async () => {
    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-01' });
      }
      // Never respond to subsequent commands
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    await transport.connect();

    const pendingPromise = transport.sendCommand({ type: 'get_health' });

    // Force disconnect
    transport.disconnect();

    await assert.rejects(async () => {
      await pendingPromise;
    }, (err: Error) => {
      return err.message.includes('disconnected');
    });
  });
});
