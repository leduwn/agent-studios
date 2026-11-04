/**
 * Agent Studios — Standalone Daemon Process for Cross-Process E2E Testing
 * Milestone M14-IPC-03 & M14 Mission Control Live Projection
 *
 * Runs as an independent OS child process with separate PID and memory space.
 * Binds to real Windows Named Pipe or Unix Domain Socket.
 * Implements authoritative Rust agsd wire protocol framing, HMAC authentication,
 * Serde tagged command/response dispatch, and seeded control plane mutations.
 */

import * as net from 'net';
import * as fs from 'fs';
import * as path from 'path';
import { RustHmacSigner } from '../../bridge/ipc/RustHmacSigner';
import {
  RustIpcCommand,
  RustIpcResponse,
  RustStudio,
  RustTaskRecord,
  RustTaskState,
  RustIpcEvent,
} from '../../bridge/ipc/RustIpcProtocolTypes';

function parseArgs(): {
  bootstrapPath: string;
  adminBootstrapPath?: string;
  pipeName?: string;
  socketPath?: string;
  keyId: string;
  clientKey: string;
  adminKey: string;
} {
  const args = process.argv.slice(2);
  let bootstrapPath = '';
  let adminBootstrapPath: string | undefined;
  let pipeName: string | undefined;
  let socketPath: string | undefined;
  let keyId = 'ags-client';
  let clientKey = 'synthetic-client-key-1234567890123456';
  let adminKey = 'synthetic-admin-key-9876543210654321';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--bootstrap-path' || args[i] === '--runtime-info-path' || args[i] === '--runtime_info_path') {
      bootstrapPath = args[++i];
    } else if (args[i] === '--admin-bootstrap-path' || args[i] === '--admin-runtime-info-path' || args[i] === '--admin_runtime_info_path') {
      adminBootstrapPath = args[++i];
    } else if (args[i] === '--pipe-name' || args[i] === '--pipe_name') {
      pipeName = args[++i];
    } else if (args[i] === '--socket-path' || args[i] === '--socket_path') {
      socketPath = args[++i];
    } else if (args[i] === '--key-id' || args[i] === '--key_id') {
      keyId = args[++i];
    } else if (args[i] === '--client-key' || args[i] === '--client_key') {
      clientKey = args[++i];
    } else if (args[i] === '--admin-key' || args[i] === '--admin_key') {
      adminKey = args[++i];
    }
  }

  if (!bootstrapPath) {
    throw new Error('Missing required --bootstrap-path / --runtime-info-path argument.');
  }

  return { bootstrapPath, adminBootstrapPath, pipeName, socketPath, keyId, clientKey, adminKey };
}

async function main(): Promise<void> {
  const config = parseArgs();
  const isWindows = process.platform === 'win32';

  const endpoint = isWindows
    ? config.pipeName ?? `\\\\.\\pipe\\ags-e2e-${process.pid}`
    : config.socketPath ?? `/tmp/ags-e2e-${process.pid}.sock`;

  // Ensure parent directory for bootstrap file exists
  const parentDir = path.dirname(config.bootstrapPath);
  if (!fs.existsSync(parentDir)) {
    fs.mkdirSync(parentDir, { recursive: true });
  }

  // Active connected sockets
  const clientSockets = new Set<net.Socket>();
  const subscribedSockets = new Set<net.Socket>();
  let eventSequence = 1;

  // In-memory seeded Control Plane state
  const studios: RustStudio[] = [
    {
      id: 'studio-e2e-01',
      name: 'E2E Cross-Process Studio',
      created_at: new Date().toISOString(),
    },
  ];

  const tasks: RustTaskRecord[] = [
    {
      id: 'task-e2e-01',
      studio_id: 'studio-e2e-01',
      title: 'Cross-Process Verification Task',
      description: 'Validate IPC transport over native socket',
      state: 'succeeded',
      dependencies: [],
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      retry_count: 0,
    },
  ];

  const server = net.createServer((socket) => {
    clientSockets.add(socket);
    let state: 'unauthenticated' | 'negotiated' | 'ready' = 'unauthenticated';
    let clientRole: 'standard' | 'admin' = 'standard';
    let agreedCapabilities: string[] = [];
    let buffer = Buffer.alloc(0);

    const sendResponse = (resp: RustIpcResponse) => {
      if (!socket.destroyed) {
        const jsonBytes = Buffer.from(JSON.stringify(resp), 'utf8');
        const header = Buffer.alloc(4);
        header.writeUInt32BE(jsonBytes.length, 0);
        socket.write(Buffer.concat([header, jsonBytes]));
      }
    };

    socket.on('data', (chunk: Buffer) => {
      buffer = Buffer.concat([buffer, chunk]);
      while (buffer.length >= 4) {
        const frameLength = buffer.readUInt32BE(0);
        if (buffer.length < 4 + frameLength) {
          break;
        }

        const payloadBuffer = buffer.subarray(4, 4 + frameLength);
        buffer = buffer.subarray(4 + frameLength);

        let command: RustIpcCommand;
        try {
          command = JSON.parse(payloadBuffer.toString('utf8')) as RustIpcCommand;
        } catch {
          sendResponse({ type: 'error', message: 'Malformed JSON payload' });
          continue;
        }

        // Command handler adhering to Rust agsd protocol
        switch (command.type) {
          case 'negotiate_capabilities': {
            agreedCapabilities = command.capabilities;
            state = 'negotiated';
            sendResponse({
              type: 'capabilities_negotiated',
              selected_version: { major: 1, minor: 0, patch: 0 },
              agreed_capabilities: agreedCapabilities,
            });
            break;
          }

          case 'authenticate': {
            if (state !== 'negotiated') {
              sendResponse({ type: 'error', message: 'Must negotiate capabilities first' });
              break;
            }

            const isAdmin = command.header.key_id === 'ags-admin' || command.header.key_id === 'ags-client-admin';
            const expectedKey = isAdmin ? config.adminKey : config.clientKey;

            const verifyRes = RustHmacSigner.verify(
              expectedKey,
              command.header,
              null,
              agreedCapabilities
            );

            if (!verifyRes.valid) {
              sendResponse({ type: 'error', message: `Authentication failed: ${verifyRes.error}` });
              break;
            }

            state = 'ready';
            clientRole = isAdmin ? 'admin' : 'standard';
            sendResponse({
              type: 'authenticated',
              session_id: `daemon-session-${process.pid}`,
            });
            break;
          }

          case 'ping': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            sendResponse({
              type: 'pong',
              payload: command.payload,
              client_timestamp: command.timestamp,
              server_timestamp: Date.now(),
            });
            break;
          }

          case 'get_health': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            sendResponse({
              type: 'health',
              status: config.adminBootstrapPath ? 'healthy' : 'ok',
              active_clients: clientSockets.size,
              uptime_secs: Math.floor(process.uptime()),
              memory_events_count: eventSequence,
            });
            break;
          }

          case 'create_studio': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            const studioId = `studio-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`;
            const newStudio: RustStudio = {
              id: studioId,
              name: command.name,
              created_at: new Date().toISOString(),
            };
            studios.unshift(newStudio);
            sendResponse({
              type: 'studio_created',
              studio: newStudio,
            });
            break;
          }

          case 'list_studios': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            sendResponse({
              type: 'studios',
              studios: [...studios],
            });
            break;
          }

          case 'get_studio': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            const studio = studios.find((s) => s.id === command.studio_id) || studios[0] || null;
            sendResponse({
              type: 'studio',
              studio,
            });
            break;
          }

          case 'submit_task': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            const spec = command.task_spec;
            const taskId = spec.key || `task-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`;
            let taskState: RustTaskState = 'running';
            if ((spec as unknown as { state?: RustTaskState }).state) {
              taskState = (spec as unknown as { state: RustTaskState }).state;
            } else if (
              spec.title.toLowerCase().includes('compile') ||
              spec.title.toLowerCase().includes('succeed')
            ) {
              taskState = 'succeeded';
            } else if (
              spec.title.toLowerCase().includes('run') ||
              spec.title.toLowerCase().includes('suite')
            ) {
              taskState = 'running';
            }

            const newTask: RustTaskRecord = {
              id: taskId,
              studio_id: command.studio_id,
              title: spec.title,
              description: spec.description,
              state: taskState,
              dependencies: spec.dependency_task_ids || spec.dependency_keys || [],
              created_at: new Date().toISOString(),
              updated_at: new Date().toISOString(),
              retry_count: 0,
            };
            tasks.push(newTask);
            sendResponse({
              type: 'tasks_submitted',
              task_ids: [taskId],
            });
            break;
          }

          case 'list_tasks': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            const matchingTasks = command.studio_id
              ? tasks.filter((t) => t.studio_id === command.studio_id)
              : tasks;
            sendResponse({
              type: 'tasks',
              tasks: matchingTasks,
            });
            break;
          }

          case 'get_task': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            const foundTask = tasks.find((t) => t.id === command.task_id) || null;
            sendResponse({
              type: 'task',
              task: foundTask,
            });
            break;
          }

          case 'list_providers': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            if (config.adminBootstrapPath) {
              sendResponse({
                type: 'providers',
                providers: [
                  {
                    id: 'prov-anthropic',
                    provider_id: 'anthropic',
                    display_name: 'Anthropic Claude',
                    enabled: true,
                  },
                  {
                    id: 'prov-openai',
                    provider_id: 'openai',
                    display_name: 'OpenAI GPT',
                    enabled: true,
                  },
                ],
              });
            } else {
              sendResponse({
                type: 'providers',
                providers: [
                  {
                    id: 'prov-mock',
                    provider_id: 'mock-provider',
                    display_name: 'Mock Test Provider',
                    enabled: true,
                  },
                ],
              });
            }
            break;
          }

          case 'list_models': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            if (config.adminBootstrapPath) {
              const instanceId = command.instance_id || 'prov-anthropic';
              const isOai = instanceId === 'prov-openai';
              sendResponse({
                type: 'models',
                models: [
                  {
                    provider_instance_id: instanceId,
                    id: isOai ? 'gpt-4o' : 'claude-3-7-sonnet',
                    display_name: isOai ? 'GPT-4o' : 'Claude 3.7 Sonnet',
                    limits: {
                      context_window_tokens: isOai ? 128000 : 200000,
                      max_output_tokens: 8192,
                    },
                  },
                ],
              });
            } else {
              sendResponse({
                type: 'models',
                models: [
                  {
                    provider_instance_id: command.instance_id ?? 'prov-mock',
                    id: 'mock-model-01',
                    display_name: 'Mock Model v1',
                    limits: { context_window_tokens: 128000, max_output_tokens: 4096 },
                  },
                ],
              });
            }
            break;
          }

          case 'subscribe_events': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            subscribedSockets.add(socket);
            // Immediately emit a task_started event as proof of event streaming
            const evtResponse: RustIpcResponse = {
              type: 'event',
              event: {
                sequence: eventSequence++,
                timestamp: new Date().toISOString(),
                event_type: 'task_started',
                payload: {
                  task_id: 'task-e2e-01',
                  summary: 'Cross-process task streaming started',
                },
              },
            };
            sendResponse(evtResponse);
            break;
          }

          case 'shutdown': {
            if (clientRole !== 'admin') {
              sendResponse({
                type: 'error',
                message: 'Permission denied: shutdown requires admin role',
              });
              break;
            }
            sendResponse({ type: 'shutdown_acknowledged' });
            setTimeout(() => {
              cleanupAndExit(0);
            }, 50);
            break;
          }

          default: {
            // Handle custom test-only command to broadcast events
            const cmdAny = command as unknown as { type: string; event_type?: string; payload?: unknown };
            if (cmdAny.type === 'emit_test_event') {
              const evt: RustIpcEvent = {
                sequence: eventSequence++,
                timestamp: new Date().toISOString(),
                event_type: cmdAny.event_type || 'task_failed',
                payload: cmdAny.payload ?? { summary: 'Broadcasted test event' },
              };
              for (const sub of subscribedSockets) {
                if (!sub.destroyed) {
                  const jsonBytes = Buffer.from(JSON.stringify({ type: 'event', event: evt }), 'utf8');
                  const hdr = Buffer.alloc(4);
                  hdr.writeUInt32BE(jsonBytes.length, 0);
                  sub.write(Buffer.concat([hdr, jsonBytes]));
                }
              }
              sendResponse({
                type: 'pong',
                payload: 'emitted',
                client_timestamp: 0,
                server_timestamp: Date.now(),
              });
              break;
            }

            sendResponse({
              type: 'error',
              message: `Unsupported command in mock daemon: ${cmdAny.type}`,
            });
            break;
          }
        }
      }
    });

    socket.on('close', () => {
      clientSockets.delete(socket);
      subscribedSockets.delete(socket);
    });

    socket.on('error', () => {
      clientSockets.delete(socket);
      subscribedSockets.delete(socket);
    });
  });

  function cleanupAndExit(code: number): void {
    try {
      if (fs.existsSync(config.bootstrapPath)) {
        fs.unlinkSync(config.bootstrapPath);
      }
      if (config.adminBootstrapPath && fs.existsSync(config.adminBootstrapPath)) {
        fs.unlinkSync(config.adminBootstrapPath);
      }
      if (!isWindows && fs.existsSync(endpoint)) {
        fs.unlinkSync(endpoint);
      }
    } catch {
      // Ignore
    }
    server.close(() => {
      process.exit(code);
    });
  }

  process.on('SIGINT', () => cleanupAndExit(0));
  process.on('SIGTERM', () => cleanupAndExit(0));

  server.listen(endpoint, () => {
    // Write agsd.json bootstrap file
    const bootstrapInfo = {
      endpoint,
      key_id: config.keyId,
      client_key: config.clientKey,
      admin_key: null,
      pid: process.pid,
      created_at: Date.now(),
    };

    fs.writeFileSync(config.bootstrapPath, JSON.stringify(bootstrapInfo, null, 2), 'utf8');

    // Write agsd-admin.json bootstrap file if path provided
    if (config.adminBootstrapPath) {
      const adminParentDir = path.dirname(config.adminBootstrapPath);
      if (!fs.existsSync(adminParentDir)) {
        fs.mkdirSync(adminParentDir, { recursive: true });
      }
      const adminInfo = {
        endpoint,
        key_id: 'ags-client-admin',
        admin_key: config.adminKey,
        pid: process.pid,
        created_at: Date.now(),
      };
      fs.writeFileSync(config.adminBootstrapPath, JSON.stringify(adminInfo, null, 2), 'utf8');
    }

    // Notify parent process on stdout
    process.stdout.write(`DAEMON_READY:${process.pid}:${endpoint}\n`);
  });
}

main().catch((err: unknown) => {
  console.error('[StandaloneMockDaemon] Fatal error:', err);
  process.exit(1);
});
