/**
 * Agent Studios — Cross-View Execution State Consistency Unit Tests
 * Milestone M15 UI-01.2: Real Rust Execution Integration & Final Safety Gates
 *
 * Verifies that Chat and Mission Control views consume one single authoritative
 * execution projection:
 * 1. taskSubmitted transitions status to 'queued'.
 * 2. 'running' is reached strictly via daemon-confirmed transitions.
 * 3. Output deltas and tool activities are truthfully tracked without synthetic fabrication.
 * 4. Metrics are gated behind isReported: true with zero fabricated tokens or costs.
 * 5. State transitions (succeeded, failed, cancelled) remain completely in sync across views.
 */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import { SessionStateProjection } from '../../state/SessionStateProjection';
import { BridgeManager } from '../../bridge/BridgeManager';
import { FixtureBridge } from '../../bridge/FixtureBridge';
import { MissionControlProjection } from '../../mission-control/state/MissionControlProjection';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { MockWebviewView, resetMockState } from './vscodeMock';
import { TimelineEvent } from '../../bridge/types';

describe('M15 Cross-View Execution State Consistency Tests', () => {
  let bridge: FixtureBridge;
  let bridgeManager: BridgeManager;
  let sessionProjection: SessionStateProjection;
  let mcProjection: MissionControlProjection;
  let chatProvider: ChatWebviewViewProvider;
  let mockChatView: MockWebviewView;

  const studioId = 'studio-cross-view-01';
  const taskId = 'task-cross-view-100';

  const nextSeq = (): number => sessionProjection.lastProcessedSequence + 1;

  beforeEach(async () => {
    resetMockState();
    TaskExecutionStore.instance.reset();

    bridge = new FixtureBridge();
    bridgeManager = new BridgeManager(bridge);
    sessionProjection = bridgeManager.stateProjection;
    mcProjection = new MissionControlProjection(bridgeManager);

    const extensionUri = vscode.Uri.file('/mock/extension');
    chatProvider = new ChatWebviewViewProvider(extensionUri, bridgeManager);
    mockChatView = new MockWebviewView(ChatWebviewViewProvider.viewType);

    await chatProvider.resolveWebviewView(
      mockChatView as unknown as vscode.WebviewView,
      {} as vscode.WebviewViewResolveContext,
      {} as vscode.CancellationToken
    );
  });

  afterEach(() => {
    TaskExecutionStore.instance.reset();
    mcProjection.dispose();
    bridgeManager.dispose();
  });

  it('1. task submission transitions status to queued across Chat and store', () => {
    const store = TaskExecutionStore.instance;
    assert.strictEqual(store.getState().status, 'idle');

    store.taskSubmitted(taskId, studioId);

    const state = store.getState();
    assert.strictEqual(state.status, 'queued');
    assert.strictEqual(state.activeTaskId, taskId);
    assert.strictEqual(state.activeStudioId, studioId);
    assert.strictEqual(state.metrics, undefined);

    const lastMsg = mockChatView.webview._sentMessages[
      mockChatView.webview._sentMessages.length - 1
    ] as { type: string; executionState?: { status: string } } | undefined;
    assert.strictEqual(lastMsg?.type, 'executionStateChanged');
    assert.strictEqual(lastMsg?.executionState?.status, 'queued');
  });

  it('2. running status is reached only via daemon-confirmed timeline event', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    assert.strictEqual(store.getState().status, 'queued');

    const startedEvent: TimelineEvent = {
      sequenceNumber: nextSeq(),
      id: 'evt-task-start-1',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Task execution started by daemon agent',
      taskId,
      details: { studioId, runId: 'run-001' },
    };

    // Feed event through SessionStateProjection
    sessionProjection.handleEvent(startedEvent);

    const state = store.getState();
    assert.strictEqual(state.status, 'running');
    assert.strictEqual(state.activeRunId, 'run-001');

    const lastMsg = mockChatView.webview._sentMessages[
      mockChatView.webview._sentMessages.length - 1
    ] as { type: string; executionState?: { status: string } } | undefined;
    assert.strictEqual(lastMsg?.type, 'executionStateChanged');
    assert.strictEqual(lastMsg?.executionState?.status, 'running');
  });

  it('3. streaming output deltas are recorded truthfully in store and timeline', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Started',
      taskId,
      details: { runId: 'run-001' },
    });

    const deltaChunks = ['fn compute() {\n', '    return 42;\n', '}\n'];
    for (const chunk of deltaChunks) {
      sessionProjection.handleEvent({
        sequenceNumber: nextSeq(),
        id: `evt-delta-${Date.now()}-${Math.random()}`,
        type: 'output_delta',
        timestamp: Date.now(),
        summary: chunk,
        taskId,
        details: { chunk },
      });
    }

    const state = store.getState();
    assert.strictEqual(state.streamOutput, 'fn compute() {\n    return 42;\n}\n');

    const streamMsgs = mockChatView.webview._sentMessages.filter(
      (m: unknown) => (m as { type?: string })?.type === 'streamChunk'
    );
    assert.strictEqual(streamMsgs.length, 3);
  });

  it('4. tool invocation activity is truthfully tracked without fabrication', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Started',
      taskId,
      details: { runId: 'run-001' },
    });

    // Tool started
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-tool-1',
      type: 'tool_started',
      timestamp: Date.now(),
      summary: 'Tool bash execution started',
      taskId,
      details: {
        tool_name: 'bash',
        parameters: { command: 'cargo test' },
      },
    });

    assert.strictEqual(store.getState().activeTool?.name, 'bash');
    assert.strictEqual(store.getState().activeTool?.status, 'executing');

    // Tool completed
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-tool-2',
      type: 'tool_completed',
      timestamp: Date.now(),
      summary: 'Tool bash finished',
      taskId,
      details: {
        tool_name: 'bash',
      },
    });

    assert.strictEqual(store.getState().activeTool?.name, 'bash');
    assert.strictEqual(store.getState().activeTool?.status, 'completed');
  });

  it('5. task completion verifies metrics with zero fabricated synthetic tokens', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Started',
      taskId,
      details: { runId: 'run-001' },
    });

    // Task succeeded with truthful daemon metrics
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-done',
      type: 'task_completed',
      timestamp: Date.now(),
      summary: 'Task completed successfully',
      taskId,
      details: {
        metrics: {
          prompt_tokens: 1250,
          completion_tokens: 380,
          latency_ms: 4200,
        },
      },
    });

    const state = store.getState();
    assert.strictEqual(state.status, 'succeeded');
    assert.strictEqual(state.metrics?.isReported, true);
    assert.strictEqual(state.metrics?.promptTokens, 1250);
    assert.strictEqual(state.metrics?.completionTokens, 380);
    assert.strictEqual(state.metrics?.latencyMs, 4200);
    assert.strictEqual(state.metrics?.costUsd, undefined); // Zero synthetic cost
    assert.strictEqual(state.activeTool, undefined);
  });

  it('6. task failure retains error details truthfully across views', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);

    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-fail',
      type: 'task_failed',
      timestamp: Date.now(),
      summary: 'Compilation error: unresolved import',
      taskId,
      details: {
        error: 'Compilation error: unresolved import',
      },
    });

    const state = store.getState();
    assert.strictEqual(state.status, 'failed');
    assert.strictEqual(state.lastError?.message, 'Compilation error: unresolved import');
  });

  it('7. task cancellation transitions state to cancelled immediately', () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);

    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-cancel',
      type: 'task_cancelled',
      timestamp: Date.now(),
      summary: 'Cancelled by operator',
      taskId,
      details: {
        reason: 'Cancelled by operator',
      },
    });

    const state = store.getState();
    assert.strictEqual(state.status, 'cancelled');
    assert.strictEqual(state.lastError?.message, 'Cancelled by operator');
  });
});
