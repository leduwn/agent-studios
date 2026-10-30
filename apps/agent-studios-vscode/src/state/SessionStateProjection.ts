/**
 * Agent Studios — Session State Projection
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import {
  ApprovalRequest,
  ApprovalStatus,
  Artifact,
  SessionInfo,
  TaskNode,
  TimelineEvent,
} from '../bridge/types';

export interface SequenceGapDetails {
  expected: number;
  received: number;
}

export type ProjectionHealthState = 'nominal' | 'degraded';

/**
 * Maintains a live, bounded projection of session state derived from
 * bridge queries and authoritative timeline events.
 *
 * Implements strict sequence tracking, gap detection, duplicate rejection,
 * and FIFO bounding for the event buffer.
 */
export class SessionStateProjection implements vscode.Disposable {
  public static readonly maxEventBufferSize = 1000;

  private _session: SessionInfo | null = null;
  private _tasks: TaskNode[] = [];
  private _approvals: ApprovalRequest[] = [];
  private _artifacts: Artifact[] = [];
  private _timeline: TimelineEvent[] = [];

  private _lastProcessedSequence: number = 0;
  private _healthState: ProjectionHealthState = 'nominal';

  private _bridge?: AgentStudiosBridge;
  private readonly _bridgeDisposables: vscode.Disposable[] = [];
  private readonly _disposables: vscode.Disposable[] = [];

  // Exposed EventEmitters
  private readonly _onSessionChanged = new vscode.EventEmitter<SessionInfo | null>();
  public readonly onSessionChanged: vscode.Event<SessionInfo | null> = this._onSessionChanged.event;

  private readonly _onTasksChanged = new vscode.EventEmitter<TaskNode[]>();
  public readonly onTasksChanged: vscode.Event<TaskNode[]> = this._onTasksChanged.event;

  private readonly _onApprovalsChanged = new vscode.EventEmitter<ApprovalRequest[]>();
  public readonly onApprovalsChanged: vscode.Event<ApprovalRequest[]> = this._onApprovalsChanged.event;

  private readonly _onArtifactsChanged = new vscode.EventEmitter<Artifact[]>();
  public readonly onArtifactsChanged: vscode.Event<Artifact[]> = this._onArtifactsChanged.event;

  private readonly _onTimelineChanged = new vscode.EventEmitter<TimelineEvent[]>();
  public readonly onTimelineChanged: vscode.Event<TimelineEvent[]> = this._onTimelineChanged.event;

  private readonly _onStateDegraded = new vscode.EventEmitter<SequenceGapDetails>();
  public readonly onStateDegraded: vscode.Event<SequenceGapDetails> = this._onStateDegraded.event;

  constructor(bridge?: AgentStudiosBridge) {
    this._disposables.push(
      this._onSessionChanged,
      this._onTasksChanged,
      this._onApprovalsChanged,
      this._onArtifactsChanged,
      this._onTimelineChanged,
      this._onStateDegraded
    );

    if (bridge) {
      this.bindBridge(bridge);
    }
  }

  // State accessors
  public get session(): SessionInfo | null {
    return this._session ? { ...this._session } : null;
  }

  public get tasks(): TaskNode[] {
    return this._tasks.map(t => ({ ...t }));
  }

  public get pendingApprovals(): ApprovalRequest[] {
    return this._approvals
      .filter(a => a.status === 'pending')
      .map(a => ({ ...a }));
  }

  public get approvals(): ApprovalRequest[] {
    return this._approvals.map(a => ({ ...a }));
  }

  public get artifacts(): Artifact[] {
    return this._artifacts.map(a => ({ ...a }));
  }

  public get timeline(): TimelineEvent[] {
    return this._timeline.map(e => ({ ...e }));
  }

  public get lastProcessedSequence(): number {
    return this._lastProcessedSequence;
  }

  public get healthState(): ProjectionHealthState {
    return this._healthState;
  }

  public get isDegraded(): boolean {
    return this._healthState === 'degraded';
  }

  /**
   * Binds to an AgentStudiosBridge, subscribing to events and connection state.
   */
  public bindBridge(bridge: AgentStudiosBridge): void {
    this.unbindBridge();
    this._bridge = bridge;

    this._bridgeDisposables.push(
      bridge.onEvent(event => this.handleEvent(event)),
      bridge.onConnectionStateChanged(() => {
        // When connection state changes, bridge might require re-sync
      })
    );
  }

  /**
   * Unbinds from current bridge and disposes active subscriptions.
   */
  public unbindBridge(): void {
    while (this._bridgeDisposables.length > 0) {
      const disposable = this._bridgeDisposables.pop();
      disposable?.dispose();
    }
    this._bridge = undefined;
  }

  /**
   * Pulls authoritative initial or refreshed snapshot from the bound bridge.
   */
  public async initializeFromBridge(): Promise<void> {
    if (!this._bridge) {
      return;
    }

    const [session, tasks, approvals, artifacts] = await Promise.all([
      this._bridge.getCurrentSession(),
      this._bridge.listTasks(),
      this._bridge.listPendingApprovals(),
      this._bridge.listArtifacts(),
    ]);

    this._session = session;
    this._tasks = [...tasks];
    this._approvals = [...approvals];
    this._artifacts = [...artifacts];

    if (
      'timeline' in this._bridge &&
      Array.isArray((this._bridge as unknown as { timeline: TimelineEvent[] }).timeline)
    ) {
      this.seedEvents(
        (this._bridge as unknown as { timeline: TimelineEvent[] }).timeline
      );
    }

    this._onSessionChanged.fire(this.session);
    this._onTasksChanged.fire(this.tasks);
    this._onApprovalsChanged.fire(this.approvals);
    this._onArtifactsChanged.fire(this.artifacts);
  }

  /**
   * Seeds historical events (e.g. from initial sync) without firing gap degradation.
   * Advances lastProcessedSequence to the maximum sequence number among seeded events.
   */
  public seedEvents(events: TimelineEvent[]): void {
    const sorted = [...events].sort((a, b) => a.sequenceNumber - b.sequenceNumber);
    for (const evt of sorted) {
      if (this._timeline.some((e) => e.id === evt.id || e.sequenceNumber === evt.sequenceNumber)) {
        continue;
      }
      if (evt.sequenceNumber > this._lastProcessedSequence) {
        this._lastProcessedSequence = evt.sequenceNumber;
      }
      this._timeline.push({ ...evt });
    }

    if (this._timeline.length > SessionStateProjection.maxEventBufferSize) {
      const excess = this._timeline.length - SessionStateProjection.maxEventBufferSize;
      this._timeline.splice(0, excess);
    }

    this._onTimelineChanged.fire(this.timeline);
  }

  /**
   * Sets the initial sequence number from loaded timeline history.
   */
  public setInitialSequenceFromTimeline(sequenceNumber: number): void {
    if (sequenceNumber > this._lastProcessedSequence) {
      this._lastProcessedSequence = sequenceNumber;
    }
  }

  /**
   * Processes an incoming timeline event.
   *
   * 1. Duplicate prevention: rejects/ignores events with sequenceNumber <= lastProcessedSequence.
   * 2. Sequence gap tracking: detects gaps, transitions state to 'degraded', and fires onStateDegraded.
   * 3. Bounded buffer: stores at most 1000 events, FIFO pruning older items.
   * 4. State updates: applies incremental projections and fires specific change events.
   */
  public handleEvent(event: TimelineEvent): void {
    // 1. Duplicate prevention: ignore stale or duplicate sequence numbers
    if (event.sequenceNumber <= this._lastProcessedSequence) {
      return;
    }

    // 2. Sequence number tracking: detect gaps
    const expectedSequence = this._lastProcessedSequence + 1;
    if (event.sequenceNumber > expectedSequence) {
      this._healthState = 'degraded';
      this._onStateDegraded.fire({
        expected: expectedSequence,
        received: event.sequenceNumber,
      });
    }

    this._lastProcessedSequence = event.sequenceNumber;

    // 3. Bounded event buffer: FIFO prune when exceeding max size
    this._timeline.push({ ...event });
    if (this._timeline.length > SessionStateProjection.maxEventBufferSize) {
      const excess = this._timeline.length - SessionStateProjection.maxEventBufferSize;
      this._timeline.splice(0, excess);
    }
    this._onTimelineChanged.fire(this.timeline);

    // 4. Update projected state according to event type
    this.projectEventState(event);
  }

  /**
   * Manually resets the degraded health state back to 'nominal'.
   */
  public resetHealthState(): void {
    this._healthState = 'nominal';
  }

  private projectEventState(event: TimelineEvent): void {
    switch (event.type) {
      case 'session_started': {
        if (this._session) {
          this._session.status = 'running';
          this._onSessionChanged.fire(this.session);
        }
        break;
      }

      case 'session_ended': {
        if (this._session) {
          this._session.status = 'completed';
          this._onSessionChanged.fire(this.session);
        }
        break;
      }

      case 'task_started': {
        if (event.taskId) {
          const task = this._tasks.find(t => t.id === event.taskId);
          if (task) {
            task.status = 'running';
            if (event.agentName) {
              task.assignedAgentName = event.agentName;
            }
            task.updatedAt = event.timestamp;
            this._onTasksChanged.fire(this.tasks);
          }
        }
        break;
      }

      case 'task_completed': {
        if (event.taskId) {
          const task = this._tasks.find(t => t.id === event.taskId);
          if (task) {
            task.status = 'succeeded';
            task.progress = 100;
            task.updatedAt = event.timestamp;
            if (this._session) {
              this._session.completedTasksCount = this._tasks.filter(
                t => t.status === 'succeeded'
              ).length;
              this._onSessionChanged.fire(this.session);
            }
            this._onTasksChanged.fire(this.tasks);
          }
        }
        break;
      }

      case 'task_failed': {
        if (event.taskId) {
          const task = this._tasks.find(t => t.id === event.taskId);
          if (task) {
            task.status = 'failed';
            task.updatedAt = event.timestamp;
            this._onTasksChanged.fire(this.tasks);
          }
        }
        break;
      }

      case 'task_blocked': {
        if (event.taskId) {
          const task = this._tasks.find(t => t.id === event.taskId);
          if (task) {
            task.status = 'blocked';
            task.updatedAt = event.timestamp;
            this._onTasksChanged.fire(this.tasks);
          }
        }
        break;
      }

      case 'agent_assigned': {
        if (event.taskId && event.agentName) {
          const task = this._tasks.find(t => t.id === event.taskId);
          if (task) {
            task.assignedAgentName = event.agentName;
            task.updatedAt = event.timestamp;
            this._onTasksChanged.fire(this.tasks);
          }
        }
        break;
      }

      case 'approval_requested': {
        if (event.details && typeof event.details === 'object') {
          const details = event.details as Record<string, unknown>;
          const approvalId = (details.approvalId as string) || event.id;
          const existing = this._approvals.find(a => a.id === approvalId);
          if (!existing) {
            const newApproval: ApprovalRequest = {
              id: approvalId,
              sessionId: (details.sessionId as string) || this._session?.id || 'session-unknown',
              taskId: event.taskId,
              taskTitle: details.taskTitle as string | undefined,
              actionType: (details.actionType as ApprovalRequest['actionType']) || 'exec_command',
              commandOrTarget: (details.command as string) || (details.commandOrTarget as string) || '',
              riskLevel: (details.risk as ApprovalRequest['riskLevel']) || (details.riskLevel as ApprovalRequest['riskLevel']) || 'medium',
              requestedPermissions: (details.permissions as string[]) || (details.requestedPermissions as string[]) || [],
              status: 'pending',
              createdAt: event.timestamp,
            };
            this._approvals.push(newApproval);
            this._onApprovalsChanged.fire(this.approvals);
          }
        }
        break;
      }

      case 'approval_decided': {
        const details = event.details as Record<string, unknown> | undefined;
        const approvalId = (details?.approvalId as string) || event.id;
        const decision = (details?.decision as ApprovalStatus) || 'approved';
        const approval = this._approvals.find(a => a.id === approvalId);
        if (approval) {
          approval.status = decision;
          this._onApprovalsChanged.fire(this.approvals);
        }
        break;
      }

      case 'artifact_created': {
        const details = event.details as Record<string, unknown> | undefined;
        const artifactData = details?.artifact as Artifact | undefined;
        if (artifactData) {
          const exists = this._artifacts.some(a => a.id === artifactData.id);
          if (!exists) {
            this._artifacts.push({ ...artifactData });
            this._onArtifactsChanged.fire(this.artifacts);
          }
        }
        break;
      }

      default:
        break;
    }
  }

  public dispose(): void {
    this.unbindBridge();
    while (this._disposables.length > 0) {
      const disposable = this._disposables.pop();
      disposable?.dispose();
    }
    this._timeline = [];
    this._tasks = [];
    this._approvals = [];
    this._artifacts = [];
    this._session = null;
  }
}
