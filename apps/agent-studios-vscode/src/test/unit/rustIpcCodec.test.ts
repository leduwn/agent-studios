/**
 * Agent Studios — Rust IPC Wire Frame Codec & HMAC Authenticator Unit Tests
 * Milestone M14-UI-02.1: Protocol Correctness & Wire Compatibility
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import {
  RUST_DEFAULT_MAX_FRAME_SIZE,
  RUST_FRAME_HEADER_SIZE,
  RUST_MAX_ACCUMULATED_BUFFER_SIZE,
  RUST_PROTOCOL_VERSION_1_0_0,
  RustHmacSigner,
  RustIpcCodec,
  RustIpcCommand,
} from '../../bridge/ipc';

describe('Rust IPC Codec & HMAC Authentication Unit Tests (Approach A)', () => {
  describe('RustIpcCodec Wire Framing & Serialization', () => {
    it('encodes Rust Serde tagged commands with 4-byte big-endian length prefix', () => {
      const command: RustIpcCommand = {
        type: 'negotiate_capabilities',
        protocol_version: RUST_PROTOCOL_VERSION_1_0_0,
        capabilities: ['worktree_isolation', 'approval_routing'],
      };

      const buffer = RustIpcCodec.encodeCommand(command);
      assert.ok(buffer.length > RUST_FRAME_HEADER_SIZE);

      const lengthPrefix = buffer.readUInt32BE(0);
      assert.strictEqual(buffer.length, RUST_FRAME_HEADER_SIZE + lengthPrefix);

      const jsonStr = buffer.toString('utf8', RUST_FRAME_HEADER_SIZE);
      const parsed = JSON.parse(jsonStr);
      assert.strictEqual(parsed.type, 'negotiate_capabilities');
      assert.deepStrictEqual(parsed.protocol_version, { major: 1, minor: 0, patch: 0 });
      assert.deepStrictEqual(parsed.capabilities, ['worktree_isolation', 'approval_routing']);
    });

    it('decodes complete Rust Serde tagged responses from stream buffer', () => {
      const responsePayload = {
        type: 'capabilities_negotiated',
        selected_version: { major: 1, minor: 0, patch: 0 },
        agreed_capabilities: ['worktree_isolation'],
      };

      const jsonBytes = Buffer.from(JSON.stringify(responsePayload), 'utf8');
      const header = Buffer.alloc(4);
      header.writeUInt32BE(jsonBytes.length, 0);
      const wire = Buffer.concat([header, jsonBytes]);

      const { responses, remaining } = RustIpcCodec.decodeResponses(wire);
      assert.strictEqual(responses.length, 1);
      assert.strictEqual(responses[0].type, 'capabilities_negotiated');
      assert.strictEqual(remaining.length, 0);
    });

    it('rejects outbound commands exceeding maximum frame size before buffer allocation', () => {
      const smallLimit = 150;
      const oversizedCommand: RustIpcCommand = {
        type: 'submit_task',
        studio_id: 'studio-99',
        task_spec: { title: 'T'.repeat(200) },
      };

      assert.throws(() => {
        RustIpcCodec.encodeCommand(oversizedCommand, smallLimit);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum frame size');
      });
    });

    it('rejects inbound length prefixes exceeding 4 MiB limit', () => {
      const oversizedHeader = Buffer.alloc(4);
      oversizedHeader.writeUInt32BE(RUST_DEFAULT_MAX_FRAME_SIZE + 1, 0);

      assert.throws(() => {
        RustIpcCodec.decodeResponses(oversizedHeader);
      }, (err: Error) => {
        return err instanceof RangeError && err.message.includes('exceeds maximum allowed size');
      });
    });

    it('bounds accumulated stream buffers against memory exhaustion DoS', () => {
      const oversizedChunk = Buffer.alloc(RUST_MAX_ACCUMULATED_BUFFER_SIZE + 10);
      oversizedChunk.writeUInt32BE(RUST_DEFAULT_MAX_FRAME_SIZE + 50, 0);

      assert.throws(() => {
        RustIpcCodec.decodeResponses(oversizedChunk);
      }, (err: Error) => {
        return err instanceof RangeError;
      });
    });
  });

  describe('RustHmacSigner Canonical Digest & Verification', () => {
    const testSecret = 'super-secure-shared-key-1234567890';
    const keyId = 'vscode-agent-01';
    const sessionId = 'session-test-uuid-42';
    const nonce = 'nonce-random-12345';

    it('computes exact canonical digest with empty payload matching Rust test vector', () => {
      const timestamp = 1761820800000;
      const canonical = RustHmacSigner.computeCanonicalDigest(
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        timestamp,
        undefined,
        null
      );

      // Empty payload SHA-256 hash constant
      const emptyPayloadSha256 = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
      const expected = `1.0.0:${keyId}:${sessionId}:${nonce}:${timestamp}::${emptyPayloadSha256}`;
      assert.strictEqual(canonical, expected);
    });

    it('sorts capabilities lexicographically ascending in canonical digest', () => {
      const timestamp = 1761820800000;
      const unsortedCaps = ['write', 'admin', 'read'];
      const sortedCaps = ['admin', 'read', 'write'];

      const digest1 = RustHmacSigner.computeCanonicalDigest(
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        timestamp,
        unsortedCaps,
        'payload data'
      );

      const digest2 = RustHmacSigner.computeCanonicalDigest(
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        timestamp,
        sortedCaps,
        'payload data'
      );

      assert.strictEqual(digest1, digest2);
      assert.ok(digest1.includes(':admin,read,write:'));
    });

    it('generates valid HMAC-SHA256 signature and verifies successfully', () => {
      const now = Date.now();
      const payload = Buffer.from('{"command": "submit_goal"}', 'utf8');
      const caps = ['read', 'write'];

      const header = RustHmacSigner.sign(
        testSecret,
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        now,
        caps,
        payload
      );

      assert.strictEqual(header.key_id, keyId);
      assert.strictEqual(header.session_id, sessionId);
      assert.strictEqual(header.nonce, nonce);
      assert.strictEqual(header.timestamp_millis, now);
      assert.strictEqual(header.signature.length, 64);

      const verifyResult = RustHmacSigner.verify(testSecret, header, payload, caps);
      assert.strictEqual(verifyResult.valid, true);
    });

    it('rejects tampered payload with signature mismatch', () => {
      const now = Date.now();
      const payload = '{"task": "clean"}';
      const tamperedPayload = '{"task": "clean", "malicious": true}';

      const header = RustHmacSigner.sign(
        testSecret,
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        now,
        undefined,
        payload
      );

      const verifyResult = RustHmacSigner.verify(testSecret, header, tamperedPayload);
      assert.strictEqual(verifyResult.valid, false);
      assert.ok(verifyResult.error?.includes('HMAC signature mismatch'));
    });

    it('rejects verification with wrong secret key', () => {
      const now = Date.now();
      const payload = '{"status": "ok"}';

      const header = RustHmacSigner.sign(
        testSecret,
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        now,
        undefined,
        payload
      );

      const verifyResult = RustHmacSigner.verify('wrong-secret-key-9999', header, payload);
      assert.strictEqual(verifyResult.valid, false);
      assert.ok(verifyResult.error?.includes('HMAC signature mismatch'));
    });

    it('rejects stale timestamp exceeding maximum skew', () => {
      const staleTimestamp = Date.now() - 120000; // 2 minutes ago
      const payload = 'ping';

      const header = RustHmacSigner.sign(
        testSecret,
        RUST_PROTOCOL_VERSION_1_0_0,
        keyId,
        sessionId,
        nonce,
        staleTimestamp,
        undefined,
        payload
      );

      const verifyResult = RustHmacSigner.verify(testSecret, header, payload, undefined, 60000);
      assert.strictEqual(verifyResult.valid, false);
      assert.ok(verifyResult.error?.includes('Timestamp skew'));
    });
  });
});
