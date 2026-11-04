/**
 * Agent Studios — Artifact & Cryptographic Security Regression Test Suite
 * Milestone M15 UI-04: Security Review & Cryptographic Verification
 *
 * Comprehensive regression tests verifying:
 * 1. TOCTOU Safety: Exporting writes byte-for-byte from verified rawBuffer, never re-encoded text.
 * 2. Path Traversal & Symlink Safety: Filename sanitization, Git header traversal immunity, and safe export containment.
 * 3. Vietnamese Diacritic Quoting: Git C-style octal escape sequences (e.g. \341\273\207 -> 'ệ') and malformed UTF-8 resilience.
 * 4. Mixed Binary/Text Diff Export Fidelity: Byte-exact preservation of binary hunks, null bytes, CRLF, and high-entropy payloads.
 * 5. Multi-Chunk Memory & DoS Bounds: Aggregated byte limits (EXCEEDED_MAX_BYTES), chunk count limits (EXCEEDED_MAX_CHUNKS), and zero-progress loops.
 * 6. Studio & Task Identity Enforcement: Cross-studio spoofing rejection and cross-task mismatch detection across transport and UI.
 * 7. Cryptographic Gating Invariants: INTEGRITY_FAILED strictly blocks, UNVERIFIED requires modal confirmation, VERIFIED exports seamlessly.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  ArtifactClient,
  ArtifactRetrievalError,
  IpcTransportSender,
  ReassembledTaskDiff,
} from '../../bridge/ipc/ArtifactClient';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import {
  cleanGitPath,
  DiffParser,
  isPathTraversal,
  parseDiffGitHeader,
  sanitizeArtifactId,
  sanitizePath,
  unquoteGitPath,
} from '../../cards/DiffParser';
import {
  ArtifactReviewOptions,
  ArtifactReviewPanel,
} from '../../webviews/ArtifactReviewPanel';
import { WorkbenchViewProvider } from '../../views/workbench/WorkbenchViewProvider';
import {
  mockWindowMessages,
  resetMockState,
} from './vscodeMock';

describe('Artifact & Cryptographic Security Regression Tests (M15 UI-04)', () => {
  const extensionUri = vscode.Uri.file('/mock/extension');
  const validStudioId = 'studio-secure-corp-001';
  const validTaskId = 'task-security-audit-002';
  let tempDir: string;

  beforeEach(() => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    resetMockState();
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ags-sec-regression-'));
  });

  afterEach(() => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    resetMockState();
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error
    }
  });

  /**
   * Helper to construct a typed ReassembledTaskDiff fixture.
   */
  function buildDiffFixture(opts: {
    studioId?: string;
    taskId?: string;
    rawBuffer: Buffer;
    integrity?: 'VERIFIED' | 'UNVERIFIED' | 'INTEGRITY_FAILED';
    expectedSha256?: string | null;
    isBinary?: boolean;
    text?: string;
  }): ReassembledTaskDiff {
    const rawBuffer = opts.rawBuffer;
    const computedSha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex').toLowerCase();
    const isBinary = opts.isBinary ?? (rawBuffer.includes(Buffer.from('GIT binary patch')) || rawBuffer.includes(0));
    const text = opts.text ?? new TextDecoder('utf-8', { fatal: false }).decode(rawBuffer);

    let integrity = opts.integrity;
    if (!integrity) {
      if (opts.expectedSha256) {
        integrity = computedSha256 === opts.expectedSha256.toLowerCase() ? 'VERIFIED' : 'INTEGRITY_FAILED';
      } else {
        integrity = 'VERIFIED';
      }
    }

    return {
      studioId: opts.studioId ?? validStudioId,
      taskId: opts.taskId ?? validTaskId,
      summary: {
        files_changed: 1,
        added_files: ['change.txt'],
        modified_files: [],
        deleted_files: [],
        renamed_files: [],
        binary_files: isBinary ? ['change.bin'] : [],
        patch_size_bytes: rawBuffer.length,
        base_commit: 'commit-base-000',
      },
      rawBuffer,
      text,
      isBinary,
      sha256: computedSha256,
      expectedSha256: opts.expectedSha256 !== undefined ? opts.expectedSha256 : computedSha256,
      integrity,
      totalBytes: rawBuffer.length,
      chunkCount: 1,
    };
  }

  // =========================================================================
  // 1. TOCTOU Safety & Raw Buffer Export
  // =========================================================================
  describe('1. TOCTOU Safety & Direct rawBuffer Export', () => {
    it('1.1 exports exact verified rawBuffer byte-for-byte even if text property is tampered', async () => {
      // Construct raw binary buffer with arbitrary non-UTF-8 bytes and null chars
      const rawBytes = Buffer.from([0x00, 0xff, 0xfe, 0x80, 0x90, 0xaa, 0xbb, 0xcc, 0xdd, 0xee]);
      const diff = buildDiffFixture({
        rawBuffer: rawBytes,
        integrity: 'VERIFIED',
      });

      // Deliberately tamper with the decoded 'text' string property
      diff.text = 'MALICIOUS_TAMPERED_TEXT_REPLACEMENT';

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(diff, panel.activeRequestId);

      const targetExportPath = path.join(tempDir, 'toctou_tampered_text.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(targetExportPath);

      // Trigger export
      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      assert.strictEqual(fs.existsSync(targetExportPath), true, 'Exported file must exist');
      const exportedFileBytes = fs.readFileSync(targetExportPath);

      // Must strictly match rawBytes, not UTF-8 encoding of 'text'
      assert.deepStrictEqual(exportedFileBytes, rawBytes);
      const exportedSha256 = crypto.createHash('sha256').update(exportedFileBytes).digest('hex').toLowerCase();
      assert.strictEqual(exportedSha256, diff.sha256);
      assert.ok(mockWindowMessages.info.some((m) => m.includes('successfully exported')));
    });

    it('1.2 prevents re-encoding mutations: preserves CRLF vs LF line endings without normalization', async () => {
      // Mixed line endings in unified diff (CRLF in hunk, LF in headers)
      const mixedLinePatch = Buffer.from('diff --git a/test.txt b/test.txt\n--- a/test.txt\r\n+++ b/test.txt\r\n@@ -1,1 +1,1 @@\r\n-old\r\n+new\r\n', 'binary');
      const diff = buildDiffFixture({
        rawBuffer: mixedLinePatch,
        integrity: 'VERIFIED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(diff, panel.activeRequestId);

      const targetExportPath = path.join(tempDir, 'crlf_preserved.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(targetExportPath);

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      const exportedFileBytes = fs.readFileSync(targetExportPath);
      assert.deepStrictEqual(exportedFileBytes, mixedLinePatch);
      assert.strictEqual(exportedFileBytes.length, mixedLinePatch.length);
    });
  });

  // =========================================================================
  // 2. Path Traversal & Symlink Safety
  // =========================================================================
  describe('2. Path Traversal & Filename Safety', () => {
    it('2.1 handles malicious directory traversal attempts in taskId during export dialog', async () => {
      const traversalTaskId = '../../../../etc/passwd';
      const options: ArtifactReviewOptions = {
        studioId: validStudioId,
        taskId: traversalTaskId,
      };

      const diff = buildDiffFixture({
        studioId: validStudioId,
        taskId: traversalTaskId,
        rawBuffer: Buffer.from('diff --git a/safe.txt b/safe.txt\n+safe\n', 'utf8'),
        integrity: 'VERIFIED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, options);
      panel.setDiffData(diff, panel.activeRequestId);

      // The save dialog is controlled by operator/VS Code: target path is locked to tempDir
      const safeTarget = path.join(tempDir, 'safe_export.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(safeTarget);

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      // Ensure write went strictly to safeTarget and did not escape sandbox
      assert.strictEqual(fs.existsSync(safeTarget), true);
      assert.strictEqual(fs.existsSync(path.join(tempDir, 'passwd')), false);
      assert.strictEqual(fs.existsSync('/etc/passwd.patch'), false);
    });

    it('2.2 parses path traversal in Git diff headers without crashing and extracts clean relative paths', () => {
      const maliciousHeader = 'diff --git "a/../../etc/shadow" "b/../../etc/shadow"';
      const parsedHeader = parseDiffGitHeader(maliciousHeader);

      assert.ok(parsedHeader !== null);
      assert.strictEqual(cleanGitPath(parsedHeader.oldPath), '../../etc/shadow');
      assert.strictEqual(cleanGitPath(parsedHeader.newPath), '../../etc/shadow');

      // Full diff parsing
      const maliciousDiff = [
        'diff --git "a/../../etc/shadow" "b/../../etc/shadow"',
        '--- a/../../etc/shadow',
        '+++ b/../../etc/shadow',
        '@@ -1,1 +1,1 @@',
        '-root:x:0:0:',
        '+root:*:0:0:',
      ].join('\n');

      const parsed = DiffParser.parseUnifiedDiff(maliciousDiff);
      assert.strictEqual(parsed.totalFiles, 1);
      assert.strictEqual(parsed.files[0].displayPath, '../../etc/shadow');
      assert.strictEqual(parsed.files[0].additions, 1);
      assert.strictEqual(parsed.files[0].deletions, 1);
    });

    it('2.3 handles Windows backslash path traversal sequences in Git diff paths', () => {
      const rawQuoted = '"a/..\\\\..\\\\Windows\\\\System32\\\\cmd.exe"';
      const cleaned = cleanGitPath(rawQuoted);
      // Verify backslashes are unescaped safely
      assert.strictEqual(cleaned, '..\\..\\Windows\\System32\\cmd.exe');
    });
  });

  // =========================================================================
  // 3. Vietnamese Diacritic Quoting & Malformed UTF-8 Resilience
  // =========================================================================
  describe('3. Vietnamese Diacritic Quoting & Malformed UTF-8 Resilience', () => {
    it('3.1 correctly decodes Git octal sequences for Vietnamese diacritics including \\341\\273\\207', () => {
      // '\341\273\207' is 'ệ' (U+1EC7) -> UTF-8 bytes: 0xE1 (341 octal), 0xBB (273 octal), 0x87 (207 octal)
      const octalQuotedE = '"\\341\\273\\207"';
      const decodedE = unquoteGitPath(octalQuotedE);
      assert.strictEqual(decodedE, 'ệ');

      // '\341\272\277' is 'ế' (U+1EBF) -> UTF-8 bytes: 0xE1 (341 octal), 0xBA (272 octal), 0xBF (277 octal)
      const octalQuotedEAcute = '"\\341\\272\\277"';
      const decodedEAcute = unquoteGitPath(octalQuotedEAcute);
      assert.strictEqual(decodedEAcute, 'ế');

      // Full complex Vietnamese filename: "tài_liệu_kế_hoạch_hệ_thống.md"
      // 'à' = \303\240
      // 'ệ' = \341\273\207
      // 'ế' = \341\272\277
      // 'ố' = \341\273\221
      const complexQuoted = '"t\\303\\240i_li\\341\\273\\207u_k\\341\\272\\277_ho\\341\\272\\241ch_h\\341\\273\\207_th\\341\\273\\221ng.md"';
      const decodedComplex = unquoteGitPath(complexQuoted);
      assert.strictEqual(decodedComplex, 'tài_liệu_kế_hoạch_hệ_thống.md');

      // Full unified diff containing Vietnamese filenames
      const diffWithVietnamese = [
        `diff --git "a/${complexQuoted.slice(1, -1)}" "b/${complexQuoted.slice(1, -1)}"`,
        '--- "a/' + complexQuoted.slice(1, -1) + '"',
        '+++ "b/' + complexQuoted.slice(1, -1) + '"',
        '@@ -1,1 +1,1 @@',
        '-bản cũ',
        '+bản mới cập nhật tiếng Việt',
      ].join('\n');

      const parsed = DiffParser.parseUnifiedDiff(diffWithVietnamese);
      assert.strictEqual(parsed.totalFiles, 1);
      assert.strictEqual(parsed.files[0].displayPath, 'tài_liệu_kế_hoạch_hệ_thống.md');
      assert.strictEqual(parsed.files[0].additions, 1);
      assert.strictEqual(parsed.files[0].deletions, 1);
    });

    it('3.2 survives malformed UTF-8 octal byte sequences without throwing (fatal: false)', () => {
      // Incomplete 3-byte sequence: \341\273 without third byte followed by ascii 'X'
      const malformedOctal = '"\\341\\273X"';
      // Must not throw exception
      let decoded = '';
      assert.doesNotThrow(() => {
        decoded = unquoteGitPath(malformedOctal);
      });
      // TextDecoder replaces invalid byte sequences with replacement character �
      assert.ok(decoded.includes('�') || decoded.endsWith('X'));
    });

    it('3.3 survives raw invalid UTF-8 bytes in diff text decoding without corruption', () => {
      // Buffer containing illegal UTF-8 byte sequences (e.g. 0xC0, 0xC1, 0xF5..0xFF)
      const invalidUtf8Buffer = Buffer.from([0x61, 0x62, 0xc0, 0xaf, 0xff, 0x63, 0x64]);
      const decoder = new TextDecoder('utf-8', { fatal: false });
      const decodedText = decoder.decode(invalidUtf8Buffer);

      // Decoded text must include replacement characters but not crash
      assert.ok(decodedText.includes('�'));
      assert.ok(decodedText.startsWith('ab'));
      assert.ok(decodedText.endsWith('cd'));

      // Raw buffer must remain completely unchanged
      assert.strictEqual(invalidUtf8Buffer.length, 7);
      assert.strictEqual(invalidUtf8Buffer[2], 0xc0);
      assert.strictEqual(invalidUtf8Buffer[4], 0xff);
    });
  });

  // =========================================================================
  // 4. Mixed Binary & Text Diff Export Byte Fidelity
  // =========================================================================
  describe('4. Mixed Binary & Text Diff Export Byte Fidelity', () => {
    it('4.1 reassembles and exports mixed diff containing both code changes and raw binary blobs', async () => {
      const textHunk = 'diff --git a/src/app.ts b/src/app.ts\n@@ -1,2 +1,3 @@\n export const name = "demo";\n+export const version = "1.0.0";\n';
      const binaryHunk = 'diff --git a/media/icon.png b/media/icon.png\nGIT binary patch\nliteral 24\nzc$@)Z000010000100001\n\0\0\0\0\xff\xfe\x01\x02\n';
      const mixedPayload = Buffer.concat([
        Buffer.from(textHunk, 'utf8'),
        Buffer.from(binaryHunk, 'binary'),
      ]);

      const expectedSha256 = crypto.createHash('sha256').update(mixedPayload).digest('hex').toLowerCase();

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (cmd: RustIpcCommand): Promise<RustIpcResponse> => {
          assert.strictEqual(cmd.type, 'get_task_diff');
          return {
            type: 'task_diff',
            diff: {
              studio_id: validStudioId,
              task_id: validTaskId,
              summary: {
                files_changed: 2,
                added_files: [],
                modified_files: ['src/app.ts'],
                deleted_files: [],
                renamed_files: [],
                binary_files: ['media/icon.png'],
                patch_size_bytes: mixedPayload.length,
                base_commit: 'abc1234',
              },
              patch: '',
              offset: 0,
              limit: 1048576,
              total_bytes: mixedPayload.length,
              has_more: false,
              patch_base64: mixedPayload.toString('base64'),
              encoding: 'base64',
              patch_sha256: expectedSha256,
              chunk_bytes: mixedPayload.length,
            },
          };
        },
      };

      const reassembled = await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);

      assert.strictEqual(reassembled.integrity, 'VERIFIED');
      assert.strictEqual(reassembled.isBinary, true, 'Diff must be flagged as binary overall');
      assert.strictEqual(reassembled.text.includes('+export const version = "1.0.0";'), true, 'Text portion must be visible');
      assert.deepStrictEqual(reassembled.rawBuffer, mixedPayload);
      assert.strictEqual(reassembled.sha256, expectedSha256);

      // Now verify export of this mixed diff
      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(reassembled, panel.activeRequestId);

      const exportDest = path.join(tempDir, 'mixed_binary_text.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(exportDest);

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      const exportedDiskBytes = fs.readFileSync(exportDest);
      assert.deepStrictEqual(exportedDiskBytes, mixedPayload, 'Exported bytes must match mixed binary payload exactly');
      const diskSha256 = crypto.createHash('sha256').update(exportedDiskBytes).digest('hex').toLowerCase();
      assert.strictEqual(diskSha256, expectedSha256);
    });
  });

  // =========================================================================
  // 5. Memory Limit Enforcement (EXCEEDED_MAX_BYTES & DoS Checks)
  // =========================================================================
  describe('5. Memory Limit & DoS Boundary Enforcement', () => {
    it('5.1 strictly enforces EXCEEDED_MAX_BYTES memory bound across paginated chunks', async () => {
      // Chunk of 100 bytes
      const chunkBytes = Buffer.alloc(100, 'A');
      let currentOffset = 0;

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => {
          const resp: RustIpcResponse = {
            type: 'task_diff',
            diff: {
              studio_id: validStudioId,
              task_id: validTaskId,
              summary: {
                files_changed: 1,
                added_files: ['big.bin'],
                modified_files: [],
                deleted_files: [],
                renamed_files: [],
                binary_files: [],
                patch_size_bytes: 1000,
                base_commit: '000',
              },
              patch: '',
              offset: currentOffset,
              limit: 100,
              total_bytes: 1000,
              has_more: true,
              patch_base64: chunkBytes.toString('base64'),
              encoding: 'base64',
              patch_sha256: null,
              chunk_bytes: 100,
            },
          };
          currentOffset += 100;
          return resp;
        },
      };

      // Set maxBytes limit to 250 bytes (3rd chunk reaches 300 bytes > 250 bytes)
      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId, {
            maxBytes: 250,
          });
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'EXCEEDED_MAX_BYTES');
          assert.ok(err.message.includes('exceeded maximum aggregate memory limit'));
          return true;
        }
      );
    });

    it('5.2 strictly enforces EXCEEDED_MAX_CHUNKS limit (Anti-DoS chunk flooding)', async () => {
      const chunkBytes = Buffer.from('x');
      let currentOffset = 0;

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => {
          const resp: RustIpcResponse = {
            type: 'task_diff',
            diff: {
              studio_id: validStudioId,
              task_id: validTaskId,
              summary: {
                files_changed: 1,
                added_files: ['f.txt'],
                modified_files: [],
                deleted_files: [],
                renamed_files: [],
                binary_files: [],
                patch_size_bytes: 50,
                base_commit: '000',
              },
              patch: '',
              offset: currentOffset,
              limit: 1,
              total_bytes: 50,
              has_more: true,
              patch_base64: chunkBytes.toString('base64'),
              encoding: 'base64',
              patch_sha256: null,
              chunk_bytes: 1,
            },
          };
          currentOffset += 1;
          return resp;
        },
      };

      // Set maxChunks limit to 5 chunks
      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId, {
            maxChunks: 5,
          });
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'EXCEEDED_MAX_CHUNKS');
          assert.ok(err.message.includes('exceeded maximum allowed chunk count'));
          return true;
        }
      );
    });

    it('5.3 detects ZERO_PROGRESS pagination loop when chunk length is zero and has_more is true', async () => {
      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: validStudioId,
            task_id: validTaskId,
            summary: {
              files_changed: 1,
              added_files: ['loop.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 100,
              base_commit: '000',
            },
            patch: '',
            offset: 0,
            limit: 100,
            total_bytes: 100,
            has_more: true,
            patch_base64: '', // Empty 0-byte payload with has_more=true
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 0,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'ZERO_PROGRESS');
          assert.ok(err.message.includes('Zero-progress chunk received'));
          return true;
        }
      );
    });
  });

  // =========================================================================
  // 6. Cross-Studio & Cross-Task Identity Spoofing Rejection
  // =========================================================================
  describe('6. Cross-Studio & Cross-Task Identity Spoofing Rejection', () => {
    it('6.1 rejects cross-studio spoofing in ArtifactClient with STUDIO_ID_MISMATCH', async () => {
      const spoofedStudioId = 'studio-attacker-controlled-666';

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: spoofedStudioId, // Attacker studio
            task_id: validTaskId,
            summary: {
              files_changed: 1,
              added_files: ['trojan.sh'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 10,
              base_commit: '000',
            },
            patch: '',
            offset: 0,
            limit: 100,
            total_bytes: 10,
            has_more: false,
            patch_base64: Buffer.from('+malicious\n').toString('base64'),
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 11,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'STUDIO_ID_MISMATCH');
          assert.ok(err.message.includes(`requested '${validStudioId}' but received '${spoofedStudioId}'`));
          return true;
        }
      );
    });

    it('6.2 rejects cross-task spoofing in ArtifactClient with TASK_ID_MISMATCH', async () => {
      const spoofedTaskId = 'task-foreign-private-999';

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: validStudioId,
            task_id: spoofedTaskId, // Attacker task
            summary: {
              files_changed: 1,
              added_files: ['leak.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 10,
              base_commit: '000',
            },
            patch: '',
            offset: 0,
            limit: 100,
            total_bytes: 10,
            has_more: false,
            patch_base64: Buffer.from('+leak\n').toString('base64'),
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 6,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'TASK_ID_MISMATCH');
          assert.ok(err.message.includes(`requested '${validTaskId}' but received '${spoofedTaskId}'`));
          return true;
        }
      );
    });

    it('6.3 rejects setDiffData in ArtifactReviewPanel when diff studio or task does not match active workbench', () => {
      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });

      // Attempt injecting diff from rogue studio
      const rogueStudioDiff = buildDiffFixture({
        studioId: 'studio-rogue-attacker',
        taskId: validTaskId,
        rawBuffer: Buffer.from('diff --git a/a b/a\n+bad\n'),
      });
      panel.setDiffData(rogueStudioDiff, panel.activeRequestId);
      assert.strictEqual(panel.diffData, null, 'Panel diffData must remain null on rogue studio mismatch');

      // Attempt injecting diff from rogue task
      const rogueTaskDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: 'task-other-project-secret',
        rawBuffer: Buffer.from('diff --git a/a b/a\n+secret\n'),
      });
      panel.setDiffData(rogueTaskDiff, panel.activeRequestId);
      assert.strictEqual(panel.diffData, null, 'Panel diffData must remain null on rogue task mismatch');
    });

    it('6.4 blocks clipboard copy and file export if diff data identity does not match workbench options', async () => {
      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });

      // Legitimate initial diff
      const initialDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: Buffer.from('diff --git a/a b/a\n+ok\n'),
      });
      panel.setDiffData(initialDiff, panel.activeRequestId);

      // Now tamper with the stored diffData object identity
      (panel as unknown as { _diffData: ReassembledTaskDiff })._diffData = {
        ...initialDiff,
        studioId: 'studio-injected-spoof',
      };

      // Attempt copy
      await (panel as unknown as { _handleCopyPatch: () => Promise<void> })._handleCopyPatch();
      assert.ok(mockWindowMessages.error.some((e) => e.includes('Diff data does not match active task identity')));
      assert.strictEqual(mockWindowMessages.clipboardText, '');

      // Attempt export
      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();
      assert.ok(mockWindowMessages.error.some((e) => e.includes('Diff data does not match active task identity')));
    });
  });

  // =========================================================================
  // 7. Cryptographic Gating Invariants
  // =========================================================================
  describe('7. Cryptographic Gating Invariants (INTEGRITY_FAILED vs UNVERIFIED vs VERIFIED)', () => {
    it('7.1 strictly blocks copy and export for INTEGRITY_FAILED diffs with prominent error', async () => {
      const genuineBytes = Buffer.from('genuine patch content');
      const tamperedBytes = Buffer.from('TAMPERED patch content');
      const expectedSha256 = crypto.createHash('sha256').update(genuineBytes).digest('hex').toLowerCase();

      const failedDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: tamperedBytes,
        expectedSha256,
        integrity: 'INTEGRITY_FAILED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(failedDiff, panel.activeRequestId);

      // Test Copy blocked
      await (panel as unknown as { _handleCopyPatch: () => Promise<void> })._handleCopyPatch();
      assert.ok(mockWindowMessages.error.some((e) => e.includes('Cryptographic SHA-256 integrity verification failed')));
      assert.strictEqual(mockWindowMessages.clipboardText, '');

      // Test Export blocked
      mockWindowMessages.error.length = 0;
      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();
      assert.ok(mockWindowMessages.error.some((e) => e.includes('Cryptographic SHA-256 integrity verification failed')));
    });

    it('7.2 requires modal confirmation for UNVERIFIED diff; aborts export if operator declines', async () => {
      const unverifiedDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: Buffer.from('diff --git a/test.txt b/test.txt\n+unverified\n'),
        expectedSha256: null,
        integrity: 'UNVERIFIED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(unverifiedDiff, panel.activeRequestId);

      const targetPath = path.join(tempDir, 'declined_unverified.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(targetPath);
      mockWindowMessages.modalWarningResponse = 'Cancel'; // Operator cancels

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      // Warning message modal was shown
      assert.ok(mockWindowMessages.warning.some((w) => w.includes('UNVERIFIED')));
      // File was NOT written
      assert.strictEqual(fs.existsSync(targetPath), false, 'Export must be aborted when operator declines');
    });

    it('7.3 exports UNVERIFIED diff when operator explicitly confirms modal warning', async () => {
      const unverifiedBytes = Buffer.from('diff --git a/test.txt b/test.txt\n+unverified line\n');
      const unverifiedDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: unverifiedBytes,
        expectedSha256: null,
        integrity: 'UNVERIFIED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(unverifiedDiff, panel.activeRequestId);

      const targetPath = path.join(tempDir, 'confirmed_unverified.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(targetPath);
      mockWindowMessages.modalWarningResponse = 'Export Unverified Patch'; // Operator confirms

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      assert.strictEqual(fs.existsSync(targetPath), true, 'File must be written on confirmation');
      assert.deepStrictEqual(fs.readFileSync(targetPath), unverifiedBytes);
    });

    it('7.4 seamlessly exports VERIFIED diff without modal warning', async () => {
      const verifiedBytes = Buffer.from('diff --git a/v.txt b/v.txt\n+verified content\n');
      const verifiedDiff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: verifiedBytes,
        integrity: 'VERIFIED',
      });

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, {
        studioId: validStudioId,
        taskId: validTaskId,
      });
      panel.setDiffData(verifiedDiff, panel.activeRequestId);

      const targetPath = path.join(tempDir, 'seamless_verified.patch');
      mockWindowMessages.saveDialogResult = vscode.Uri.file(targetPath);

      await (panel as unknown as { _handleExportPatch: () => Promise<void> })._handleExportPatch();

      // No warning modal prompt triggered
      assert.strictEqual(mockWindowMessages.warning.length, 0);
      assert.strictEqual(fs.existsSync(targetPath), true);
      assert.deepStrictEqual(fs.readFileSync(targetPath), verifiedBytes);
    });
  });

  // =========================================================================
  // 8. Phase E Security Hardening Invariants (M15 UI-05A)
  // =========================================================================
  describe('8. Phase E Security Hardening Invariants (M15 UI-05A)', () => {
    it('8.1 malformed SHA-256 hash triggers INTEGRITY_FAILED and blocks export', async () => {
      const payload = Buffer.from('diff --git a/file.txt b/file.txt\n+line\n', 'utf8');
      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: validStudioId,
            task_id: validTaskId,
            summary: {
              files_changed: 1,
              added_files: ['file.txt'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: payload.length,
              base_commit: 'abc',
            },
            patch: '',
            offset: 0,
            limit: 1024,
            total_bytes: payload.length,
            has_more: false,
            patch_base64: payload.toString('base64'),
            encoding: 'base64',
            patch_sha256: 'malformed_not_64_hex_digits',
            chunk_bytes: payload.length,
          },
        }),
      };

      const diff = await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);
      assert.strictEqual(diff.integrity, 'INTEGRITY_FAILED');

      const provider = new WorkbenchViewProvider();
      assert.strictEqual(provider.canExport(diff), false);

      const targetPath = path.join(tempDir, 'blocked_malformed.patch');
      await assert.rejects(
        async () => {
          await provider.exportPatch(diff, targetPath);
        },
        /Export blocked: Cryptographic SHA-256 integrity verification failed/
      );
      assert.strictEqual(fs.existsSync(targetPath), false);
    });

    it('8.2 tampered SHA-256 hash triggers INTEGRITY_FAILED and blocks export', async () => {
      const payload = Buffer.from('diff --git a/code.ts b/code.ts\n+code\n', 'utf8');
      const tamperedHex64 = '0000000000000000000000000000000000000000000000000000000000000000';
      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: validStudioId,
            task_id: validTaskId,
            summary: {
              files_changed: 1,
              added_files: ['code.ts'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: payload.length,
              base_commit: 'abc',
            },
            patch: '',
            offset: 0,
            limit: 1024,
            total_bytes: payload.length,
            has_more: false,
            patch_base64: payload.toString('base64'),
            encoding: 'base64',
            patch_sha256: tamperedHex64,
            chunk_bytes: payload.length,
          },
        }),
      };

      const diff = await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId);
      assert.strictEqual(diff.integrity, 'INTEGRITY_FAILED');

      const provider = new WorkbenchViewProvider();
      assert.strictEqual(provider.canExport(diff), false);

      const targetPath = path.join(tempDir, 'blocked_tampered.patch');
      await assert.rejects(
        async () => {
          await provider.exportPatch(diff, targetPath);
        },
        /Export blocked: Cryptographic SHA-256 integrity verification failed/
      );
    });

    it('8.3 path traversal in diff headers rejected when rejectTraversal is true', () => {
      const traversalHeader = 'diff --git "a/../../etc/passwd" "b/../../etc/passwd"';
      const rejectedHeader = parseDiffGitHeader(traversalHeader, { rejectTraversal: true });
      assert.strictEqual(rejectedHeader, null);

      const traversalDiff = [
        'diff --git "a/../../../etc/passwd" "b/../../../etc/passwd"',
        '--- a/../../../etc/passwd',
        '+++ b/../../../etc/passwd',
        '@@ -1,1 +1,1 @@',
        '+root:0:0:',
      ].join('\n');

      const rejectedParsed = DiffParser.parseUnifiedDiff(traversalDiff, { rejectTraversal: true });
      assert.strictEqual(rejectedParsed.files.length, 0);

      // Windows traversal header
      const winTraversalHeader = 'diff --git "a/C:\\secret.txt" "b/C:\\secret.txt"';
      const winRejected = parseDiffGitHeader(winTraversalHeader, { rejectTraversal: true });
      assert.strictEqual(winRejected, null);

      // Sanitization safely eliminates traversal segments
      const sanitized = sanitizePath('../../var/log/../../etc/passwd');
      assert.strictEqual(sanitized, 'passwd');
      assert.strictEqual(isPathTraversal(sanitized), false);
    });

    it('8.4 path traversal in studioId, taskId, or artifactId triggers PATH_TRAVERSAL_DETECTED', async () => {
      const dummyTransport: IpcTransportSender = {
        sendCommand: async () => ({ type: 'error', code: 'UNEXPECTED', message: 'Should not reach' }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(dummyTransport, '../rogue_studio', validTaskId);
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'PATH_TRAVERSAL_DETECTED');
          return true;
        }
      );

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(dummyTransport, validStudioId, '..\\..\\Windows\\System32');
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'PATH_TRAVERSAL_DETECTED');
          return true;
        }
      );

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchArtifactContent(dummyTransport, validStudioId, '/etc/passwd');
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'PATH_TRAVERSAL_DETECTED');
          return true;
        }
      );

      assert.strictEqual(sanitizeArtifactId('../../../etc/passwd'), 'passwd');
      assert.strictEqual(sanitizeArtifactId('C:\\Windows\\System32\\cmd.exe'), 'Windows_System32_cmd.exe');
    });

    it('8.5 accurate Vietnamese diacritic octal decoding for complex diacritics and mixed UTF-8', () => {
      // ệ = \341\273\207
      assert.strictEqual(unquoteGitPath('"\\341\\273\\207"'), 'ệ');
      // ắ = \341\272\257
      assert.strictEqual(unquoteGitPath('"\\341\\272\\257"'), 'ắ');
      // ễ = \341\273\205
      assert.strictEqual(unquoteGitPath('"\\341\\273\\205"'), 'ễ');
      // ỗ = \341\273\227
      assert.strictEqual(unquoteGitPath('"\\341\\273\\227"'), 'ỗ');
      // ở = \341\273\237
      assert.strictEqual(unquoteGitPath('"\\341\\273\\237"'), 'ở');
      // ự = \341\273\261
      assert.strictEqual(unquoteGitPath('"\\341\\273\\261"'), 'ự');
      // đ = \304\221
      assert.strictEqual(unquoteGitPath('"\\304\\221"'), 'đ');

      // Mixed raw UTF-8 string with embedded octal escapes
      const mixedString = '"báo_cáo_\\341\\273\\207_hệ_thống.txt"';
      assert.strictEqual(unquoteGitPath(mixedString), 'báo_cáo_ệ_hệ_thống.txt');
    });

    it('8.6 memory limits enforced upfront when declared total_bytes exceeds maxBytes', async () => {
      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => ({
          type: 'task_diff',
          diff: {
            studio_id: validStudioId,
            task_id: validTaskId,
            summary: {
              files_changed: 1,
              added_files: ['huge.bin'],
              modified_files: [],
              deleted_files: [],
              renamed_files: [],
              binary_files: [],
              patch_size_bytes: 200 * 1024 * 1024,
              base_commit: 'abc',
            },
            patch: '',
            offset: 0,
            limit: 1024,
            total_bytes: 150 * 1024 * 1024, // 150MB > 100MB max
            has_more: true,
            patch_base64: Buffer.from('data').toString('base64'),
            encoding: 'base64',
            patch_sha256: null,
            chunk_bytes: 4,
          },
        }),
      };

      await assert.rejects(
        async () => {
          await ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId, {
            maxBytes: 100 * 1024 * 1024,
          });
        },
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'EXCEEDED_MAX_BYTES');
          return true;
        }
      );
    });

    it('8.7 AbortController terminates in-flight stream cleanly', async () => {
      const abortController = new AbortController();

      const fakeTransport: IpcTransportSender = {
        sendCommand: async (): Promise<RustIpcResponse> => {
          // Delay simulating network transport latency
          await new Promise((resolve) => setTimeout(resolve, 50));
          return {
            type: 'task_diff',
            diff: {
              studio_id: validStudioId,
              task_id: validTaskId,
              summary: {
                files_changed: 0,
                added_files: [],
                modified_files: [],
                deleted_files: [],
                renamed_files: [],
                binary_files: [],
                patch_size_bytes: 0,
                base_commit: 'abc',
              },
              patch: '',
              offset: 0,
              limit: 1024,
              total_bytes: 0,
              has_more: false,
              patch_base64: '',
              encoding: 'base64',
              patch_sha256: null,
              chunk_bytes: 0,
            },
          };
        },
      };

      // Trigger abort while in-flight
      const fetchPromise = ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId, {
        signal: abortController.signal,
      });

      // Abort after 10ms
      setTimeout(() => abortController.abort(), 10);

      await assert.rejects(
        fetchPromise,
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'ABORTED');
          assert.ok(err.message.includes('aborted by client'));
          return true;
        }
      );

      // Pre-aborted signal check
      const preAborted = new AbortController();
      preAborted.abort();
      await assert.rejects(
        ArtifactClient.fetchTaskDiff(fakeTransport, validStudioId, validTaskId, {
          signal: preAborted.signal,
        }),
        (err: ArtifactRetrievalError) => {
          assert.strictEqual(err.code, 'ABORTED');
          return true;
        }
      );
    });

    it('8.8 studio switch invalidates active artifact cache', () => {
      const diff1 = buildDiffFixture({
        studioId: 'studio-alpha',
        taskId: 'task-101',
        rawBuffer: Buffer.from('diff alpha'),
      });
      const diff2 = buildDiffFixture({
        studioId: 'studio-beta',
        taskId: 'task-202',
        rawBuffer: Buffer.from('diff beta'),
      });

      ArtifactClient.cacheTaskDiff(diff1);
      ArtifactClient.cacheTaskDiff(diff2);

      assert.ok(ArtifactClient.getCachedTaskDiff('studio-alpha', 'task-101') !== undefined);
      assert.ok(ArtifactClient.getCachedTaskDiff('studio-beta', 'task-202') !== undefined);

      const workbench = new WorkbenchViewProvider();
      workbench.setStudio('studio-alpha');
      // Switching studio to studio-gamma must invalidate studio-alpha cache
      workbench.setStudio('studio-gamma');

      assert.strictEqual(ArtifactClient.getCachedTaskDiff('studio-alpha', 'task-101'), undefined);
      // studio-beta remains intact
      assert.ok(ArtifactClient.getCachedTaskDiff('studio-beta', 'task-202') !== undefined);

      // Invalidate studio-beta
      ArtifactClient.invalidateCache('studio-beta');
      assert.strictEqual(ArtifactClient.getCachedTaskDiff('studio-beta', 'task-202'), undefined);
    });

    it('8.9 WorkbenchViewProvider preserves CRLF line endings byte-for-byte and rejects traversal export path', async () => {
      const crlfBuffer = Buffer.from('diff --git a/crlf.txt b/crlf.txt\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n');
      const sha256 = crypto.createHash('sha256').update(crlfBuffer).digest('hex').toLowerCase();
      const diff = buildDiffFixture({
        studioId: validStudioId,
        taskId: validTaskId,
        rawBuffer: crlfBuffer,
        expectedSha256: sha256,
        integrity: 'VERIFIED',
      });

      const provider = new WorkbenchViewProvider();
      const exportFile = path.join(tempDir, 'crlf_verified.patch');
      await provider.exportPatch(diff, exportFile);

      const writtenBytes = fs.readFileSync(exportFile);
      assert.deepStrictEqual(writtenBytes, crlfBuffer);
      assert.ok(writtenBytes.includes(Buffer.from('\r\n')));

      // Path traversal export path rejected
      await assert.rejects(
        async () => {
          await provider.exportPatch(diff, '../../../../etc/escape.patch');
        },
        /Export blocked: Target path contains directory traversal sequence/
      );
    });
  });
});
