/**
 * Agent Studios — IPC Bridge Contract & Framing
 * Milestone M14-UI-02: Extension Hardening & IPC-03 Integration Preparation
 *
 * Implements frame encoding, decoding, length-prefix framing, and RPC correlation
 * for the Agent Studios native IPC protocol. Aligned with authoritative Rust daemon
 * limits (DEFAULT_MAX_FRAME_SIZE = 4 MiB, Anti-DoS bounded accumulation).
 */

import {
  IPC_PROTOCOL_VERSION,
  IpcEventFrame,
  IpcMethod,
  IpcRequestFrame,
  IpcResponseFrame,
} from './IpcProtocolTypes';

/** Authoritative 4 MiB maximum frame size matching Rust ipc-protocol & ipc-transport */
export const DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024; // 4,194,304 bytes

/** Length of the 4-byte big-endian unsigned integer prefix */
export const FRAME_HEADER_SIZE = 4;

/** Maximum allowable accumulated stream buffer before eviction to prevent memory exhaustion DoS */
export const MAX_ACCUMULATED_BUFFER_SIZE = DEFAULT_MAX_FRAME_SIZE + FRAME_HEADER_SIZE;

export class IpcFrameCodec {
  /**
   * Encodes a request frame into a 4-byte length-prefixed Buffer.
   * Bounds outbound payload size BEFORE allocating wire buffers to prevent heap spikes.
   */
  public static encodeRequest<T>(
    id: string,
    method: IpcMethod,
    params: T,
    maxSize = DEFAULT_MAX_FRAME_SIZE
  ): Buffer {
    const frame: IpcRequestFrame<T> = {
      version: IPC_PROTOCOL_VERSION,
      id,
      method,
      params,
      timestamp: Date.now(),
    };

    const json = JSON.stringify(frame);
    const payloadLength = Buffer.byteLength(json, 'utf8');

    // Bound outbound frame size before allocating wire buffer
    if (payloadLength > maxSize) {
      throw new RangeError(
        `Outbound frame size ${payloadLength} bytes exceeds maximum allowed size (${maxSize} bytes / 4 MiB).`
      );
    }

    // Single target buffer allocation — zero intermediate copies
    const wireBuffer = Buffer.allocUnsafe(FRAME_HEADER_SIZE + payloadLength);
    wireBuffer.writeUInt32BE(payloadLength, 0);
    wireBuffer.write(json, FRAME_HEADER_SIZE, payloadLength, 'utf8');

    return wireBuffer;
  }

  /**
   * Decodes a stream buffer into frames.
   * Returns parsed frames and remaining unparsed buffer.
   * Enforces strict frame size limit (4 MiB) and anti-DoS buffer accumulation bounds.
   */
  public static decodeFrames(
    buffer: Buffer,
    maxSize = DEFAULT_MAX_FRAME_SIZE
  ): {
    frames: Array<IpcResponseFrame | IpcEventFrame>;
    remaining: Buffer;
  } {
    const frames: Array<IpcResponseFrame | IpcEventFrame> = [];
    let offset = 0;

    // Guard against unbounded accumulation buffer in stream receiver
    if (buffer.length > MAX_ACCUMULATED_BUFFER_SIZE) {
      // If the buffer exceeds maximum accumulation and no full frame can be parsed, reject immediately
      if (buffer.length < FRAME_HEADER_SIZE) {
        throw new RangeError(
          `Accumulated stream buffer (${buffer.length} bytes) exceeds maximum allowed buffer size (${MAX_ACCUMULATED_BUFFER_SIZE} bytes).`
        );
      }
      const leadingFrameLen = buffer.readUInt32BE(0);
      if (leadingFrameLen > maxSize) {
        throw new RangeError(
          `Frame length prefix ${leadingFrameLen} exceeds maximum allowed size (${maxSize} bytes / 4 MiB).`
        );
      }
    }

    while (offset + FRAME_HEADER_SIZE <= buffer.length) {
      const length = buffer.readUInt32BE(offset);

      // Guard against corrupted or oversized frames (4 MiB max) before slicing or parsing
      if (length > maxSize) {
        throw new RangeError(
          `Frame length prefix ${length} exceeds maximum allowed size (${maxSize} bytes / 4 MiB).`
        );
      }

      if (offset + FRAME_HEADER_SIZE + length > buffer.length) {
        // Frame not fully received yet
        break;
      }

      const jsonStr = buffer.toString('utf8', offset + FRAME_HEADER_SIZE, offset + FRAME_HEADER_SIZE + length);
      try {
        const parsed = JSON.parse(jsonStr) as IpcResponseFrame | IpcEventFrame;
        frames.push(parsed);
      } catch (err) {
        throw new Error(`Failed to parse IPC JSON frame: ${err}`);
      }

      offset += FRAME_HEADER_SIZE + length;
    }

    const remaining = buffer.subarray(offset);

    // If remaining buffer without progress exceeds maximum accumulation threshold, reject
    if (remaining.length > MAX_ACCUMULATED_BUFFER_SIZE) {
      throw new RangeError(
        `Accumulated unparsed stream buffer (${remaining.length} bytes) exceeds maximum allowed size (${MAX_ACCUMULATED_BUFFER_SIZE} bytes).`
      );
    }

    return {
      frames,
      remaining,
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
