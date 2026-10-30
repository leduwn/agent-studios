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
  ToolCallInfo,
} from './types';
import { SubmitTaskRequest, SubmitTaskResult } from './ipc/SubmitTaskAdapter';
import { CancelTaskResult } from './ipc/ExecutionClient';
import { RustExecutionReadinessReport } from './ipc/RustIpcProtocolTypes';

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

  private _sessions: SessionInfo[] = [
    {
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
    },
    {
      id: 'session-m13-refactor',
      workspacePath: 'd:/Projects/agent-studios-ide-ui/apps/agent-studios-vscode',
      goalSummary: 'Refactor bridge interfaces and add state projection unit tests',
      status: 'completed',
      activeAgentCount: 0,
      totalAgentCount: 1,
      completedTasksCount: 4,
      totalTasksCount: 4,
      providerSummary: 'Anthropic Claude 3.5 Sonnet',
      startedAt: 1715900000000,
      elapsedMs: 720000,
      isPreview: true,
    },
    {
      id: 'session-m12-dag-layout',
      workspacePath: 'd:/Projects/agent-studios-ide-ui/apps/agent-studios-vscode',
      goalSummary: 'Implement DAG cycle detection and topological layout algorithms',
      status: 'completed',
      activeAgentCount: 0,
      totalAgentCount: 1,
      completedTasksCount: 6,
      totalTasksCount: 6,
      providerSummary: 'OpenAI GPT-4o',
      startedAt: 1715800000000,
      elapsedMs: 540000,
      isPreview: true,
    },
  ];

  private _session: SessionInfo = this._sessions[0];

  private _chatMessages: ChatMessage[] = [
    {
      id: 'msg-preview-001',
      role: 'assistant',
      content: 'Welcome to Agent Studios! I am your AI coding assistant in developer preview mode.',
      timestamp: 1716000000000,
      mode: 'ask',
    },
  ];

  private _isCancelled: boolean = false;

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

  public get chatMessages(): ChatMessage[] {
    return this._chatMessages.map(m => ({ ...m }));
  }

  public async sendMessage(
    request: ChatSendRequest,
    onChunk: (chunk: ChatStreamChunk) => void
  ): Promise<ChatMessage> {
    this._isCancelled = false;

    // 1. Record user message
    const userMsg: ChatMessage = {
      id: `msg-user-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`,
      role: 'user',
      content: request.message,
      timestamp: Date.now(),
      mode: request.mode,
      attachments: request.attachments ? [...request.attachments] : undefined,
    };
    this._chatMessages.push(userMsg);

    const asstMsgId = `msg-asst-${Date.now()}-${Math.random().toString(36).substring(2, 7)}`;
    const toolCalls: ToolCallInfo[] = [];
    let fullContent = '';
    let approvalId: string | undefined;

    const emitDelta = async (text: string): Promise<boolean> => {
      if (this._isCancelled) {
        return false;
      }
      fullContent += text;
      onChunk({ type: 'text_delta', text });
      await Promise.resolve();
      return true;
    };

    const emitToolCall = async (toolCall: ToolCallInfo): Promise<boolean> => {
      if (this._isCancelled) {
        return false;
      }
      const existingIdx = toolCalls.findIndex(t => t.id === toolCall.id);
      if (existingIdx >= 0) {
        toolCalls[existingIdx] = { ...toolCall };
      } else {
        toolCalls.push({ ...toolCall });
      }
      onChunk({ type: 'tool_call', toolCall: { ...toolCall } });
      await Promise.resolve();
      return true;
    };

    const emitTaskProgress = async (taskSummary: string): Promise<boolean> => {
      if (this._isCancelled) {
        return false;
      }
      onChunk({ type: 'task_progress', taskSummary });
      await Promise.resolve();
      return true;
    };

    if (request.mode === 'ask') {
      const tool1: ToolCallInfo = {
        id: `call-read-${Date.now()}`,
        name: 'read_file',
        status: 'running',
        target: request.attachments?.[0]?.relativePath ?? 'src/bridge/types.ts',
        inputSummary: `Reading file ${request.attachments?.[0]?.relativePath ?? 'src/bridge/types.ts'}`,
      };
      if (await emitToolCall(tool1)) {
        tool1.status = 'completed';
        tool1.durationMs = 45;
        tool1.outputSummary = 'Read 220 lines';
        await emitToolCall(tool1);
      }

      await emitDelta('I have analyzed your request and codebase context.\n\n');
      await emitDelta('Here is an overview of the architecture:\n');
      await emitDelta('- The bridge pattern decouples UI views from background agent execution.\n');
      await emitDelta('- `SessionStateProjection` enforces monotonic event ordering and gap detection.\n');
      await emitDelta('- Preview mode uses `FixtureBridge` for deterministic testing.');
    } else if (request.mode === 'plan') {
      await emitTaskProgress('Analyzing workspace structure and requirements');

      const tool1: ToolCallInfo = {
        id: `call-read-${Date.now()}`,
        name: 'read_file',
        status: 'running',
        target: 'package.json',
        inputSummary: 'Inspecting package manifests and script targets',
      };
      if (await emitToolCall(tool1)) {
        tool1.status = 'completed';
        tool1.durationMs = 30;
        tool1.outputSummary = 'Found scripts: compile, test, typecheck';
        await emitToolCall(tool1);
      }

      await emitDelta('### Proposed Execution Plan\n\n');
      await emitDelta('1. **Context & Types**: Define domain models for chat sessions and streaming chunks.\n');
      await emitDelta('2. **Bridge Interfaces**: Extend `AgentStudiosBridge` with streaming and session management.\n');
      await emitDelta('3. **State Projection**: Project chat messages and fire reactive change events.\n');
      await emitDelta('4. **Verification**: Execute test suite to confirm backward compatibility.');
    } else {
      // mode === 'agent'
      await emitTaskProgress('Starting automated agent workflow');

      const tool1: ToolCallInfo = {
        id: `call-read-${Date.now()}`,
        name: 'read_file',
        status: 'completed',
        target: 'src/bridge/types.ts',
        inputSummary: 'Reading type definitions',
        outputSummary: 'Loaded types',
        durationMs: 25,
      };
      await emitToolCall(tool1);

      const isHighRisk = /delete|destroy|critical|remove/i.test(request.message);
      const riskLevel = isHighRisk ? 'high' : 'medium';
      const command = 'npm run build';

      const tool2: ToolCallInfo = {
        id: `call-exec-${Date.now()}`,
        name: 'exec_command',
        status: 'running',
        target: command,
        inputSummary: `Executing: ${command}`,
      };
      await emitToolCall(tool2);

      approvalId = `approval-chat-${Date.now()}`;
      const approval: ApprovalRequest = {
        id: approvalId,
        sessionId: this._session.id,
        actionType: 'exec_command',
        commandOrTarget: command,
        riskLevel,
        requestedPermissions: ['shell:exec'],
        status: 'pending',
        createdAt: Date.now(),
      };
      this._approvals.push(approval);

      this._nextSequenceNumber++;
      const approvalEvent: TimelineEvent = {
        sequenceNumber: this._nextSequenceNumber,
        id: `evt-${this._nextSequenceNumber}`,
        timestamp: Date.now(),
        type: 'approval_requested',
        summary: `Approval required for command: ${command}`,
        agentName: 'Coder Agent',
        details: {
          approvalId,
          command,
          risk: riskLevel,
        },
      };
      this._timeline.push(approvalEvent);
      this._onEvent.fire(approvalEvent);

      onChunk({ type: 'approval_required', approval });
      await Promise.resolve();

      tool2.status = 'completed';
      tool2.durationMs = 120;
      tool2.outputSummary = 'Command succeeded with exit code 0';
      await emitToolCall(tool2);

      const tool3: ToolCallInfo = {
        id: `call-patch-${Date.now()}`,
        name: 'apply_patch',
        status: 'completed',
        target: 'src/bridge/types.ts',
        inputSummary: 'Applying type changes',
        outputSummary: 'Patch applied successfully',
        durationMs: 40,
      };
      await emitToolCall(tool3);

      await emitDelta('Task executed successfully.\n');
      await emitDelta('All tool calls completed and artifacts were generated.');
    }

    if (this._isCancelled) {
      onChunk({ type: 'error', error: 'Execution cancelled by user' });
      const cancelledMsg: ChatMessage = {
        id: asstMsgId,
        role: 'assistant',
        content: fullContent + '\n\n[Execution cancelled by user]',
        timestamp: Date.now(),
        mode: request.mode,
        toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
        approvalId,
        isStreaming: false,
      };
      this._chatMessages.push(cancelledMsg);
      return cancelledMsg;
    }

    onChunk({ type: 'done' });

    const assistantMsg: ChatMessage = {
      id: asstMsgId,
      role: 'assistant',
      content: fullContent,
      timestamp: Date.now(),
      mode: request.mode,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      approvalId,
      isStreaming: false,
    };
    this._chatMessages.push(assistantMsg);
    return assistantMsg;
  }

  public async cancelCurrentRun(): Promise<void> {
    this._isCancelled = true;
  }

  public async submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResult> {
    const taskId = `task-fixture-${Date.now()}`;
    this._nextSequenceNumber++;
    const startEvt: TimelineEvent = {
      sequenceNumber: this._nextSequenceNumber,
      id: `evt-${this._nextSequenceNumber}`,
      timestamp: Date.now(),
      type: 'task_started',
      summary: `Task "${req.prompt}" started`,
      taskId,
    };
    this._timeline.push(startEvt);
    this._onEvent.fire(startEvt);

    return {
      success: true,
      taskId,
      taskKey: `key-${taskId}`,
      studioId: req.studioId,
      displayTitle: req.prompt,
    };
  }

  public async cancelTask(taskId: string, _reason?: string): Promise<CancelTaskResult> {
    this._nextSequenceNumber++;
    const cancelEvt: TimelineEvent = {
      sequenceNumber: this._nextSequenceNumber,
      id: `evt-${this._nextSequenceNumber}`,
      timestamp: Date.now(),
      type: 'task_cancelled',
      summary: `Task ${taskId} cancelled`,
      taskId,
    };
    this._timeline.push(cancelEvt);
    this._onEvent.fire(cancelEvt);

    return {
      success: true,
      taskId,
      studioId: 'fixture-studio',
      summary: {
        cancelled_tasks: [taskId],
        cancelled_runs: [],
        cancelled_approvals: [],
      },
    };
  }

  public async getExecutionReadiness(): Promise<RustExecutionReadinessReport> {
    return {
      is_ready: true,
      available_runtimes: ['claude-code'],
      runtime_instances: [
        {
          instance_id: 'claude-code-local',
          implementation: 'claude-code',
          healthy: true,
        },
      ],
      available_models: [
        {
          provider_instance_id: 'provider-anthropic-01',
          model_id: 'claude-3-5-sonnet',
          display_name: 'Claude 3.5 Sonnet',
        },
      ],
      authorized_workspaces: [this._session.workspacePath],
      defaults: {
        default_repo_path: this._session.workspacePath,
        default_provider_instance_id: 'provider-anthropic-01',
        default_model_id: 'claude-3-5-sonnet',
        configured_instance_id: 'claude-code-local',
      },
      blocking_reasons: [],
    };
  }

  public async listSessions(): Promise<SessionSummary[]> {
    return this._sessions.map(s => ({
      id: s.id,
      title: s.goalSummary,
      status: s.status,
      updatedAt: s.startedAt + s.elapsedMs,
      isPreview: s.isPreview,
    }));
  }

  public async createSession(goal?: string): Promise<SessionInfo> {
    const newSession: SessionInfo = {
      id: `session-preview-${Date.now()}`,
      workspacePath: this._session.workspacePath,
      goalSummary: goal || 'New Preview Session',
      status: 'idle',
      activeAgentCount: 0,
      totalAgentCount: 0,
      completedTasksCount: 0,
      totalTasksCount: 0,
      providerSummary: 'Anthropic Claude 3.5 Sonnet',
      startedAt: Date.now(),
      elapsedMs: 0,
      isPreview: true,
    };

    this._sessions.unshift(newSession);
    this._session = newSession;

    this._nextSequenceNumber++;
    const event: TimelineEvent = {
      sequenceNumber: this._nextSequenceNumber,
      id: `evt-${this._nextSequenceNumber}`,
      timestamp: Date.now(),
      type: 'session_started',
      summary: `Session "${newSession.goalSummary}" started`,
    };
    this._timeline.push(event);
    this._onEvent.fire(event);

    return { ...this._session };
  }

  public async switchSession(sessionId: string): Promise<SessionInfo> {
    const target = this._sessions.find(s => s.id === sessionId);
    if (!target) {
      throw new Error(`Session '${sessionId}' not found.`);
    }
    this._session = target;
    return { ...this._session };
  }

  public setSessionWorkspacePathForTesting(wsPath: string): void {
    this._session = { ...this._session, workspacePath: wsPath };
  }

  public dispose(): void {
    this._onConnectionStateChanged.dispose();
    this._onEvent.dispose();
  }
}
