/**
 * Agent Studios — IPC Bridge Contract & Framing
 * Milestone M14-UI-02: Extension Hardening & IPC-03 Integration Preparation
 *
 * Implements frame encoding, decoding, length-prefix framing, and RPC correlation
 * for the Agent Studios native IPC protocol.
 */

import {
  IPC_PROTOCOL_VERSION,
  IpcEventFrame,
  IpcMethod,
  IpcRequestFrame,
  IpcResponseFrame,
} from './IpcProtocolTypes';

export class IpcFrameCodec {
  /**
   * Encodes a request frame into a 4-byte length-prefixed Buffer.
   */
  public static encodeRequest<T>(
    id: string,
    method: IpcMethod,
    params: T
  ): Buffer {
    const frame: IpcRequestFrame<T> = {
      version: IPC_PROTOCOL_VERSION,
      id,
      method,
      params,
      timestamp: Date.now(),
    };

    const json = JSON.stringify(frame);
    const jsonBytes = Buffer.from(json, 'utf8');
    const header = Buffer.alloc(4);
    header.writeUInt32BE(jsonBytes.length, 0);

    return Buffer.concat([header, jsonBytes]);
  }

  /**
   * Decodes a stream buffer into frames.
   * Returns parsed frames and remaining unparsed buffer.
   */
  public static decodeFrames(
    buffer: Buffer
  ): {
    frames: Array<IpcResponseFrame | IpcEventFrame>;
    remaining: Buffer;
  } {
    const frames: Array<IpcResponseFrame | IpcEventFrame> = [];
    let offset = 0;

    while (offset + 4 <= buffer.length) {
      const length = buffer.readUInt32BE(offset);

      // Guard against corrupted or oversized frames (16MB max)
      if (length > 16 * 1024 * 1024) {
        throw new Error(`Frame length ${length} exceeds maximum allowed size (16MB).`);
      }

      if (offset + 4 + length > buffer.length) {
        // Frame not fully received yet
        break;
      }

      const jsonStr = buffer.toString('utf8', offset + 4, offset + 4 + length);
      try {
        const parsed = JSON.parse(jsonStr) as IpcResponseFrame | IpcEventFrame;
        frames.push(parsed);
      } catch (err) {
        throw new Error(`Failed to parse IPC JSON frame: ${err}`);
      }

      offset += 4 + length;
    }

    return {
      frames,
      remaining: buffer.subarray(offset),
    };
  }
}

export interface IpcClientOptions {
  socketPath: string;
  timeoutMs?: number;
  maxRetries?: number;
}

export class IpcCorrelationManager {
  private _nextId = 1;
  private readonly _pending = new Map<
    string,
    {
      resolve: (res: unknown) => void;
      reject: (err: Error) => void;
      timeout: NodeJS.Timeout;
    }
  >();

  public createRequest<T>(
    method: IpcMethod,
    params: T,
    timeoutMs: number = 10000
  ): { id: string; buffer: Buffer; promise: Promise<unknown> } {
    const id = `req-${Date.now()}-${this._nextId++}`;
    const buffer = IpcFrameCodec.encodeRequest(id, method, params);

    const promise = new Promise<unknown>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this._pending.delete(id);
        reject(new Error(`IPC request '${method}' (id: ${id}) timed out after ${timeoutMs}ms.`));
      }, timeoutMs);

      this._pending.set(id, { resolve, reject, timeout });
    });

    return { id, buffer, promise };
  }

  public handleResponse(response: IpcResponseFrame): boolean {
    const pending = this._pending.get(response.id);
    if (!pending) {
      return false;
    }

    this._pending.delete(response.id);
    clearTimeout(pending.timeout);

    if (response.error) {
      const err = new Error(response.error.message || `IPC error code ${response.error.code}`);
      (err as { code?: number }).code = response.error.code;
      pending.reject(err);
    } else {
      pending.resolve(response.result);
    }

    return true;
  }

  public failAll(reason: Error): void {
    for (const [id, pending] of this._pending.entries()) {
      clearTimeout(pending.timeout);
      pending.reject(reason);
      this._pending.delete(id);
    }
  }

  public get pendingCount(): number {
    return this._pending.size;
  }
}
