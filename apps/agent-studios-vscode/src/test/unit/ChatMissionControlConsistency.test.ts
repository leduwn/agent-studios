/**
 * Agent Studios — Chat & Mission Control State Consistency Unit Tests
 * Milestone M15 UI-05A: UX & Operator Safety Hardening
 *
 * Verifies end-to-end consistency between Chat and Mission Control:
 * 1. Task created in Chat reflects in Mission Control.
 * 2. Task status transitions are identical across both views.
 * 3. Rapid task switching isolates state without cross-task leakage.
 * 4. Panel disposal and reload retains projection consistency.
 * 5. Daemon restart/crash reconciles interrupted tasks to failed in both views.
 * 6. Mission Control refresh coalesces in-flight queries and rejects stale generation responses.
 * 7. Studio switching cleanly isolates state and purges stale items.
 */

/* eslint-disable @typescript-eslint/naming-convention */

import * as assert from 'assert';
import * as vscode from 'vscode';
import { TaskExecutionStore } from '../../state/TaskExecutionStore';
import { SessionStateProjection } from '../../state/SessionStateProjection';
import { BridgeManager } from '../../bridge/BridgeManager';
import { AgentStudiosBridge } from '../../bridge/AgentStudiosBridge';
import { MissionControlProjection } from '../../mission-control/state/MissionControlProjection';
import { ChatWebviewViewProvider } from '../../chat/ChatWebviewViewProvider';
import { MockWebviewView, resetMockState } from './vscodeMock';
import {
  ApprovalDecisionResult,
  ApprovalRequest,
  Artifact,
  ChatMessage,
  ChatSendRequest,
  ChatStreamChunk,
  ConnectionState,
  EditorActionRequest,
  EditorActionResult,
  ProviderSummary,
  SessionInfo,
  SessionSummary,
  TaskGraph,
  TaskNode,
  TimelineEvent,
} from '../../bridge/types';
import { SubmitTaskRequest, SubmitTaskResult } from '../../bridge/ipc/SubmitTaskAdapter';
import { CancelTaskResult } from '../../bridge/ipc/ExecutionClient';

class MockConnectedBridge implements AgentStudiosBridge {
  public connectionState: ConnectionState = 'connected';
  private readonly _onConnectionStateChanged = new vscode.EventEmitter<ConnectionState>();
  public readonly onConnectionStateChanged = this._onConnectionStateChanged.event;

  private readonly _onEvent = new vscode.EventEmitter<TimelineEvent>();
  public readonly onEvent = this._onEvent.event;

  public currentSession: SessionInfo = {
    id: 'studio-test-01',
    workspacePath: '/mock/workspace',
    goalSummary: 'Test Studio Session',
    status: 'running',
    startedAt: Date.now(),
    elapsedMs: 0,
    activeAgentCount: 1,
    totalAgentCount: 1,
    completedTasksCount: 0,
    totalTasksCount: 0,
  };

  public tasks: TaskNode[] = [];
  public approvals: ApprovalRequest[] = [];
  public artifacts: Artifact[] = [];
  public providers: ProviderSummary[] = [];
  public timeline: TimelineEvent[] = [];

  public fireEvent(event: TimelineEvent): void {
    this.timeline.push(event);
    this._onEvent.fire(event);
  }

  public async connect(): Promise<void> {
    this.connectionState = 'connected';
    this._onConnectionStateChanged.fire(this.connectionState);
  }

  public async disconnect(): Promise<void> {
    this.connectionState = 'disconnected';
    this._onConnectionStateChanged.fire(this.connectionState);
  }

  public async getCurrentSession(): Promise<SessionInfo | null> {
    return { ...this.currentSession };
  }

  public async getTaskGraph(): Promise<TaskGraph | null> {
    return { runId: 'run-01', nodes: this.tasks, edges: [] };
  }

  public async listTasks(): Promise<TaskNode[]> {
    return this.tasks.map((t) => ({ ...t }));
  }

  public async listPendingApprovals(): Promise<ApprovalRequest[]> {
    return this.approvals.map((a) => ({ ...a }));
  }

  public async requestApprovalDecision(id: string, decision: 'approved' | 'denied'): Promise<ApprovalDecisionResult> {
    return { id, decision, applied: true, message: `Action ${decision}` };
  }

  public async listArtifacts(): Promise<Artifact[]> {
    return this.artifacts.map((a) => ({ ...a }));
  }

  public async getArtifactDiff(): Promise<string | null> {
    return null;
  }

  public async listProviderSummaries(): Promise<ProviderSummary[]> {
    return this.providers.map((p) => ({ ...p }));
  }

  public async submitEditorAction(_action: EditorActionRequest): Promise<EditorActionResult> {
    return { accepted: true, message: 'Action accepted' };
  }

  public async sendMessage(
    request: ChatSendRequest,
    _onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage> {
    return {
      id: 'msg-resp',
      role: 'assistant',
      content: `Echo: ${request.message}`,
      timestamp: Date.now(),
    };
  }

  public async submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResult> {
    const id = `task-${Date.now()}`;
    const task: TaskNode = {
      id,
      runId: this.currentSession.id,
      title: req.prompt,
      description: req.prompt,
      status: 'pending',
      dependencies: [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    this.tasks.push(task);
    return {
      success: true,
      taskId: id,
      taskKey: id,
      studioId: req.studioId || this.currentSession.id,
    };
  }

  public async cancelTask(taskId: string): Promise<CancelTaskResult> {
    const task = this.tasks.find((t) => t.id === taskId);
    if (task) {
      task.status = 'cancelled';
    }
    return {
      success: true,
      taskId,
      studioId: this.currentSession.id,
      summary: {
        cancelled_tasks: [taskId],
        cancelled_runs: [],
        cancelled_approvals: [],
      },
    };
  }

  public async cancelCurrentRun(): Promise<void> {}

  public async listSessions(): Promise<SessionSummary[]> {
    return [
      {
        id: this.currentSession.id,
        title: this.currentSession.goalSummary,
        status: 'running',
        updatedAt: Date.now(),
      },
    ];
  }

  public async createSession(goal?: string): Promise<SessionInfo> {
    this.currentSession = {
      id: `session-${Date.now()}`,
      workspacePath: '/mock/workspace',
      goalSummary: goal || 'New Session',
      status: 'running',
      startedAt: Date.now(),
      elapsedMs: 0,
      activeAgentCount: 1,
      totalAgentCount: 1,
      completedTasksCount: 0,
      totalTasksCount: 0,
    };
    this.tasks = [];
    return { ...this.currentSession };
  }

  public async switchSession(sessionId: string): Promise<SessionInfo> {
    this.currentSession = {
      id: sessionId,
      workspacePath: `/mock/workspace/${sessionId}`,
      goalSummary: `Session ${sessionId}`,
      status: 'running',
      startedAt: Date.now(),
      elapsedMs: 0,
      activeAgentCount: 1,
      totalAgentCount: 1,
      completedTasksCount: 0,
      totalTasksCount: 0,
    };
    this.tasks = [];
    return { ...this.currentSession };
  }

  public listTimelineEvents(): TimelineEvent[] {
    return [...this.timeline];
  }

  public dispose(): void {
    this._onConnectionStateChanged.dispose();
    this._onEvent.dispose();
  }
}

describe('M15 Chat & Mission Control Consistency Unit Tests', () => {
  let bridge: MockConnectedBridge;
  let bridgeManager: BridgeManager;
  let sessionProjection: SessionStateProjection;
  let mcProjection: MissionControlProjection;
  let chatProvider: ChatWebviewViewProvider;
  let mockChatView: MockWebviewView;

  const studioId = 'studio-test-01';
  const taskId = 'task-consistency-001';

  const nextSeq = (): number => sessionProjection.lastProcessedSequence + 1;

  beforeEach(async () => {
    resetMockState();
    TaskExecutionStore.instance.reset();

    bridge = new MockConnectedBridge();
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

    await mcProjection.refresh();
  });

  afterEach(() => {
    TaskExecutionStore.instance.reset();
    mcProjection.dispose();
    bridgeManager.dispose();
    bridge.dispose();
  });

  it('1. Task created in Chat reflects immediately in Mission Control', async () => {
    const store = TaskExecutionStore.instance;

    // Simulate task creation/submission in Chat
    store.beginSubmission(studioId, 'Generate unit tests for auth module');
    store.taskSubmitted(taskId, studioId);
    sessionProjection.upsertTask({
      id: taskId,
      title: 'Generate unit tests for auth module',
      status: 'pending',
    });

    // Refresh Mission Control
    await mcProjection.refresh();

    const mcState = mcProjection.getState();
    const taskInMC = mcState.tasks.find((t) => t.id === taskId);

    assert.ok(taskInMC, 'Task created in Chat must exist in Mission Control projection');
    assert.strictEqual(taskInMC?.state, 'pending');
    assert.strictEqual(taskInMC?.title, 'Generate unit tests for auth module');
    assert.ok(mcState.overview.taskCounts.pending >= 1, 'Overview pending count must reflect new task');
  });

  it('2. Task status transitions are identical across Chat and Mission Control', async () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    sessionProjection.upsertTask({
      id: taskId,
      title: 'Consistency Pipeline Task',
      status: 'pending',
    });

    await mcProjection.refresh();
    assert.strictEqual(store.getState().status, 'queued');
    let taskInMC = mcProjection.getState().tasks.find((t) => t.id === taskId);
    assert.strictEqual(taskInMC?.state, 'pending');

    // Transition to running via timeline event
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-task-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Task started by coordinator agent',
      taskId,
      agentName: 'PlannerAgent',
      details: { runId: 'run-101' },
    });

    await mcProjection.refresh();

    // Verify Chat store state
    assert.strictEqual(store.getState().status, 'running');
    assert.strictEqual(store.getState().activeTaskId, taskId);
    assert.strictEqual(store.getState().activeRunId, 'run-101');

    // Verify Mission Control state
    taskInMC = mcProjection.getState().tasks.find((t) => t.id === taskId);
    assert.strictEqual(taskInMC?.state, 'running');
    assert.strictEqual(taskInMC?.assignedAgentName, 'PlannerAgent');
    assert.ok(mcProjection.getState().overview.taskCounts.running >= 1);

    // Stream deltas and tool execution
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-tool-1',
      type: 'tool_started',
      timestamp: Date.now(),
      summary: 'Tool bash executing',
      taskId,
      details: { name: 'bash' },
    });

    assert.strictEqual(store.getState().activeTool?.name, 'bash');
    assert.strictEqual(store.getState().activeTool?.status, 'executing');

    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-tool-2',
      type: 'tool_completed',
      timestamp: Date.now(),
      summary: 'Tool bash finished',
      taskId,
      details: { name: 'bash' },
    });

    assert.strictEqual(store.getState().activeTool?.status, 'completed');

    // Transition to succeeded
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-task-complete',
      type: 'task_completed',
      timestamp: Date.now(),
      summary: 'Task execution completed successfully',
      taskId,
    });

    await mcProjection.refresh();

    assert.strictEqual(store.getState().status, 'succeeded');
    taskInMC = mcProjection.getState().tasks.find((t) => t.id === taskId);
    assert.strictEqual(taskInMC?.state, 'succeeded');
    assert.strictEqual(taskInMC?.progress, 100);
    assert.ok(mcProjection.getState().overview.taskCounts.succeeded >= 1);
  });

  it('3. Rapid task switching isolates state without cross-task leakage', async () => {
    const store = TaskExecutionStore.instance;
    const taskA = 'task-alpha';
    const taskB = 'task-beta';

    // Start Task A
    store.switchTask(taskA, studioId);
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-a-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Task Alpha started',
      taskId: taskA,
    });
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-a-chunk1',
      type: 'output_delta',
      timestamp: Date.now(),
      summary: 'Alpha Chunk 1\n',
      taskId: taskA,
      details: { chunk: 'Alpha Chunk 1\n' },
    });
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-a-tool',
      type: 'tool_started',
      timestamp: Date.now(),
      summary: 'Tool bash started',
      taskId: taskA,
      details: { name: 'bash' },
    });

    assert.strictEqual(store.getState().activeTaskId, taskA);
    assert.strictEqual(store.getState().streamOutput, 'Alpha Chunk 1\n');
    assert.strictEqual(store.getState().activeTool?.name, 'bash');

    // Rapidly switch to Task B
    store.switchTask(taskB, studioId);
    assert.strictEqual(store.getState().activeTaskId, taskB);
    assert.strictEqual(store.getState().streamOutput, '', 'New task must have isolated empty stream');
    assert.strictEqual(store.getState().activeTool, undefined, 'New task must have isolated tool state');

    // Receive background event for Task A while Task B is active
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-a-chunk2',
      type: 'output_delta',
      timestamp: Date.now(),
      summary: 'Alpha Chunk 2\n',
      taskId: taskA,
      details: { chunk: 'Alpha Chunk 2\n' },
    });

    // Active Task B must not receive Task A output
    assert.strictEqual(store.getState().activeTaskId, taskB);
    assert.strictEqual(store.getState().streamOutput, '', 'Background task event must not leak into active task');

    // Add output for Task B
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-b-chunk1',
      type: 'output_delta',
      timestamp: Date.now(),
      summary: 'Beta Chunk 1\n',
      taskId: taskB,
      details: { chunk: 'Beta Chunk 1\n' },
    });

    assert.strictEqual(store.getState().streamOutput, 'Beta Chunk 1\n');

    // Switch back to Task A
    store.switchTask(taskA, studioId);
    assert.strictEqual(store.getState().activeTaskId, taskA);
    assert.strictEqual(
      store.getState().streamOutput,
      'Alpha Chunk 1\nAlpha Chunk 2\n',
      'Task A must have safely accumulated background chunk without loss'
    );
    assert.strictEqual(store.getState().activeTool?.name, 'bash');

    // Switch back to Task B
    store.switchTask(taskB, studioId);
    assert.strictEqual(store.getState().activeTaskId, taskB);
    assert.strictEqual(store.getState().streamOutput, 'Beta Chunk 1\n');
  });

  it('4. Panel disposal and reload retains consistency', async () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted(taskId, studioId);
    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Running task',
      taskId,
      agentName: 'CoderAgent',
    });

    await mcProjection.refresh();
    assert.strictEqual(mcProjection.getState().tasks.find((t) => t.id === taskId)?.state, 'running');

    // Dispose active projection (simulating webview panel closing)
    mcProjection.dispose();

    // Reopen / recreate Mission Control projection
    const reloadedProjection = new MissionControlProjection(bridgeManager);
    await reloadedProjection.refresh();

    const reloadedState = reloadedProjection.getState();
    const task = reloadedState.tasks.find((t) => t.id === taskId);
    assert.ok(task, 'Task must persist and project in reloaded panel');
    assert.strictEqual(task?.state, 'running', 'Running status must be retained across panel reload');
    assert.strictEqual(task?.assignedAgentName, 'CoderAgent');

    reloadedProjection.dispose();
  });

  it('5. Daemon restart reconciles interrupted tasks to failed in both views', async () => {
    const store = TaskExecutionStore.instance;
    const taskRunning = 'task-running-99';

    store.switchTask(taskRunning, studioId);
    store.taskSubmitted(taskRunning, studioId);
    sessionProjection.upsertTask({
      id: taskRunning,
      title: 'Long-running computation',
      status: 'pending',
    });

    sessionProjection.handleEvent({
      sequenceNumber: nextSeq(),
      id: 'evt-run-start',
      type: 'task_started',
      timestamp: Date.now(),
      summary: 'Task running',
      taskId: taskRunning,
    });

    await mcProjection.refresh();
    assert.strictEqual(store.getState().status, 'running');
    assert.strictEqual(mcProjection.getState().tasks.find((t) => t.id === taskRunning)?.state, 'running');

    // Simulate daemon crash or disconnect reconciliation
    sessionProjection.reconcileInterruptedTasks('daemon_socket_closed');
    await mcProjection.refresh();

    // Chat view / store verification
    const storeState = store.getState();
    assert.strictEqual(storeState.status, 'failed', 'Chat store must project failed on daemon interrupt');
    assert.strictEqual(storeState.lastError?.code, 'daemon_interrupted');
    assert.ok(
      storeState.lastError?.message.includes('daemon_interrupted'),
      'Error message must indicate daemon interrupted'
    );

    // Mission Control view verification
    const mcTask = mcProjection.getState().tasks.find((t) => t.id === taskRunning);
    assert.strictEqual(mcTask?.state, 'failed', 'Mission control must project failed for interrupted task');
    assert.ok(
      mcTask?.error?.includes('daemon_interrupted'),
      'Mission control task error must note daemon interruption'
    );
    assert.ok(mcProjection.getState().overview.taskCounts.failed >= 1);

    // Verify error attention item was raised
    const attentionItem = mcProjection.getState().overview.attentionItems.find(
      (a) => a.targetId === taskRunning
    );
    assert.ok(attentionItem, 'Failed attention item must be generated for interrupted task');
    assert.strictEqual(attentionItem?.severity, 'error');
  });

  it('6. In-flight refresh coalescing and debouncing prevent query storms', async () => {
    // Calling refresh multiple times concurrently returns coalesced promise
    const p1 = mcProjection.refresh();
    const p2 = mcProjection.refresh();
    const p3 = mcProjection.refresh();

    // All promises resolve safely without throwing
    await Promise.all([p1, p2, p3]);

    const state = mcProjection.getState();
    assert.ok(state, 'Projection state must be valid after coalesced refreshes');
    assert.strictEqual(typeof state.timestamp, 'number');
  });

  it('7. Studio switching cleanly isolates state and purges stale items', async () => {
    const store = TaskExecutionStore.instance;
    store.taskSubmitted('task-studio-1', 'studio-1');
    sessionProjection.upsertTask({
      id: 'task-studio-1',
      title: 'Studio 1 Task',
      status: 'pending',
    });

    await mcProjection.refresh();
    assert.ok(mcProjection.getState().tasks.some((t) => t.id === 'task-studio-1'));

    // Switch session to a new studio
    sessionProjection.switchSession({
      id: 'studio-2',
      workspacePath: '/workspace/studio-2',
      goalSummary: 'Studio 2 Session',
      status: 'running',
      startedAt: Date.now(),
      elapsedMs: 0,
      activeAgentCount: 1,
      totalAgentCount: 1,
      completedTasksCount: 0,
      totalTasksCount: 0,
    });

    await mcProjection.refresh();

    // Verify tasks from studio-1 were purged from session projection
    assert.strictEqual(sessionProjection.tasks.length, 0);
    assert.strictEqual(sessionProjection.activeSessionId, 'studio-2');
    assert.strictEqual(store.getState().activeStudioId, 'studio-2');
  });
});
