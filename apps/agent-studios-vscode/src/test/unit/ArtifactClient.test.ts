/**
 * Agent Studios — Lossless Artifact & Diff Client Unit Tests
 * Milestone M15 UI-03: Lossless Pagination, Unicode Boundary Safety & Cryptographic Integrity
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as crypto from 'crypto';
import {
  ArtifactClient,
  ArtifactRetrievalError,
  IpcTransportSender,
} from '../../bridge/ipc/ArtifactClient';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';

describe('ArtifactClient Lossless Streaming & Cryptographic Integrity Tests', () => {
  const studioId = 'studio-alpha-001';
  const taskId = 'task-beta-002';

  it('1. reassembles single-chunk diff with matching SHA-256 (VERIFIED)', async () => {
    const rawPatch = 'diff --git a/hello.txt b/hello.txt\n+hello world\n';
    const rawBuffer = Buffer.from(rawPatch, 'utf8');
    const expectedSha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex');

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
        assert.strictEqual(cmd.type, 'get_task_diff');
        return {
          type: 'task_diff',
          diff: {
            studio_id: studioId,
            task_id: taskId,
            artifact_id: 'art-01',
            base_commit: 'abc1234',
            head_commit: 'def5678',
            summary: {
              files_changed: 1,
              added_files: ['hello.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: rawBuffer.length,
              base_commit: 'abc1234',
            },
            patch: rawPatch,
            offset: 0,
            limit: 1048576,
            total_bytes: rawBuffer.length,
            has_more: false,
            patch_base64: rawBuffer.toString('base64'),
            encoding: 'base64',
            patch_sha256: `sha256:${expectedSha256}`,
            chunk_bytes: rawBuffer.length,
          },
        };
      },
    };

    const result = await ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId);

    assert.strictEqual(result.integrity, 'VERIFIED');
    assert.strictEqual(result.sha256, expectedSha256);
    assert.strictEqual(result.isBinary, false);
    assert.strictEqual(result.text, rawPatch);
    assert.strictEqual(result.totalBytes, rawBuffer.length);
    assert.strictEqual(result.chunkCount, 1);
  });

  it('2. preserves multi-byte Vietnamese Unicode split across chunk boundaries', async () => {
    // String containing Vietnamese multi-byte characters
    const vietnameseText = 'diff --git a/vi.txt b/vi.txt\n+Tiếng Việt có dấu: ế, ệ, ẩ, ờ, đ.\n';
    const rawBuffer = Buffer.from(vietnameseText, 'utf8');
    const expectedSha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex');

    // Split arbitrarily in middle of multi-byte sequence (e.g. byte 35)
    const splitIndex = 35;
    const chunk1 = rawBuffer.subarray(0, splitIndex);
    const chunk2 = rawBuffer.subarray(splitIndex);

    let callCount = 0;
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
        assert.strictEqual(cmd.type, 'get_task_diff');
        if (cmd.type === 'get_task_diff') {
          callCount++;
          if (cmd.offset === 0) {
            return {
              type: 'task_diff',
              diff: {
                studio_id: studioId,
                task_id: taskId,
                summary: {
                  files_changed: 1,
                  added_files: ['vi.txt'],
                  modified_files: [],
                  deleted_files: [],
                  renamed_files: [],
                  binary_files: [],
                  patch_size_bytes: rawBuffer.length,
                  base_commit: '0000000',
                },
                patch: '', // String suppressed
                offset: 0,
                limit: chunk1.length,
                total_bytes: rawBuffer.length,
                has_more: true,
                patch_base64: chunk1.toString('base64'),
                encoding: 'base64',
                patch_sha256: expectedSha256,
                chunk_bytes: chunk1.length,
              },
            };
          } else if (cmd.offset === chunk1.length) {
            return {
              type: 'task_diff',
              diff: {
                studio_id: studioId,
                task_id: taskId,
                summary: {
                  files_changed: 1,
                  added_files: ['vi.txt'],
                  modified_files: [],
                  deleted_files: [],
                  renamed_files: [],
                  binary_files: [],
                  patch_size_bytes: rawBuffer.length,
                  base_commit: '0000000',
                },
                patch: '',
                offset: chunk1.length,
                limit: chunk2.length,
                total_bytes: rawBuffer.length,
                has_more: false,
                patch_base64: chunk2.toString('base64'),
                encoding: 'base64',
                patch_sha256: expectedSha256,
                chunk_bytes: chunk2.length,
              },
            };
          }
        }
        throw new Error(`Unexpected command: ${JSON.stringify(cmd)}`);
      },
    };

    const result = await ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId);

    assert.strictEqual(callCount, 2);
    assert.strictEqual(result.integrity, 'VERIFIED');
    assert.strictEqual(result.totalBytes, rawBuffer.length);
    assert.strictEqual(result.text, vietnameseText);
    assert.strictEqual(result.isBinary, false);
    assert.deepStrictEqual(result.rawBuffer, rawBuffer);
  });

  it('3. reassembles binary Git patch faithfully (isBinary: true)', async () => {
    const binaryHeader = 'diff --git a/image.png b/image.png\nGIT binary patch\nliteral 16\nzc$@)Z0000100001\n\0\0\0\0';
    const rawBuffer = Buffer.from(binaryHeader, 'utf8');
    const expectedSha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex');

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: ['image.png'],
            patch_size_bytes: rawBuffer.length,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: rawBuffer.length,
          has_more: false,
          patch_base64: rawBuffer.toString('base64'),
          encoding: 'base64',
          patch_sha256: expectedSha256,
          chunk_bytes: rawBuffer.length,
        },
      }),
    };

    const result = await ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId);

    assert.strictEqual(result.isBinary, true);
    assert.strictEqual(result.text, '');
    assert.strictEqual(result.integrity, 'VERIFIED');
    assert.deepStrictEqual(result.rawBuffer, rawBuffer);
  });

  it('4. marks INTEGRITY_FAILED when tampered bytes do not match expected sha256', async () => {
    const originalBuffer = Buffer.from('original content', 'utf8');
    const expectedSha256 = crypto.createHash('sha256').update(originalBuffer).digest('hex');

    const tamperedBuffer = Buffer.from('tampered content', 'utf8');

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: ['test.txt'],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: tamperedBuffer.length,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: tamperedBuffer.length,
          has_more: false,
          patch_base64: tamperedBuffer.toString('base64'),
          encoding: 'base64',
          patch_sha256: expectedSha256,
          chunk_bytes: tamperedBuffer.length,
        },
      }),
    };

    const result = await ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId);

    assert.strictEqual(result.integrity, 'INTEGRITY_FAILED');
    assert.notStrictEqual(result.sha256, expectedSha256);
  });

  it('5. marks UNVERIFIED when patch_sha256 is missing', async () => {
    const rawBuffer = Buffer.from('some diff', 'utf8');

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: ['test.txt'],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: rawBuffer.length,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: rawBuffer.length,
          has_more: false,
          patch_base64: rawBuffer.toString('base64'),
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: rawBuffer.length,
        },
      }),
    };

    const result = await ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId);

    assert.strictEqual(result.integrity, 'UNVERIFIED');
  });

  it('6. rejects chunk offset mismatch with OFFSET_MISMATCH', async () => {
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: 100,
            base_commit: 'abc',
          },
          patch: '',
          offset: 42, // Expected 0
          limit: 1048576,
          total_bytes: 100,
          has_more: false,
          patch_base64: Buffer.from('hello').toString('base64'),
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: 5,
        },
      }),
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'OFFSET_MISMATCH');
        return true;
      }
    );
  });

  it('7. rejects chunk_bytes metadata mismatch with CHUNK_BYTES_MISMATCH', async () => {
    const chunk = Buffer.from('hello', 'utf8');
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: 5,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: 5,
          has_more: false,
          patch_base64: chunk.toString('base64'),
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: 999, // Mismatched
        },
      }),
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'CHUNK_BYTES_MISMATCH');
        return true;
      }
    );
  });

  it('8. detects zero-progress loop with ZERO_PROGRESS', async () => {
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: 100,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: 100,
          has_more: true,
          patch_base64: '', // Zero bytes
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: 0,
        },
      }),
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'ZERO_PROGRESS');
        return true;
      }
    );
  });

  it('9. rejects total_bytes change mid-stream with TOTAL_BYTES_CHANGED', async () => {
    let call = 0;
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => {
        call++;
        return {
          type: 'task_diff',
          diff: {
            studio_id: studioId,
            task_id: taskId,
            summary: {
              files_changed: 1,
              added_files: [],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 100,
              base_commit: 'abc',
            },
            patch: '',
            offset: (call - 1) * 5,
            limit: 5,
            total_bytes: call === 1 ? 100 : 200, // Inconsistent
            has_more: true,
            patch_base64: Buffer.from('12345').toString('base64'),
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 5,
          },
        };
      },
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'TOTAL_BYTES_CHANGED');
        return true;
      }
    );
  });

  it('10. rejects truncated download with TRUNCATED_DOWNLOAD', async () => {
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: 100,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 1048576,
          total_bytes: 100,
          has_more: false, // Premature end
          patch_base64: Buffer.from('only 10 bytes').toString('base64'),
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: 13,
        },
      }),
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'TRUNCATED_DOWNLOAD');
        return true;
      }
    );
  });

  it('11. enforces max aggregate memory bounds with EXCEEDED_MAX_BYTES', async () => {
    const chunk = Buffer.from('1234567890', 'utf8'); // 10 bytes
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => ({
        type: 'task_diff',
        diff: {
          studio_id: studioId,
          task_id: taskId,
          summary: {
            files_changed: 1,
            added_files: [],
            modified_files: [],
            deleted_files: [],
            renamed_files: [],
            binary_files: [],
            patch_size_bytes: 100,
            base_commit: 'abc',
          },
          patch: '',
          offset: 0,
          limit: 10,
          total_bytes: 100,
          has_more: true,
          patch_base64: chunk.toString('base64'),
          encoding: 'base64',
          patch_sha256: null,
          chunk_bytes: 10,
        },
      }),
    };

    // Set maxBytes to 5 bytes, chunk is 10 bytes -> immediate fail
    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId, { maxBytes: 5 }),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'EXCEEDED_MAX_BYTES');
        return true;
      }
    );
  });

  it('12. aborts on AbortSignal with ABORTED', async () => {
    const controller = new AbortController();
    controller.abort();

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (): Promise<RustIpcResponse> => {
        throw new Error('Should not be called');
      },
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId, { signal: controller.signal }),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'ABORTED');
        return true;
      }
    );
  });

  it('13. detects session reconnection during download with SESSION_CHANGED', async () => {
    let session = 'session-1';
    const fakeTransport: IpcTransportSender = {
      getSessionId: () => session,
      sendCommand: async (): Promise<RustIpcResponse> => {
        session = 'session-2'; // Reconnect simulated
        return {
          type: 'task_diff',
          diff: {
            studio_id: studioId,
            task_id: taskId,
            summary: {
              files_changed: 1,
              added_files: [],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 100,
              base_commit: 'abc',
            },
            patch: '',
            offset: 0,
            limit: 50,
            total_bytes: 100,
            has_more: true,
            patch_base64: Buffer.from('part 1').toString('base64'),
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 6,
          },
        };
      },
    };

    await assert.rejects(
      async () => ArtifactClient.fetchTaskDiff(fakeTransport, studioId, taskId, { sessionId: 'session-1' }),
      (err: ArtifactRetrievalError) => {
        assert.strictEqual(err.code, 'SESSION_CHANGED');
        return true;
      }
    );
  });

  it('14. fetches artifact content and verifies sha256', async () => {
    const content = Buffer.from('Report content here', 'utf8');
    const sha256 = crypto.createHash('sha256').update(content).digest('hex');

    const fakeTransport: IpcTransportSender = {
      sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
        assert.strictEqual(cmd.type, 'get_artifact');
        return {
          type: 'artifact',
          artifact: {
            id: 'art-001',
            studio_id: studioId,
            task_id: taskId,
            producer_agent_id: 'agent-1',
            kind: 'report',
            logical_name: 'test-report.md',
            content_hash: `sha256:${sha256}`,
            location: 'blobs/sha256/...',
            created_at: new Date().toISOString(),
          },
          content_base64: content.toString('base64'),
          encoding: 'base64',
          offset: 0,
          limit: 1048576,
          total_bytes: content.length,
          has_more: false,
          sha256,
        };
      },
    };

    const res = await ArtifactClient.fetchArtifactContent(fakeTransport, studioId, 'art-001');

    assert.strictEqual(res.integrity, 'VERIFIED');
    assert.strictEqual(res.sha256, sha256);
    assert.strictEqual(res.text, 'Report content here');
    assert.strictEqual(res.isBinary, false);
  });

  it('15. lists task artifacts via list_task_artifacts', async () => {
    const fakeTransport: IpcTransportSender = {
      sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
        assert.strictEqual(cmd.type, 'list_task_artifacts');
        return {
          type: 'task_artifacts',
          artifacts: [
            {
              id: 'art-001',
              studio_id: studioId,
              task_id: taskId,
              producer_agent_id: 'agent-1',
              kind: 'patch',
              logical_name: 'task.diff',
              content_hash: 'sha256:abc',
              location: 'blobs/...',
              created_at: new Date().toISOString(),
            },
          ],
        };
      },
    };

    const artifacts = await ArtifactClient.listTaskArtifacts(fakeTransport, studioId, taskId);

    assert.strictEqual(artifacts.length, 1);
    assert.strictEqual(artifacts[0].id, 'art-001');
    assert.strictEqual(artifacts[0].kind, 'patch');
  });
});
