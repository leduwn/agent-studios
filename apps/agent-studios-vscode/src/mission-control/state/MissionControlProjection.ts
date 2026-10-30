/**
 * Agent Studios — Mission Control State Projection
 * Milestone M14 Mission Control Workbench
 *
 * Projects data from AgentStudiosBridge and SessionStateProjection
 * into the authoritative MissionControlState schema.
 */

import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import { DaemonConnectedBridge } from '../../bridge/DaemonConnectedBridge';
import {
  ApprovalRequest,
  Artifact,
  ConnectionState,
  ProviderSummary,
  SessionInfo,
  TaskNode,
  TimelineEvent,
} from '../../bridge/types';
import { RustHealthResponse } from '../../bridge/ipc/RustIpcProtocolTypes';
import {
  MissionControlState,
  MissionControlTabId,
  AgentFleetMember,
  TaskLifecycleModel,
  ArtifactRecord,
  ApprovalRecord,
  ProviderUsageSummary,
  AttentionItem,
  QueryErrors,
  TimelineEventRecord,
} from '../models/types';
import { createPreviewMissionControlState } from '../fixtures/missionControlFixtures';

export class MissionControlProjection implements vscode.Disposable {
  private readonly _onDidChangeState = new vscode.EventEmitter<MissionControlState>();
  public readonly onDidChangeState = this._onDidChangeState.event;

  private _currentState: MissionControlState;
  private _activeTab: MissionControlTabId = 'overview';
  private _selectedAgentId?: string;
  private _selectedTaskId?: string;
  private _selectedRunId?: string;
  private readonly _timelineEvents: TimelineEventRecord[] = [];

  private _disposables: vscode.Disposable[] = [];
  private _bridgeSubscriptions: vscode.Disposable[] = [];
  private _isDisposed = false;

  constructor(private readonly _bridgeManager: BridgeManager) {
    this._currentState = this.buildInitialState();
    this.bindBridgeListeners();
  }

  public get state(): MissionControlState {
    return this._currentState;
  }

  public getState(): MissionControlState {
    return this._currentState;
  }

  public get activeTab(): MissionControlTabId {
    return this._activeTab;
  }

  public setActiveTab(tab: MissionControlTabId): void {
    if (this._activeTab !== tab) {
      this._activeTab = tab;
      this._currentState = { ...this._currentState, activeTab: tab };
      this._onDidChangeState.fire(this._currentState);
    }
  }

  public selectAgent(agentId: string | null): void {
    this._selectedAgentId = agentId ?? undefined;
    this._currentState = { ...this._currentState, selectedAgentId: this._selectedAgentId };
    this._onDidChangeState.fire(this._currentState);
  }

  public selectTask(taskId: string | null): void {
    this._selectedTaskId = taskId ?? undefined;
    this._currentState = { ...this._currentState, selectedTaskId: this._selectedTaskId };
    this._onDidChangeState.fire(this._currentState);
  }

  public selectRun(runId: string | null): void {
    this._selectedRunId = runId ?? undefined;
    this._currentState = { ...this._currentState, selectedRunId: this._selectedRunId };
    this._onDidChangeState.fire(this._currentState);
  }

  public async refresh(): Promise<void> {
    if (this._isDisposed) {
      return;
    }

    if (this._bridgeManager.isPreviewMode) {
      const preview = createPreviewMissionControlState();
      preview.activeTab = this._activeTab;
      preview.selectedAgentId = this._selectedAgentId ?? preview.selectedAgentId;
      preview.selectedTaskId = this._selectedTaskId ?? preview.selectedTaskId;
      preview.selectedRunId = this._selectedRunId ?? preview.selectedRunId;
      this._currentState = preview;
      this._onDidChangeState.fire(this._currentState);
      return;
    }

    const bridge = this._bridgeManager.bridge;
    const connState = bridge.connectionState;

    if (connState === 'disconnected') {
      this._currentState = this.buildDisconnectedState();
      this._onDidChangeState.fire(this._currentState);
      return;
    }

    const isConnectedBridge = bridge instanceof DaemonConnectedBridge;

    if (bridge && typeof (bridge as unknown as { listTimelineEvents?: () => TimelineEvent[] }).listTimelineEvents === 'function') {
      try {
        const bridgeEvents = (bridge as unknown as { listTimelineEvents: () => TimelineEvent[] }).listTimelineEvents();
        for (const ev of bridgeEvents) {
          const rec: TimelineEventRecord = {
            sequence: ev.sequenceNumber,
            timestamp: new Date(ev.timestamp).toISOString(),
            type: ev.type,
            taskId: ev.taskId,
            agentId: ev.agentName,
            summary: ev.summary,
            details: ev.details,
          };
          if (!this._timelineEvents.some((e) => e.sequence === rec.sequence && rec.sequence > 0)) {
            this._timelineEvents.push(rec);
          }
        }
      } catch {
        // Safe ignore
      }
    }

    try {
      const [
        sessionRes,
        tasksRes,
        approvalsRes,
        artifactsRes,
        providersRes,
        healthRes,
      ] = await Promise.allSettled([
        bridge.getCurrentSession(),
        bridge.listTasks(),
        bridge.listPendingApprovals(),
        bridge.listArtifacts(),
        bridge.listProviderSummaries(),
        isConnectedBridge ? (bridge as DaemonConnectedBridge).getHealth() : Promise.resolve(null),
      ]);

      const queryErrors: QueryErrors = {};

      const session = sessionRes.status === 'fulfilled' ? sessionRes.value : null;
      if (sessionRes.status === 'rejected') {
        queryErrors.session =
          sessionRes.reason instanceof Error ? sessionRes.reason.message : String(sessionRes.reason);
      }

      const tasks = tasksRes.status === 'fulfilled' ? tasksRes.value : [];
      if (tasksRes.status === 'rejected') {
        queryErrors.tasks =
          tasksRes.reason instanceof Error ? tasksRes.reason.message : String(tasksRes.reason);
      }

      const approvals = approvalsRes.status === 'fulfilled' ? approvalsRes.value : [];
      if (approvalsRes.status === 'rejected') {
        queryErrors.approvals =
          approvalsRes.reason instanceof Error ? approvalsRes.reason.message : String(approvalsRes.reason);
      }

      const artifacts = artifactsRes.status === 'fulfilled' ? artifactsRes.value : [];
      if (artifactsRes.status === 'rejected') {
        queryErrors.artifacts =
          artifactsRes.reason instanceof Error ? artifactsRes.reason.message : String(artifactsRes.reason);
      }

      const providers = providersRes.status === 'fulfilled' ? providersRes.value : [];
      if (providersRes.status === 'rejected') {
        queryErrors.providers =
          providersRes.reason instanceof Error ? providersRes.reason.message : String(providersRes.reason);
      }

      const health = healthRes.status === 'fulfilled' ? healthRes.value : null;
      if (healthRes.status === 'rejected') {
        queryErrors.health =
          healthRes.reason instanceof Error ? healthRes.reason.message : String(healthRes.reason);
      }

      this._currentState = this.projectRealState(
        connState,
        session,
        tasks,
        approvals,
        artifacts,
        providers,
        health,
        queryErrors
      );
      this._onDidChangeState.fire(this._currentState);
    } catch {
      this._currentState = {
        ...this._currentState,
        connectionState: connState,
      };
      this._onDidChangeState.fire(this._currentState);
    }
  }

  private buildInitialState(): MissionControlState {
    if (this._bridgeManager.isPreviewMode) {
      return createPreviewMissionControlState();
    }
    return this.buildDisconnectedState();
  }

  private buildDisconnectedState(): MissionControlState {
    return {
      version: '0.1.0',
      timestamp: Date.now(),
      connectionState: 'disconnected',
      isPreview: false,
      activeTab: this._activeTab,
      activeStudio: null,
      currentSession: null,
      overview: {
        activeAgentCount: 0,
        totalAgentsCount: 0,
        taskCounts: {
          pending: 0,
          ready: 0,
          running: 0,
          blocked: 0,
          review: 0,
          retrying: 0,
          paused: 0,
          succeeded: 0,
          failed: 0,
          cancelled: 0,
        },
        attentionItems: [
          {
            id: 'att-disconnected',
            severity: 'warning',
            title: 'Agent Studios Daemon Disconnected',
            description: 'No active connection to agsd daemon. Enable Developer Preview Mode or connect daemon.',
            targetTab: 'diagnostics',
            timestamp: Date.now(),
          },
        ],
        recentEvents: [...this._timelineEvents].slice(-10).reverse(),
        activeRunCount: 0,
      },
      fleet: [],
      tasks: [],
      runs: [],
      timeline: [...this._timelineEvents],
      artifacts: [],
      approvals: [],
      providers: [],
      diagnostics: {
        endpoint: 'ipc://agent-studios.sock',
        sessionId: null,
        protocolVersion: '1.0.0',
        agreedCapabilities: [],
        watermark: 0,
        activeClients: 0,
        uptimeSecs: 0,
        lastError: null,
      },
      selectedAgentId: this._selectedAgentId,
      selectedTaskId: this._selectedTaskId,
      selectedRunId: this._selectedRunId,
    };
  }

  private projectRealState(
    connState: ConnectionState,
    session: SessionInfo | null,
    tasks: TaskNode[],
    approvals: ApprovalRequest[],
    artifacts: Artifact[],
    providers: ProviderSummary[],
    health: RustHealthResponse | null,
    queryErrors: QueryErrors
  ): MissionControlState {
    const mappedTasks: TaskLifecycleModel[] = tasks.map((t) => {
      let state: TaskLifecycleModel['state'] = 'pending';
      const statusLower = (t.status || '').toLowerCase();
      if (statusLower === 'running') {
        state = 'running';
      } else if (statusLower === 'succeeded') {
        state = 'succeeded';
      } else if (statusLower === 'failed') {
        state = 'failed';
      } else if (statusLower === 'blocked') {
        state = 'blocked';
      } else if (statusLower === 'ready') {
        state = 'ready';
      } else if (statusLower === 'review') {
        state = 'review';
      } else if (statusLower === 'retrying') {
        state = 'retrying';
      } else if (statusLower === 'paused') {
        state = 'paused';
      } else if (statusLower === 'cancelled') {
        state = 'cancelled';
      }

      return {
        id: t.id,
        studioId: session?.id ?? 'default-studio',
        title: t.title,
        description: t.description,
        state,
        dependencies: t.dependencies ?? [],
        assignedAgentId: t.assignedAgentId,
        assignedAgentName: t.assignedAgentName,
        createdAt: new Date(t.createdAt || Date.now()).toISOString(),
        updatedAt: new Date(t.updatedAt || Date.now()).toISOString(),
        progress:
          state === 'succeeded'
            ? (t.progress !== undefined ? t.progress : 100)
            : state === 'running'
            ? undefined
            : t.progress,
        error: t.error,
      };
    });

    const taskCounts = {
      pending: mappedTasks.filter((t) => t.state === 'pending').length,
      ready: mappedTasks.filter((t) => t.state === 'ready').length,
      running: mappedTasks.filter((t) => t.state === 'running').length,
      blocked: mappedTasks.filter((t) => t.state === 'blocked').length,
      review: mappedTasks.filter((t) => t.state === 'review').length,
      retrying: mappedTasks.filter((t) => t.state === 'retrying').length,
      paused: mappedTasks.filter((t) => t.state === 'paused').length,
      succeeded: mappedTasks.filter((t) => t.state === 'succeeded').length,
      failed: mappedTasks.filter((t) => t.state === 'failed').length,
      cancelled: mappedTasks.filter((t) => t.state === 'cancelled').length,
    };

    // Synthesize fleet members from task assignments without fabricated runtimes or models
    const agentMap = new Map<string, AgentFleetMember>();
    for (const t of tasks) {
      if (t.assignedAgentId) {
        if (!agentMap.has(t.assignedAgentId)) {
          agentMap.set(t.assignedAgentId, {
            agentId: t.assignedAgentId,
            name: t.assignedAgentName ?? t.assignedAgentId,
            role: 'Assigned Agent',
            status: t.status === 'running' ? 'running' : 'idle',
            assignedTaskId: t.id,
            assignedTaskTitle: t.title,
            runtime: 'unspecified',
            providerModel: 'unspecified',
            lastActivityAt: Date.now(),
            toolCallCount: 0,
          });
        }
      }
    }
    const fleet = Array.from(agentMap.values());

    const mappedApprovals: ApprovalRecord[] = approvals.map((a) => ({
      id: a.id,
      sessionId: a.sessionId,
      actionType: a.actionType,
      commandOrTarget: a.commandOrTarget,
      riskLevel: a.riskLevel,
      status: a.status,
      createdAt: a.createdAt,
      requestedPermissions: a.requestedPermissions,
    }));

    const mappedArtifacts: ArtifactRecord[] = artifacts.map((art) => ({
      id: art.id,
      sessionId: art.sessionId,
      path: art.path,
      changeType: 'modified',
      diffSummary: art.name,
      originalUri: art.path,
      modifiedUri: `agent-studios-diff://artifact/${art.id}/${art.name}`,
    }));

    // Truthful provider summaries: omit unmeasured latency instead of fabricating 120ms
    const mappedProviders: ProviderUsageSummary[] = providers.map((p) => ({
      id: p.instanceId,
      displayName: p.displayName,
      modelId: p.models[0]?.name ?? p.models[0]?.id ?? 'unspecified',
      status: p.status === 'available' ? 'available' : 'degraded',
      contextWindow: p.models[0]?.contextWindow,
      latencyMs: undefined,
    }));

    const attentionItems: AttentionItem[] = [];

    // Attention for query errors
    if (Object.keys(queryErrors).length > 0) {
      attentionItems.push({
        id: 'att-query-errors',
        severity: 'warning',
        title: 'Query Error During Projection Refresh',
        description: `Failed to query: ${Object.keys(queryErrors).join(', ')}`,
        targetTab: 'diagnostics',
        timestamp: Date.now(),
      });
    }

    if (mappedApprovals.filter((a) => a.status === 'pending').length > 0) {
      attentionItems.push({
        id: 'att-approvals',
        severity: 'warning',
        title: `${mappedApprovals.length} Approval Requests Pending`,
        description: 'Human approval requested for tool or file operations.',
        targetTab: 'approvals',
        timestamp: Date.now(),
      });
    }

    const failedTasks = mappedTasks.filter((t) => t.state === 'failed');
    for (const f of failedTasks) {
      attentionItems.push({
        id: `att-task-${f.id}`,
        severity: 'error',
        title: `Task Failed: ${f.title}`,
        description: f.error ?? 'Task execution encountered an error.',
        targetTab: 'tasks',
        targetId: f.id,
        timestamp: Date.now(),
      });
    }

    // Event subscription updates attention queue when events warrant attention
    for (const evt of this._timelineEvents) {
      if (evt.type === 'task_failed') {
        const id = `att-evt-task-failed-${evt.sequence}`;
        if (!attentionItems.some((a) => a.id === id || (evt.taskId && a.targetId === evt.taskId))) {
          attentionItems.push({
            id,
            severity: 'error',
            title: `Task Failed Alert: ${evt.summary}`,
            description: (evt.details?.error as string) || evt.summary,
            targetTab: 'tasks',
            targetId: evt.taskId,
            timestamp: new Date(evt.timestamp).getTime() || Date.now(),
          });
        }
      } else if (evt.type === 'approval_requested') {
        const id = `att-evt-appr-${evt.sequence}`;
        if (!attentionItems.some((a) => a.id === id)) {
          attentionItems.push({
            id,
            severity: 'warning',
            title: `Approval Required: ${evt.summary}`,
            description: (evt.details?.description as string) || evt.summary,
            targetTab: 'approvals',
            timestamp: new Date(evt.timestamp).getTime() || Date.now(),
          });
        }
      } else if (
        evt.details &&
        (evt.details.severity === 'warning' ||
          evt.details.severity === 'error' ||
          evt.details.severity === 'critical')
      ) {
        const id = `att-evt-${evt.sequence}`;
        if (!attentionItems.some((a) => a.id === id)) {
          attentionItems.push({
            id,
            severity: evt.details.severity as AttentionItem['severity'],
            title: (evt.details.title as string) || evt.summary,
            description: (evt.details.description as string) || evt.summary,
            targetTab: (evt.details.targetTab as MissionControlTabId) || 'overview',
            timestamp: new Date(evt.timestamp).getTime() || Date.now(),
          });
        }
      }
    }

    // Read real diagnostic telemetry from bridge/transport/health
    const bridge = this._bridgeManager.bridge;
    let endpoint = 'ipc://agent-studios.sock';
    let agreedCapabilities: string[] = [];
    let watermark = 0;
    let activeClients = 1;
    let uptimeSecs = 0;

    if (bridge instanceof DaemonConnectedBridge) {
      if (bridge.endpoint) {
        endpoint = bridge.endpoint;
      }
      if (bridge.transport) {
        agreedCapabilities = bridge.transport.getAgreedCapabilities();
        watermark = bridge.transport.getLastReceivedSequence();
      }
      if (health) {
        activeClients = health.active_clients;
        uptimeSecs = health.uptime_secs;
      }
    }

    return {
      version: '0.1.0',
      timestamp: Date.now(),
      connectionState: connState,
      isPreview: false,
      activeTab: this._activeTab,
      activeStudio: session
        ? {
            id: session.id,
            name: session.goalSummary,
            workspacePath: session.workspacePath || '',
            createdAt: new Date().toISOString(),
            runStatus: session.status === 'running' ? 'running' : 'idle',
          }
        : null,
      currentSession: session,
      overview: {
        activeAgentCount: fleet.filter((a) => a.status === 'running').length,
        totalAgentsCount: fleet.length,
        taskCounts,
        attentionItems,
        recentEvents: [...this._timelineEvents].slice(-10).reverse(),
        activeRunCount: mappedTasks.filter((t) => t.state === 'running').length,
      },
      fleet,
      tasks: mappedTasks,
      runs: [],
      timeline: [...this._timelineEvents],
      artifacts: mappedArtifacts,
      approvals: mappedApprovals,
      providers: mappedProviders,
      diagnostics: {
        endpoint,
        sessionId:
          session?.id ??
          (bridge instanceof DaemonConnectedBridge && bridge.transport
            ? bridge.transport.getSessionId()
            : null),
        protocolVersion: '1.0.0',
        agreedCapabilities,
        watermark,
        activeClients,
        uptimeSecs,
        lastError:
          queryErrors.tasks ||
          queryErrors.session ||
          queryErrors.health ||
          null,
      },
      selectedAgentId: this._selectedAgentId,
      selectedTaskId: this._selectedTaskId,
      selectedRunId: this._selectedRunId,
      queryErrors: Object.keys(queryErrors).length > 0 ? queryErrors : undefined,
    };
  }

  private bindBridgeListeners(): void {
    this._disposables.push(
      this._bridgeManager.onBridgeChanged(() => {
        this.rebindActiveBridgeListeners();
        this.refresh();
      }),
      this._bridgeManager.stateProjection.onSessionChanged(() => {
        this.refresh();
      })
    );

    this.rebindActiveBridgeListeners();
  }

  private rebindActiveBridgeListeners(): void {
    for (const d of this._bridgeSubscriptions) {
      d.dispose();
    }
    this._bridgeSubscriptions = [];

    const bridge = this._bridgeManager.bridge;

    try {
      const eventDisp = bridge.onEvent((event: TimelineEvent) => {
        this.handleIncomingTimelineEvent(event);
      });
      if (eventDisp && typeof eventDisp.dispose === 'function') {
        this._bridgeSubscriptions.push(eventDisp);
      }
    } catch {
      // Safe ignore
    }

    try {
      const stateDisp = bridge.onConnectionStateChanged((_state: ConnectionState) => {
        this.refresh();
      });
      if (stateDisp && typeof stateDisp.dispose === 'function') {
        this._bridgeSubscriptions.push(stateDisp);
      }
    } catch {
      // Safe ignore
    }
  }

  private handleIncomingTimelineEvent(event: TimelineEvent): void {
    const record: TimelineEventRecord = {
      sequence: event.sequenceNumber,
      timestamp: new Date(event.timestamp).toISOString(),
      type: event.type,
      taskId: event.taskId,
      agentId: event.agentName,
      summary: event.summary,
      details: event.details,
    };
    if (!this._timelineEvents.some((e) => e.sequence === record.sequence && record.sequence > 0)) {
      this._timelineEvents.push(record);
      if (this._timelineEvents.length > 500) {
        this._timelineEvents.shift();
      }
    }
    this.refresh();
  }

  public dispose(): void {
    if (!this._isDisposed) {
      this._isDisposed = true;
      this._onDidChangeState.dispose();

      for (const d of this._bridgeSubscriptions) {
        d.dispose();
      }
      this._bridgeSubscriptions = [];

      for (const d of this._disposables) {
        d.dispose();
      }
      this._disposables = [];
    }
  }
}
