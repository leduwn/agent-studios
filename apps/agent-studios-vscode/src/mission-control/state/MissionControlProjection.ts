/**
 * Agent Studios — Mission Control State Projection
 * Milestone M14 Mission Control Workbench
 *
 * Projects data from AgentStudiosBridge and SessionStateProjection
 * into the authoritative MissionControlState schema.
 */

import * as vscode from 'vscode';
import { BridgeManager } from '../../bridge/BridgeManager';
import {
  ApprovalRequest,
  Artifact,
  ConnectionState,
  ProviderSummary,
  SessionInfo,
  TaskNode,
  TimelineEvent,
} from '../../bridge/types';
import {
  MissionControlState,
  MissionControlTabId,
  AgentFleetMember,
  TaskLifecycleModel,
  ArtifactRecord,
  ApprovalRecord,
  ProviderUsageSummary,
  AttentionItem,
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

  private _disposables: vscode.Disposable[] = [];
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

    try {
      const [session, tasks, approvals, artifacts, providers] = await Promise.all([
        bridge.getCurrentSession().catch(() => null),
        bridge.listTasks().catch(() => []),
        bridge.listPendingApprovals().catch(() => []),
        bridge.listArtifacts().catch(() => []),
        bridge.listProviderSummaries().catch(() => []),
      ]);

      this._currentState = this.projectRealState(
        connState,
        session,
        tasks,
        approvals,
        artifacts,
        providers
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
      overview: {
        activeAgentCount: 0,
        totalAgentsCount: 0,
        taskCounts: {
          pending: 0,
          ready: 0,
          running: 0,
          blocked: 0,
          review: 0,
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
        recentEvents: [],
        activeRunCount: 0,
      },
      fleet: [],
      tasks: [],
      runs: [],
      timeline: [],
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
    providers: ProviderSummary[]
  ): MissionControlState {
    const mappedTasks: TaskLifecycleModel[] = tasks.map((t) => {
      let state: TaskLifecycleModel['state'] = 'pending';
      if (t.status === 'running') {
        state = 'running';
      } else if (t.status === 'succeeded') {
        state = 'succeeded';
      } else if (t.status === 'failed') {
        state = 'failed';
      } else if (t.status === 'blocked') {
        state = 'blocked';
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
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        progress: t.progress,
        error: t.error,
      };
    });

    const taskCounts = {
      pending: mappedTasks.filter((t) => t.state === 'pending').length,
      ready: mappedTasks.filter((t) => t.state === 'ready').length,
      running: mappedTasks.filter((t) => t.state === 'running').length,
      blocked: mappedTasks.filter((t) => t.state === 'blocked').length,
      review: mappedTasks.filter((t) => t.state === 'review').length,
      succeeded: mappedTasks.filter((t) => t.state === 'succeeded').length,
      failed: mappedTasks.filter((t) => t.state === 'failed').length,
      cancelled: mappedTasks.filter((t) => t.state === 'cancelled').length,
    };

    // Synthesize fleet members from task assignments
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
            runtime: 'codex-rs',
            providerModel: 'default',
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

    const mappedProviders: ProviderUsageSummary[] = providers.map((p) => ({
      id: p.instanceId,
      displayName: p.displayName,
      modelId: p.models[0]?.name ?? 'default-model',
      status: p.status === 'available' ? 'available' : 'degraded',
      contextWindow: p.models[0]?.contextWindow,
      latencyMs: 120,
    }));

    const attentionItems: AttentionItem[] = [];
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
            workspacePath: '',
            createdAt: new Date().toISOString(),
            runStatus: session.status === 'running' ? 'running' : 'idle',
          }
        : null,
      overview: {
        activeAgentCount: fleet.filter((a) => a.status === 'running').length,
        totalAgentsCount: fleet.length,
        taskCounts,
        attentionItems,
        recentEvents: [],
        activeRunCount: mappedTasks.filter((t) => t.state === 'running').length,
      },
      fleet,
      tasks: mappedTasks,
      runs: [],
      timeline: [],
      artifacts: mappedArtifacts,
      approvals: mappedApprovals,
      providers: mappedProviders,
      diagnostics: {
        endpoint: 'ipc://agent-studios.sock',
        sessionId: session?.id ?? null,
        protocolVersion: '1.0.0',
        agreedCapabilities: ['worktree_isolation', 'event_streaming'],
        watermark: 0,
        activeClients: 1,
        uptimeSecs: 0,
        lastError: null,
      },
      selectedAgentId: this._selectedAgentId,
      selectedTaskId: this._selectedTaskId,
      selectedRunId: this._selectedRunId,
    };
  }

  private bindBridgeListeners(): void {
    this._disposables.push(
      this._bridgeManager.onBridgeChanged(() => {
        this.refresh();
      }),
      this._bridgeManager.stateProjection.onSessionChanged(() => {
        this.refresh();
      })
    );

    // Also listen to bridge events
    const bindBridge = () => {
      try {
        const disposable = this._bridgeManager.bridge.onEvent((_event: TimelineEvent) => {
          this.refresh();
        });
        if (disposable && typeof disposable.dispose === 'function') {
          this._disposables.push(disposable);
        }
      } catch {
        // Safe ignore
      }
    };
    bindBridge();
  }

  public dispose(): void {
    if (!this._isDisposed) {
      this._isDisposed = true;
      this._onDidChangeState.dispose();
      for (const d of this._disposables) {
        d.dispose();
      }
      this._disposables = [];
    }
  }
}
