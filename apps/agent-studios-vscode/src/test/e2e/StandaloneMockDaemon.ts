/**
 * Agent Studios — Standalone Daemon Process for Cross-Process E2E Testing
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Runs as an independent OS child process with separate PID and memory space.
 * Binds to real Windows Named Pipe or Unix Domain Socket.
 * Implements authoritative Rust agsd wire protocol framing, HMAC authentication,
 * and Serde tagged command/response dispatch.
 */

import * as net from 'net';
import * as fs from 'fs';
import * as path from 'path';
import { RustHmacSigner } from '../../bridge/ipc/RustHmacSigner';
import { RustIpcCommand, RustIpcResponse } from '../../bridge/ipc/RustIpcProtocolTypes';

function parseArgs(): {
  bootstrapPath: string;
  pipeName?: string;
  socketPath?: string;
  keyId: string;
  clientKey: string;
} {
  const args = process.argv.slice(2);
  let bootstrapPath = '';
  let pipeName: string | undefined;
  let socketPath: string | undefined;
  let keyId = 'ags-client';
  let clientKey = 'synthetic-client-key-1234567890123456';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--bootstrap-path' && i + 1 < args.length) {
      bootstrapPath = args[++i];
    } else if (args[i] === '--pipe-name' && i + 1 < args.length) {
      pipeName = args[++i];
    } else if (args[i] === '--socket-path' && i + 1 < args.length) {
      socketPath = args[++i];
    } else if (args[i] === '--key-id' && i + 1 < args.length) {
      keyId = args[++i];
    } else if (args[i] === '--client-key' && i + 1 < args.length) {
      clientKey = args[++i];
    }
  }

  if (!bootstrapPath) {
    throw new Error('Missing required --bootstrap-path argument.');
  }

  return { bootstrapPath, pipeName, socketPath, keyId, clientKey };
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
  let eventSequence = 1;

  const server = net.createServer((socket) => {
    clientSockets.add(socket);
    let state: 'unauthenticated' | 'negotiated' | 'ready' = 'unauthenticated';
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

            const verifyRes = RustHmacSigner.verify(
              config.clientKey,
              command.header,
              null,
              agreedCapabilities
            );

            if (!verifyRes.valid) {
              sendResponse({ type: 'error', message: `Authentication failed: ${verifyRes.error}` });
              break;
            }

            state = 'ready';
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
              status: 'ok',
              active_clients: clientSockets.size,
              uptime_secs: Math.floor(process.uptime()),
              memory_events_count: eventSequence,
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
              studios: [
                {
                  id: 'studio-e2e-01',
                  name: 'E2E Cross-Process Studio',
                  created_at: new Date().toISOString(),
                },
              ],
            });
            break;
          }

          case 'get_studio': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            sendResponse({
              type: 'studio',
              studio: {
                id: command.studio_id,
                name: 'E2E Cross-Process Studio',
                created_at: new Date().toISOString(),
              },
            });
            break;
          }

          case 'list_tasks': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
            sendResponse({
              type: 'tasks',
              tasks: [
                {
                  id: 'task-e2e-01',
                  studio_id: command.studio_id,
                  title: 'Cross-Process Verification Task',
                  description: 'Validate IPC transport over native socket',
                  state: 'succeeded',
                  dependencies: [],
                  created_at: new Date().toISOString(),
                  updated_at: new Date().toISOString(),
                  retry_count: 0,
                },
              ],
            });
            break;
          }

          case 'list_providers': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
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
            break;
          }

          case 'list_models': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }
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
            break;
          }

          case 'subscribe_events': {
            if (state !== 'ready') {
              sendResponse({ type: 'error', message: 'Unauthorized' });
              break;
            }

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
            sendResponse({ type: 'shutdown_acknowledged' });
            setTimeout(() => {
              cleanupAndExit(0);
            }, 50);
            break;
          }

          default:
            sendResponse({
              type: 'error',
              message: `Unsupported command in mock daemon: ${(command as { type: string }).type}`,
            });
            break;
        }
      }
    });

    socket.on('close', () => {
      clientSockets.delete(socket);
    });

    socket.on('error', () => {
      clientSockets.delete(socket);
    });
  });

  function cleanupAndExit(code: number): void {
    try {
      if (fs.existsSync(config.bootstrapPath)) {
        fs.unlinkSync(config.bootstrapPath);
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

    // Notify parent process on stdout
    process.stdout.write(`DAEMON_READY:${process.pid}:${endpoint}\n`);
  });
}

main().catch((err: unknown) => {
  console.error('[StandaloneMockDaemon] Fatal error:', err);
  process.exit(1);
});
