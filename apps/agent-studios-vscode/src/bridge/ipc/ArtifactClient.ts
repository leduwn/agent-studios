/**
 * Agent Studios — Lossless Binary-Safe Artifact & Diff Client
 * Milestone M15 UI-03: Lossless Pagination & Cryptographic Integrity
 *
 * Implements byte-accurate streaming reassembly for unified patches and binary artifacts:
 * - Base64 chunk decoding into Buffer/Uint8Array (prevents UTF-8 slicing corruption)
 * - Strict chunk offset & chunk_bytes validation
 * - Anti-infinite loop zero-progress detection
 * - Aggregate memory and chunk count bounds (Anti-DoS)
 * - True cryptographic Node.js crypto SHA-256 calculation in trusted Extension Host
 * - Detection of binary Git patches and multi-byte Unicode text
 * - AbortSignal cancellation support
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as crypto from 'crypto';
import {
  RustArtifactRecord,
  RustDiffSummary,
  RustIpcCommand,
  RustIpcResponse,
} from './RustIpcProtocolTypes';

export type IntegrityStatus = 'VERIFIED' | 'INTEGRITY_FAILED' | 'UNVERIFIED';

export interface ReassembledTaskDiff {
  studioId: string;
  taskId: string;
  artifactId?: string | null;
  baseCommit?: string | null;
  headCommit?: string | null;
  summary: RustDiffSummary;
  rawBuffer: Buffer;
  text: string;
  isBinary: boolean;
  sha256: string;
  expectedSha256?: string | null;
  integrity: IntegrityStatus;
  totalBytes: number;
  chunkCount: number;
}

export interface ReassembledArtifactContent {
  studioId: string;
  artifactId: string;
  artifact?: RustArtifactRecord | null;
  rawBuffer: Buffer;
  text?: string;
  isBinary: boolean;
  sha256: string;
  expectedSha256?: string | null;
  integrity: IntegrityStatus;
  totalBytes: number;
  chunkCount: number;
}

export class ArtifactRetrievalError extends Error {
  constructor(
    message: string,
    public readonly code:
      | 'OFFSET_MISMATCH'
      | 'CHUNK_BYTES_MISMATCH'
      | 'ZERO_PROGRESS'
      | 'ENCODING_CHANGED'
      | 'TOTAL_BYTES_CHANGED'
      | 'ARTIFACT_IDENTITY_CHANGED'
      | 'HASH_CHANGED'
      | 'EXCEEDED_MAX_BYTES'
      | 'EXCEEDED_MAX_CHUNKS'
      | 'ABORTED'
      | 'SESSION_CHANGED'
      | 'TRUNCATED_DOWNLOAD'
      | 'INTEGRITY_FAILED'
      | 'IPC_ERROR'
  ) {
    super(message);
    this.name = 'ArtifactRetrievalError';
  }
}

export interface FetchOptions {
  chunkSize?: number; // default 1 MiB (1048576)
  maxBytes?: number;  // default 16 MiB (16777216)
  maxChunks?: number; // default 1024
  signal?: AbortSignal;
  sessionId?: string | null;
}

export interface IpcTransportSender {
  sendCommand: (cmd: RustIpcCommand, timeoutMs?: number) => Promise<RustIpcResponse>;
  getSessionId?: () => string | null;
}

export class ArtifactClient {
  public static readonly DEFAULT_CHUNK_SIZE = 1024 * 1024; // 1 MiB
  public static readonly DEFAULT_MAX_BYTES = 16 * 1024 * 1024; // 16 MiB
  public static readonly DEFAULT_MAX_CHUNKS = 1024;

  /**
   * Fetches and lossless-reassembles the complete task diff across paginated chunks.
   * Performs byte-accurate SHA-256 integrity verification.
   */
  public static async fetchTaskDiff(
    transport: IpcTransportSender,
    studioId: string,
    taskId: string,
    options?: FetchOptions
  ): Promise<ReassembledTaskDiff> {
    const chunkSize = options?.chunkSize ?? this.DEFAULT_CHUNK_SIZE;
    const maxBytes = options?.maxBytes ?? this.DEFAULT_MAX_BYTES;
    const maxChunks = options?.maxChunks ?? this.DEFAULT_MAX_CHUNKS;
    const signal = options?.signal;
    const initialSessionId = options?.sessionId ?? transport.getSessionId?.() ?? null;

    let expectedOffset = 0;
    let accumulatedBytes = 0;
    const chunks: Buffer[] = [];
    let expectedSha256: string | null = null;
    let totalBytes: number | null = null;
    let summary: RustDiffSummary | null = null;
    let artifactId: string | null = null;
    let baseCommit: string | null = null;
    let headCommit: string | null = null;
    let encoding: string | null = null;
    let loopCount = 0;

    while (true) {
      if (signal?.aborted) {
        throw new ArtifactRetrievalError('Task diff download was aborted by client.', 'ABORTED');
      }

      if (initialSessionId && transport.getSessionId) {
        const currentSession = transport.getSessionId();
        if (currentSession && currentSession !== initialSessionId) {
          throw new ArtifactRetrievalError(
            'Daemon transport session reconnected during artifact retrieval.',
            'SESSION_CHANGED'
          );
        }
      }

      const resp = await transport.sendCommand({
        type: 'get_task_diff',
        studio_id: studioId,
        task_id: taskId,
        offset: expectedOffset,
        limit: chunkSize,
      });

      if (resp.type === 'error') {
        throw new ArtifactRetrievalError(`Daemon rejected get_task_diff: ${resp.message}`, 'IPC_ERROR');
      }

      if (resp.type !== 'task_diff') {
        throw new ArtifactRetrievalError(`Unexpected response from get_task_diff: ${JSON.stringify(resp)}`, 'IPC_ERROR');
      }

      const diff = resp.diff;

      // 1. Validate response offset
      if (diff.offset !== expectedOffset) {
        throw new ArtifactRetrievalError(
          `Chunk offset mismatch: expected offset ${expectedOffset}, but daemon returned ${diff.offset}.`,
          'OFFSET_MISMATCH'
        );
      }

      // 2. Decode raw byte buffer
      let chunkBuffer: Buffer;
      if (diff.patch_base64 !== undefined && diff.patch_base64 !== null) {
        const chunkEncoding = diff.encoding ?? 'base64';
        if (encoding === null) {
          encoding = chunkEncoding;
        } else if (encoding !== chunkEncoding) {
          throw new ArtifactRetrievalError(
            `Inconsistent chunk encoding: was '${encoding}' but received '${chunkEncoding}'.`,
            'ENCODING_CHANGED'
          );
        }
        chunkBuffer = Buffer.from(diff.patch_base64, 'base64');
      } else {
        // Fallback for older daemons
        chunkBuffer = Buffer.from(diff.patch || '', 'utf8');
      }

      // 3. Validate chunk_bytes if provided
      if (diff.chunk_bytes !== undefined && diff.chunk_bytes !== null) {
        if (diff.chunk_bytes !== chunkBuffer.length) {
          throw new ArtifactRetrievalError(
            `chunk_bytes metadata mismatch: report stated ${diff.chunk_bytes} bytes, but decoded ${chunkBuffer.length} bytes.`,
            'CHUNK_BYTES_MISMATCH'
          );
        }
      }

      // 4. Anti-infinite loop zero progress detection
      if (chunkBuffer.length === 0 && diff.has_more) {
        throw new ArtifactRetrievalError(
          `Zero-progress chunk received at offset ${expectedOffset} while has_more=true. Aborting infinite pagination loop.`,
          'ZERO_PROGRESS'
        );
      }

      // 5. Invariant consistency across chunks
      if (totalBytes === null) {
        totalBytes = diff.total_bytes;
        summary = diff.summary;
        artifactId = diff.artifact_id ?? null;
        baseCommit = diff.base_commit ?? null;
        headCommit = diff.head_commit ?? null;
      } else if (totalBytes !== diff.total_bytes) {
        throw new ArtifactRetrievalError(
          `total_bytes changed during retrieval: was ${totalBytes}, now ${diff.total_bytes}.`,
          'TOTAL_BYTES_CHANGED'
        );
      }

      if (expectedSha256 === null) {
        expectedSha256 = diff.patch_sha256 ?? null;
      } else if (diff.patch_sha256 && expectedSha256 !== diff.patch_sha256) {
        throw new ArtifactRetrievalError(
          `patch_sha256 changed during retrieval: was '${expectedSha256}', now '${diff.patch_sha256}'.`,
          'HASH_CHANGED'
        );
      }

      // 6. Memory and chunk bounds
      accumulatedBytes += chunkBuffer.length;
      if (accumulatedBytes > maxBytes) {
        throw new ArtifactRetrievalError(
          `Diff retrieval exceeded maximum aggregate memory limit (${accumulatedBytes} > ${maxBytes} bytes).`,
          'EXCEEDED_MAX_BYTES'
        );
      }

      loopCount++;
      if (loopCount > maxChunks) {
        throw new ArtifactRetrievalError(
          `Diff retrieval exceeded maximum allowed chunk count (${loopCount} > ${maxChunks}).`,
          'EXCEEDED_MAX_CHUNKS'
        );
      }

      chunks.push(chunkBuffer);
      expectedOffset += chunkBuffer.length;

      if (!diff.has_more || (totalBytes !== null && expectedOffset >= totalBytes)) {
        break;
      }
    }

    // 7. Verify total assembled bytes
    if (totalBytes !== null && expectedOffset < totalBytes) {
      throw new ArtifactRetrievalError(
        `Truncated diff download: expected ${totalBytes} bytes, but only received ${expectedOffset} bytes.`,
        'TRUNCATED_DOWNLOAD'
      );
    }

    const rawBuffer = Buffer.concat(chunks);

    // 8. Compute genuine SHA-256 digest
    const sha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex').toLowerCase();

    // 9. Evaluate cryptographic integrity
    let integrity: IntegrityStatus = 'UNVERIFIED';
    if (expectedSha256) {
      const normalizedExpected = expectedSha256.startsWith('sha256:')
        ? expectedSha256.slice(7).toLowerCase()
        : expectedSha256.toLowerCase();

      if (sha256 === normalizedExpected) {
        integrity = 'VERIFIED';
      } else {
        integrity = 'INTEGRITY_FAILED';
      }
    }

    // 10. Detect binary patch
    const isBinary =
      rawBuffer.includes(Buffer.from('GIT binary patch')) ||
      rawBuffer.includes(Buffer.from('\0'));

    // 11. Decode text without multi-byte slicing corruption
    const decoder = new TextDecoder('utf-8', { fatal: false });
    const text = isBinary ? '' : decoder.decode(rawBuffer);

    return {
      studioId,
      taskId,
      artifactId,
      baseCommit,
      headCommit,
      summary: summary || {
        files_changed: 0,
        added_files: [],
        modified_files: [],
        deleted_files: [],
        renamed_files: [],
        binary_files: [],
        patch_size_bytes: 0,
        base_commit: '',
      },
      rawBuffer,
      text,
      isBinary,
      sha256,
      expectedSha256,
      integrity,
      totalBytes: rawBuffer.length,
      chunkCount: chunks.length,
    };
  }

  /**
   * Fetches and lossless-reassembles a specific artifact's binary/text content.
   */
  public static async fetchArtifactContent(
    transport: IpcTransportSender,
    studioId: string,
    artifactId: string,
    options?: FetchOptions
  ): Promise<ReassembledArtifactContent> {
    const chunkSize = options?.chunkSize ?? this.DEFAULT_CHUNK_SIZE;
    const maxBytes = options?.maxBytes ?? this.DEFAULT_MAX_BYTES;
    const maxChunks = options?.maxChunks ?? this.DEFAULT_MAX_CHUNKS;
    const signal = options?.signal;
    const initialSessionId = options?.sessionId ?? transport.getSessionId?.() ?? null;

    let expectedOffset = 0;
    let accumulatedBytes = 0;
    const chunks: Buffer[] = [];
    let expectedSha256: string | null = null;
    let totalBytes: number | null = null;
    let artifactRecord: RustArtifactRecord | null = null;
    let loopCount = 0;

    while (true) {
      if (signal?.aborted) {
        throw new ArtifactRetrievalError('Artifact download was aborted by client.', 'ABORTED');
      }

      if (initialSessionId && transport.getSessionId) {
        const currentSession = transport.getSessionId();
        if (currentSession && currentSession !== initialSessionId) {
          throw new ArtifactRetrievalError(
            'Daemon transport session reconnected during artifact retrieval.',
            'SESSION_CHANGED'
          );
        }
      }

      const resp = await transport.sendCommand({
        type: 'get_artifact',
        studio_id: studioId,
        artifact_id: artifactId,
        offset: expectedOffset,
        limit: chunkSize,
      });

      if (resp.type === 'error') {
        throw new ArtifactRetrievalError(`Daemon rejected get_artifact: ${resp.message}`, 'IPC_ERROR');
      }

      if (resp.type !== 'artifact') {
        throw new ArtifactRetrievalError(`Unexpected response from get_artifact: ${JSON.stringify(resp)}`, 'IPC_ERROR');
      }

      // Check offset
      const respOffset = resp.offset ?? 0;
      if (respOffset !== expectedOffset) {
        throw new ArtifactRetrievalError(
          `Chunk offset mismatch: expected ${expectedOffset}, but daemon returned ${respOffset}.`,
          'OFFSET_MISMATCH'
        );
      }

      if (artifactRecord === null && resp.artifact) {
        artifactRecord = resp.artifact;
      }

      // Decode buffer
      let chunkBuffer: Buffer;
      if (resp.content_base64 !== undefined && resp.content_base64 !== null) {
        chunkBuffer = Buffer.from(resp.content_base64, 'base64');
      } else {
        chunkBuffer = Buffer.from(resp.content || '', 'utf8');
      }

      if (chunkBuffer.length === 0 && resp.has_more) {
        throw new ArtifactRetrievalError('Zero progress pagination loop detected.', 'ZERO_PROGRESS');
      }

      if (totalBytes === null && resp.total_bytes !== undefined && resp.total_bytes !== null) {
        totalBytes = resp.total_bytes;
      } else if (
        totalBytes !== null &&
        resp.total_bytes !== undefined &&
        resp.total_bytes !== null &&
        totalBytes !== resp.total_bytes
      ) {
        throw new ArtifactRetrievalError(
          `total_bytes changed during retrieval: was ${totalBytes}, now ${resp.total_bytes}.`,
          'TOTAL_BYTES_CHANGED'
        );
      }

      if (expectedSha256 === null) {
        expectedSha256 = resp.sha256 ?? artifactRecord?.content_hash ?? null;
      } else if (resp.sha256 && expectedSha256 !== resp.sha256) {
        throw new ArtifactRetrievalError('sha256 changed during retrieval.', 'HASH_CHANGED');
      }

      accumulatedBytes += chunkBuffer.length;
      if (accumulatedBytes > maxBytes) {
        throw new ArtifactRetrievalError(`Exceeded maximum allowed bytes (${accumulatedBytes} > ${maxBytes}).`, 'EXCEEDED_MAX_BYTES');
      }

      loopCount++;
      if (loopCount > maxChunks) {
        throw new ArtifactRetrievalError(`Exceeded maximum chunk count (${loopCount} > ${maxChunks}).`, 'EXCEEDED_MAX_CHUNKS');
      }

      chunks.push(chunkBuffer);
      expectedOffset += chunkBuffer.length;

      if (!resp.has_more || (totalBytes !== null && expectedOffset >= totalBytes)) {
        break;
      }
    }

    if (totalBytes !== null && expectedOffset < totalBytes) {
      throw new ArtifactRetrievalError(
        `Truncated artifact download: expected ${totalBytes} bytes, but only received ${expectedOffset} bytes.`,
        'TRUNCATED_DOWNLOAD'
      );
    }

    const rawBuffer = Buffer.concat(chunks);
    const sha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex').toLowerCase();

    let integrity: IntegrityStatus = 'UNVERIFIED';
    if (expectedSha256) {
      const normalizedExpected = expectedSha256.startsWith('sha256:')
        ? expectedSha256.slice(7).toLowerCase()
        : expectedSha256.toLowerCase();

      if (sha256 === normalizedExpected) {
        integrity = 'VERIFIED';
      } else {
        integrity = 'INTEGRITY_FAILED';
      }
    }

    const isBinary = rawBuffer.includes(Buffer.from('\0'));
    const text = isBinary ? undefined : new TextDecoder('utf-8', { fatal: false }).decode(rawBuffer);

    return {
      studioId,
      artifactId,
      artifact: artifactRecord,
      rawBuffer,
      text,
      isBinary,
      sha256,
      expectedSha256,
      integrity,
      totalBytes: rawBuffer.length,
      chunkCount: chunks.length,
    };
  }

  /**
   * Queries list of artifact records for a given task.
   */
  public static async listTaskArtifacts(
    transport: IpcTransportSender,
    studioId: string,
    taskId: string
  ): Promise<RustArtifactRecord[]> {
    const resp = await transport.sendCommand({
      type: 'list_task_artifacts',
      studio_id: studioId,
      task_id: taskId,
    });

    if (resp.type === 'error') {
      throw new ArtifactRetrievalError(`Daemon rejected list_task_artifacts: ${resp.message}`, 'IPC_ERROR');
    }

    if (resp.type !== 'task_artifacts') {
      throw new ArtifactRetrievalError(`Unexpected response from list_task_artifacts: ${JSON.stringify(resp)}`, 'IPC_ERROR');
    }

    return resp.artifacts;
  }
}
