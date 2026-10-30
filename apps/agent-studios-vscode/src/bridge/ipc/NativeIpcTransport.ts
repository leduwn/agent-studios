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
}

interface QueuedCommand {
  command: RustIpcCommand;
  resolve: (response: RustIpcResponse) => void;
  reject: (error: Error) => void;
  timeoutTimer: NodeJS.Timeout;
}

export class NativeIpcTransport extends EventEmitter {
  private socket: net.Socket | null = null;
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
   */
  public async connect(): Promise<void> {
    if (this.state === 'ready' || this.state === 'connecting' || this.state === 'authenticating') {
      return;
    }

    this.setState('connecting');

    await new Promise<void>((resolve, reject) => {
      let isConnected = false;
      const socket = net.createConnection(this.options.endpoint, () => {
        isConnected = true;
        this.socket = socket;
        resolve();
      });

      socket.on('data', (chunk: Buffer) => {
        this.handleData(chunk);
      });

      socket.on('error', (err: Error) => {
        if (!isConnected) {
          this.setState('error');
          reject(err);
        } else {
          this.handleSocketError(err);
        }
      });

      socket.on('close', () => {
        this.handleSocketClose();
      });
    });

    try {
      await this.performHandshake();
      this.setState('ready');
    } catch (err: unknown) {
      this.disconnect();
      throw err;
    }
  }

  /**
   * Disconnects the socket cleanly and clears pending command queue.
   */
  public disconnect(): void {
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
