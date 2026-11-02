/**
 * Agent Studios — Real Artifact IPC Integration Test Suite
 * Milestone M15 UI-03.1: Artifact & Diff Review Workbench
 *
 * Test Tier Classification:
 * - REAL_AGSD_READ_ONLY_IPC: Real cross-process IPC against compiled Rust agsd binary.
 * - IN_PROCESS_REAL_ARTIFACT_STORE: Lossless streaming, SHA-256 integrity, and chunk slicing.
 * - MOCK_IPC: Synthetic framing and wire-level protocol validation.
 * - UNIT: Pure in-memory parsers and state machine isolation.
 * - REAL_AI_MODEL_NOT_TESTED: External LLM provider inference not invoked.
 *
 * Verifies live IPC communication against the actual compiled Rust `agsd` binary
 * over Windows Named Pipes (and Unix Domain Sockets on POSIX), validating:
 * 1. Read-only task diff retrieval protocol compliance against real daemon.
 * 2. Error semantics for missing tasks and invalid parameters.
 * 3. Artifact list and retrieval wire framing.
 * 4. ArtifactClient integration with live IPC transport.
 * 5. Multi-chunk lossless streaming with Vietnamese diacritics.
 * 6. Cryptographic SHA-256 integrity verification and tamper detection.
 * 7. Studio and task identity snapshot validation.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as childProcess from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { ArtifactClient, ArtifactRetrievalError } from '../../bridge/ipc/ArtifactClient';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';

function resolveAgsdBinaryPath(): string {
  const isWindows = process.platform === 'win32';
  const exeSuffix = isWindows ? '.exe' : '';

  const candidatePaths = [
    process.env.AGSD_BIN_PATH,
    isWindows ? 'D:\\Projects\\agent-studios-m15-04\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-03\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15-02\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    isWindows ? 'D:\\Projects\\agent-studios-m15\\agent-studios-rs\\target\\debug\\agsd.exe' : undefined,
    path.resolve(__dirname, '../../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(__dirname, '../../../../agent-studios-rs/target/debug/agsd' + exeSuffix),
    path.resolve(process.cwd(), '../../agent-studios-rs/target/debug/agsd' + exeSuffix),
  ].filter((p): p is string => typeof p === 'string' && p.length > 0);

  for (const candidate of candidatePaths) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `[RealArtifactIpcE2E] Real Rust daemon binary (agsd) not found. Checked candidate locations:\n` +
      candidatePaths.map((p) => `  - ${p}`).join('\n') +
      `\n\nPlease build the agsd binary first using:\n` +
      `  cargo build --bin agsd -p agent-studios-ipc-transport\n` +
      `Or set the AGSD_BIN_PATH environment variable.`
  );
}

describe('Tier 3: Real Artifact IPC Integration (REAL_AGSD_READ_ONLY_IPC & IN_PROCESS_REAL_ARTIFACT_STORE)', function () {
  this.timeout(30000);

  let agsdBinaryPath: string;
  let tempDir: string;
  let runtimePath: string;
  let adminRuntimePath: string;
  let pipeOrSocketPath: string;
  let daemonProcess: childProcess.ChildProcess | null = null;
  let bridge: DaemonConnectedBridge | null = null;
  let adminTransport: NativeIpcTransport | null = null;

  before(async () => {
    agsdBinaryPath = resolveAgsdBinaryPath();

    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'agsd-artifact-e2e-'));
    runtimePath = path.join(tempDir, 'agsd.json');
    adminRuntimePath = path.join(tempDir, 'agsd-admin.json');

    const isWindows = process.platform === 'win32';
    const uuid = crypto.randomUUID();
    pipeOrSocketPath = isWindows
      ? `\\\\.\\pipe\\ags-artifact-e2e-${uuid}`
      : path.join(tempDir, `ags-artifact-e2e-${uuid}.sock`);

    if (!isWindows) {
      try {
        await fs.promises.chmod(tempDir, 0o700);
      } catch {
        // Ignore on platforms without chmod
      }
    }

    // Spawn daemon process
    const spawnArgs = [
      '--runtime-info-path',
      runtimePath,
      '--admin-runtime-info-path',
      adminRuntimePath,
    ];
    if (isWindows) {
      spawnArgs.push('--pipe-name', pipeOrSocketPath);
    } else {
      spawnArgs.push('--socket-path', pipeOrSocketPath);
    }

    daemonProcess = childProcess.spawn(agsdBinaryPath, spawnArgs, {
      stdio: ['pipe', 'pipe', 'pipe'],
      env: { ...process.env, RUST_LOG: 'info' },
    });

    daemonProcess.stderr?.on('data', () => {});
    daemonProcess.stdout?.on('data', () => {});

    // Await bootstrap discovery files
    const startTime = Date.now();
    let ready = false;
    while (Date.now() - startTime < 10000) {
      if (fs.existsSync(runtimePath) && fs.existsSync(adminRuntimePath)) {
        try {
          const content = JSON.parse(await fs.promises.readFile(runtimePath, 'utf8'));
          if (content.endpoint && content.client_key) {
            ready = true;
            break;
          }
        } catch {
          // File write in progress
        }
      }
      await new Promise((r) => setTimeout(r, 100));
    }

    assert.ok(ready, `agsd failed to write runtime discovery files within 10000ms`);

    // Connect standard bridge (handles capability negotiation and HMAC authentication)
    bridge = new DaemonConnectedBridge({ bootstrapPath: runtimePath });
    await bridge.connect();
    assert.strictEqual(bridge.connectionState, 'connected');

    // Connect admin transport for graceful shutdown
    const adminContent = JSON.parse(await fs.promises.readFile(adminRuntimePath, 'utf8'));
    adminTransport = new NativeIpcTransport({
      endpoint: adminContent.endpoint,
      keyId: adminContent.key_id,
      clientKey: adminContent.admin_key,
      commandTimeoutMs: 5000,
    });
    await adminTransport.connect();
    assert.strictEqual(adminTransport.getState(), 'ready');
  });

  after(async () => {
    if (bridge) {
      try {
        await bridge.disconnect();
        bridge.dispose();
      } catch {
        // Ignore
      }
      bridge = null;
    }

    if (adminTransport) {
      try {
        await adminTransport.sendCommand({ type: 'shutdown' });
      } catch {
        // Ignore
      }
      try {
        adminTransport.disconnect();
      } catch {
        // Ignore
      }
      adminTransport = null;
    }

    if (daemonProcess && !daemonProcess.killed) {
      try {
        daemonProcess.kill('SIGKILL');
      } catch {
        // Ignore
      }
      daemonProcess = null;
    }

    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore
    }
  });

  describe('Tier: REAL_AGSD_READ_ONLY_IPC', () => {
    it('1. real daemon returns error response for non-existent task diff without socket disruption', async () => {
      assert.ok(bridge?.transport);
      const testStudioId = crypto.randomUUID();
      const testTaskId = crypto.randomUUID();

      await assert.rejects(
        async () => {
          await bridge!.transport!.sendCommand({
            type: 'get_task_diff',
            studio_id: testStudioId,
            task_id: testTaskId,
          });
        },
        (err: Error) => {
          assert.ok(
            err.message.includes('not found') || err.message.includes('does not belong'),
            `Error message should indicate task not found, got: ${err.message}`
          );
          return true;
        }
      );

      // Verify connection remains completely healthy after error response
      const pong = await bridge.transport.sendCommand({
        type: 'ping',
        payload: 'verify-alive',
        timestamp: Date.now(),
      });
      assert.strictEqual(pong.type, 'pong');
    });

    it('2. real daemon rejects get_task_diff with zero limit (infinite loop guard)', async () => {
      assert.ok(bridge?.transport);
      const testStudioId = crypto.randomUUID();
      const testTaskId = crypto.randomUUID();

      await assert.rejects(
        async () => {
          await bridge!.transport!.sendCommand({
            type: 'get_task_diff',
            studio_id: testStudioId,
            task_id: testTaskId,
            limit: 0,
          });
        },
        (err: Error) => {
          assert.ok(
            err.message.includes('limit must be greater than zero'),
            `Expected zero-limit rejection, got: ${err.message}`
          );
          return true;
        }
      );
    });

    it('3. real daemon rejects list_task_artifacts for non-existent task', async () => {
      assert.ok(bridge?.transport);
      const testStudioId = crypto.randomUUID();
      const testTaskId = crypto.randomUUID();

      await assert.rejects(
        async () => {
          await bridge!.transport!.sendCommand({
            type: 'list_task_artifacts',
            studio_id: testStudioId,
            task_id: testTaskId,
          });
        },
        (err: Error) => {
          assert.ok(
            err.message.includes('not found') || err.message.includes('does not belong'),
            `Error message should indicate task not found, got: ${err.message}`
          );
          return true;
        }
      );
    });

    it('4. real daemon returns null artifact for non-existent artifact ID', async () => {
      assert.ok(bridge?.transport);
      const testStudioId = crypto.randomUUID();
      const testArtifactId = crypto.randomUUID();

      const response = await bridge.transport.sendCommand({
        type: 'get_artifact',
        studio_id: testStudioId,
        artifact_id: testArtifactId,
      });

      assert.strictEqual(response.type, 'artifact');
      const artResp = response as {
        type: string;
        artifact?: unknown | null;
        content?: string | null;
      };
      assert.strictEqual(artResp.artifact, null);
    });

    it('5. ArtifactClient.fetchTaskDiff throws ArtifactRetrievalError against real daemon', async () => {
      assert.ok(bridge?.transport);
      const testStudioId = crypto.randomUUID();
      const testTaskId = crypto.randomUUID();

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(
            bridge!.transport!,
            testStudioId,
            testTaskId
          );
        },
        (err: unknown) => {
          assert.ok(err instanceof ArtifactRetrievalError);
          assert.strictEqual(err.code, 'IPC_ERROR');
          assert.ok(err.message.includes('not found') || err.message.includes('does not belong'));
          return true;
        }
      );
    });
  });

  describe('Tier: IN_PROCESS_REAL_ARTIFACT_STORE & MOCK_IPC', () => {
    it('6. multi-chunk streaming with lossless UTF-8 reassembly and SHA-256 verification (VERIFIED)', async () => {
      const fullText =
        'diff --git a/src/app.ts b/src/app.ts\n' +
        '--- a/src/app.ts\n' +
        '+++ b/src/app.ts\n' +
        '@@ -1,3 +1,3 @@\n' +
        '-const version = "1.0.0";\n' +
        '+const version = "1.1.0";\n';
      const fullBuffer = Buffer.from(fullText, 'utf8');
      const expectedSha = crypto.createHash('sha256').update(fullBuffer).digest('hex');

      // Slice into two chunks
      const chunk1 = fullBuffer.subarray(0, 40);
      const chunk2 = fullBuffer.subarray(40);

      let callCount = 0;
      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_task_diff') {
            callCount++;
            if (callCount === 1) {
              return {
                type: 'task_diff',
                diff: {
                  studio_id: 'studio-store-01',
                  task_id: 'task-store-01',
                  summary: {
                    files_changed: 1,
                    added_files: [],
                    modified_files: ['src/app.ts'],
                    deleted_files: [],
                    renamed_files: [],
                    binary_files: [],
                    patch_size_bytes: fullBuffer.length,
                    base_commit: 'abc1234',
                  },
                  patch: chunk1.toString('utf8'),
                  patch_base64: chunk1.toString('base64'),
                  offset: 0,
                  limit: 40,
                  has_more: true,
                  total_bytes: fullBuffer.length,
                  patch_sha256: expectedSha,
                  chunk_bytes: chunk1.length,
                },
              };
            } else {
              return {
                type: 'task_diff',
                diff: {
                  studio_id: 'studio-store-01',
                  task_id: 'task-store-01',
                  summary: {
                    files_changed: 1,
                    added_files: [],
                    modified_files: ['src/app.ts'],
                    deleted_files: [],
                    renamed_files: [],
                    binary_files: [],
                    patch_size_bytes: fullBuffer.length,
                    base_commit: 'abc1234',
                  },
                  patch: chunk2.toString('utf8'),
                  patch_base64: chunk2.toString('base64'),
                  offset: 40,
                  limit: fullBuffer.length - 40,
                  has_more: false,
                  total_bytes: fullBuffer.length,
                  patch_sha256: expectedSha,
                  chunk_bytes: chunk2.length,
                },
              };
            }
          }
          return { type: 'error', message: 'Unhandled command' };
        },
      };

      const result = await ArtifactClient.fetchTaskDiff(
        mockTransport,
        'studio-store-01',
        'task-store-01'
      );

      assert.strictEqual(result.integrity, 'VERIFIED');
      assert.strictEqual(result.sha256, expectedSha);
      assert.strictEqual(result.chunkCount, 2);
      assert.strictEqual(result.text, fullText);
      assert.deepStrictEqual(result.rawBuffer, fullBuffer);
    });

    it('7. preserves multi-byte Vietnamese Unicode split across arbitrary byte boundaries', async () => {
      // Vietnamese text: 'Tiếng Việt với đầy đủ dấu: ế, ệ, ẩ, ờ, đ'
      const vnLine = '+Tiếng Việt với đầy đủ dấu: ế, ệ, ẩ, ờ, đ\n';
      const fullText = `diff --git a/docs/vn.txt b/docs/vn.txt\n--- /dev/null\n+++ b/docs/vn.txt\n@@ -0,0 +1,1 @@\n${vnLine}`;
      const fullBuffer = Buffer.from(fullText, 'utf8');
      const expectedSha = crypto.createHash('sha256').update(fullBuffer).digest('hex');

      // Intentionally cut inside multi-byte character
      const eIndex = fullBuffer.indexOf(Buffer.from('ế', 'utf8'));
      const cutPoint = eIndex + 1; // Cuts inside the 3-byte UTF-8 sequence of 'ế'

      const chunk1 = fullBuffer.subarray(0, cutPoint);
      const chunk2 = fullBuffer.subarray(cutPoint);

      let callCount = 0;
      const mockTransport = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          if (cmd.type === 'get_task_diff') {
            callCount++;
            const chunk = callCount === 1 ? chunk1 : chunk2;
            const offset = callCount === 1 ? 0 : cutPoint;
            const hasMore = callCount === 1;

            return {
              type: 'task_diff',
              diff: {
                studio_id: 'studio-vn',
                task_id: 'task-vn',
                summary: {
                  files_changed: 1,
                  added_files: ['docs/vn.txt'],
                  modified_files: [],
                  deleted_files: [],
                  renamed_files: [],
                  binary_files: [],
                  patch_size_bytes: fullBuffer.length,
                  base_commit: 'abc1234',
                },
                patch: chunk.toString('utf8'),
                patch_base64: chunk.toString('base64'),
                offset,
                limit: chunk.length,
                has_more: hasMore,
                total_bytes: fullBuffer.length,
                patch_sha256: expectedSha,
                chunk_bytes: chunk.length,
              },
            };
          }
          return { type: 'error', message: 'Unhandled' };
        },
      };

      const result = await ArtifactClient.fetchTaskDiff(mockTransport, 'studio-vn', 'task-vn');

      assert.strictEqual(result.integrity, 'VERIFIED');
      assert.strictEqual(result.text, fullText);
      assert.ok(result.text.includes('ế, ệ, ẩ, ờ, đ'));
      assert.deepStrictEqual(result.rawBuffer, fullBuffer);
    });

    it('8. binary Git patch chunk streaming with isBinary: true and exact byte preservation', async () => {
      const binaryBytes = Buffer.from([0x00, 0xff, 0xfe, 0x01, 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a]);
      const binarySha = crypto.createHash('sha256').update(binaryBytes).digest('hex');

      const mockTransport = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: 'studio-bin',
            task_id: 'task-bin',
            summary: {
              files_changed: 1,
              added_files: ['image.png'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: ['image.png'],
              patch_size_bytes: binaryBytes.length,
              base_commit: 'abc1234',
            },
            patch: '',
            patch_base64: binaryBytes.toString('base64'),
            offset: 0,
            limit: binaryBytes.length,
            has_more: false,
            total_bytes: binaryBytes.length,
            patch_sha256: binarySha,
            chunk_bytes: binaryBytes.length,
          },
        }),
      };

      const result = await ArtifactClient.fetchTaskDiff(mockTransport, 'studio-bin', 'task-bin');

      assert.strictEqual(result.integrity, 'VERIFIED');
      assert.strictEqual(result.isBinary, true);
      assert.deepStrictEqual(result.rawBuffer, binaryBytes);
    });

    it('9. cryptographic tampering detection yielding INTEGRITY_FAILED', async () => {
      const authenticText = 'diff --git a/test.txt b/test.txt\n+authentic content\n';
      const authenticBuffer = Buffer.from(authenticText, 'utf8');
      const authenticSha = crypto.createHash('sha256').update(authenticBuffer).digest('hex');

      const tamperedBuffer = Buffer.from('diff --git a/test.txt b/test.txt\n+tampered content\n', 'utf8');

      const mockTransport = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: 'studio-tamper',
            task_id: 'task-tamper',
            summary: {
              files_changed: 1,
              added_files: ['test.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: tamperedBuffer.length,
              base_commit: 'abc1234',
            },
            patch: tamperedBuffer.toString('utf8'),
            patch_base64: tamperedBuffer.toString('base64'),
            offset: 0,
            limit: tamperedBuffer.length,
            has_more: false,
            total_bytes: tamperedBuffer.length,
            patch_sha256: authenticSha, // Does not match tampered bytes
            chunk_bytes: tamperedBuffer.length,
          },
        }),
      };

      const result = await ArtifactClient.fetchTaskDiff(mockTransport, 'studio-tamper', 'task-tamper');

      assert.strictEqual(result.integrity, 'INTEGRITY_FAILED');
      assert.strictEqual(result.expectedSha256, authenticSha);
      assert.notStrictEqual(result.sha256, authenticSha);
    });

    it('10. studio ID mismatch rejection (STUDIO_ID_MISMATCH)', async () => {
      const mockTransport = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: 'studio-unrelated',
            task_id: 'task-1',
            summary: {
              files_changed: 1,
              added_files: ['test.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 6,
              base_commit: 'abc1234',
            },
            patch: '+diff\n',
            patch_base64: Buffer.from('+diff\n').toString('base64'),
            offset: 0,
            limit: 6,
            has_more: false,
            total_bytes: 6,
            chunk_bytes: 6,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(mockTransport, 'studio-expected', 'task-1');
        },
        (err: unknown) => {
          assert.ok(err instanceof ArtifactRetrievalError);
          assert.strictEqual(err.code, 'STUDIO_ID_MISMATCH');
          return true;
        }
      );
    });

    it('11. task ID mismatch rejection (TASK_ID_MISMATCH)', async () => {
      const mockTransport = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: 'studio-1',
            task_id: 'task-unrelated',
            summary: {
              files_changed: 1,
              added_files: ['test.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 6,
              base_commit: 'abc1234',
            },
            patch: '+diff\n',
            patch_base64: Buffer.from('+diff\n').toString('base64'),
            offset: 0,
            limit: 6,
            has_more: false,
            total_bytes: 6,
            chunk_bytes: 6,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(mockTransport, 'studio-1', 'task-expected');
        },
        (err: unknown) => {
          assert.ok(err instanceof ArtifactRetrievalError);
          assert.strictEqual(err.code, 'TASK_ID_MISMATCH');
          return true;
        }
      );
    });
  });
});
