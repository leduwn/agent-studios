/**
 * Agent Studios — Rust IPC Wire Frame Codec (Approach A)
 * Milestone M14-UI-02.1: Protocol Correctness & Wire Compatibility
 *
 * Implements binary wire framing and serialization matching the authoritative Rust daemon
 * (agent-studios-rs / crates ipc-protocol & ipc-transport).
 *
 * Framing details:
 * - 4-byte big-endian length prefix
 * - DEFAULT_MAX_FRAME_SIZE = 4 MiB (4,194,304 bytes)
 * - MAX_ACCUMULATED_BUFFER_SIZE = 4 MiB + 4 bytes (Anti-DoS bounded stream accumulation)
 * - Outbound frames bounded before buffer allocation
 */

import { RustIpcCommand, RustIpcResponse } from './RustIpcProtocolTypes';

export const RUST_DEFAULT_MAX_FRAME_SIZE = 4 * 1024 * 1024; // 4 MiB
export const RUST_FRAME_HEADER_SIZE = 4;
export const RUST_MAX_ACCUMULATED_BUFFER_SIZE = RUST_DEFAULT_MAX_FRAME_SIZE + RUST_FRAME_HEADER_SIZE;

export class RustIpcCodec {
  /**
   * Encodes a RustIpcCommand into a 4-byte BE length-prefixed wire Buffer.
   * Validates payload size before allocating memory.
   */
  public static encodeCommand(
    command: RustIpcCommand,
    maxSize: number = RUST_DEFAULT_MAX_FRAME_SIZE
  ): Buffer {
    const json = JSON.stringify(command);
    const payloadLength = Buffer.byteLength(json, 'utf8');

    if (payloadLength > maxSize) {
      throw new RangeError(
        `Outbound command size ${payloadLength} bytes exceeds maximum frame size (${maxSize} bytes / 4 MiB).`
      );
    }

    const wireBuffer = Buffer.allocUnsafe(RUST_FRAME_HEADER_SIZE + payloadLength);
    wireBuffer.writeUInt32BE(payloadLength, 0);
    wireBuffer.write(json, RUST_FRAME_HEADER_SIZE, payloadLength, 'utf8');

    return wireBuffer;
  }

  /**
   * Decodes a stream buffer into RustIpcResponse objects.
   * Rejects oversized length prefixes and bounds accumulated partial data.
   */
  public static decodeResponses(
    buffer: Buffer,
    maxSize: number = RUST_DEFAULT_MAX_FRAME_SIZE
  ): {
    responses: RustIpcResponse[];
    remaining: Buffer;
  } {
    const responses: RustIpcResponse[] = [];
    let offset = 0;

    // Guard against memory exhaustion via unbounded partial buffer accumulation
    if (buffer.length > RUST_MAX_ACCUMULATED_BUFFER_SIZE) {
      if (buffer.length < RUST_FRAME_HEADER_SIZE) {
        throw new RangeError(
          `Accumulated stream buffer (${buffer.length} bytes) exceeds maximum allowed buffer size (${RUST_MAX_ACCUMULATED_BUFFER_SIZE} bytes).`
        );
      }
      const leadingLen = buffer.readUInt32BE(0);
      if (leadingLen > maxSize) {
        throw new RangeError(
          `Frame length prefix ${leadingLen} exceeds maximum allowed size (${maxSize} bytes / 4 MiB).`
        );
      }
    }

    while (offset + RUST_FRAME_HEADER_SIZE <= buffer.length) {
      const length = buffer.readUInt32BE(offset);

      if (length > maxSize) {
        throw new RangeError(
          `Frame length prefix ${length} exceeds maximum allowed size (${maxSize} bytes / 4 MiB).`
        );
      }

      if (offset + RUST_FRAME_HEADER_SIZE + length > buffer.length) {
        // Frame truncated / incomplete
        break;
      }

      const jsonStr = buffer.toString('utf8', offset + RUST_FRAME_HEADER_SIZE, offset + RUST_FRAME_HEADER_SIZE + length);
      try {
        const parsed = JSON.parse(jsonStr) as RustIpcResponse;
        responses.push(parsed);
      } catch (err) {
        throw new Error(`Failed to parse Rust IPC JSON response: ${err}`);
      }

      offset += RUST_FRAME_HEADER_SIZE + length;
    }

    const remaining = buffer.subarray(offset);

    if (remaining.length > RUST_MAX_ACCUMULATED_BUFFER_SIZE) {
      throw new RangeError(
        `Accumulated unparsed stream buffer (${remaining.length} bytes) exceeds maximum allowed size (${RUST_MAX_ACCUMULATED_BUFFER_SIZE} bytes).`
      );
    }

    return {
      responses,
      remaining,
    };
  }
}
