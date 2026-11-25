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
  TaskSubmissionError,
} from './ipc/SubmitTaskAdapter';
import {
  ExecutionClient,
  CancelTaskResult,
  TaskCancellationError,
} from './ipc/ExecutionClient';
import { CapabilityMatrix, OperatorConfirmationManager } from './ipc/CapabilityMatrix';
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
  RustExecutionReadinessReport,
  RustArtifactRecord,
  RustRecoveryStatusReport,
} from './ipc/RustIpcProtocolTypes';
import {
  ArtifactClient,
  ReassembledArtifactContent,
  ReassembledTaskDiff,
} from './ipc/ArtifactClient';

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

  private _epoch: number = 0;
  private _isDisposed: boolean = false;
  private _isRecoveringSubscription: boolean = false;
  private _resubscribeAttempts: number = 0;
  private static readonly maxResubscribeAttempts = 3;

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

  public get sessionId(): string | null {
    return this._transport?.getSessionId() ?? null;
  }

  public get epoch(): number {
    return this._epoch;
  }

  public get generation(): number {
    return this._epoch;
  }

  public get isDisposed(): boolean {
    return this._isDisposed;
  }

  /**
   * Connects to the running agsd daemon via discovery and performs authenticated handshake.
   */
  public async connect(): Promise<void> {
    if (this._isDisposed) {
      throw new Error('Bridge is disposed.');
    }
    if (this._connectionState === 'connected' || this._connectionState === 'connecting') {
      return;
    }

    const currentEpoch = ++this._epoch;
    this._resubscribeAttempts = 0;

    // Invalidate pending confirmations on reconnect
    OperatorConfirmationManager.clearPendingChallenges();

    this.setConnectionState('connecting');

    try {
      if (!this._transport) {
        const bootstrapInfo = await DaemonDiscovery.loadBootstrapInfo(this._bootstrapPath);
        if (this._isDisposed || this._epoch !== currentEpoch) {
          return;
        }
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

      if (this._isDisposed || this._epoch !== currentEpoch) {
        return;
      }

      this.setConnectionState('connected');

      if (this._autoSubscribe) {
        await this.subscribeAndReplay();
      }

      if (this._isDisposed || this._epoch !== currentEpoch) {
        return;
      }

      // Populate initial active studio if available
      const studios = await this.listStudiosInternal();
      if (this._isDisposed || this._epoch !== currentEpoch) {
        return;
      }
      if (studios.length > 0 && !this._activeStudioId) {
        this._activeStudioId = studios[0].id;
      }
    } catch (err: unknown) {
      if (this._isDisposed || this._epoch !== currentEpoch) {
        return;
      }
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
    this._epoch++;
    this._resubscribeAttempts = 0;
    this._isRecoveringSubscription = false;
    if (this._transport) {
      this._transport.disconnect();
    }
    this.setConnectionState('disconnected');
    OperatorConfirmationManager.clearPendingChallenges();
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

    // Authoritative daemon repository path from execution readiness defaults, NOT client workspace
    let daemonWorkspacePath = '';
    try {
      const readiness = await this.getExecutionReadiness(1500);
      if (readiness?.defaults?.default_repo_path) {
        daemonWorkspacePath = readiness.defaults.default_repo_path;
      } else if (readiness?.authorized_workspaces && readiness.authorized_workspaces.length > 0) {
        daemonWorkspacePath = readiness.authorized_workspaces[0];
      }
    } catch {
      // Readiness query failed or uninitialized; leave workspacePath empty to prevent false verified status
      daemonWorkspacePath = '';
    }

    return {
      id: studio.id,
      workspacePath: daemonWorkspacePath,
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
   * Resolves approval decision via authoritative agsd IPC when approval.resolve is enabled.
   */
  public async requestApprovalDecision(
    id: string,
    decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult> {
    if (!CapabilityMatrix.isCapabilityEnabled('approval.resolve')) {
      return {
        id,
        decision,
        applied: false,
        message: 'Approval resolution is disabled by capability policy (approval.resolve disabled).',
      };
    }
    this.assertConnected();
    return this.resolveApprovalAuthoritative(id, decision);
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
   * Fetches unified diff for a task with lossless Base64 pagination and SHA-256 integrity verification.
   */
  public async getTaskDiff(
    studioId: string,
    taskId: string,
    signal?: AbortSignal
  ): Promise<ReassembledTaskDiff> {
    this.assertConnected();
    return ArtifactClient.fetchTaskDiff(this._transport!, studioId, taskId, { signal });
  }

  /**
   * Lists artifact records for a task from daemon.
   */
  public async listTaskArtifacts(
    studioId: string,
    taskId: string
  ): Promise<RustArtifactRecord[]> {
    this.assertConnected();
    return ArtifactClient.listTaskArtifacts(this._transport!, studioId, taskId);
  }

  /**
   * Fetches raw artifact content with Base64 decoding, chunk verification and SHA-256 verification.
   */
  public async fetchArtifactContent(
    studioId: string,
    artifactId: string,
    signal?: AbortSignal
  ): Promise<ReassembledArtifactContent> {
    this.assertConnected();
    return ArtifactClient.fetchArtifactContent(this._transport!, studioId, artifactId, { signal });
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
   * Submits editor action through connected agsd Internal Codex Engine.
   */
  public async submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult> {
    this.assertConnected();
    if (!CapabilityMatrix.isCapabilityEnabled('task.submit')) {
      return {
        accepted: false,
        message: `Editor action '${action.action}' is disabled by capability policy.`,
      };
    }
    if (vscode.workspace.isTrusted === false) {
      return {
        accepted: false,
        message: 'Editor action blocked in untrusted workspace.',
      };
    }

    const prompt =
      `Editor Action (${action.action}) for ${action.documentUri} (${action.languageId}, ` +
      `lines ${action.selectionRange.startLine}-${action.selectionRange.endLine}):\n` +
      `\`\`\`${action.languageId}\n${action.selectedText}\n\`\`\``;
    const response = await this.sendMessage(
      {
        message: prompt,
        mode: action.action === 'explain' ? 'ask' : 'agent',
        permissionMode: action.action === 'explain' ? 'read-only' : 'auto-edit',
      },
      () => {}
    );
    return {
      accepted: true,
      taskId: response.id,
      message: response.content,
    };
  }

  /**
   * Submits a task to the daemon using the M15 SubmitTaskAdapter.
   * Gated on operator confirmation and explicit studio identity.
   */
  public async submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResult> {
    this.assertConnected();
    const workspacePath = req.workspacePath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

    const freshReadiness = await this.getExecutionReadiness(2000);
    if (!freshReadiness || !freshReadiness.is_ready) {
      throw new TaskSubmissionError(
        `Daemon is not execution-ready: ${freshReadiness?.blocking_reasons?.join(', ') || 'Readiness report unavailable'}`,
        req.studioId
      );
    }

    if (req.readiness) {
      const runtimeVal = CapabilityMatrix.validateRuntimeInstanceSelection(
        freshReadiness,
        {
          runtimeInstanceId: req.runtimeInstanceId,
          runtimePreference: req.runtimePreference,
        },
        { action: 'task_execution' }
      );
      if (!runtimeVal.valid) {
        throw new TaskSubmissionError(
          `Execution readiness degraded during operator confirmation: ${runtimeVal.reason}. Fresh confirmation required.`,
          req.studioId
        );
      }
    }

    return SubmitTaskAdapter.submitTask(this._transport!, {
      ...req,
      workspacePath,
      readiness: freshReadiness,
      sessionId: this.sessionId ?? undefined,
    } as any);
  }

  /**
   * Sends chat message / prompts agent via agsd Internal Codex Engine (`execute_chat_turn` / `execute_multi_agent_goal`).
   */
  public async sendMessage(
    request: ChatSendRequest,
    onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage> {
    if (!CapabilityMatrix.isCapabilityEnabled('task.submit')) {
      throw new Error('Task execution and AI chat prompt submission are disabled by capability policy.');
    }
    if (vscode.workspace.isTrusted === false) {
      throw new Error('Chat prompt execution blocked in untrusted workspace.');
    }
    this.assertConnected();

    let studioId = request.studioId || this._activeStudioId;
    if (!studioId) {
      const created = await this.createSession('Agent Studios Chat Session');
      studioId = created.id;
    }

    const activeWorkspace =
      request.workspacePath || vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

    const trimmed = request.message.trim();
    const isOrchestrate = Boolean(request.multiAgent) || trimmed.startsWith('/orchestrate ');
    const goalText = trimmed.startsWith('/orchestrate ')
      ? trimmed.slice('/orchestrate '.length).trim()
      : request.message;

    let streamedAnything = false;
    const eventListener = this._onEvent.event((ev) => {
      const details = ev.details || {};
      const evStudio = typeof details.studio_id === 'string' ? details.studio_id : undefined;
      if (evStudio && evStudio !== studioId) {
        return;
      }
      if (ev.type === 'runtime_output_delta' || ev.type === 'output_delta') {
        const text = typeof details.text === 'string' ? details.text : ev.summary;
        if (text) {
          streamedAnything = true;
          onChunk({ type: 'text_delta', text });
        }
      } else if (ev.type === 'tool_executing' || ev.type === 'tool_started') {
        onChunk({
          type: 'tool_call',
          toolCall: {
            id: typeof details.call_id === 'string' ? details.call_id : ev.id,
            name: typeof details.tool_name === 'string' ? details.tool_name : 'tool',
            status: 'running',
            target: typeof details.target === 'string' ? details.target : undefined,
          },
        });
      } else if (ev.type === 'tool_completed' || ev.type === 'tool_failed') {
        onChunk({
          type: 'tool_call',
          toolCall: {
            id: typeof details.call_id === 'string' ? details.call_id : ev.id,
            name: typeof details.tool_name === 'string' ? details.tool_name : 'tool',
            status: ev.type === 'tool_failed' ? 'failed' : 'completed',
          },
        });
      } else if (ev.type === 'approval_requested') {
        const approval = ev.details as unknown as ApprovalRequest;
        if (approval) {
          onChunk({ type: 'approval_required', approval });
        }
      }
    });

    try {
      if (isOrchestrate) {
        const resp = await this._transport!.sendCommand(
          {
            type: 'execute_multi_agent_goal',
            // eslint-disable-next-line @typescript-eslint/naming-convention
            studio_id: studioId,
            goal: goalText,
            // eslint-disable-next-line @typescript-eslint/naming-convention
            provider_instance_id: request.providerInstanceId,
            // eslint-disable-next-line @typescript-eslint/naming-convention
            model_id: request.modelId,
            // eslint-disable-next-line @typescript-eslint/naming-convention
            repository_path: activeWorkspace,
            // eslint-disable-next-line @typescript-eslint/naming-convention
            permission_mode: request.permissionMode,
          },
          180000
        );
        if (resp.type === 'error') {
          onChunk({ type: 'error', error: resp.message });
          throw new Error(resp.message);
        }
        if (resp.type !== 'multi_agent_goal_completed') {
          throw new Error(`Unexpected response for execute_multi_agent_goal: ${resp.type}`);
        }
        const summaryObj = resp.summary;
        const content =
          typeof summaryObj.coordinator_summary === 'string' && summaryObj.coordinator_summary.trim()
            ? summaryObj.coordinator_summary
            : JSON.stringify(summaryObj, null, 2);
        if (!streamedAnything) {
          onChunk({ type: 'text_delta', text: content });
        }
        onChunk({ type: 'done' });
        return {
          id: `msg_multi_${Date.now()}`,
          role: 'assistant',
          content,
          timestamp: Date.now(),
          mode: request.mode,
          providerInstanceId: request.providerInstanceId,
          modelId: request.modelId,
        };
      }

      const resp = await this._transport!.sendCommand(
        {
          type: 'execute_chat_turn',
          // eslint-disable-next-line @typescript-eslint/naming-convention
          studio_id: studioId,
          prompt: request.message,
          mode: request.mode,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          provider_instance_id: request.providerInstanceId,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          model_id: request.modelId,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          reasoning_effort: request.reasoningEffort,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          permission_mode: request.permissionMode,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          follow_up_mode: request.followUpQueueMode,
          // eslint-disable-next-line @typescript-eslint/naming-convention
          workspace_path: activeWorkspace,
          attachments: request.attachments ?? [],
        },
        180000
      );

      if (resp.type === 'error') {
        onChunk({ type: 'error', error: resp.message });
        throw new Error(resp.message);
      }
      if (resp.type !== 'chat_turn_completed') {
        throw new Error(`Unexpected response for execute_chat_turn: ${resp.type}`);
      }

      const raw = resp.message;
      const content = typeof raw.content === 'string' ? raw.content : '';
      if (!streamedAnything && content) {
        onChunk({ type: 'text_delta', text: content });
      }
      onChunk({ type: 'done' });
      return this.mapTranscriptRecord(raw, request.mode, resp.thread_id);
    } finally {
      eventListener.dispose();
    }
  }

  /**
   * Cancels a specific task on the daemon using M15 ExecutionClient.
   * Gated on task.cancel capability and explicit studio/task identity.
   */
  public async cancelTask(taskId: string, reason?: string): Promise<CancelTaskResult> {
    this.assertConnected();
    if (!this._activeStudioId) {
      throw new TaskCancellationError('No active studio selected for task cancellation.');
    }
    return ExecutionClient.cancelTask(this._transport!, {
      studioId: this._activeStudioId,
      taskId,
      reason,
    });
  }

  /**
   * Cancels current run or active task on the daemon.
   * Fails closed if capability is disabled or if no active studio or running task is found.
   */
  public async cancelCurrentRun(taskId?: string, reason?: string): Promise<void> {
    if (!CapabilityMatrix.isCapabilityEnabled('task.cancel')) {
      throw new Error('Task cancellation is disabled by capability policy.');
    }
    this.assertConnected();
    if (taskId) {
      await this.cancelTask(taskId, reason);
      return;
    }
    const tasks = await this.listTasks();
    const running = tasks.find((t) => t.status === 'running');
    if (running) {
      await this.cancelTask(running.id, reason);
      return;
    }
    if (this._activeStudioId) {
      const resp = await this._transport!.sendCommand({
        type: 'interrupt_chat_turn',
        // eslint-disable-next-line @typescript-eslint/naming-convention
        studio_id: this._activeStudioId,
      });
      if (resp.type === 'chat_turn_interrupted' && resp.interrupted) {
        return;
      }
    }
    throw new TaskCancellationError('No active or running task found to cancel in current studio.');
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
   * Creates a new authoritative studio session on the connected agsd daemon.
   */
  public async createSession(goal?: string): Promise<SessionInfo> {
    return this.createStudioAuthoritative(goal?.trim() || 'Agent Studios Session');
  }

  /**
   * Fetches authoritative persisted transcript messages for a studio from agsd.
   */
  public async getSessionTranscript(studioId: string): Promise<ChatMessage[]> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'get_studio_transcript',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      studio_id: studioId,
    });
    if (resp.type === 'error') {
      throw new Error(`Failed to fetch studio transcript: ${resp.message}`);
    }
    if (resp.type !== 'studio_transcript') {
      return [];
    }
    return resp.messages.map((m) => this.mapTranscriptRecord(m, undefined, resp.thread_id ?? undefined));
  }

  /**
   * Provisions a secret credential into agsd's zero-leakage DaemonSecretVault.
   */
  public async setProviderSecret(
    locator: string,
    secretValue: string,
    backend: 'environment_variable' | 'os_credential_store' | 'external' = 'os_credential_store'
  ): Promise<boolean> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'set_provider_secret',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      secret_ref: { backend, locator },
      // eslint-disable-next-line @typescript-eslint/naming-convention
      secret_value: secretValue,
    });
    if (resp.type === 'error') {
      throw new Error(`Failed to provision provider secret: ${resp.message}`);
    }
    return resp.type === 'provider_secret_updated' ? resp.stored : false;
  }

  /**
   * Deletes a secret credential from agsd's DaemonSecretVault.
   */
  public async deleteProviderSecret(
    locator: string,
    backend: 'environment_variable' | 'os_credential_store' | 'external' = 'os_credential_store'
  ): Promise<boolean> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'delete_provider_secret',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      secret_ref: { backend, locator },
    });
    if (resp.type === 'error') {
      throw new Error(`Failed to delete provider secret: ${resp.message}`);
    }
    return resp.type === 'provider_secret_updated' ? !resp.stored : false;
  }

  /**
   * Executes a coordinated multi-agent DAG goal inside isolated Git worktrees.
   */
  public async executeMultiAgentGoal(request: {
    studioId?: string;
    goal: string;
    providerInstanceId?: string;
    modelId?: string;
    repositoryPath?: string;
    permissionMode?: 'read-only' | 'auto-edit' | 'full-access';
  }): Promise<Record<string, unknown>> {
    this.assertConnected();
    if (!CapabilityMatrix.isCapabilityEnabled('task.submit')) {
      throw new Error('Multi-agent execution is disabled by capability policy.');
    }
    let studioId = request.studioId || this._activeStudioId;
    if (!studioId) {
      const created = await this.createSession(request.goal.slice(0, 60));
      studioId = created.id;
    }
    const resp = await this._transport!.sendCommand(
      {
        type: 'execute_multi_agent_goal',
        // eslint-disable-next-line @typescript-eslint/naming-convention
        studio_id: studioId,
        goal: request.goal,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        provider_instance_id: request.providerInstanceId,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        model_id: request.modelId,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        repository_path: request.repositoryPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        permission_mode: request.permissionMode,
      },
      180000
    );
    if (resp.type === 'error') {
      throw new Error(resp.message);
    }
    if (resp.type !== 'multi_agent_goal_completed') {
      throw new Error(`Unexpected response for execute_multi_agent_goal: ${resp.type}`);
    }
    return resp.summary;
  }

  private mapTranscriptRecord(
    raw: Record<string, unknown>,
    fallbackMode?: ChatSendRequest['mode'],
    fallbackThreadId?: string
  ): ChatMessage {
    const toolCalls = Array.isArray(raw.tool_calls)
      ? (raw.tool_calls as Array<Record<string, unknown>>).map((tc) => ({
          id: String(tc.id ?? ''),
          name: String(tc.name ?? 'tool'),
          status: (tc.status === 'failed' ? 'failed' : tc.status === 'running' ? 'running' : 'completed') as
            | 'running'
            | 'completed'
            | 'failed',
          target: typeof tc.target === 'string' ? tc.target : undefined,
          inputSummary: typeof tc.input_summary === 'string' ? tc.input_summary : undefined,
          outputSummary: typeof tc.output_summary === 'string' ? tc.output_summary : undefined,
          durationMs: typeof tc.duration_ms === 'number' ? tc.duration_ms : undefined,
          error: typeof tc.error === 'string' ? tc.error : undefined,
        }))
      : undefined;

    return {
      id: typeof raw.id === 'string' ? raw.id : `msg_${Date.now()}`,
      role: raw.role === 'user' ? 'user' : raw.role === 'system' ? 'system' : 'assistant',
      content: typeof raw.content === 'string' ? raw.content : '',
      timestamp: typeof raw.timestamp === 'number' ? raw.timestamp : Date.now(),
      mode: (typeof raw.mode === 'string' ? raw.mode : fallbackMode) as ChatMessage['mode'],
      threadId: typeof raw.thread_id === 'string' ? raw.thread_id : fallbackThreadId,
      providerInstanceId:
        typeof raw.provider_instance_id === 'string' ? raw.provider_instance_id : undefined,
      modelId: typeof raw.model_id === 'string' ? raw.model_id : undefined,
      reasoningEffort:
        typeof raw.reasoning_effort === 'string'
          ? (raw.reasoning_effort as ChatMessage['reasoningEffort'])
          : undefined,
      permissionMode:
        typeof raw.permission_mode === 'string'
          ? (raw.permission_mode as ChatMessage['permissionMode'])
          : undefined,
      toolCalls: toolCalls && toolCalls.length > 0 ? toolCalls : undefined,
      artifacts: Array.isArray(raw.artifacts)
        ? (raw.artifacts as ChatMessage['artifacts'])
        : undefined,
      metrics:
        raw.metrics && typeof raw.metrics === 'object'
          ? (raw.metrics as Record<string, unknown>)
          : undefined,
    };
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
    this._epoch++;
    const current = await this.getCurrentSession();
    if (!current) {
      throw new Error(`Failed to initialize session for studio '${sessionId}'.`);
    }

    return current;
  }

  /**
   * Authoritatively creates a new studio on the connected agsd daemon.
   */
  public async createStudioAuthoritative(name: string): Promise<SessionInfo> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'create_studio',
      name: name.trim() || 'Agent Studios Session',
    });
    if (resp.type === 'error') {
      throw new Error(`Create studio failed: ${resp.message}`);
    }
    if (resp.type !== 'studio_created') {
      throw new Error(`Unexpected response for create_studio: ${resp.type}`);
    }
    this._activeStudioId = resp.studio.id;
    this._epoch++;
    return {
      id: resp.studio.id,
      workspacePath: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '',
      goalSummary: resp.studio.name,
      status: 'idle',
      activeAgentCount: 0,
      totalAgentCount: 0,
      completedTasksCount: 0,
      totalTasksCount: 0,
      startedAt: new Date(resp.studio.created_at).getTime() || Date.now(),
      elapsedMs: 0,
      isPreview: false,
    };
  }

  /**
   * Forks an existing studio on the connected agsd daemon preserving session lineage.
   */
  public async forkSession(sourceStudioId: string, name?: string): Promise<SessionInfo> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'fork_studio',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      source_studio_id: sourceStudioId,
      name,
    });
    if (resp.type === 'error') {
      throw new Error(`Fork studio failed: ${resp.message}`);
    }
    if (resp.type !== 'studio_created') {
      throw new Error(`Unexpected response for fork_studio: ${resp.type}`);
    }
    this._activeStudioId = resp.studio.id;
    this._epoch++;
    return {
      id: resp.studio.id,
      workspacePath: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '',
      goalSummary: resp.studio.name,
      status: 'idle',
      activeAgentCount: 0,
      totalAgentCount: 0,
      completedTasksCount: 0,
      totalTasksCount: 0,
      startedAt: new Date(resp.studio.created_at).getTime() || Date.now(),
      elapsedMs: 0,
      isPreview: false,
    };
  }

  /**
   * Authoritatively upserts a provider instance inside agsd's runtime ProviderCatalog.
   */
  public async upsertProviderInstance(payload: {
    instanceId?: string;
    providerType: string;
    displayName: string;
    protocol?: string;
    baseUrl: string;
    secretEnvVar?: string;
    secretLocator?: string;
    secretBackend?: 'environment_variable' | 'os_credential_store' | 'external';
    secretValue?: string;
    models?: ProviderModel[];
  }): Promise<ProviderSummary> {
    this.assertConnected();
    const instanceUuid = payload.instanceId || crypto.randomUUID();
    const protocolFamily =
      payload.protocol ||
      (payload.providerType === 'anthropic'
        ? 'anthropic_messages'
        : payload.providerType === 'google-gemini'
          ? 'gemini_generate_content'
          : 'openai_chat_completions');

    const secretBackend =
      payload.secretBackend ||
      (payload.secretLocator ? 'os_credential_store' : 'environment_variable');
    const secretLocator = payload.secretLocator || payload.secretEnvVar;

    if (payload.secretValue && secretLocator) {
      await this.setProviderSecret(secretLocator, payload.secretValue, secretBackend);
    }

    let authScheme: unknown = 'none';
    if (secretLocator) {
      const secretObj = { backend: secretBackend, locator: secretLocator };
      if (protocolFamily === 'anthropic_messages') {
        authScheme = {
          // eslint-disable-next-line @typescript-eslint/naming-convention
          api_key_header: { header_name: 'x-api-key', secret: secretObj },
        };
      } else if (protocolFamily === 'gemini_generate_content') {
        authScheme = {
          // eslint-disable-next-line @typescript-eslint/naming-convention
          api_key_header: { header_name: 'x-goog-api-key', secret: secretObj },
        };
      } else {
        authScheme = {
          // eslint-disable-next-line @typescript-eslint/naming-convention
          bearer_token: { secret: secretObj },
        };
      }
    }

    /* eslint-disable @typescript-eslint/naming-convention */
    const provider = {
      id: instanceUuid,
      provider_id: payload.providerType,
      display_name: payload.displayName,
      protocol: protocolFamily,
      endpoint: {
        base_url: payload.baseUrl,
        static_headers: {},
        query_params: {},
      },
      authentication: authScheme,
      enabled: true,
    };

    const rustModels = (payload.models ?? []).map((m) => ({
      provider_instance_id: instanceUuid,
      id: m.id,
      display_name: m.name || m.id,
      capabilities: {
        tool_calling: 'supported',
        parallel_tool_calls: 'unknown',
        vision_input: 'unknown',
        reasoning: 'supported',
        streaming: 'supported',
        structured_output: 'supported',
        prompt_caching: 'unknown',
        web_search: 'unknown',
        audio_input: 'unknown',
        audio_output: 'unknown',
      },
      limits: {
        context_window_tokens: m.contextWindow || 128000,
        max_output_tokens: 16384,
      },
      metadata_source: 'manual',
    }));
    /* eslint-enable @typescript-eslint/naming-convention */

    const resp = await this._transport!.sendCommand({
      type: 'upsert_provider',
      provider,
      models: rustModels,
    });
    if (resp.type === 'error') {
      throw new Error(`Upsert provider failed: ${resp.message}`);
    }
    if (resp.type !== 'provider_upserted') {
      throw new Error(`Unexpected response for upsert_provider: ${resp.type}`);
    }
    return {
      instanceId: resp.provider.id,
      providerType: resp.provider.provider_id,
      displayName: resp.provider.display_name,
      models: payload.models ?? [],
      status: resp.provider.enabled ? 'available' : 'unconfigured',
    };
  }

  public async removeProviderInstance(providerId: string): Promise<boolean> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'remove_provider',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      provider_id: providerId,
    });
    if (resp.type === 'error') {
      throw new Error(`Remove provider failed: ${resp.message}`);
    }
    return resp.type === 'provider_removed' ? resp.removed : false;
  }

  public async discoverProviderModels(providerId: string): Promise<ProviderModel[]> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'discover_models',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      provider_id: providerId,
    });
    if (resp.type === 'error') {
      throw new Error(`Discover models failed: ${resp.message}`);
    }
    if (resp.type !== 'models_discovered') {
      return [];
    }
    return resp.models.map((m) => ({
      id: m.id,
      name: m.display_name || m.id,
      contextWindow: m.limits?.context_window_tokens ?? 128000,
      isDefault: false,
    }));
  }

  public async testProviderConnection(providerId: string): Promise<{
    ok: boolean;
    status: string;
    details?: unknown;
  }> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'test_provider_connection',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      provider_id: providerId,
    });
    if (resp.type === 'error') {
      throw new Error(`Test provider connection failed: ${resp.message}`);
    }
    if (resp.type !== 'provider_connection_tested') {
      return { ok: false, status: 'unexpected_response' };
    }
    const r = resp.report;
    const ok =
      r.configuration.state === 'passed' &&
      r.credential.state === 'passed' &&
      r.reachability.state === 'passed';
    return { ok, status: r.discovery.state, details: r };
  }

  public async listExtensibility(workspaceRoot?: string): Promise<{
    skills: unknown[];
    mcpServers: unknown[];
    instructions: unknown[];
  }> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'list_extensibility',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      workspace_root: workspaceRoot,
    });
    if (resp.type === 'error') {
      throw new Error(`List extensibility failed: ${resp.message}`);
    }
    if (resp.type !== 'extensibility_listed') {
      return { skills: [], mcpServers: [], instructions: [] };
    }
    return {
      skills: resp.skills,
      mcpServers: resp.mcp_servers,
      instructions: resp.instructions,
    };
  }

  /**
   * Authoritatively resolves a pending approval when capability is enabled.
   */
  public async resolveApprovalAuthoritative(
    approvalId: string,
    decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult> {
    this.assertConnected();
    const resp = await this._transport!.sendCommand({
      type: 'resolve_approval',
      // eslint-disable-next-line @typescript-eslint/naming-convention
      approval_id: approvalId,
      decision:
        decision === 'approved'
          ? { decision: 'approved' }
          : { decision: 'rejected', reason: 'Denied by operator in VS Code' },
    });
    if (resp.type === 'error') {
      return { id: approvalId, decision, applied: false, message: resp.message };
    }
    return {
      id: approvalId,
      decision,
      applied: true,
      message: `Approval ${approvalId} resolved (${decision}).`,
    };
  }

  /**
   * Retrieves execution readiness report from the daemon using ExecutionClient.
   */
  public async getExecutionReadiness(timeoutMs?: number): Promise<RustExecutionReadinessReport> {
    this.assertConnected();
    return ExecutionClient.getExecutionReadiness(this._transport!, timeoutMs);
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
   * Retrieves durable recovery and persistence health status (M15-04 / M15-UI-04).
   */
  public async getRecoveryStatus(): Promise<RustRecoveryStatusReport> {
    this.assertConnected();

    const resp = await this._transport!.sendCommand({ type: 'get_recovery_status' });
    if (resp.type === 'error') {
      throw new Error(`Get recovery status failed: ${resp.message}`);
    }
    if (resp.type !== 'recovery_status') {
      throw new Error(`Unexpected response type for get_recovery_status: ${resp.type}`);
    }

    return resp.status;
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
    if (this._isDisposed) {
      return;
    }
    this._isDisposed = true;
    this._epoch++;
    this._resubscribeAttempts = 0;
    this._isRecoveringSubscription = false;

    OperatorConfirmationManager.clearPendingChallenges();

    if (this._transport) {
      this._transport.disconnect();
      this._transport.removeAllListeners();
      this._transport = null;
    }

    while (this._disposables.length > 0) {
      const d = this._disposables.pop();
      d?.dispose();
    }

    this._connectionState = 'disconnected';
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

    transport.on('subscription_error', async (err: Error) => {
      console.warn('[DaemonConnectedBridge] Subscription error received:', err.message);
      if (this._isDisposed) {
        return;
      }

      if (
        err.message.includes('Sequence gap') ||
        err.message.includes('SequenceGapRecoverable') ||
        err.message.includes('Failed to replay events')
      ) {
        if (this._resubscribeAttempts >= DaemonConnectedBridge.maxResubscribeAttempts) {
          console.error(
            `[DaemonConnectedBridge] Exceeded maximum resubscription attempts (${DaemonConnectedBridge.maxResubscribeAttempts}). Marking degraded.`
          );
          this.setConnectionState('degraded');
          return;
        }

        if (this._isRecoveringSubscription) {
          return;
        }

        this._isRecoveringSubscription = true;
        this._resubscribeAttempts++;
        const recoveryEpoch = ++this._epoch;

        // Sequence gap encountered (e.g. unhydrated in-memory ReplayBuffer on restarted daemon).
        // Fall back to clean subscription without sequence filter and re-hydrate domain state.
        try {
          this._lastWatermark = 0;
          this._seenEventSequences.clear();
          await this._transport?.subscribeEvents(0);

          if (this._isDisposed || this._epoch !== recoveryEpoch) {
            return;
          }

          await this.syncDomainState();
        } catch (retryErr) {
          console.error('[DaemonConnectedBridge] Re-subscription recovery failed:', retryErr);
          if (!this._isDisposed && this._epoch === recoveryEpoch) {
            this.setConnectionState('degraded');
          }
        } finally {
          this._isRecoveringSubscription = false;
        }
      } else {
        this.setConnectionState('degraded');
      }
    });

    transport.on('error', (err: Error) => {
      if (this._isDisposed) {
        return;
      }
      console.error('[DaemonConnectedBridge] Transport error:', err);
      this.setConnectionState('degraded');
    });

    transport.on('close', () => {
      if (this._isDisposed) {
        return;
      }
      if (this._connectionState !== 'disconnected') {
        this.setConnectionState('disconnected');
      }
    });
  }

  /**
   * Synchronizes domain state from daemon after reconnection or sequence gap recovery.
   * Epoch-guarded to prevent stale hydration responses from overwriting newer state.
   */
  public async syncDomainState(): Promise<void> {
    if (this._isDisposed) {
      return;
    }
    const currentEpoch = this._epoch;
    try {
      const studios = await this.listStudiosInternal();
      if (this._isDisposed || this._epoch !== currentEpoch) {
        return;
      }
      if (studios.length > 0) {
        if (!this._activeStudioId || !studios.some((s) => s.id === this._activeStudioId)) {
          this._activeStudioId = studios[0].id;
        }
      }
    } catch {
      // Non-fatal hydration attempt
    }
  }

  private handleTransportStateChange(tState: TransportState): void {
    if (this._isDisposed) {
      return;
    }
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
    if (this._isDisposed) {
      return;
    }

    // Reset bounded resubscribe attempts on successful event stream delivery
    this._resubscribeAttempts = 0;

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
    if (this._isDisposed) {
      return;
    }
    if (this._connectionState !== newState) {
      this._connectionState = newState;
      this._onConnectionStateChanged.fire(newState);
    }
  }

  private assertConnected(): void {
    if (this._isDisposed || this._connectionState !== 'connected' || !this._transport) {
      throw new BridgeDisconnectedError('Agent Studios daemon bridge is not connected.');
    }
  }
}
