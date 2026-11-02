/**
 * Agent Studios — Session Lifecycle, Recovery, and Async Hardening Tests
 * Milestone M15 UI-04: Robust Task Switching, Stream Ordering, Disconnect Recovery & Clean Disposal
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as net from 'net';
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
  ArtifactReviewOptions,
  ArtifactReviewPanel,
} from '../../webviews/ArtifactReviewPanel';
import {
  SequenceGapDetails,
  SessionStateProjection,
} from '../../state/SessionStateProjection';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import { NativeIpcTransport } from '../../bridge/ipc/NativeIpcTransport';
import {
  RustIpcCommand,
  RustIpcResponse,
} from '../../bridge/ipc/RustIpcProtocolTypes';
import { OperatorConfirmationManager } from '../../bridge/ipc/CapabilityMatrix';
import { TimelineEvent } from '../../bridge/types';
import { resetMockState } from './vscodeMock';

describe('Session Lifecycle, Recovery & Hardening Tests (M15 UI-04)', () => {
  const extensionUri = vscode.Uri.file('/mock/extension');
  let tempDir: string;
  let server: net.Server | null = null;
  let endpoint: string;
  const keyId = 'test-client-key';
  const clientKey = 'secret-client-key-1234567890abcdef';

  beforeEach(async () => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    TaskExecutionStore.instance.reset();
    OperatorConfirmationManager.clearPendingChallenges();
    resetMockState();

    tempDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'ags-lifecycle-test-'));
    if (process.platform === 'win32') {
      const pipeId = Math.random().toString(36).substring(2, 10);
      endpoint = `\\\\.\\pipe\\ags-lifecycle-${pipeId}`;
    } else {
      endpoint = path.join(tempDir, 'lifecycle.sock');
    }
  });

  afterEach(async () => {
    if (ArtifactReviewPanel.currentPanel) {
      ArtifactReviewPanel.currentPanel.dispose();
    }
    TaskExecutionStore.instance.reset();
    OperatorConfirmationManager.clearPendingChallenges();
    resetMockState();

    if (server) {
      await new Promise<void>((resolve) => server!.close(() => resolve()));
      server = null;
    }

    try {
      await fs.promises.rm(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore temporary directory cleanup errors
    }
  });

  function createMockDiff(options: {
    studioId: string;
    taskId: string;
    rawText: string;
    integrity?: 'VERIFIED' | 'UNVERIFIED' | 'INTEGRITY_FAILED';
    isBinary?: boolean;
  }): ReassembledTaskDiff {
    const rawBuffer = Buffer.from(options.rawText, 'utf8');
    const sha256 = crypto.createHash('sha256').update(rawBuffer).digest('hex');

    return {
      studioId: options.studioId,
      taskId: options.taskId,
      summary: {
        files_changed: 1,
        added_files: ['file.txt'],
        modified_files: [],
        deleted_files: [],
        renamed_files: [],
        binary_files: options.isBinary ? ['file.txt'] : [],
        patch_size_bytes: rawBuffer.length,
        base_commit: 'abc1234',
      },
      rawBuffer,
      text: options.rawText,
      isBinary: !!options.isBinary,
      sha256,
      expectedSha256: options.integrity === 'UNVERIFIED' ? null : sha256,
      integrity: options.integrity || 'VERIFIED',
      totalBytes: rawBuffer.length,
      chunkCount: 1,
    };
  }

  function startMockDaemon(
    onRequest: (cmd: RustIpcCommand, sendResponse: (resp: unknown) => void, rawSocket: net.Socket) => void
  ): Promise<net.Server> {
    return new Promise((resolve) => {
      server = net.createServer((socket) => {
        let buffer = Buffer.alloc(0);
        socket.on('data', (chunk) => {
          buffer = Buffer.concat([buffer, chunk]);
          while (buffer.length >= 4) {
            const length = buffer.readUInt32BE(0);
            if (buffer.length < 4 + length) {
              break;
            }
            const jsonStr = buffer.toString('utf8', 4, 4 + length);
            buffer = buffer.subarray(4 + length);
            const cmd = JSON.parse(jsonStr) as RustIpcCommand;

            const sendResponse = (resp: unknown) => {
              const respBytes = Buffer.from(JSON.stringify(resp), 'utf8');
              const header = Buffer.alloc(4);
              header.writeUInt32BE(respBytes.length, 0);
              socket.write(Buffer.concat([header, respBytes]));
            };

            onRequest(cmd, sendResponse, socket);
          }
        });
      });

      server.listen(endpoint, () => {
        resolve(server!);
      });
    });
  }

  // =========================================================================
  // 1. EVENT STREAM ORDERING & DEDUPLICATION
  // =========================================================================
  describe('1. Event Stream Ordering & Deduplication', () => {
    it('processes monotonic sequential events under nominal health', () => {
      const projection = new SessionStateProjection();

      const events: TimelineEvent[] = [
        { sequenceNumber: 1, id: 'e1', timestamp: 1000, type: 'session_started', summary: 'Session 1' },
        { sequenceNumber: 2, id: 'e2', timestamp: 2000, type: 'run_started', summary: 'Run 1' },
        { sequenceNumber: 3, id: 'e3', timestamp: 3000, type: 'task_started', summary: 'Task 1', taskId: 't-1' },
      ];

      for (const evt of events) {
        projection.handleEvent(evt);
      }

      assert.strictEqual(projection.healthState, 'nominal');
      assert.strictEqual(projection.isDegraded, false);
      assert.strictEqual(projection.lastProcessedSequence, 3);
      assert.strictEqual(projection.timeline.length, 3);
      assert.strictEqual(projection.timeline[0].sequenceNumber, 1);
      assert.strictEqual(projection.timeline[1].sequenceNumber, 2);
      assert.strictEqual(projection.timeline[2].sequenceNumber, 3);

      projection.dispose();
    });

    it('detects sequence gaps, transitions to degraded state, and fires onStateDegraded', () => {
      const projection = new SessionStateProjection();
      const degradationRecords: SequenceGapDetails[] = [];
      projection.onStateDegraded((details) => degradationRecords.push(details));

      // Event 1 arrives
      projection.handleEvent({
        sequenceNumber: 1,
        id: 'e1',
        timestamp: 1000,
        type: 'session_started',
        summary: 'Session 1',
      });
      assert.strictEqual(projection.healthState, 'nominal');
      assert.strictEqual(projection.lastProcessedSequence, 1);

      // Event 5 arrives (skipping 2, 3, 4)
      projection.handleEvent({
        sequenceNumber: 5,
        id: 'e5',
        timestamp: 5000,
        type: 'task_started',
        summary: 'Task 5',
        taskId: 't-5',
      });

      assert.strictEqual(projection.healthState, 'degraded');
      assert.strictEqual(projection.isDegraded, true);
      assert.strictEqual(projection.lastProcessedSequence, 5);
      assert.strictEqual(degradationRecords.length, 1);
      assert.deepStrictEqual(degradationRecords[0], { expected: 2, received: 5 });

      // Can manually reset degraded health state
      projection.resetHealthState();
      assert.strictEqual(projection.healthState, 'nominal');
      assert.strictEqual(projection.isDegraded, false);

      projection.dispose();
    });

    it('rejects out-of-order and stale sequence numbers (seq <= lastProcessedSequence)', () => {
      const projection = new SessionStateProjection();

      // Seed with sequence 10
      projection.handleEvent({
        sequenceNumber: 10,
        id: 'e10',
        timestamp: 10000,
        type: 'task_started',
        summary: 'Task 10',
        taskId: 't-10',
      });
      assert.strictEqual(projection.lastProcessedSequence, 10);
      assert.strictEqual(projection.timeline.length, 1);

      // Attempt to deliver late events with seq 3, 8, 10
      projection.handleEvent({
        sequenceNumber: 3,
        id: 'e3-late',
        timestamp: 3000,
        type: 'task_started',
        summary: 'Late Task 3',
        taskId: 't-3',
      });
      projection.handleEvent({
        sequenceNumber: 8,
        id: 'e8-late',
        timestamp: 8000,
        type: 'task_started',
        summary: 'Late Task 8',
        taskId: 't-8',
      });
      projection.handleEvent({
        sequenceNumber: 10,
        id: 'e10-dup',
        timestamp: 10050,
        type: 'task_started',
        summary: 'Duplicate Task 10',
        taskId: 't-10',
      });

      // None of the stale or duplicate events should be added or mutate watermark
      assert.strictEqual(projection.lastProcessedSequence, 10);
      assert.strictEqual(projection.timeline.length, 1);
      assert.strictEqual(projection.timeline[0].id, 'e10');

      projection.dispose();
    });

    it('seeds historical events in arbitrary order, sorts them, and advances watermark without degradation', () => {
      const projection = new SessionStateProjection();
      let degradationFired = false;
      projection.onStateDegraded(() => {
        degradationFired = true;
      });

      const unseededEvents: TimelineEvent[] = [
        { sequenceNumber: 8, id: 'e8', timestamp: 8000, type: 'task_completed', summary: 'Task 8', taskId: 't-1' },
        { sequenceNumber: 2, id: 'e2', timestamp: 2000, type: 'session_started', summary: 'Session' },
        { sequenceNumber: 5, id: 'e5', timestamp: 5000, type: 'task_started', summary: 'Task 5', taskId: 't-1' },
      ];

      projection.seedEvents(unseededEvents);

      assert.strictEqual(degradationFired, false);
      assert.strictEqual(projection.healthState, 'nominal');
      assert.strictEqual(projection.lastProcessedSequence, 8);
      assert.strictEqual(projection.timeline.length, 3);
      assert.strictEqual(projection.timeline[0].sequenceNumber, 2);
      assert.strictEqual(projection.timeline[1].sequenceNumber, 5);
      assert.strictEqual(projection.timeline[2].sequenceNumber, 8);

      // Reseeding identical sequence numbers skips duplicates
      projection.seedEvents([
        { sequenceNumber: 5, id: 'e5-dup', timestamp: 5001, type: 'task_started', summary: 'Dup' },
      ]);
      assert.strictEqual(projection.timeline.length, 3);

      projection.dispose();
    });
  });

  // =========================================================================
  // 2. LATE EVENTS ARRIVING AFTER ACTIVE TASK SWITCHING
  // =========================================================================
  describe('2. Late Events Arriving After Active Task Switching', () => {
    it('discards late diff resolutions for previous task without polluting new view', async () => {
      let resolveTaskAlpha: ((diff: ReassembledTaskDiff) => void) | null = null;
      let resolveTaskBeta: ((diff: ReassembledTaskDiff) => void) | null = null;

      const loadTaskAlpha = () =>
        new Promise<ReassembledTaskDiff>((resolve) => {
          resolveTaskAlpha = resolve;
        });
      const loadTaskBeta = () =>
        new Promise<ReassembledTaskDiff>((resolve) => {
          resolveTaskBeta = resolve;
        });

      // 1. Open workbench on Task Alpha
      const optionsAlpha: ArtifactReviewOptions = {
        studioId: 'studio-01',
        taskId: 'task-alpha',
        taskTitle: 'Alpha Processing',
      };
      const panel = ArtifactReviewPanel.createOrShow(extensionUri, optionsAlpha, loadTaskAlpha);
      const reqAlpha = panel.activeRequestId;
      assert.strictEqual(panel.isLoading, true);
      assert.strictEqual(panel.expectedTaskId, 'task-alpha');
      assert.strictEqual(panel.diffData, null);

      // 2. User quickly switches to Task Beta before Alpha loads
      const optionsBeta: ArtifactReviewOptions = {
        studioId: 'studio-01',
        taskId: 'task-beta',
        taskTitle: 'Beta Processing',
      };
      ArtifactReviewPanel.createOrShow(extensionUri, optionsBeta, loadTaskBeta);
      const reqBeta = panel.activeRequestId;
      assert.strictEqual(reqBeta, reqAlpha + 1);
      assert.strictEqual(panel.isLoading, true);
      assert.strictEqual(panel.expectedTaskId, 'task-beta');
      assert.strictEqual(panel.diffData, null);

      // 3. Late response for Alpha arrives now
      const diffAlpha = createMockDiff({
        studioId: 'studio-01',
        taskId: 'task-alpha',
        rawText: 'diff --git a/alpha.txt b/alpha.txt\n+Alpha Content',
      });
      resolveTaskAlpha!(diffAlpha);
      await new Promise((r) => setTimeout(r, 20));

      // Must remain in loading state for Task Beta, zero pollution from Alpha
      assert.strictEqual(panel.isLoading, true);
      assert.strictEqual(panel.expectedTaskId, 'task-beta');
      assert.strictEqual(panel.diffData, null);

      // 4. Manual setDiffData targeting task-alpha is also rejected by identity guard
      panel.setDiffData(diffAlpha);
      assert.strictEqual(panel.diffData, null);

      // 5. Task Beta response arrives
      const diffBeta = createMockDiff({
        studioId: 'studio-01',
        taskId: 'task-beta',
        rawText: 'diff --git a/beta.txt b/beta.txt\n+Beta Content',
      });
      resolveTaskBeta!(diffBeta);
      await new Promise((r) => setTimeout(r, 20));

      assert.strictEqual(panel.isLoading, false);
      assert.strictEqual(panel.expectedTaskId, 'task-beta');
      const loadedDiff = panel.diffData as ReassembledTaskDiff | null;
      assert.ok(loadedDiff !== null);
      assert.strictEqual(loadedDiff.taskId, 'task-beta');
      assert.ok(loadedDiff.text.includes('+Beta Content'));
    });

    it('TaskExecutionStore ignores late timeline events for non-active tasks', () => {
      const store = TaskExecutionStore.instance;

      // Start execution for task-100
      store.beginSubmission('studio-01', 'Build parser', 'Build parser');
      store.taskSubmitted('task-100', 'studio-01');
      assert.strictEqual(store.state.activeTaskId, 'task-100');
      assert.strictEqual(store.state.status, 'queued');

      // Late event from previous task-099 arrives
      store.consumeDaemonTimelineEvent({
        sequenceNumber: 50,
        id: 'evt-50',
        timestamp: Date.now(),
        type: 'task_started',
        summary: 'Task 099 started',
        taskId: 'task-099',
        details: { execution: { runtime_preference: 'opencode' } },
      });

      // State must remain queued for task-100, not modified by task-099
      assert.strictEqual(store.state.activeTaskId, 'task-100');
      assert.strictEqual(store.state.status, 'queued');

      // Valid event for active task-100 arrives
      store.consumeDaemonTimelineEvent({
        sequenceNumber: 51,
        id: 'evt-51',
        timestamp: Date.now(),
        type: 'task_started',
        summary: 'Task 100 started',
        taskId: 'task-100',
        details: { run_id: 'run-100', execution: { runtime_preference: 'opencode' } },
      });

      assert.strictEqual(store.state.activeTaskId, 'task-100');
      assert.strictEqual(store.state.status, 'running');
      assert.strictEqual(store.state.activeRunId, 'run-100');
    });
  });

  // =========================================================================
  // 3. CANCELLATION DURING CHUNK STREAMING
  // =========================================================================
  describe('3. Cancellation During Chunk Streaming', () => {
    it('aborts multi-chunk diff retrieval cleanly on AbortSignal without corrupting client state', async () => {
      let commandCallCount = 0;
      const abortController = new AbortController();

      const mockTransport: IpcTransportSender = {
        sendCommand: async (cmd: RustIpcCommand) => {
          commandCallCount++;
          if (cmd.type === 'get_task_diff') {
            if (cmd.offset === 0) {
              // Return first 5 bytes of 10 bytes total
              const chunkBase64 = Buffer.from('hello').toString('base64');
              return {
                type: 'task_diff',
                diff: {
                  studio_id: 'studio-01',
                  task_id: 'task-stream-1',
                  artifact_id: 'art-1',
                  base_commit: 'aaa',
                  head_commit: 'bbb',
                  summary: {
                    files_changed: 1,
                    added_files: ['test.txt'],
                    modified_files: [],
                    deleted_files: [],
                    renamed_files: [],
                    binary_files: [],
                    patch_size_bytes: 10,
                    base_commit: 'aaa',
                  },
                  offset: 0,
                  chunk_bytes: 5,
                  total_bytes: 10,
                  limit: 5,
                  has_more: true,
                  encoding: 'base64',
                  patch: '',
                  patch_base64: chunkBase64,
                  patch_sha256: 'somehash',
                },
              } as RustIpcResponse;
            }
          }
          throw new Error(`Unexpected command: ${JSON.stringify(cmd)}`);
        },
      };

      // Trigger abort immediately after first command resolution
      const fetchPromise = ArtifactClient.fetchTaskDiff(mockTransport, 'studio-01', 'task-stream-1', {
        chunkSize: 5,
        signal: abortController.signal,
      });

      // Abort in flight
      abortController.abort();

      await assert.rejects(
        fetchPromise,
        (err: Error) => {
          assert.ok(err instanceof ArtifactRetrievalError);
          assert.strictEqual(err.code, 'ABORTED');
          return true;
        }
      );

      // Verify that after aborting, no second chunk request was sent
      assert.strictEqual(commandCallCount, 1);
    });

    it('ArtifactReviewPanel swallows aborted fetch without displaying error or stale diff', async () => {
      let rejectFirst!: (err: Error) => void;
      let loadCallCount = 0;

      const abortableLoad = () => {
        loadCallCount++;
        if (loadCallCount === 1) {
          return new Promise<ReassembledTaskDiff>((_, reject) => {
            rejectFirst = reject;
          });
        }
        return new Promise<ReassembledTaskDiff>(() => {});
      };

      const options: ArtifactReviewOptions = {
        studioId: 'studio-01',
        taskId: 'task-abortable',
        taskTitle: 'Abortable Task',
      };

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, options, abortableLoad);
      assert.strictEqual(panel.isLoading, true);
      assert.strictEqual(loadCallCount, 1);

      // Initiate a second refresh to abort request 1
      panel.refresh();
      assert.strictEqual(loadCallCount, 2);

      // Reject the first fetch with an aborted error
      rejectFirst(new ArtifactRetrievalError('Task diff download was aborted by client.', 'ABORTED'));
      await new Promise((r) => setTimeout(r, 20));

      // Panel must not display error message from aborted prior request
      assert.strictEqual(panel.errorMessage, null);
    });
  });

  // =========================================================================
  // 4. RAPID TASK SWITCHING UNDER LOAD
  // =========================================================================
  describe('4. Rapid Task Switching Under Load', () => {
    it('monotonic request ID guarantees only the final task render takes effect when responses resolve out of order', async () => {
      const resolvers: Map<string, (diff: ReassembledTaskDiff) => void> = new Map();

      const makeLoader = (taskId: string) => () =>
        new Promise<ReassembledTaskDiff>((resolve) => {
          resolvers.set(taskId, resolve);
        });

      let panel: ArtifactReviewPanel | null = null;
      const taskIds = ['task-1', 'task-2', 'task-3', 'task-4', 'task-5'];

      // Rapidly switch through 5 tasks in sequence
      for (const taskId of taskIds) {
        panel = ArtifactReviewPanel.createOrShow(
          extensionUri,
          { studioId: 'studio-01', taskId, taskTitle: `Task ${taskId}` },
          makeLoader(taskId)
        );
      }

      assert.ok(panel !== null);
      assert.strictEqual(panel?.expectedTaskId, 'task-5');
      assert.strictEqual(panel?.activeRequestId, 5);
      assert.strictEqual(panel?.isLoading, true);
      assert.strictEqual(panel?.diffData, null);

      // Now resolve the tasks in reverse/arbitrary order: task-3, task-1, task-4, task-2
      const outOfOrderKeys = ['task-3', 'task-1', 'task-4', 'task-2'];
      for (const taskId of outOfOrderKeys) {
        const resolveFn = resolvers.get(taskId);
        assert.ok(resolveFn, `Resolver for ${taskId} must exist`);
        resolveFn(
          createMockDiff({
            studioId: 'studio-01',
            taskId,
            rawText: `diff --git a/${taskId}.txt b/${taskId}.txt\n+Content for ${taskId}`,
          })
        );
      }

      await new Promise((r) => setTimeout(r, 30));

      // Stale resolutions must NOT update diffData or reset loading state of task-5
      assert.strictEqual(panel?.expectedTaskId, 'task-5');
      assert.strictEqual(panel?.isLoading, true);
      assert.strictEqual(panel?.diffData, null);

      // Finally resolve the legitimate target: task-5
      const resolveTask5 = resolvers.get('task-5');
      assert.ok(resolveTask5);
      resolveTask5(
        createMockDiff({
          studioId: 'studio-01',
          taskId: 'task-5',
          rawText: 'diff --git a/task-5.txt b/task-5.txt\n+Content for task-5',
        })
      );

      await new Promise((r) => setTimeout(r, 30));

      // View state must now reflect task-5 accurately
      assert.strictEqual(panel?.isLoading, false);
      assert.strictEqual(panel?.errorMessage, null);
      assert.strictEqual(panel?.expectedTaskId, 'task-5');
      const loadedDiff5 = panel?.diffData as ReassembledTaskDiff | null;
      assert.ok(loadedDiff5 !== null);
      assert.strictEqual(loadedDiff5.taskId, 'task-5');
      assert.ok(loadedDiff5.text.includes('+Content for task-5'));
    });
  });

  // =========================================================================
  // 5. BRIDGE DISCONNECT & RECONNECT RECOVERY
  // =========================================================================
  describe('5. Bridge Disconnect & Reconnect Recovery', () => {
    it('manages connection state, fails closed on disconnect, clears challenges, and recovers with watermark replay', async () => {
      let currentSocket: net.Socket;
      let requestedFromSequence: number | undefined;

      await startMockDaemon((cmd, sendResponse, socket) => {
        currentSocket = socket;
        if (cmd.type === 'negotiate_capabilities') {
          sendResponse({
            type: 'capabilities_negotiated',
            selected_version: { major: 1, minor: 0, patch: 0 },
            agreed_capabilities: ['worktree_isolation'],
          });
        } else if (cmd.type === 'authenticate') {
          sendResponse({ type: 'authenticated', session_id: 'sess-lifecycle-01' });
        } else if (cmd.type === 'subscribe_events') {
          requestedFromSequence = cmd.from_sequence;
        } else if (cmd.type === 'list_studios') {
          sendResponse({
            type: 'studios',
            studios: [
              {
                id: 'studio-recovery-01',
                name: 'Recovery Studio',
                workspace_path: '/workspaces/rec',
                created_at: '2026-10-30T12:00:00Z',
              },
            ],
          });
        } else if (cmd.type === 'get_studio') {
          sendResponse({
            type: 'studio',
            studio: {
              id: cmd.studio_id,
              name: 'Recovery Studio',
              workspace_path: '/workspaces/rec',
              created_at: '2026-10-30T12:00:00Z',
            },
          });
        } else if (cmd.type === 'list_tasks') {
          sendResponse({ type: 'tasks', tasks: [] });
        } else if (cmd.type === 'get_execution_readiness') {
          sendResponse({
            type: 'execution_readiness',
            readiness: {
              is_ready: true,
              available_runtimes: ['opencode'],
              runtime_instances: [],
              authorized_workspaces: ['/workspaces/rec'],
              auth_state: { authenticated: true, active_sessions: 1 },
              defaults: { default_repo_path: '/workspaces/rec' },
              blocking_reasons: [],
            },
          });
        }
      });

      const sendEventFrame = (seq: number, eventType: string, summary: string) => {
        const frame = {
          type: 'event',
          event: {
            sequence: seq,
            timestamp: new Date().toISOString(),
            event_type: eventType,
            payload: { summary },
          },
        };
        const bytes = Buffer.from(JSON.stringify(frame), 'utf8');
        const header = Buffer.alloc(4);
        header.writeUInt32BE(bytes.length, 0);
        currentSocket.write(Buffer.concat([header, bytes]));
      };

      const transport = new NativeIpcTransport({ endpoint, keyId, clientKey });
      const bridge = new DaemonConnectedBridge({ transport });
      const projection = new SessionStateProjection(bridge);

      // 1. Connect
      await bridge.connect();
      assert.strictEqual(bridge.connectionState, 'connected');
      assert.strictEqual(requestedFromSequence, undefined); // initial subscription has no watermark

      // 2. Stream events 1, 2
      sendEventFrame(1, 'session_started', 'Session started');
      sendEventFrame(2, 'task_started', 'Task started');
      await new Promise((r) => setTimeout(r, 40));

      assert.strictEqual(bridge.lastWatermark, 2);
      assert.strictEqual(projection.lastProcessedSequence, 2);
      assert.strictEqual(projection.timeline.length, 2);

      // Create an operator challenge to verify clearance on disconnect
      OperatorConfirmationManager.createChallenge('studio-recovery-01', 'Prompt for recovery');
      assert.strictEqual(
        OperatorConfirmationManager.getActiveChallengesCountForTesting(),
        1
      );

      // 3. Disconnect
      await bridge.disconnect();
      assert.strictEqual(bridge.connectionState, 'disconnected');

      // Operator challenges must be invalidated on disconnect
      assert.strictEqual(
        OperatorConfirmationManager.getActiveChallengesCountForTesting(),
        0
      );

      // Queries must fail closed while disconnected
      await assert.rejects(async () => {
        await bridge.listTasks();
      }, (err: Error) => {
        assert.ok(err.message.includes('not connected'));
        return true;
      });

      // 4. Reconnect
      await bridge.connect();
      assert.strictEqual(bridge.connectionState, 'connected');

      // Watermark resumption: Bridge must have asked for watermark + 1 (3)
      assert.strictEqual(requestedFromSequence, 3);

      // 5. Daemon re-sends event 2 (replayed) and then new event 3
      sendEventFrame(2, 'task_started', 'Task 2 replayed duplicate');
      sendEventFrame(3, 'task_completed', 'Task 3 completed');
      await new Promise((r) => setTimeout(r, 40));

      // Watermark updated to 3, event 2 was deduplicated
      assert.strictEqual(bridge.lastWatermark, 3);
      assert.strictEqual(projection.lastProcessedSequence, 3);
      assert.strictEqual(projection.timeline.length, 3);
      assert.strictEqual(projection.timeline[2].sequenceNumber, 3);

      projection.dispose();
      bridge.dispose();
    });
  });

  // =========================================================================
  // 6. PANEL DISPOSAL DURING ACTIVE FETCH
  // =========================================================================
  describe('6. Panel Disposal During Active Fetch', () => {
    it('aborts in-flight controller cleanly on dispose and prevents errors on subsequent promise resolution', async () => {
      let resolveDiff: ((diff: ReassembledTaskDiff) => void) | null = null;

      const hangingLoad = () =>
        new Promise<ReassembledTaskDiff>((resolve) => {
          resolveDiff = resolve;
        });

      const options: ArtifactReviewOptions = {
        studioId: 'studio-01',
        taskId: 'task-disposal-test',
        taskTitle: 'Disposal Test',
      };

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, options, hangingLoad);
      const initialRequestId = panel.activeRequestId;
      assert.strictEqual(ArtifactReviewPanel.currentPanel, panel);
      assert.strictEqual(panel.isLoading, true);

      // User closes the webview tab while diff is fetching
      panel.dispose();

      // Verification: Singleton reference cleared, request ID incremented
      assert.strictEqual(ArtifactReviewPanel.currentPanel, undefined);
      assert.ok(panel.activeRequestId > initialRequestId);

      // Subsequent async resolution must not crash or invoke any methods on disposed panel
      assert.doesNotThrow(() => {
        const dummyDiff = createMockDiff({
          studioId: 'studio-01',
          taskId: 'task-disposal-test',
          rawText: 'diff --git a/test.txt b/test.txt\n+Finished',
        });
        if (resolveDiff) {
          resolveDiff(dummyDiff);
        }
      });

      await new Promise((r) => setTimeout(r, 20));

      // Panel diff remains null and does not leak into memory
      assert.strictEqual(panel.diffData, null);
    });

    it('handles async rejection gracefully when panel is disposed before failure arrives', async () => {
      let rejectDiff: ((err: Error) => void) | null = null;

      const failingLoad = () =>
        new Promise<ReassembledTaskDiff>((_, reject) => {
          rejectDiff = reject;
        });

      const options: ArtifactReviewOptions = {
        studioId: 'studio-01',
        taskId: 'task-failing-disposal',
        taskTitle: 'Failing Disposal Test',
      };

      const panel = ArtifactReviewPanel.createOrShow(extensionUri, options, failingLoad);
      assert.strictEqual(panel.isLoading, true);

      panel.dispose();
      assert.strictEqual(ArtifactReviewPanel.currentPanel, undefined);

      // Subsequent async rejection must be safely discarded without unhandled rejection or error popup
      assert.doesNotThrow(() => {
        if (rejectDiff) {
          rejectDiff(new Error('Backend IPC connection abruptly severed.'));
        }
      });

      await new Promise((r) => setTimeout(r, 20));
      assert.strictEqual(panel.errorMessage, null);
    });
  });
});
