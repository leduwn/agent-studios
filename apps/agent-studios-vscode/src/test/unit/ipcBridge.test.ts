/**
 * Agent Studios — IPC Bridge Contract & Framing Unit Tests
 * Milestone M14-UI-02.1: Protocol Correctness & Wire Compatibility
 */

import * as assert from 'assert';
import {
  DEFAULT_MAX_FRAME_SIZE,
  FRAME_HEADER_SIZE,
  IPC_PROTOCOL_VERSION,
  IpcCorrelationManager,
  IpcErrorCode,
  IpcFrameCodec,
  IpcResponseFrame,
  IPC_03_READINESS_MATRIX,
  MAX_ACCUMULATED_BUFFER_SIZE,
  getReadinessSummary,
} from '../../bridge/ipc';

describe('IPC-03 Bridge Contract & Framing Tests', () => {
  describe('IpcFrameCodec Framing & 4 MiB Limits', () => {
    it('encodes a request frame with 4-byte big-endian length prefix', () => {
      const buffer = IpcFrameCodec.encodeRequest('req-001', 'daemon.handshake', {
        clientId: 'vscode-client-1',
        clientVersion: '0.1.0',
        protocolVersion: IPC_PROTOCOL_VERSION,
      });

      assert.ok(buffer.length > 4);
      const lengthPrefix = buffer.readUInt32BE(0);
      assert.strictEqual(buffer.length, 4 + lengthPrefix);

      const jsonStr = buffer.toString('utf8', 4);
      const parsed = JSON.parse(jsonStr);
      assert.strictEqual(parsed.id, 'req-001');
      assert.strictEqual(parsed.method, 'daemon.handshake');
      assert.strictEqual(parsed.version, IPC_PROTOCOL_VERSION);
      assert.strictEqual(parsed.params.clientId, 'vscode-client-1');
    });

    it('decodes complete length-prefixed frames and handles chunked buffers', () => {
      const frameData = {
        version: IPC_PROTOCOL_VERSION,
        id: 'req-001',
        result: { status: 'ok', serverVersion: '0.2.0' },
        timestamp: Date.now(),
      };
      const jsonBytes = Buffer.from(JSON.stringify(frameData), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(jsonBytes.length, 0);
      const wireBytes = Buffer.concat([header, jsonBytes]);

      // Decode full buffer
      const { frames, remaining } = IpcFrameCodec.decodeFrames(wireBytes);
      assert.strictEqual(frames.length, 1);
      const res = frames[0] as IpcResponseFrame;
      assert.strictEqual(res.id, 'req-001');
      assert.strictEqual(remaining.length, 0);
    });

    it('leaves incomplete frame bytes in remaining buffer', () => {
      const frameData = { version: IPC_PROTOCOL_VERSION, id: 'req-002', result: { active: true } };
      const jsonBytes = Buffer.from(JSON.stringify(frameData), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(jsonBytes.length, 0);
      const wireBytes = Buffer.concat([header, jsonBytes]);

      // Partial slice: header + 5 bytes of JSON
      const partial = wireBytes.subarray(0, 4 + 5);
      const { frames, remaining } = IpcFrameCodec.decodeFrames(partial);

      assert.strictEqual(frames.length, 0);
      assert.strictEqual(remaining.length, partial.length);
    });

    it('guards against oversized outbound payload before allocation', () => {
      // Small maxSize override for test speed
      const testLimit = 100;
      assert.throws(() => {
        IpcFrameCodec.encodeRequest('req-big', 'session.sendMessage', {
          text: 'X'.repeat(200),
        }, testLimit);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum allowed size');
      });
    });

    it('guards against oversized inbound length prefix (> 4 MiB)', () => {
      const oversizedHeader = Buffer.alloc(4);
      oversizedHeader.writeUInt32BE(DEFAULT_MAX_FRAME_SIZE + 1, 0); // 4 MiB + 1

      assert.throws(() => {
        IpcFrameCodec.decodeFrames(oversizedHeader);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum allowed size');
      });
    });

    it('guards against max u32 malformed length prefix', () => {
      const malformedHeader = Buffer.alloc(4);
      malformedHeader.writeUInt32BE(0xffffffff, 0);

      assert.throws(() => {
        IpcFrameCodec.decodeFrames(malformedHeader);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum allowed size');
      });
    });

    it('handles exactly maximum frame size (boundary condition)', () => {
      const sample = {
        id: 'req-exact-boundary',
        method: 'session.list' as const,
        params: { pad: 'X' },
      };
      // Determine exact serialized length with timestamp dynamically
      const probeBuffer = IpcFrameCodec.encodeRequest(sample.id, sample.method, sample.params);
      const exactPayloadLength = probeBuffer.readUInt32BE(0);

      // Encoding with exactly max_size === exactPayloadLength must succeed
      const wire = IpcFrameCodec.encodeRequest(sample.id, sample.method, sample.params, exactPayloadLength);
      assert.strictEqual(wire.length, FRAME_HEADER_SIZE + exactPayloadLength);

      // Decoding with exactly max_size === exactPayloadLength must succeed
      const { frames, remaining } = IpcFrameCodec.decodeFrames(wire, exactPayloadLength);
      assert.strictEqual(frames.length, 1);
      assert.strictEqual(remaining.length, 0);

      // Encoding with max_size === exactPayloadLength - 1 must fail fast
      assert.throws(() => {
        IpcFrameCodec.encodeRequest(sample.id, sample.method, sample.params, exactPayloadLength - 1);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum allowed size');
      });
    });

    it('handles truncated frame with partial header (< 4 bytes)', () => {
      for (let len = 1; len < 4; len++) {
        const partialHeader = Buffer.alloc(len);
        const { frames, remaining } = IpcFrameCodec.decodeFrames(partialHeader);
        assert.strictEqual(frames.length, 0);
        assert.strictEqual(remaining.length, len);
      }
    });

    it('handles multiple concatenated back-to-back frames', () => {
      const frame1 = { version: IPC_PROTOCOL_VERSION, id: 'req-1', result: { f: 1 }, timestamp: 100 };
      const frame2 = { version: IPC_PROTOCOL_VERSION, id: 'req-2', result: { f: 2 }, timestamp: 200 };

      const json1 = Buffer.from(JSON.stringify(frame1), 'utf8');
      const h1 = Buffer.alloc(4);
      h1.writeUInt32BE(json1.length, 0);

      const json2 = Buffer.from(JSON.stringify(frame2), 'utf8');
      const h2 = Buffer.alloc(4);
      h2.writeUInt32BE(json2.length, 0);

      const stream = Buffer.concat([h1, json1, h2, json2]);
      const { frames, remaining } = IpcFrameCodec.decodeFrames(stream);

      assert.strictEqual(frames.length, 2);
      assert.strictEqual((frames[0] as IpcResponseFrame).id, 'req-1');
      assert.strictEqual((frames[1] as IpcResponseFrame).id, 'req-2');
      assert.strictEqual(remaining.length, 0);
    });

    it('throws error when JSON payload is malformed', () => {
      const malformedJson = Buffer.from('{"broken_json: ', 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(malformedJson.length, 0);
      const stream = Buffer.concat([header, malformedJson]);

      assert.throws(() => {
        IpcFrameCodec.decodeFrames(stream);
      }, /Failed to parse IPC JSON frame/);
    });

    it('guards against unbounded partial-frame accumulation (anti-DoS)', () => {
      // Construct a stream that exceeds MAX_ACCUMULATED_BUFFER_SIZE
      const oversizedChunk = Buffer.alloc(MAX_ACCUMULATED_BUFFER_SIZE + 10);
      oversizedChunk.writeUInt32BE(DEFAULT_MAX_FRAME_SIZE + 100, 0);

      assert.throws(() => {
        IpcFrameCodec.decodeFrames(oversizedChunk);
      }, (err: Error) => {
        return err instanceof RangeError;
      });
    });
  });

  describe('IpcCorrelationManager Request & Response Tracking', () => {
    let correlationManager: IpcCorrelationManager;

    beforeEach(() => {
      correlationManager = new IpcCorrelationManager();
    });

    it('creates request with monotonic id and resolves when matching response arrives', async () => {
      const { id, promise } = correlationManager.createRequest('session.list', {});
      assert.ok(id.startsWith('req-'));
      assert.strictEqual(correlationManager.pendingCount, 1);

      // Simulate daemon response frame
      const handled = correlationManager.handleResponse({
        version: IPC_PROTOCOL_VERSION,
        id,
        result: [{ id: 'sess-1', title: 'Test Session' }],
        timestamp: Date.now(),
      });

      assert.strictEqual(handled, true);
      assert.strictEqual(correlationManager.pendingCount, 0);

      const result = await promise;
      assert.deepStrictEqual(result, [{ id: 'sess-1', title: 'Test Session' }]);
    });

    it('rejects request promise with typed error when response contains error frame', async () => {
      const { id, promise } = correlationManager.createRequest('approval.decide', { approvalId: 'app-99' });

      correlationManager.handleResponse({
        version: IPC_PROTOCOL_VERSION,
        id,
        error: {
          code: IpcErrorCode.Unauthorized,
          message: 'Origin not authorized to approve high-risk action',
        },
        timestamp: Date.now(),
      });

      await assert.rejects(
        promise,
        /Origin not authorized to approve high-risk action/
      );
    });

    it('times out and rejects when response is not received within timeoutMs', async () => {
      const { promise } = correlationManager.createRequest('session.create', {}, 50);

      await assert.rejects(
        promise,
        /timed out after 50ms/
      );
      assert.strictEqual(correlationManager.pendingCount, 0);
    });

    it('failAll rejects all pending requests on connection close', async () => {
      const req1 = correlationManager.createRequest('session.list', {});
      const req2 = correlationManager.createRequest('provider.list', {});
      assert.strictEqual(correlationManager.pendingCount, 2);

      correlationManager.failAll(new Error('Connection terminated by remote host'));
      assert.strictEqual(correlationManager.pendingCount, 0);

      await assert.rejects(req1.promise, /Connection terminated/);
      await assert.rejects(req2.promise, /Connection terminated/);
    });
  });

  describe('IPC-03 Machine-Readable Readiness Matrix', () => {
    it('contains all core control plane capabilities with truthful status classifications', () => {
      assert.strictEqual(IPC_03_READINESS_MATRIX.length, 12);

      const handshake = IPC_03_READINESS_MATRIX.find(c => c.id === 'daemon.handshake');
      assert.ok(handshake);
      assert.strictEqual(handshake.state, 'INCOMPATIBLE_WIRE_FORMAT');
      assert.strictEqual(handshake.securityVerdict, 'FAIL_CLOSED');

      const approval = IPC_03_READINESS_MATRIX.find(c => c.id === 'approval.decide');
      assert.ok(approval);
      assert.strictEqual(approval.state, 'INCOMPATIBLE_WIRE_FORMAT');
      assert.strictEqual(approval.securityVerdict, 'FAIL_CLOSED');

      const diff = IPC_03_READINESS_MATRIX.find(c => c.id === 'artifact.getDiff');
      assert.ok(diff);
      assert.strictEqual(diff.state, 'TYPESCRIPT_CONTRACT_ONLY');
      assert.strictEqual(diff.securityVerdict, 'VERIFIED');

      const providers = IPC_03_READINESS_MATRIX.find(c => c.id === 'provider.list');
      assert.ok(providers);
      assert.strictEqual(providers.state, 'INCOMPATIBLE_WIRE_FORMAT');
      assert.strictEqual(providers.securityVerdict, 'VERIFIED');
    });

    it('computes accurate readiness summary metrics', () => {
      const summary = getReadinessSummary();
      assert.strictEqual(summary.total, 12);
      assert.strictEqual(summary.verifiedRust, 0); // Honest: no direct wire RPC verified against daemon yet
      assert.strictEqual(summary.typescriptContractOnly, 1); // artifact.getDiff
      assert.strictEqual(summary.incompatibleWireFormat, 8); // JSON-RPC vs Rust Serde tagged
      assert.strictEqual(summary.unimplemented, 3); // session.switch, approval.list, artifact.list
    });
  });
});
