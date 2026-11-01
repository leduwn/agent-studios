/**
 * Agent Studios — Artifact & Diff Protocol Wire Fixture Tests
 * Milestone M15 UI-03: Authoritative Rust IPC Golden Fixture Compatibility
 */

import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { RustIpcCodec } from '../../bridge/ipc/RustIpcCodec';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';

describe('Artifact & Diff Rust IPC Wire Protocol Tests', () => {
  const fixturesDir = fs.existsSync(path.join(__dirname, 'fixtures', 'golden_wire'))
    ? path.join(__dirname, 'fixtures', 'golden_wire')
    : path.join(__dirname, '..', '..', '..', 'src', 'test', 'unit', 'fixtures', 'golden_wire');

  it('1. encodes and decodes get_task_diff_request golden fixture', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'get_task_diff_request.json'), 'utf8');
    const expectedCmd = JSON.parse(raw) as RustIpcCommand;
    assert.strictEqual(expectedCmd.type, 'get_task_diff');

    const encoded = RustIpcCodec.encodeCommand(expectedCmd);
    assert.ok(encoded.length > 4, 'Encoded frame must include 4-byte header');

    const lengthPrefix = encoded.readUInt32BE(0);
    assert.strictEqual(lengthPrefix, encoded.length - 4);

    const payload = JSON.parse(encoded.subarray(4).toString('utf8'));
    assert.deepStrictEqual(payload, expectedCmd);
  });

  it('2. encodes and decodes get_artifact_request golden fixture', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'get_artifact_request.json'), 'utf8');
    const expectedCmd = JSON.parse(raw) as RustIpcCommand;
    assert.strictEqual(expectedCmd.type, 'get_artifact');

    const encoded = RustIpcCodec.encodeCommand(expectedCmd);
    const lengthPrefix = encoded.readUInt32BE(0);
    assert.strictEqual(lengthPrefix, encoded.length - 4);

    const payload = JSON.parse(encoded.subarray(4).toString('utf8'));
    assert.deepStrictEqual(payload, expectedCmd);
  });

  it('3. encodes and decodes list_task_artifacts_request golden fixture', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'list_task_artifacts_request.json'), 'utf8');
    const expectedCmd = JSON.parse(raw) as RustIpcCommand;
    assert.strictEqual(expectedCmd.type, 'list_task_artifacts');

    const encoded = RustIpcCodec.encodeCommand(expectedCmd);
    const lengthPrefix = encoded.readUInt32BE(0);
    assert.strictEqual(lengthPrefix, encoded.length - 4);

    const payload = JSON.parse(encoded.subarray(4).toString('utf8'));
    assert.deepStrictEqual(payload, expectedCmd);
  });

  it('4. decodes task_diff_response golden fixture into typed model', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'task_diff_response.json'), 'utf8');
    const payloadBuffer = Buffer.from(raw, 'utf8');
    const frame = Buffer.allocUnsafe(4 + payloadBuffer.length);
    frame.writeUInt32BE(payloadBuffer.length, 0);
    payloadBuffer.copy(frame, 4);

    const { responses, remaining } = RustIpcCodec.decodeResponses(frame);
    assert.strictEqual(remaining.length, 0);
    assert.strictEqual(responses.length, 1);

    const resp = responses[0] as RustIpcResponse;
    assert.strictEqual(resp.type, 'task_diff');
    if (resp.type === 'task_diff') {
      assert.strictEqual(resp.diff.studio_id, 'c0a80101-0000-0000-0000-000000000001');
      assert.strictEqual(resp.diff.task_id, 'c0a80101-0000-0000-0000-000000000003');
      assert.strictEqual(resp.diff.encoding, 'base64');
      assert.strictEqual(resp.diff.has_more, false);
      assert.ok(resp.diff.patch_base64, 'Must have patch_base64');
      assert.ok(resp.diff.patch_sha256, 'Must have patch_sha256');
      assert.strictEqual(resp.diff.summary.files_changed, 1);
      assert.deepStrictEqual(resp.diff.summary.added_files, ['file.txt']);
    }
  });

  it('5. decodes artifact_response golden fixture into typed model', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'artifact_response.json'), 'utf8');
    const payloadBuffer = Buffer.from(raw, 'utf8');
    const frame = Buffer.allocUnsafe(4 + payloadBuffer.length);
    frame.writeUInt32BE(payloadBuffer.length, 0);
    payloadBuffer.copy(frame, 4);

    const { responses, remaining } = RustIpcCodec.decodeResponses(frame);
    assert.strictEqual(remaining.length, 0);
    assert.strictEqual(responses.length, 1);

    const resp = responses[0] as RustIpcResponse;
    assert.strictEqual(resp.type, 'artifact');
    if (resp.type === 'artifact') {
      assert.ok(resp.artifact, 'ArtifactRecord must exist');
      assert.strictEqual(resp.artifact?.id, 'c0a80101-0000-0000-0000-000000000002');
      assert.strictEqual(resp.artifact?.kind, 'patch');
      assert.strictEqual(resp.artifact?.logical_name, 'task-patch.diff');
      assert.strictEqual(resp.encoding, 'base64');
      assert.ok(resp.content_base64);
      assert.strictEqual(resp.has_more, false);
      assert.ok(resp.sha256);
    }
  });

  it('6. decodes task_artifacts_response golden fixture into typed model', () => {
    const raw = fs.readFileSync(path.join(fixturesDir, 'task_artifacts_response.json'), 'utf8');
    const payloadBuffer = Buffer.from(raw, 'utf8');
    const frame = Buffer.allocUnsafe(4 + payloadBuffer.length);
    frame.writeUInt32BE(payloadBuffer.length, 0);
    payloadBuffer.copy(frame, 4);

    const { responses, remaining } = RustIpcCodec.decodeResponses(frame);
    assert.strictEqual(remaining.length, 0);
    assert.strictEqual(responses.length, 1);

    const resp = responses[0] as RustIpcResponse;
    assert.strictEqual(resp.type, 'task_artifacts');
    if (resp.type === 'task_artifacts') {
      assert.strictEqual(resp.artifacts.length, 1);
      assert.strictEqual(resp.artifacts[0].id, 'c0a80101-0000-0000-0000-000000000002');
      assert.strictEqual(resp.artifacts[0].kind, 'patch');
      assert.strictEqual(resp.artifacts[0].logical_name, 'task-patch.diff');
    }
  });
});
