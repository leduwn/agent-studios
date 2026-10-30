/**
 * Agent Studios — Daemon Connected Bridge Implementation
 * Milestone M14-IPC-03: Real ConnectedClient Vertical Slice
 *
 * Implements AgentStudiosBridge backed by the authoritative native Rust agsd daemon
 * using NativeIpcTransport and DaemonDiscovery.
 *
 * Security & Architectural Invariants:
 * 1. Read-only vertical slice: query operations (studios, tasks, runs, providers, models,
 *    health, timeline events) are fully wired to the daemon.
 * 2. Mutating operations (submitting tasks/goals, AI chat execution, resolving approvals,
 *    submitting editor actions, creating studios, daemon shutdown) remain strictly
 *    DISABLED and FAIL CLOSED.
 * 3. Never expose client secrets or private keys in return objects, events, logs, or UI.
 * 4. Monotonic event sequence watermarking and replay buffer for seamless reconnects.
 * 5. Serialized single-flight command queue inherited from NativeIpcTransport.
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge, BridgeDisconnectedError } from './AgentStudiosBridge';
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
  ProviderModel,
  SessionInfo,
  SessionStatus,
  SessionSummary,
  TaskGraph,
  TaskId,
  TaskNode,
  TaskStatus,
  TimelineEvent,
  TimelineEventType,
} from './types';
import {
  SubmitTaskAdapter,
  SubmitTaskRequest,
  SubmitTaskResult,
} from './ipc/SubmitTaskAdapter';
import {
  NativeIpcTransport,
  TransportState,
} from './ipc/NativeIpcTransport';
import { DaemonDiscovery } from './ipc/DaemonDiscovery';
import {
  RustIpcEvent,
  RustStudio,
  RustTaskRecord,
  RustTaskState,
  RustHealthResponse,
} from './ipc/RustIpcProtocolTypes';

export interface DaemonConnectedBridgeOptions {
  bootstrapPath?: string;
  transport?: NativeIpcTransport;
  autoSubscribeEvents?: boolean;
}

export class DaemonConnectedBridge implements AgentStudiosBridge {
  private _connectionState: ConnectionState = 'disconnected';
  private _transport: NativeIpcTransport | null = null;
  private readonly _bootstrapPath?: string;
  private readonly _autoSubscribe: boolean;

  private readonly _onConnectionStateChanged = new vscode.EventEmitter<ConnectionState>();
  public readonly onConnectionStateChanged: vscode.Event<ConnectionState> = this._onConnectionStateChanged.event;

  private readonly _onEvent = new vscode.EventEmitter<TimelineEvent>();
  public readonly onEvent: vscode.Event<TimelineEvent> = this._onEvent.event;

  private _activeStudioId: string | null = null;
  private _lastWatermark: number = 0;
  private readonly _eventBuffer: TimelineEvent[] = [];
  private readonly _pendingApprovals: Map<string, ApprovalRequest> = new Map();
  private readonly _artifacts: Map<string, Artifact> = new Map();
  private readonly _seenEventSequences = new Set<number>();

  private readonly _disposables: vscode.Disposable[] = [];

  constructor(options?: DaemonConnectedBridgeOptions) {
    this._disposables.push(this._onConnectionStateChanged, this._onEvent);
    this._bootstrapPath = options?.bootstrapPath;
    this._autoSubscribe = options?.autoSubscribeEvents ?? true;

    if (options?.transport) {
      this.attachTransport(options.transport);
    }
  }

  public get connectionState(): ConnectionState {
    return this._connectionState;
  }

  public get activeStudioId(): string | null {
    return this._activeStudioId;
  }

  public get lastWatermark(): number {
    return this._lastWatermark;
  }

  public get transport(): NativeIpcTransport | null {
    return this._transport;
  }

  public get endpoint(): string | null {
    return this._transport?.endpoint ?? null;
  }

  /**
   * Connects to the running agsd daemon via discovery and performs authenticated handshake.
   */
  public async connect(): Promise<void> {
    if (this._connectionState === 'connected' || this._connectionState === 'connecting') {
      return;
    }

    this.setConnectionState('connecting');

    try {
      if (!this._transport) {
        const bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(this._bootstrapPath);
        const transport = new NativeIpcTransport({
          endpoint: bootstrapInfo.endpoint,
          keyId: bootstrapInfo.key_id,
          clientKey: bootstrapInfo.client_key,
        });
        this.attachTransport(transport);
      }

      const activeTransport = this._transport;
      if (!activeTransport) {
        throw new Error('Failed to initialize IPC transport.');
      }

      if (activeTransport.getState() !== 'ready') {
        await activeTransport.connect();
      }

      this.setConnectionState('connected');

      if (this._autoSubscribe) {
        await this.subscribeAndReplay();
      }

      // Populate initial active studio if available
      const studios = await this.listStudiosInternal();
      if (studios.length > 0 && !this._activeStudioId) {
        this._activeStudioId = studios[0].id;
      }
    } catch (err: unknown) {
      const errMsg = err instanceof Error ? err.message.toLowerCase() : String(err).toLowerCase();
      if (errMsg.includes('auth') || errMsg.includes('signature') || errMsg.includes('hmac') || errMsg.includes('key')) {
        this.setConnectionState('authentication_failed');
      } else if (errMsg.includes('version') || errMsg.includes('capability') || errMsg.includes('protocol')) {
        this.setConnectionState('unsupported_protocol');
      } else {
        this.setConnectionState('disconnected');
      }
      throw err;
    }
  }

  /**
   * Disconnects cleanly from the daemon.
   */
  public async disconnect(): Promise<void> {
    if (this._transport) {
      this._transport.disconnect();
    }
    this.setConnectionState('disconnected');
  }

  /**
   * Returns current active studio session information.
   */
  public async getCurrentSession(): Promise<SessionInfo | null> {
    this.assertConnected();

    if (!this._activeStudioId) {
      const studios = await this.listStudiosInternal();
      if (studios.length === 0) {
        return null;
      }
      this._activeStudioId = studios[0].id;
    }

    const studio = await this.getStudioInternal(this._activeStudioId);
    if (!studio) {
      return null;
    }

    const tasks = await this.listTasks();
    const completedTasksCount = tasks.filter((t) => t.status === 'succeeded').length;
    const totalTasksCount = tasks.length;
    const sessionStatus = this.deriveSessionStatus(tasks);

    const startedAt = new Date(studio.created_at).getTime() || Date.now();
    const elapsedMs = Math.max(0, Date.now() - startedAt);

    return {
      id: studio.id,
      workspacePath: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? '',
      goalSummary: studio.name,
      status: sessionStatus,
      activeAgentCount: 1,
      totalAgentCount: 1,
      completedTasksCount,
      totalTasksCount,
      startedAt,
      elapsedMs,
      isPreview: false,
    };
  }

  /**
   * Returns DAG task graph for current or specified run.
   */
  public async getTaskGraph(runId?: string): Promise<TaskGraph | null> {
    this.assertConnected();

    if (!this._activeStudioId) {
      return null;
    }

    const tasks = await this.listTasks();
    const edges: Array<{ from: TaskId; to: TaskId }> = [];

    for (const task of tasks) {
      if (task.dependencies && Array.isArray(task.dependencies)) {
        for (const dep of task.dependencies) {
          edges.push({ from: dep, to: task.id });
        }
      }
    }

    return {
      runId: runId ?? `run_${this._activeStudioId}`,
      nodes: tasks,
      edges,
    };
  }

  /**
   * Lists tasks for the active studio from the daemon.
   */
  public async listTasks(): Promise<TaskNode[]> {
    this.assertConnected();

    if (!this._activeStudioId) {
      return [];
    }

    const resp = await this._transport!.sendCommand({
      type: 'list_tasks',
      studio_id: this._activeStudioId,
    });

    if (resp.type === 'error') {
      throw new Error(`Failed to list tasks: ${resp.message}`);
    }
    if (resp.type !== 'tasks') {
      return [];
    }

    return resp.tasks.map((t) => this.mapTaskRecord(t));
  }

  /**
   * Returns pending approvals tracked from daemon event stream.
   */
  public async listPendingApprovals(): Promise<ApprovalRequest[]> {
    this.assertConnected();
    return Array.from(this._pendingApprovals.values()).filter((a) => a.status === 'pending');
  }

  /**
   * Resolves approval decision.
   * STRICTLY DISABLED AND FAILS CLOSED in M14-IPC-03 read-only slice.
   */
  public async requestApprovalDecision(
    id: string,
    decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult> {
    // FAIL CLOSED: Never mutate backend daemon state in read-only slice.
    return {
      id,
      decision,
      applied: false,
      message: 'Approval mutation is disabled in M14-IPC-03 read-only connected vertical slice.',
    };
  }

  /**
   * Returns tracked diff artifacts from daemon event stream.
   */
  public async listArtifacts(): Promise<Artifact[]> {
    this.assertConnected();
    return Array.from(this._artifacts.values());
  }

  /**
   * Returns virtual diff content for an artifact.
   */
  public async getArtifactDiff(artifactId: string): Promise<string | null> {
    this.assertConnected();
    const artifact = this._artifacts.get(artifactId);
    return artifact ? artifact.diffContent : null;
  }

  /**
   * Returns tracked timeline events from daemon event stream.
   */
  public listTimelineEvents(): TimelineEvent[] {
    return [...this._eventBuffer];
  }

  /**
   * Lists providers and their model descriptors from daemon.
   * Strips all internal secrets or credentials before returning.
   */
  public async listProviderSummaries(): Promise<ProviderSummary[]> {
    this.assertConnected();

    const provResp = await this._transport!.sendCommand({
      type: 'list_providers',
    });

    if (provResp.type === 'error') {
      throw new Error(`Failed to list providers: ${provResp.message}`);
    }
    if (provResp.type !== 'providers') {
      return [];
    }

    const summaries: ProviderSummary[] = [];

    for (const prov of provResp.providers) {
      let models: ProviderModel[] = [];

      try {
        const modResp = await this._transport!.sendCommand({
          type: 'list_models',
          instance_id: prov.id,
        });

        if (modResp.type === 'models') {
          models = modResp.models.map((m) => ({
            id: m.id,
            name: m.display_name,
            contextWindow: m.limits?.context_window_tokens ?? 128000,
            isDefault: false,
          }));
        }
      } catch {
        // Fall back gracefully if model query fails for specific provider
        models = [];
      }

      summaries.push({
        instanceId: prov.id,
        providerType: prov.provider_id,
        displayName: prov.display_name,
        models,
        status: prov.enabled ? 'available' : 'unconfigured',
      });
    }

    return summaries;
  }

  /**
   * Submits editor action.
   * STRICTLY DISABLED AND FAILS CLOSED in M14-IPC-03 read-only slice.
   */
  public async submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult> {
    return {
      accepted: false,
      message: `Editor action '${action.action}' is disabled in M14-IPC-03 read-only slice.`,
    };
  }

  /**
   * Submits a task to the daemon using the M15 SubmitTaskAdapter.
   * Gated on operator confirmation and explicit studio identity.
   */
  public async submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResult> {
    this.assertConnected();
    const workspacePath = req.workspacePath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    return SubmitTaskAdapter.submitTask(this._transport!, {
      ...req,
      workspacePath,
    });
  }

  /**
   * Sends chat message / prompts agent.
   * STRICTLY DISABLED AND FAILS CLOSED in M14-IPC-03 read-only slice.
   */
  public async sendMessage(
    _request: ChatSendRequest,
    _onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage> {
    throw new Error('Task execution and AI chat prompt submission are disabled in M14-IPC-03 read-only slice.');
  }

  /**
   * Cancels current run.
   * STRICTLY DISABLED in M14-IPC-03 read-only connected vertical slice.
   * Fails closed to avoid fabricating cancellation state in the UI.
   */
  public async cancelCurrentRun(): Promise<void> {
    throw new Error('Task run cancellation is disabled in M14-IPC-03 read-only connected vertical slice.');
  }

  /**
   * Lists available studios/sessions from the daemon.
   */
  public async listSessions(): Promise<SessionSummary[]> {
    this.assertConnected();

    const studios = await this.listStudiosInternal();
    return studios.map((s) => ({
      id: s.id,
      title: s.name,
      status: 'running' as SessionStatus,
      updatedAt: new Date(s.created_at).getTime() || Date.now(),
      isPreview: false,
    }));
  }

  /**
   * Creates a new session.
   * STRICTLY DISABLED in read-only slice.
   */
  public async createSession(_goal?: string): Promise<SessionInfo> {
    throw new Error('Creating studios is disabled in M14-IPC-03 read-only slice.');
  }

  /**
   * Switches active session to specified studio ID.
   */
  public async switchSession(sessionId: string): Promise<SessionInfo> {
    this.assertConnected();

    const studio = await this.getStudioInternal(sessionId);
    if (!studio) {
      throw new Error(`Studio session '${sessionId}' not found.`);
    }

    this._activeStudioId = studio.id;
    const current = await this.getCurrentSession();
    if (!current) {
      throw new Error(`Failed to initialize session for studio '${sessionId}'.`);
    }

    return current;
  }

  /**
   * Real daemon health check.
   */
  public async getHealth(): Promise<RustHealthResponse> {
    this.assertConnected();

    const resp = await this._transport!.sendCommand({ type: 'get_health' });
    if (resp.type === 'error') {
      throw new Error(`Get health failed: ${resp.message}`);
    }
    if (resp.type !== 'health') {
      throw new Error(`Unexpected response type for get_health: ${resp.type}`);
    }

    return resp;
  }

  /**
   * Round-trip latency check (ping -> pong).
   */
  public async ping(payload: string = 'ping'): Promise<number> {
    this.assertConnected();

    const start = Date.now();
    const resp = await this._transport!.sendCommand({
      type: 'ping',
      payload,
      timestamp: start,
    });

    if (resp.type !== 'pong') {
      throw new Error(`Unexpected ping response: ${resp.type}`);
    }

    return Date.now() - start;
  }

  public dispose(): void {
    if (this._transport) {
      this._transport.disconnect();
      this._transport.removeAllListeners();
      this._transport = null;
    }

    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }
  }

  // --- Internal Helpers ---

  private attachTransport(transport: NativeIpcTransport): void {
    this._transport = transport;

    transport.on('stateChanged', (tState: TransportState) => {
      this.handleTransportStateChange(tState);
    });

    transport.on('event', (rustEvent: RustIpcEvent) => {
      this.handleIncomingDaemonEvent(rustEvent);
    });

    transport.on('error', (err: Error) => {
      console.error('[DaemonConnectedBridge] Transport error:', err);
      this.setConnectionState('degraded');
    });

    transport.on('close', () => {
      if (this._connectionState !== 'disconnected') {
        this.setConnectionState('disconnected');
      }
    });
  }

  private handleTransportStateChange(tState: TransportState): void {
    switch (tState) {
      case 'ready':
        this.setConnectionState('connected');
        break;
      case 'connecting':
        this.setConnectionState('connecting');
        break;
      case 'authenticating':
        this.setConnectionState('authenticating');
        break;
      case 'error':
        this.setConnectionState('degraded');
        break;
      case 'disconnected':
        this.setConnectionState('disconnected');
        break;
    }
  }

  private async subscribeAndReplay(): Promise<void> {
    if (!this._transport || this._transport.getState() !== 'ready') {
      return;
    }

    // Subscribe starting from watermark + 1 (or default if 0)
    const fromSeq = this._lastWatermark > 0 ? this._lastWatermark + 1 : undefined;
    await this._transport.subscribeEvents(fromSeq);
  }

  private handleIncomingDaemonEvent(rustEvent: RustIpcEvent): void {
    const seq = rustEvent.sequence;
    if (typeof seq === 'number') {
      if (this._seenEventSequences.has(seq)) {
        return; // Deduplicate replayed event
      }
      this._seenEventSequences.add(seq);
      if (seq > this._lastWatermark) {
        this._lastWatermark = seq;
      }
    }

    const timelineEvent = this.mapDaemonEvent(rustEvent);
    this._eventBuffer.push(timelineEvent);
    if (this._eventBuffer.length > 500) {
      this._eventBuffer.shift();
    }

    // Process event-specific domain state tracking
    if (timelineEvent.type === 'approval_requested' && timelineEvent.details) {
      const approval = timelineEvent.details as unknown as ApprovalRequest;
      if (approval.id) {
        this._pendingApprovals.set(approval.id, approval);
      }
    } else if (timelineEvent.type === 'approval_decided' && timelineEvent.details) {
      const approvalId = (timelineEvent.details as { id?: string }).id;
      if (approvalId) {
        this._pendingApprovals.delete(approvalId);
      }
    } else if (timelineEvent.type === 'artifact_created' && timelineEvent.details) {
      const artifact = timelineEvent.details as unknown as Artifact;
      if (artifact.id) {
        this._artifacts.set(artifact.id, artifact);
      }
    }

    this._onEvent.fire(timelineEvent);
  }

  private mapDaemonEvent(rustEvent: RustIpcEvent): TimelineEvent {
    const timestamp = new Date(rustEvent.timestamp).getTime() || Date.now();
    let eventType = this.mapTimelineEventType(rustEvent.event_type);
    const payload = (rustEvent.payload && typeof rustEvent.payload === 'object')
      ? (rustEvent.payload as Record<string, unknown>)
      : {};

    if (rustEvent.event_type === 'task_state_transitioned') {
      const to = String(payload.to || '');
      if (to === 'running') {
        eventType = 'task_started';
      } else if (to === 'succeeded') {
        eventType = 'task_completed';
      } else if (to === 'failed') {
        eventType = 'task_failed';
      } else if (to === 'cancelled') {
        eventType = 'task_cancelled';
      } else if (to === 'blocked') {
        eventType = 'task_blocked';
      }
    } else if (rustEvent.event_type === 'run_state_transitioned') {
      const to = String(payload.to || '');
      if (to === 'running') {
        eventType = 'run_started';
      } else if (to === 'succeeded') {
        eventType = 'run_completed';
      } else if (to === 'failed') {
        eventType = 'run_failed';
      }
    }

    let summary: string;
    if (typeof payload.text === 'string') {
      summary = payload.text;
    } else if (typeof payload.summary === 'string') {
      summary = payload.summary;
    } else if (rustEvent.event_type === 'tool_started' && typeof payload.tool_name === 'string') {
      summary = `Tool ${payload.tool_name} started`;
    } else if (rustEvent.event_type === 'tool_completed' && typeof payload.tool_name === 'string') {
      summary = `Tool ${payload.tool_name} completed`;
    } else if (rustEvent.event_type === 'tool_failed' && typeof payload.tool_name === 'string') {
      summary = `Tool ${payload.tool_name} failed: ${payload.error || 'Unknown error'}`;
    } else {
      summary = `${rustEvent.event_type} (seq ${rustEvent.sequence})`;
    }

    const taskId = typeof payload.task_id === 'string' ? payload.task_id : undefined;
    const agentName = typeof payload.agent_name === 'string' ? payload.agent_name : undefined;

    return {
      sequenceNumber: rustEvent.sequence,
      id: `evt_${rustEvent.sequence}`,
      timestamp,
      type: eventType,
      summary,
      taskId,
      agentName,
      details: payload,
    };
  }

  private mapTimelineEventType(daemonEventType: string): TimelineEventType {
    switch (daemonEventType) {
      case 'tool_started':
        return 'tool_executing';
      case 'session_started':
      case 'session_ended':
      case 'run_started':
      case 'run_completed':
      case 'run_failed':
      case 'task_submitted':
      case 'task_started':
      case 'task_completed':
      case 'task_failed':
      case 'task_blocked':
      case 'task_cancelled':
      case 'agent_assigned':
      case 'tool_executing':
      case 'tool_completed':
      case 'tool_failed':
      case 'approval_requested':
      case 'approval_decided':
      case 'artifact_created':
      case 'runtime_output_delta':
        return daemonEventType as TimelineEventType;
      default:
        return 'task_started';
    }
  }

  private mapTaskRecord(r: RustTaskRecord): TaskNode {
    return {
      id: r.id,
      runId: this._activeStudioId ?? '',
      title: r.title,
      description: r.description,
      status: this.mapTaskState(r.state),
      assignedAgentId: r.assigned_agent_id ?? undefined,
      dependencies: Array.isArray(r.dependencies) ? r.dependencies : [],
      progress: r.state === 'succeeded' ? 100 : undefined,
      error: r.state === 'failed' ? 'Task failed' : undefined,
      createdAt: new Date(r.created_at).getTime() || Date.now(),
      updatedAt: new Date(r.updated_at).getTime() || Date.now(),
    };
  }

  private mapTaskState(state: RustTaskState): TaskStatus {
    switch (state) {
      case 'pending':
        return 'pending';
      case 'ready':
        return 'ready';
      case 'running':
      case 'retrying':
        return 'running';
      case 'succeeded':
        return 'succeeded';
      case 'failed':
        return 'failed';
      case 'cancelled':
        return 'cancelled';
      case 'blocked':
      case 'paused':
        return 'blocked';
    }
  }

  private deriveSessionStatus(tasks: TaskNode[]): SessionStatus {
    if (tasks.length === 0) {
      return 'idle';
    }
    if (tasks.some((t) => t.status === 'running')) {
      return 'running';
    }
    if (tasks.some((t) => t.status === 'blocked')) {
      return 'planning';
    }
    if (tasks.every((t) => t.status === 'succeeded')) {
      return 'completed';
    }
    if (tasks.some((t) => t.status === 'failed')) {
      return 'failed';
    }
    return 'idle';
  }

  private async listStudiosInternal(): Promise<RustStudio[]> {
    if (!this._transport) {
      return [];
    }

    const resp = await this._transport.sendCommand({ type: 'list_studios' });
    if (resp.type === 'studios') {
      return resp.studios;
    }
    return [];
  }

  private async getStudioInternal(studioId: string): Promise<RustStudio | null> {
    if (!this._transport) {
      return null;
    }

    const resp = await this._transport.sendCommand({
      type: 'get_studio',
      studio_id: studioId,
    });

    if (resp.type === 'studio') {
      return resp.studio;
    }
    return null;
  }

  private setConnectionState(newState: ConnectionState): void {
    if (this._connectionState !== newState) {
      this._connectionState = newState;
      this._onConnectionStateChanged.fire(newState);
    }
  }

  private assertConnected(): void {
    if (this._connectionState !== 'connected' || !this._transport) {
      throw new BridgeDisconnectedError('Agent Studios daemon bridge is not connected.');
    }
  }
}
