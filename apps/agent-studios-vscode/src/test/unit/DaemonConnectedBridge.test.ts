/**
 * Agent Studios — DaemonConnectedBridge Unit Tests
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 */

import * as assert from 'assert';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';
import * as fs from 'fs';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import { RustIpcCommand } from '../../bridge/ipc/RustIpcProtocolTypes';

describe('DaemonConnectedBridge Unit Tests', () => {
  let server: net.Server;
  let endpoint: string;
  let tempDir: string;
  const keyId = 'test-client-key';
  const clientKey = 'secret-client-key-1234567890abcdef';

  beforeEach(async () => {
    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-bridge-test-'));
    if (process.platform === 'win32') {
      const pipeId = Math.random().toString(36).substring(2, 10);
      endpoint = `\\\\.\\pipe\\ags-bridge-${pipeId}`;
    } else {
      endpoint = path.join(tempDir, 'bridge.sock');
    }
  });

  afterEach(async () => {
    if (server) {
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
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

  it('starts disconnected and fails closed before connect', async () => {
    const bridge = new DaemonConnectedBridge();
    assert.strictEqual(bridge.connectionState, 'disconnected');

    await assert.rejects(async () => {
      await bridge.getCurrentSession();
    }, (err: Error) => {
      return err.message.includes('not connected');
    });

    await assert.rejects(async () => {
      await bridge.listTasks();
    }, (err: Error) => {
      return err.message.includes('not connected');
    });

    bridge.dispose();
  });

  it('performs full connection lifecycle and queries studios and tasks', async () => {
    let subscribeRequested = false;

    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: ['worktree_isolation'],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-001' });
      } else if (cmd.type === 'subscribe_events') {
        subscribeRequested = true;
        // In the Rust protocol, subscribe_events activates streaming without sending a command response
      } else if (cmd.type === 'list_studios') {
        sendResponse({
          type: 'studios',
          studios: [
            {
              id: 'studio-main-01',
              name: 'Autonomous Architecture Agent',
              workspace_path: '/workspace/test',
              created_at: '2026-10-30T12:00:00Z',
            },
          ],
        });
      } else if (cmd.type === 'get_studio') {
        sendResponse({
          type: 'studio',
          studio: {
            id: cmd.studio_id,
            name: 'Autonomous Architecture Agent',
            workspace_path: '/workspace/test',
            created_at: '2026-10-30T12:00:00Z',
          },
        });
      } else if (cmd.type === 'list_tasks') {
        sendResponse({
          type: 'tasks',
          tasks: [
            {
              id: 'task-1',
              studio_id: 'studio-main-01',
              title: 'Parse Schema',
              description: 'Analyze protobuf schemas',
              state: 'succeeded',
              dependencies: [],
              created_at: '2026-10-30T12:00:00Z',
              updated_at: '2026-10-30T12:05:00Z',
              retry_count: 0,
            },
            {
              id: 'task-2',
              studio_id: 'studio-main-01',
              title: 'Generate Rust Client',
              description: 'Synthesize client bindings',
              state: 'running',
              dependencies: ['task-1'],
              created_at: '2026-10-30T12:05:00Z',
              updated_at: '2026-10-30T12:06:00Z',
              retry_count: 0,
            },
          ],
        });
      } else if (cmd.type === 'get_execution_readiness') {
        sendResponse({
          type: 'execution_readiness',
          readiness: {
            is_ready: true,
            available_runtimes: ['opencode'],
            runtime_instances: [],
            authorized_workspaces: ['/workspace/test'],
            auth_state: { authenticated: true, active_sessions: 1 },
            defaults: {
              default_repo_path: '/workspace/test',
            },
            blocking_reasons: [],
          },
        });
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    const bridge = new DaemonConnectedBridge({ transport });

    const stateChanges: string[] = [];
    bridge.onConnectionStateChanged((state) => stateChanges.push(state));

    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    assert.strictEqual(subscribeRequested, true);
    assert.strictEqual(bridge.activeStudioId, 'studio-main-01');

    // Query session
    const session = await bridge.getCurrentSession();
    assert.ok(session !== null);
    assert.strictEqual(session.id, 'studio-main-01');
    assert.strictEqual(session.goalSummary, 'Autonomous Architecture Agent');
    assert.strictEqual(session.completedTasksCount, 1);
    assert.strictEqual(session.totalTasksCount, 2);
    assert.strictEqual(session.status, 'running');

    // Query tasks
    const tasks = await bridge.listTasks();
    assert.strictEqual(tasks.length, 2);
    assert.strictEqual(tasks[0].id, 'task-1');
    assert.strictEqual(tasks[0].status, 'succeeded');
    assert.strictEqual(tasks[0].progress, 100);
    assert.strictEqual(tasks[1].id, 'task-2');
    assert.strictEqual(tasks[1].status, 'running');
    assert.strictEqual(tasks[1].progress, undefined);

    // Query task graph
    const graph = await bridge.getTaskGraph();
    assert.ok(graph !== null);
    assert.strictEqual(graph.nodes.length, 2);
    assert.strictEqual(graph.edges.length, 1);
    assert.strictEqual(graph.edges[0].from, 'task-1');
    assert.strictEqual(graph.edges[0].to, 'task-2');

    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected');
    bridge.dispose();
  });

  it('queries providers and sanitizes all private secrets', async () => {
    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-001' });
      } else if (cmd.type === 'subscribe_events') {
        // Stream activation, no response
      } else if (cmd.type === 'list_studios') {
        sendResponse({ type: 'studios', studios: [] });
      } else if (cmd.type === 'list_providers') {
        sendResponse({
          type: 'providers',
          providers: [
            {
              id: 'prov-anthropic',
              provider_id: 'anthropic',
              display_name: 'Anthropic Cloud',
              enabled: true,
              authentication: { secret_key: 'sk-ant-SUPER-SECRET-NEVER-LEAK' },
            },
          ],
        });
      } else if (cmd.type === 'list_models') {
        sendResponse({
          type: 'models',
          models: [
            {
              provider_instance_id: 'prov-anthropic',
              id: 'claude-3-7-sonnet',
              display_name: 'Claude 3.7 Sonnet',
              limits: { context_window_tokens: 200000, max_output_tokens: 8192 },
            },
          ],
        });
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    const bridge = new DaemonConnectedBridge({ transport });
    await bridge.connect();

    const providers = await bridge.listProviderSummaries();
    assert.strictEqual(providers.length, 1);
    assert.strictEqual(providers[0].displayName, 'Anthropic Cloud');
    assert.strictEqual(providers[0].status, 'available');
    assert.strictEqual(providers[0].models.length, 1);
    assert.strictEqual(providers[0].models[0].name, 'Claude 3.7 Sonnet');
    assert.strictEqual(providers[0].models[0].contextWindow, 200000);

    // Verify secret key was not leaked
    const jsonString = JSON.stringify(providers);
    assert.strictEqual(jsonString.includes('SUPER-SECRET'), false);

    bridge.dispose();
  });

  it('fails closed on mutating operations in read-only slice', async () => {
    await startMockDaemon((cmd, sendResponse) => {
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-001' });
      } else if (cmd.type === 'subscribe_events') {
        // Stream activation, no response
      } else if (cmd.type === 'list_studios') {
        sendResponse({ type: 'studios', studios: [] });
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    const bridge = new DaemonConnectedBridge({ transport });
    await bridge.connect();

    // 1. Approval decision fails closed
    const approvalRes = await bridge.requestApprovalDecision('appr-01', 'approved');
    assert.strictEqual(approvalRes.applied, false);
    assert.ok(approvalRes.message?.includes('disabled in M14-IPC-03'));

    // 2. Editor action fails closed
    const editorRes = await bridge.submitEditorAction({
      action: 'refactor',
      documentUri: 'file:///test.ts',
      languageId: 'typescript',
      selectedText: 'code',
      selectionRange: { startLine: 0, startCol: 0, endLine: 1, endCol: 1 },
    });
    assert.strictEqual(editorRes.accepted, false);
    assert.ok(editorRes.message.includes('disabled in M14-IPC-03'));

    // 3. Send message fails closed
    await assert.rejects(async () => {
      await bridge.sendMessage(
        { message: 'Do something', mode: 'agent' },
        () => {}
      );
    }, (err: Error) => {
      return err.message.includes('disabled in M14-IPC-03');
    });

    // 4. Create session fails closed
    await assert.rejects(async () => {
      await bridge.createSession('New goal');
    }, (err: Error) => {
      return err.message.includes('disabled in M14-IPC-03');
    });

    bridge.dispose();
  });

  it('handles event streaming, watermarking, and deduplication', async () => {
    let mockSocket: net.Socket;

    await startMockDaemon((cmd, sendResponse, socket) => {
      mockSocket = socket;
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-001' });
      } else if (cmd.type === 'subscribe_events') {
        // Stream activation, no response
      } else if (cmd.type === 'list_studios') {
        sendResponse({ type: 'studios', studios: [] });
      }
    });

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    const bridge = new DaemonConnectedBridge({ transport });
    await bridge.connect();

    const receivedEvents: string[] = [];
    bridge.onEvent((evt) => {
      receivedEvents.push(`${evt.sequenceNumber}:${evt.type}`);
    });

    const sendEvent = (seq: number, eventType: string, payload: Record<string, unknown> = {}) => {
      const frame = {
        type: 'event',
        event: {
          sequence: seq,
          timestamp: '2026-10-30T12:00:00Z',
          event_type: eventType,
          payload,
        },
      };
      const bytes = Buffer.from(JSON.stringify(frame), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(bytes.length, 0);
      mockSocket.write(Buffer.concat([header, bytes]));
    };

    // Emit event 1
    sendEvent(1, 'task_started', { task_id: 'task-1', summary: 'Task 1 started' });

    // Emit duplicate event 1 (must be deduplicated)
    sendEvent(1, 'task_started', { task_id: 'task-1', summary: 'Task 1 duplicate' });

    // Emit event 2 (approval requested)
    sendEvent(2, 'approval_requested', {
      id: 'appr-101',
      sessionId: 'studio-01',
      actionType: 'exec_command',
      commandOrTarget: 'cargo build',
      riskLevel: 'medium',
      requestedPermissions: ['shell'],
      status: 'pending',
      createdAt: Date.now(),
    });

    // Wait briefly for asynchronous delivery
    await new Promise((r) => setTimeout(r, 40));

    assert.strictEqual(receivedEvents.length, 2);
    assert.strictEqual(receivedEvents[0], '1:task_started');
    assert.strictEqual(receivedEvents[1], '2:approval_requested');
    assert.strictEqual(bridge.lastWatermark, 2);

    const pending = await bridge.listPendingApprovals();
    assert.strictEqual(pending.length, 1);
    assert.strictEqual(pending[0].id, 'appr-101');

    bridge.dispose();
  });

  it('reconnects with monotonic sequence watermark and handles replay deduplication', async () => {
    let requestedFromSeq: number | undefined;
    let currentSocket: net.Socket;

    await startMockDaemon((cmd, sendResponse, socket) => {
      currentSocket = socket;
      if (cmd.type === 'negotiate_capabilities') {
        sendResponse({
          type: 'capabilities_negotiated',
          selected_version: { major: 1, minor: 0, patch: 0 },
          agreed_capabilities: [],
        });
      } else if (cmd.type === 'authenticate') {
        sendResponse({ type: 'authenticated', session_id: 'sess-001' });
      } else if (cmd.type === 'subscribe_events') {
        requestedFromSeq = cmd.from_sequence;
      } else if (cmd.type === 'list_studios') {
        sendResponse({ type: 'studios', studios: [] });
      }
    });

    const sendEvent = (seq: number, eventType: string) => {
      const frame = {
        type: 'event',
        event: {
          sequence: seq,
          timestamp: '2026-10-30T12:00:00Z',
          event_type: eventType,
          payload: { summary: `Event ${seq}` },
        },
      };
      const bytes = Buffer.from(JSON.stringify(frame), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(bytes.length, 0);
      currentSocket.write(Buffer.concat([header, bytes]));
    };

    const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
    const bridge = new DaemonConnectedBridge({ transport });
    await bridge.connect();

    // Initial subscription had undefined sequence watermark
    assert.strictEqual(requestedFromSeq, undefined);

    const received: number[] = [];
    bridge.onEvent((e) => received.push(e.sequenceNumber));

    // Send events 1, 2, 3
    sendEvent(1, 'task_started');
    sendEvent(2, 'task_started');
    sendEvent(3, 'task_started');
    await new Promise((r) => setTimeout(r, 40));

    assert.strictEqual(bridge.lastWatermark, 3);
    assert.deepStrictEqual(received, [1, 2, 3]);

    // Disconnect
    await bridge.disconnect();
    assert.strictEqual(bridge.connectionState, 'disconnected');

    // Reconnect
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');
    // Daemon should have been asked for sequence watermark 4 (lastWatermark + 1)
    assert.strictEqual(requestedFromSeq, 4);

    // If daemon re-sends event 3 along with event 4 (e.g. at-least-once delivery):
    sendEvent(3, 'task_started');
    sendEvent(4, 'task_completed');
    await new Promise((r) => setTimeout(r, 40));

    // Event 3 was deduplicated, event 4 was processed
    assert.deepStrictEqual(received, [1, 2, 3, 4]);
    assert.strictEqual(bridge.lastWatermark, 4);

    bridge.dispose();
  });
});
