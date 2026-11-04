/**
 * Agent Studios — Native Node.js IPC Transport
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Implements native IPC communication over Windows Named Pipes (`\\.\pipe\...`)
 * and Linux/macOS Unix Domain Sockets (`/tmp/...`), matching Rust agsd daemon.
 *
 * Technical Disciplines:
 * 1. 4-byte BE length prefix with authoritative 4 MiB frame limit.
 * 2. Anti-DoS bounded buffer accumulation (`MAX_ACCUMULATED_BUFFER_SIZE = 4 MiB + 4`).
 * 3. Serialized single-flight command queue (daemon responses do not carry request IDs).
 * 4. Asynchronous event routing without corrupting in-flight command promises.
 * 5. Two-phase HMAC-SHA256 authenticated handshake (`negotiate_capabilities` -> `authenticate`).
 * 6. Monotonic event sequence watermarking.
 */

import * as net from 'net';
import * as crypto from 'crypto';
import { EventEmitter } from 'events';
import { RustIpcCodec } from './RustIpcCodec';
import { RustHmacSigner } from './RustHmacSigner';
import {
  RustIpcCommand,
  RustIpcResponse,
  RustIpcEvent,
  RustProtocolVersion,
  RUST_PROTOCOL_VERSION_1_0_0,
} from './RustIpcProtocolTypes';

export type TransportState =
  | 'disconnected'
  | 'connecting'
  | 'authenticating'
  | 'ready'
  | 'error';

export interface TransportOptions {
  endpoint: string;
  keyId: string;
  clientKey: string;
  capabilities?: string[];
  commandTimeoutMs?: number;
  connectTimeoutMs?: number;
  connectRetryDelayMs?: number;
}

interface QueuedCommand {
  command: RustIpcCommand;
  resolve: (response: RustIpcResponse) => void;
  reject: (error: Error) => void;
  timeoutTimer: NodeJS.Timeout;
}

export class NativeIpcTransport extends EventEmitter {
  private socket: net.Socket | null = null;
  private connectingSocket: net.Socket | null = null;
  private connectRetryTimer: NodeJS.Timeout | null = null;
  private connectRetryAbort: (() => void) | null = null;
  private state: TransportState = 'disconnected';
  private accumulatedBuffer: Buffer = Buffer.alloc(0);

  private readonly requestQueue: QueuedCommand[] = [];
  private inFlight: QueuedCommand | null = null;

  private sessionId: string | null = null;
  private agreedCapabilities: string[] = [];
  private lastReceivedSequence: number = 0;

  constructor(private readonly options: TransportOptions) {
    super();
  }

  public getState(): TransportState {
    return this.state;
  }

  public get endpoint(): string {
    return this.options.endpoint;
  }

  public getSessionId(): string | null {
    return this.sessionId;
  }

  public getAgreedCapabilities(): string[] {
    return [...this.agreedCapabilities];
  }

  public getLastReceivedSequence(): number {
    return this.lastReceivedSequence;
  }

  /**
   * Connects to the native socket and drives the 2-phase handshake.
   * Includes resilient connection retry logic on transient ENOENT, ECONNREFUSED, EPIPE.
   */
  public async connect(): Promise<void> {
    if (this.state === 'ready' || this.state === 'connecting' || this.state === 'authenticating') {
      return;
    }

    this.setState('connecting');

    let socket: net.Socket;
    try {
      socket = await this.connectSocketWithRetry();
    } catch (err: unknown) {
      if ((this.state as TransportState) !== 'disconnected') {
        this.setState('error');
      }
      throw err;
    }

    this.socket = socket;

    socket.on('data', (chunk: Buffer) => {
      this.handleData(chunk);
    });

    socket.on('error', (err: Error) => {
      this.handleSocketError(err);
    });

    socket.on('close', () => {
      this.handleSocketClose();
    });

    try {
      await this.performHandshake();
      this.setState('ready');
    } catch (err: unknown) {
      this.disconnect();
      throw err;
    }
  }

  private async connectSocketWithRetry(): Promise<net.Socket> {
    const timeoutMs = this.options.connectTimeoutMs ?? 1500;
    const retryDelayMs = this.options.connectRetryDelayMs ?? 50;
    const deadline = Date.now() + timeoutMs;

    const isRetryable = (err: unknown): boolean => {
      if (!err || typeof err !== 'object') {
        return false;
      }
      const code = (err as { code?: string }).code;
      if (code === 'ENOENT' || code === 'ECONNREFUSED' || code === 'EPIPE') {
        return true;
      }
      const msg = (err as { message?: string }).message;
      return (
        typeof msg === 'string' &&
        (msg.includes('ENOENT') || msg.includes('ECONNREFUSED') || msg.includes('EPIPE'))
      );
    };

    while (true) {
      if (this.state !== 'connecting') {
        throw new Error('Connection aborted: transport is no longer in connecting state.');
      }

      try {
        const socket = await new Promise<net.Socket>((resolve, reject) => {
          let connected = false;
          let candidateSocket: net.Socket | null = null;

          const cleanup = () => {
            if (candidateSocket && !connected) {
              candidateSocket.removeAllListeners();
              candidateSocket.destroy();
            }
            if (this.connectingSocket === candidateSocket) {
              this.connectingSocket = null;
            }
          };

          try {
            candidateSocket = net.createConnection(this.options.endpoint, () => {
              connected = true;
              this.connectingSocket = null;
              resolve(candidateSocket!);
            });
            this.connectingSocket = candidateSocket;
          } catch (syncErr) {
            cleanup();
            reject(syncErr);
            return;
          }

          candidateSocket.once('error', (err: Error) => {
            if (!connected) {
              cleanup();
              reject(err);
            }
          });
        });

        return socket;
      } catch (err: unknown) {
        if (this.state !== 'connecting') {
          throw err;
        }

        if (isRetryable(err) && Date.now() < deadline) {
          const remaining = deadline - Date.now();
          const delay = Math.min(retryDelayMs, remaining);
          if (delay > 0) {
            await new Promise<void>((resolve, reject) => {
              this.connectRetryAbort = () => {
                reject(new Error('Connection aborted: transport disconnected.'));
              };
              this.connectRetryTimer = setTimeout(() => {
                this.connectRetryTimer = null;
                this.connectRetryAbort = null;
                resolve();
              }, delay);
            });
            continue;
          }
        }

        throw err;
      }
    }
  }

  /**
   * Disconnects the socket cleanly and clears pending command queue.
   */
  public disconnect(): void {
    if (this.connectRetryTimer) {
      clearTimeout(this.connectRetryTimer);
      this.connectRetryTimer = null;
    }
    if (this.connectRetryAbort) {
      this.connectRetryAbort();
      this.connectRetryAbort = null;
    }
    if (this.connectingSocket) {
      this.connectingSocket.removeAllListeners();
      this.connectingSocket.destroy();
      this.connectingSocket = null;
    }
    if (this.socket) {
      this.socket.removeAllListeners();
      this.socket.destroy();
      this.socket = null;
    }
    this.failAllPending(new Error('IPC transport disconnected.'));
    this.setState('disconnected');
    this.accumulatedBuffer = Buffer.alloc(0);
    this.sessionId = null;
  }

  /**
   * Sends a RustIpcCommand adhering to serialized single-flight discipline.
   */
  public sendCommand(command: RustIpcCommand, timeoutMs?: number): Promise<RustIpcResponse> {
    if (this.state === 'disconnected' || this.state === 'error') {
      return Promise.reject(new Error(`Cannot send command in transport state '${this.state}'.`));
    }

    return new Promise<RustIpcResponse>((resolve, reject) => {
      const effectiveTimeout = timeoutMs ?? this.options.commandTimeoutMs ?? 30000;

      const timeoutTimer = setTimeout(() => {
        if (this.inFlight && this.inFlight.command === command) {
          const timeoutErr = new Error(`Command '${command.type}' timed out after ${effectiveTimeout}ms.`);
          this.inFlight.reject(timeoutErr);
          this.inFlight = null;
          this.drainQueue();
        }
      }, effectiveTimeout);

      this.requestQueue.push({
        command,
        resolve,
        reject,
        timeoutTimer,
      });

      this.drainQueue();
    });
  }

  /**
   * Subscribes to events starting from a specific sequence watermark.
   * Note: In the authoritative Rust wire protocol, SubscribeEvents activates streaming
   * and yields event frames directly; it does not return a dedicated command response.
   */
  public async subscribeEvents(fromSequence?: number): Promise<void> {
    if (this.state === 'disconnected' || this.state === 'error' || !this.socket) {
      throw new Error(`Cannot subscribe to events in transport state '${this.state}'.`);
    }

    const seq = fromSequence ?? (this.lastReceivedSequence > 0 ? this.lastReceivedSequence + 1 : undefined);
    const command: RustIpcCommand = {
      type: 'subscribe_events',
      from_sequence: seq,
    };

    const wireFrame = RustIpcCodec.encodeCommand(command);
    this.socket.write(wireFrame);
  }

  private async performHandshake(): Promise<void> {
    this.setState('authenticating');

    const capabilities = this.options.capabilities ?? ['worktree_isolation', 'event_streaming'];
    const version: RustProtocolVersion = RUST_PROTOCOL_VERSION_1_0_0;

    // Step 1: Negotiate capabilities
    const negResp = await this.sendCommand({
      type: 'negotiate_capabilities',
      protocol_version: version,
      capabilities,
    });

    if (negResp.type === 'error') {
      throw new Error(`Handshake failed on capability negotiation: ${negResp.message}`);
    }
    if (negResp.type !== 'capabilities_negotiated') {
      throw new Error(`Unexpected response during capability negotiation: ${negResp.type}`);
    }

    this.agreedCapabilities = negResp.agreed_capabilities;

    // Step 2: Generate HMAC signature and Authenticate
    const sessionId = `sess_${crypto.randomUUID().replace(/-/g, '')}`;
    const nonce = `nonce_${crypto.randomUUID().replace(/-/g, '')}`;
    const timestampMillis = Date.now();

    const authHeader = RustHmacSigner.sign(
      this.options.clientKey,
      negResp.selected_version,
      this.options.keyId,
      sessionId,
      nonce,
      timestampMillis,
      negResp.agreed_capabilities,
      null
    );

    const authResp = await this.sendCommand({
      type: 'authenticate',
      header: authHeader,
      payload: null,
    });

    if (authResp.type === 'error') {
      throw new Error(`Handshake failed on authentication: ${authResp.message}`);
    }
    if (authResp.type !== 'authenticated') {
      throw new Error(`Unexpected response during authentication: ${authResp.type}`);
    }

    this.sessionId = authResp.session_id;
  }

  private drainQueue(): void {
    if (this.inFlight !== null || this.requestQueue.length === 0 || !this.socket || this.socket.destroyed) {
      return;
    }

    const item = this.requestQueue.shift();
    if (!item) {
      return;
    }

    this.inFlight = item;

    try {
      const wireFrame = RustIpcCodec.encodeCommand(item.command);
      this.socket.write(wireFrame);
    } catch (err: unknown) {
      clearTimeout(item.timeoutTimer);
      this.inFlight = null;
      item.reject(err instanceof Error ? err : new Error(String(err)));
      this.drainQueue();
    }
  }

  private handleData(chunk: Buffer): void {
    this.accumulatedBuffer = Buffer.concat([this.accumulatedBuffer, chunk]);

    try {
      const { responses, remaining } = RustIpcCodec.decodeResponses(this.accumulatedBuffer);
      this.accumulatedBuffer = remaining;

      for (const response of responses) {
        if (response.type === 'event') {
          // Asynchronous unsolicited event frame: dispatch out-of-band without completing in-flight command
          this.handleIncomingEvent(response.event);
        } else {
          // Command response: resolve current in-flight command
          this.handleCommandResponse(response);
        }
      }
    } catch (err: unknown) {
      // Framing or length violation: terminate connection fail-closed
      this.handleSocketError(err instanceof Error ? err : new Error(String(err)));
      this.disconnect();
    }
  }

  private handleCommandResponse(response: RustIpcResponse): void {
    if (!this.inFlight) {
      if (response.type === 'error') {
        const err = new Error(response.message);
        this.emit('subscription_error', err);
        const isSequenceGap =
          response.message.includes('Sequence gap') ||
          response.message.includes('SequenceGapRecoverable') ||
          response.message.includes('Failed to replay events');
        if (!isSequenceGap || this.listenerCount('subscription_error') === 0) {
          if (this.listenerCount('error') > 0) {
            this.emit('error', err);
          }
        }
      }
      return;
    }

    const current = this.inFlight;
    this.inFlight = null;
    clearTimeout(current.timeoutTimer);

    if (response.type === 'error') {
      current.reject(new Error(response.message));
    } else {
      current.resolve(response);
    }

    this.drainQueue();
  }

  private handleIncomingEvent(event: RustIpcEvent): void {
    if (typeof event.sequence === 'number' && event.sequence > this.lastReceivedSequence) {
      this.lastReceivedSequence = event.sequence;
    }
    this.emit('event', event);
  }

  private handleSocketError(err: Error): void {
    this.emit('error', err);
    this.failAllPending(err);
    this.setState('error');
  }

  private handleSocketClose(): void {
    if (this.state !== 'disconnected') {
      this.failAllPending(new Error('IPC socket closed unexpectedly.'));
      this.setState('disconnected');
      this.emit('close');
    }
  }

  private failAllPending(error: Error): void {
    if (this.inFlight) {
      clearTimeout(this.inFlight.timeoutTimer);
      this.inFlight.reject(error);
      this.inFlight = null;
    }

    while (this.requestQueue.length > 0) {
      const item = this.requestQueue.shift();
      if (item) {
        clearTimeout(item.timeoutTimer);
        item.reject(error);
      }
    }
  }

  private setState(newState: TransportState): void {
    if (this.state !== newState) {
      this.state = newState;
      this.emit('stateChanged', newState);
    }
  }
}
