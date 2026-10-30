/**
 * Agent Studios — Fixture Bridge Implementation
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from './AgentStudiosBridge';
import {
  ApprovalDecisionResult,
  ApprovalRequest,
  Artifact,
  ConnectionState,
  EditorActionRequest,
  EditorActionResult,
  ProviderSummary,
  SessionInfo,
  TaskGraph,
  TaskNode,
  TimelineEvent,
} from './types';

/**
 * Developer fixture / preview bridge providing deterministic mock session data
 * for Extension Development Host testing without requiring a live daemon.
 */
export class FixtureBridge implements AgentStudiosBridge {
  private _connectionState: ConnectionState = 'connected';
  private _nextSequenceNumber: number = 10;

  private readonly _onConnectionStateChanged = new vscode.EventEmitter<ConnectionState>();
  public readonly onConnectionStateChanged: vscode.Event<ConnectionState> = this._onConnectionStateChanged.event;

  private readonly _onEvent = new vscode.EventEmitter<TimelineEvent>();
  public readonly onEvent: vscode.Event<TimelineEvent> = this._onEvent.event;

  private _session: SessionInfo = {
    id: 'session-m14-fixture',
    workspacePath: 'd:/Projects/agent-studios-ide-ui/apps/agent-studios-vscode',
    goalSummary: 'Implement Code-OSS native agents workbench UI with progressive disclosure',
    status: 'running',
    activeRunId: 'run-m14-001',
    activeAgentCount: 2,
    totalAgentCount: 2,
    completedTasksCount: 3,
    totalTasksCount: 5,
    providerSummary: 'Anthropic Claude 3.5 Sonnet',
    startedAt: 1716000000000,
    elapsedMs: 360000,
    isPreview: true,
  };

  private _tasks: TaskNode[] = [
    {
      id: 'task-1',
      runId: 'run-m14-001',
      title: 'Analyze workspace',
      description: 'Inspect codebase structure, package configurations, and extension manifest',
      status: 'succeeded',
      assignedAgentId: 'agent-arch-01',
      assignedAgentName: 'Architect Agent',
      dependencies: [],
      progress: 100,
      createdAt: 1716000010000,
      updatedAt: 1716000045000,
    },
    {
      id: 'task-2',
      runId: 'run-m14-001',
      title: 'Design bridge contracts',
      description: 'Define AgentStudiosBridge interface, connection states, and typed timeline events',
      status: 'succeeded',
      assignedAgentId: 'agent-arch-01',
      assignedAgentName: 'Architect Agent',
      dependencies: ['task-1'],
      progress: 100,
      createdAt: 1716000045000,
      updatedAt: 1716000075000,
    },
    {
      id: 'task-3',
      runId: 'run-m14-001',
      title: 'Implement TreeViews',
      description: 'Build Session, Tasks, Approvals, Artifacts, Timeline, and Providers tree views',
      status: 'succeeded',
      assignedAgentId: 'agent-coder-01',
      assignedAgentName: 'Coder Agent',
      dependencies: ['task-2'],
      progress: 100,
      createdAt: 1716000075000,
      updatedAt: 1716000110000,
    },
    {
      id: 'task-4',
      runId: 'run-m14-001',
      title: 'Interactive DAG Webview',
      description: 'Create interactive directed acyclic graph visualization with progressive disclosure',
      status: 'running',
      assignedAgentId: 'agent-coder-01',
      assignedAgentName: 'Coder Agent',
      dependencies: ['task-3'],
      progress: 60,
      createdAt: 1716000110000,
      updatedAt: 1716000120000,
    },
    {
      id: 'task-5',
      runId: 'run-m14-001',
      title: 'Verification tests',
      description: 'Run unit and integration suites validating workbench views and state projection',
      status: 'pending',
      assignedAgentId: 'agent-coder-01',
      assignedAgentName: 'Coder Agent',
      dependencies: ['task-4'],
      progress: 0,
      createdAt: 1716000110000,
      updatedAt: 1716000110000,
    },
  ];

  private _approvals: ApprovalRequest[] = [
    {
      id: 'approval-m14-001',
      sessionId: 'session-m14-fixture',
      taskId: 'task-4',
      taskTitle: 'Interactive DAG Webview',
      actionType: 'exec_command',
      commandOrTarget: 'npm install @vscode/test-electron',
      riskLevel: 'medium',
      requestedPermissions: ['shell:exec'],
      status: 'pending',
      createdAt: 1716000115000,
    },
  ];

  private _artifacts: Artifact[] = [
    {
      id: 'artifact-diff-001',
      sessionId: 'session-m14-fixture',
      taskId: 'task-3',
      taskTitle: 'Implement TreeViews',
      name: 'task-m14-workbench-diff.patch',
      path: 'diffs/task-m14-workbench-diff.patch',
      diffContent: `--- a/package.json\n+++ b/package.json\n@@ -24,0 +25,37 @@\n+    "views": {\n+      "agent-studios-activitybar": [\n+        {\n+          "id": "agent-studios.session",\n+          "name": "Current Session"\n+        },\n+        {\n+          "id": "agent-studios.tasks",\n+          "name": "Tasks"\n+        }\n+      ]\n+    }`,
      fileCount: 4,
      sha256: '4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945',
      version: 1,
      createdAt: 1716000105000,
    },
    {
      id: 'artifact-diff-002',
      sessionId: 'session-m14-fixture',
      taskId: 'task-2',
      taskTitle: 'Design bridge contracts',
      name: 'task-m14-bridge-contracts.patch',
      path: 'diffs/task-m14-bridge-contracts.patch',
      diffContent: `--- /dev/null\n+++ b/src/bridge/AgentStudiosBridge.ts\n@@ -0,0 +1,25 @@\n+export interface AgentStudiosBridge {\n+  readonly connectionState: ConnectionState;\n+  connect(): Promise<void>;\n+}`,
      fileCount: 1,
      sha256: 'b5a2c9b6e8a4d7c0f1e2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4',
      version: 1,
      createdAt: 1716000070000,
    },
  ];

  private _timeline: TimelineEvent[] = [
    {
      sequenceNumber: 1,
      id: 'evt-1',
      timestamp: 1716000000000,
      type: 'session_started',
      summary: 'Session "Implement Code-OSS native agents workbench UI" started',
    },
    {
      sequenceNumber: 2,
      id: 'evt-2',
      timestamp: 1716000005000,
      type: 'run_started',
      summary: 'Execution run run-m14-001 started',
    },
    {
      sequenceNumber: 3,
      id: 'evt-3',
      timestamp: 1716000010000,
      type: 'agent_assigned',
      summary: 'Architect Agent assigned to analyze workspace',
      agentName: 'Architect Agent',
      taskId: 'task-1',
    },
    {
      sequenceNumber: 4,
      id: 'evt-4',
      timestamp: 1716000015000,
      type: 'task_started',
      summary: 'Task "Analyze workspace" started',
      agentName: 'Architect Agent',
      taskId: 'task-1',
    },
    {
      sequenceNumber: 5,
      id: 'evt-5',
      timestamp: 1716000045000,
      type: 'task_completed',
      summary: 'Task "Analyze workspace" completed (100%)',
      agentName: 'Architect Agent',
      taskId: 'task-1',
    },
    {
      sequenceNumber: 6,
      id: 'evt-6',
      timestamp: 1716000050000,
      type: 'task_started',
      summary: 'Task "Design bridge contracts" started',
      agentName: 'Architect Agent',
      taskId: 'task-2',
    },
    {
      sequenceNumber: 7,
      id: 'evt-7',
      timestamp: 1716000070000,
      type: 'artifact_created',
      summary: 'Artifact task-m14-bridge-contracts.patch generated',
      taskId: 'task-2',
    },
    {
      sequenceNumber: 8,
      id: 'evt-8',
      timestamp: 1716000075000,
      type: 'task_completed',
      summary: 'Task "Design bridge contracts" completed (100%)',
      agentName: 'Architect Agent',
      taskId: 'task-2',
    },
    {
      sequenceNumber: 9,
      id: 'evt-9',
      timestamp: 1716000080000,
      type: 'task_started',
      summary: 'Task "Implement TreeViews" started',
      agentName: 'Coder Agent',
      taskId: 'task-3',
    },
    {
      sequenceNumber: 10,
      id: 'evt-10',
      timestamp: 1716000115000,
      type: 'approval_requested',
      summary: 'Approval required for command: npm install @vscode/test-electron',
      agentName: 'Coder Agent',
      taskId: 'task-4',
      details: {
        command: 'npm install @vscode/test-electron',
        risk: 'medium',
      },
    },
  ];

  private readonly _providers: ProviderSummary[] = [
    {
      instanceId: 'provider-anthropic-01',
      providerType: 'anthropic',
      displayName: 'Anthropic Claude 3.5 Sonnet',
      models: [
        {
          id: 'claude-3-5-sonnet-20241022',
          name: 'Claude 3.5 Sonnet',
          contextWindow: 200000,
          isDefault: true,
        },
        {
          id: 'claude-3-5-haiku-20241022',
          name: 'Claude 3.5 Haiku',
          contextWindow: 200000,
          isDefault: false,
        },
      ],
      status: 'available',
    },
    {
      instanceId: 'provider-openai-01',
      providerType: 'openai',
      displayName: 'OpenAI GPT-4o',
      models: [
        {
          id: 'gpt-4o',
          name: 'GPT-4o',
          contextWindow: 128000,
          isDefault: true,
        },
        {
          id: 'gpt-4o-mini',
          name: 'GPT-4o Mini',
          contextWindow: 128000,
          isDefault: false,
        },
      ],
      status: 'available',
    },
    {
      instanceId: 'provider-ollama-01',
      providerType: 'ollama',
      displayName: 'Local Ollama DeepSeek Coder',
      models: [
        {
          id: 'deepseek-coder:6.7b',
          name: 'DeepSeek Coder 6.7B',
          contextWindow: 16000,
          isDefault: true,
        },
      ],
      status: 'offline',
    },
  ];

  public get connectionState(): ConnectionState {
    return this._connectionState;
  }

  public get timeline(): TimelineEvent[] {
    return this._timeline.map((e) => ({ ...e }));
  }

  public emitAllTimelineEvents(): void {
    for (const evt of this._timeline) {
      this._onEvent.fire({ ...evt });
    }
  }

  public emitEvent(event: TimelineEvent): void {
    this._onEvent.fire(event);
  }

  public async connect(): Promise<void> {
    if (this._connectionState !== 'connected') {
      this._connectionState = 'connected';
      this._onConnectionStateChanged.fire(this._connectionState);
    }
  }

  public async disconnect(): Promise<void> {
    if (this._connectionState !== 'disconnected') {
      this._connectionState = 'disconnected';
      this._onConnectionStateChanged.fire(this._connectionState);
    }
  }

  public async getCurrentSession(): Promise<SessionInfo | null> {
    return { ...this._session };
  }

  public async getTaskGraph(runId?: string): Promise<TaskGraph | null> {
    const edges: Array<{ from: string; to: string }> = [];
    for (const task of this._tasks) {
      for (const depId of task.dependencies) {
        edges.push({ from: depId, to: task.id });
      }
    }
    return {
      runId: runId ?? this._session.activeRunId ?? 'run-m14-001',
      nodes: this._tasks.map(t => ({ ...t })),
      edges,
    };
  }

  public async listTasks(): Promise<TaskNode[]> {
    return this._tasks.map(t => ({ ...t }));
  }

  public async listPendingApprovals(): Promise<ApprovalRequest[]> {
    return this._approvals
      .filter(a => a.status === 'pending')
      .map(a => ({ ...a }));
  }

  public async requestApprovalDecision(
    id: string,
    decision: 'approved' | 'denied'
  ): Promise<ApprovalDecisionResult> {
    const approval = this._approvals.find(a => a.id === id);
    if (!approval) {
      return {
        id,
        decision,
        applied: false,
        message: `Approval request '${id}' not found.`,
      };
    }

    approval.status = decision;
    this._nextSequenceNumber++;

    const event: TimelineEvent = {
      sequenceNumber: this._nextSequenceNumber,
      id: `evt-${this._nextSequenceNumber}`,
      timestamp: Date.now(),
      type: 'approval_decided',
      summary: `Approval '${id}' was ${decision} (${approval.commandOrTarget})`,
      agentName: 'Architect Agent',
      taskId: approval.taskId,
      details: {
        approvalId: id,
        decision,
        commandOrTarget: approval.commandOrTarget,
      },
    };

    this._timeline.push(event);
    this._onEvent.fire(event);

    return {
      id,
      decision,
      applied: true,
      message: `Approval '${id}' ${decision} successfully.`,
    };
  }

  public async listArtifacts(): Promise<Artifact[]> {
    return this._artifacts.map(a => ({ ...a }));
  }

  public async getArtifactDiff(artifactId: string): Promise<string | null> {
    const artifact = this._artifacts.find(a => a.id === artifactId);
    return artifact ? artifact.diffContent : null;
  }

  public async listProviderSummaries(): Promise<ProviderSummary[]> {
    return this._providers.map(p => ({
      ...p,
      models: p.models.map(m => ({ ...m })),
    }));
  }

  public async submitEditorAction(action: EditorActionRequest): Promise<EditorActionResult> {
    const newTaskId = `task-action-${this._tasks.length + 1}`;
    const newTask: TaskNode = {
      id: newTaskId,
      runId: this._session.activeRunId ?? 'run-m14-001',
      title: `Editor ${action.action}: ${action.languageId}`,
      description: `User triggered ${action.action} on ${action.documentUri} (L${action.selectionRange.startLine}:${action.selectionRange.startCol} - L${action.selectionRange.endLine}:${action.selectionRange.endCol})`,
      status: 'ready',
      assignedAgentId: 'agent-coder-01',
      assignedAgentName: 'Coder Agent',
      dependencies: [],
      progress: 0,
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };

    this._tasks.push(newTask);
    this._session.totalTasksCount = this._tasks.length;

    this._nextSequenceNumber++;
    const event: TimelineEvent = {
      sequenceNumber: this._nextSequenceNumber,
      id: `evt-${this._nextSequenceNumber}`,
      timestamp: Date.now(),
      type: 'task_started',
      summary: `Task "${newTask.title}" queued via editor action`,
      agentName: newTask.assignedAgentName,
      taskId: newTask.id,
      details: {
        action: action.action,
        documentUri: action.documentUri,
        languageId: action.languageId,
      },
    };

    this._timeline.push(event);
    this._onEvent.fire(event);

    return {
      accepted: true,
      taskId: newTaskId,
      message: `Editor action '${action.action}' accepted and scheduled as ${newTaskId}.`,
    };
  }

  public dispose(): void {
    this._onConnectionStateChanged.dispose();
    this._onEvent.dispose();
  }
}
