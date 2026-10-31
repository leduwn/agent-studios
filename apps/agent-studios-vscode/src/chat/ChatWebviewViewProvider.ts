/**
 * Agent Studios — Chat WebviewView Provider
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 *
 * Implements vscode.WebviewViewProvider for the 'agent-studios.chat' view.
 * Handles lifecycle resolution, bidirectional messaging between Webview and daemon/fixture bridge,
 * streaming chunk updates, context attachments, mode/model switching, session history,
 * and composer draft persistence.
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { CapabilityMatrix } from '../bridge/ipc/CapabilityMatrix';
import {
  AgentMode,
  ApprovalDecisionResult,
  ApprovalRequest,
  ChatMessage,
  ChatSendRequest,
  ChatStreamChunk,
  ContextAttachment,
  TimelineEvent,
} from '../bridge/types';
import { OperatorConfirmationModal } from '../security/OperatorConfirmationModal';
import { WorkspaceVerification } from '../security/WorkspaceVerification';
import { SessionStateProjection } from '../state/SessionStateProjection';
import { TaskExecutionStore } from '../state/TaskExecutionStore';
import { ChatHtmlBuilder } from './ChatHtmlBuilder';
import { ChatSecurityGuards } from './ChatSecurityGuards';

export class ChatWebviewViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  public static readonly viewType = 'agent-studios.chat';

  private readonly _extensionUri: vscode.Uri;
  private _bridgeManager?: BridgeManager;
  private _bridge: AgentStudiosBridge;
  private _stateProjection?: SessionStateProjection;
  private _view?: vscode.WebviewView;
  private _composerDraft: string = '';
  private _activeMode: AgentMode = 'agent';
  private _selectedModelId?: string;
  private _selectedProviderId?: string;
  private _attachments: ContextAttachment[] = [];
  private _messages: ChatMessage[] = [];
  private _hasSeenOnboarding = false;
  private readonly _disposables: vscode.Disposable[] = [];
  private _isDisposed = false;

  constructor(
    extensionUri: vscode.Uri,
    bridgeOrManager: AgentStudiosBridge | BridgeManager,
    stateProjection?: SessionStateProjection
  ) {
    this._extensionUri = extensionUri;

    if (bridgeOrManager instanceof BridgeManager) {
      this._bridgeManager = bridgeOrManager;
      this._bridge = bridgeOrManager.bridge;
      this._stateProjection = bridgeOrManager.stateProjection;

      this._disposables.push(
        bridgeOrManager.onBridgeChanged((newBridge) => {
          this._bridge = newBridge;
          void this.syncState();
        })
      );
    } else {
      this._bridge = bridgeOrManager;
      this._stateProjection = stateProjection;
    }

    if (this._stateProjection) {
      this._disposables.push(
        this._stateProjection.onSessionChanged(() => {
          void this.syncState();
        })
      );
    }

    this._disposables.push(
      TaskExecutionStore.instance.onDidChangeExecutionState((state) => {
        void this.postMessage({
          type: 'executionStateChanged',
          executionState: state,
        });
      })
    );

    this._disposables.push(
      TaskExecutionStore.instance.onDidStreamChunk((chunk) => {
        void this.postMessage({
          type: 'streamChunk',
          chunk,
        });
      })
    );
  }

  public get bridge(): AgentStudiosBridge {
    return this._bridgeManager ? this._bridgeManager.bridge : this._bridge;
  }

  public get isPreview(): boolean {
    if (this._bridgeManager) {
      return this._bridgeManager.isPreviewMode;
    }
    return Boolean((this._bridge as { isPreview?: boolean }).isPreview);
  }

  public get composerDraft(): string {
    return this._composerDraft;
  }

  public get activeMode(): AgentMode {
    return this._activeMode;
  }

  public get messages(): ChatMessage[] {
    return [...this._messages];
  }

  public get attachments(): ContextAttachment[] {
    return [...this._attachments];
  }

  /**
   * Resolves the WebviewView when it becomes visible.
   */
  public async resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): Promise<void> {
    this._view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._extensionUri],
    };

    const session = await this.bridge.getCurrentSession().catch(() => null);
    const sessions = await this.bridge.listSessions().catch(() => []);
    const providers = await this.bridge.listProviderSummaries().catch(() => []);
    const localWorkspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '';

    const readiness = typeof this.bridge.getExecutionReadiness === 'function'
      ? await this.bridge.getExecutionReadiness().catch(() => null)
      : null;

    let runtimeOptions: { id: string; name: string; enabled: boolean; tooltip?: string }[] | undefined;
    let selectedRuntime = '';
    if (readiness) {
      if (readiness.runtime_instances && readiness.runtime_instances.length > 0) {
        runtimeOptions = readiness.runtime_instances.map((inst) => ({
          id: inst.implementation,
          name: `${inst.implementation} (${inst.instance_id})`,
          enabled: inst.healthy,
          tooltip: inst.healthy ? 'Healthy runtime instance' : 'Degraded runtime instance',
        }));
      } else if (readiness.available_runtimes && readiness.available_runtimes.length > 0) {
        runtimeOptions = readiness.available_runtimes.map((r) => ({
          id: r,
          name: r,
          enabled: true,
        }));
      }
      if (readiness.defaults?.configured_instance_id) {
        selectedRuntime = readiness.defaults.configured_instance_id;
      } else if (readiness.available_runtimes && readiness.available_runtimes.length > 0) {
        selectedRuntime = readiness.available_runtimes[0];
      } else if (runtimeOptions && runtimeOptions.length > 0) {
        selectedRuntime = runtimeOptions[0].id;
      }
    }

    let studioOptions: { id: string; name: string }[] | undefined;
    if (sessions && sessions.length > 0) {
      studioOptions = sessions.map((s) => ({
        id: s.id,
        name: s.title ? `${s.title} (${s.id})` : s.id,
      }));
    } else if (session) {
      studioOptions = [{ id: session.id, name: session.goalSummary || session.id }];
    }
    const selectedStudioId = session?.id || (studioOptions && studioOptions[0]?.id) || '';

    const wsVerification = WorkspaceVerification.verify(localWorkspace, readiness);
    const workspaceStatus = wsVerification.status;
    const isWorkspaceMatched = wsVerification.isAuthorized;
    const workspaceMismatchReason = wsVerification.isAuthorized ? '' : wsVerification.reason;

    const isTaskSubmitEnabled = CapabilityMatrix.isCapabilityEnabled('task.submit') && isWorkspaceMatched;

    webviewView.webview.html = ChatHtmlBuilder.buildHtml(
      webviewView.webview,
      this._extensionUri,
      {
        session,
        sessions,
        connectionState: this.bridge.connectionState,
        isPreview: this.isPreview,
        providers,
        activeMode: this._activeMode,
        selectedModelId: this._selectedModelId,
        selectedProviderId: this._selectedProviderId,
        composerDraft: this._composerDraft,
        messages: this._messages,
        attachments: this._attachments,
        hasSeenOnboarding: this._hasSeenOnboarding,
        selectedRuntime,
        selectedStudioId,
        runtimes: runtimeOptions,
        studios: studioOptions,
        workspacePath: localWorkspace,
        workspaceStatus,
        isWorkspaceMatched,
        workspaceMismatchReason,
        isTaskSubmitEnabled,
        executionState: TaskExecutionStore.instance.getState(),
      }
    );

    // Reconnect state when view becomes visible again
    webviewView.onDidChangeVisibility(() => {
      if (webviewView.visible) {
        void this.syncState();
      }
    }, null, this._disposables);

    // Handle incoming messages from the webview
    webviewView.webview.onDidReceiveMessage(
      (message) => void this.handleWebviewMessage(message),
      null,
      this._disposables
    );

    // Listen to bridge timeline events
    this._disposables.push(
      this.bridge.onEvent((event) => {
        this._handleTimelineEvent(event);
      })
    );
  }

  /**
   * Dispatches incoming message commands from the webview client.
   */
  public async handleWebviewMessage(message: {
    command: string;
    text?: string;
    prompt?: string;
    runtime?: string;
    studioId?: string;
    taskId?: string;
    mode?: AgentMode;
    attachments?: ContextAttachment[];
    modelId?: string;
    providerId?: string;
    approvalId?: string;
    artifactId?: string;
    filePath?: string;
    source?: 'filePicker' | 'activeEditor';
    sessionId?: string;
    draft?: string;
    path?: string;
  }): Promise<void> {
    if (!message || !message.command) {
      return;
    }

    // Security Validation: Prototype pollution, command allowlist, path traversal
    const validation = ChatSecurityGuards.validateWebviewMessage(message);
    if (!validation.valid) {
      await this.postMessage({
        type: 'error',
        error: `Security guard rejected request: ${validation.error}`,
      });
      return;
    }

    // Security Validation: Workspace trust enforcement
    if (!ChatSecurityGuards.verifyWorkspaceTrust(message.command)) {
      await this.postMessage({
        type: 'error',
        error: 'Action blocked: This workspace is untrusted. Agent Studios requires workspace trust to execute coordinator commands.',
      });
      return;
    }

    switch (message.command) {
      case 'sendMessage':
        await this._handleSendMessage(message);
        break;

      case 'submitTask':
        await this._handleSubmitTask(message);
        break;

      case 'cancelTask':
        await this._handleCancelTask(message);
        break;

      case 'cancelRun':
        await this._handleCancelRun();
        break;

      case 'approveAction':
        await this._handleApproveAction(message.approvalId);
        break;

      case 'denyAction':
        await this._handleDenyAction(message.approvalId);
        break;

      case 'openDiff':
        await this._handleOpenDiff(message.artifactId, message.filePath);
        break;

      case 'attachContext':
        await this._handleAttachContext(message.source);
        break;

      case 'removeAttachment':
        if (message.path) {
          this._attachments = this._attachments.filter(
            (a) => (a.uri || a.relativePath) !== message.path
          );
        }
        break;

      case 'switchMode':
        if (message.mode) {
          this._activeMode = message.mode;
          await this.postMessage({ type: 'modeChanged', mode: this._activeMode });
        }
        break;

      case 'switchProviderModel':
        this._selectedModelId = message.modelId;
        this._selectedProviderId = message.providerId;
        await this.postMessage({
          type: 'modelChanged',
          modelId: this._selectedModelId,
          providerId: this._selectedProviderId,
        });
        break;

      case 'switchSession':
        if (message.sessionId) {
          await this._handleSwitchSession(message.sessionId);
        }
        break;

      case 'newSession':
        await this._handleNewSession();
        break;

      case 'openTaskGraph':
        await vscode.commands.executeCommand('agent-studios.showTaskGraph');
        break;

      case 'saveDraft':
        this._composerDraft = message.draft ?? '';
        break;

      case 'enablePreviewMode':
        await vscode.commands.executeCommand('agent-studios.toggleFixturePreview');
        break;

      case 'connectDaemon':
        await vscode.commands.executeCommand('agent-studios.connectDaemon');
        break;

      case 'dismissOnboarding':
        this._hasSeenOnboarding = true;
        break;
    }
  }

  private async _handleSendMessage(message: {
    text?: string;
    mode?: AgentMode;
    attachments?: ContextAttachment[];
    modelId?: string;
    providerId?: string;
  }): Promise<void> {
    const prompt = message.text || '';
    if (!prompt.trim()) {
      return;
    }

    const mode = message.mode || this._activeMode;
    const attachments = message.attachments || this._attachments;

    const request: ChatSendRequest = {
      message: prompt,
      mode,
      modelId: message.modelId || this._selectedModelId,
      providerInstanceId: message.providerId || this._selectedProviderId,
      attachments,
    };

    const userMsg: ChatMessage = {
      id: `msg-user-${Date.now()}`,
      role: 'user',
      content: prompt,
      timestamp: Date.now(),
      mode,
      attachments: attachments.length > 0 ? [...attachments] : undefined,
    };
    this._messages.push(userMsg);

    try {
      const result = await this.bridge.sendMessage(request, (chunk: ChatStreamChunk) => {
        void this.postMessage({
          type: 'streamChunk',
          chunk,
        });
      });

      this._messages.push(result);
      await this.postMessage({
        type: 'messageComplete',
        message: result,
      });
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      await this.postMessage({
        type: 'error',
        error: errorMsg,
      });
    }
  }

  private async _handleCancelRun(): Promise<void> {
    try {
      await this.bridge.cancelCurrentRun();
      await this.postMessage({ type: 'runCancelled' });
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      await this.postMessage({ type: 'error', error: errorMsg });
    }
  }

  private async _handleApproveAction(approvalId?: string): Promise<void> {
    if (!approvalId) {
      return;
    }
    try {
      const res = await vscode.commands.executeCommand<ApprovalDecisionResult>(
        'agent-studios.approveAction',
        approvalId
      );
      await this.postMessage({
        type: 'approvalResult',
        approvalId,
        decision: 'approved',
        applied: res ? res.applied : true,
        message: res?.message,
      });
    } catch {
      try {
        const res = await this.bridge.requestApprovalDecision(approvalId, 'approved');
        await this.postMessage({
          type: 'approvalResult',
          approvalId,
          decision: 'approved',
          applied: res.applied,
          message: res.message,
        });
      } catch (err: unknown) {
        await this.postMessage({
          type: 'approvalResult',
          approvalId,
          decision: 'approved',
          applied: false,
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }

  private async _handleDenyAction(approvalId?: string): Promise<void> {
    if (!approvalId) {
      return;
    }
    try {
      const res = await vscode.commands.executeCommand<ApprovalDecisionResult>(
        'agent-studios.denyAction',
        approvalId
      );
      await this.postMessage({
        type: 'approvalResult',
        approvalId,
        decision: 'denied',
        applied: res ? res.applied : true,
        message: res?.message,
      });
    } catch {
      try {
        const res = await this.bridge.requestApprovalDecision(approvalId, 'denied');
        await this.postMessage({
          type: 'approvalResult',
          approvalId,
          decision: 'denied',
          applied: res.applied,
          message: res.message,
        });
      } catch (err: unknown) {
        await this.postMessage({
          type: 'approvalResult',
          approvalId,
          decision: 'denied',
          applied: false,
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }

  private async _handleOpenDiff(artifactId?: string, filePath?: string): Promise<void> {
    if (artifactId) {
      await vscode.commands.executeCommand('agent-studios.inspectArtifact', artifactId);
    } else if (filePath) {
      const artifacts = await this.bridge.listArtifacts().catch(() => []);
      const matched = artifacts.find((a) => a.path === filePath || a.name === filePath);
      if (matched) {
        await vscode.commands.executeCommand('agent-studios.inspectArtifact', matched.id);
      } else {
        const uri = vscode.Uri.file(filePath);
        await vscode.window.showTextDocument(uri);
      }
    }
  }

  private async _handleAttachContext(source?: 'filePicker' | 'activeEditor'): Promise<void> {
    if (source === 'activeEditor') {
      const editor = vscode.window.activeTextEditor;
      if (editor) {
        const filePath = editor.document.uri.fsPath || editor.document.uri.path;
        const relativePath = (vscode.workspace as { asRelativePath?: (u: unknown) => string }).asRelativePath
          ? (vscode.workspace as { asRelativePath: (u: unknown) => string }).asRelativePath(editor.document.uri)
          : filePath;

        const attachment: ContextAttachment = {
          type: 'file',
          uri: editor.document.uri.toString(),
          relativePath,
          languageId: editor.document.languageId,
          contentSnippet:
            editor.selection && !editor.selection.isEmpty
              ? editor.document.getText(editor.selection)
              : undefined,
        };

        this._attachments.push(attachment);
        await this.postMessage({
          type: 'contextAttached',
          attachment,
          attachments: this._attachments,
        });
      } else {
        await vscode.window.showInformationMessage('No active editor open to attach.');
      }
      return;
    }

    // Default: file picker dialog
    const uris = await vscode.window.showOpenDialog({
      canSelectMany: true,
      openLabel: 'Attach to Agent Studios',
    });

    if (uris && uris.length > 0) {
      for (const u of uris) {
        const filePath = u.fsPath || u.path;
        const relativePath = (vscode.workspace as { asRelativePath?: (u: unknown) => string }).asRelativePath
          ? (vscode.workspace as { asRelativePath: (u: unknown) => string }).asRelativePath(u)
          : filePath;

        this._attachments.push({
          type: 'file',
          uri: u.toString(),
          relativePath,
        });
      }
      await this.postMessage({
        type: 'contextAttached',
        attachments: this._attachments,
      });
    }
  }

  private async _handleSwitchSession(sessionId: string): Promise<void> {
    try {
      const session = await this.bridge.switchSession(sessionId);
      this._messages = [];
      await this.postMessage({
        type: 'syncState',
        state: {
          session,
          messages: this._messages,
        },
      });
    } catch (err: unknown) {
      await vscode.window.showErrorMessage(
        `Failed to switch session: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  private async _handleNewSession(): Promise<void> {
    try {
      const session = await this.bridge.createSession();
      this._messages = [];
      this._attachments = [];
      this._composerDraft = '';
      await this.postMessage({
        type: 'syncState',
        state: {
          session,
          messages: [],
          attachments: [],
          composerDraft: '',
        },
      });
    } catch (err: unknown) {
      await vscode.window.showErrorMessage(
        `Failed to create session: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  private async _handleSubmitTask(message: {
    prompt?: string;
    runtime?: string;
    studioId?: string;
    modelId?: string;
    providerId?: string;
  }): Promise<void> {
    const prompt = message.prompt || '';
    if (!prompt || /^\s*$/.test(prompt)) {
      await this.postMessage({ type: 'error', error: 'Prompt cannot be empty.' });
      return;
    }
    const promptByteLength = Buffer.byteLength(prompt, 'utf8');
    if (promptByteLength > 4 * 1024 * 1024) {
      await this.postMessage({ type: 'error', error: 'Prompt exceeds 4 MiB wire frame limit.' });
      return;
    }
    if (promptByteLength > 100 * 1024) {
      void vscode.window.showWarningMessage('Prompt size is larger than 100 KB; wire framing limits may apply.');
    }

    if (!CapabilityMatrix.isCapabilityEnabled('task.submit')) {
      await this.postMessage({
        type: 'error',
        error: 'Task submission is disabled by the capability matrix (task.submit capability is not enabled).',
      });
      return;
    }

    const localWorkspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '';
    const readiness = typeof this.bridge.getExecutionReadiness === 'function'
      ? await this.bridge.getExecutionReadiness().catch(() => null)
      : null;

    const wsVerification = WorkspaceVerification.verify(localWorkspace, readiness);
    if (!wsVerification.isAuthorized) {
      await this.postMessage({
        type: 'error',
        error: `Workspace mismatch (${wsVerification.status}): ${wsVerification.reason}`,
      });
      return;
    }

    const studioId = message.studioId;
    if (!studioId || studioId.trim().length === 0) {
      await this.postMessage({
        type: 'error',
        error: 'Active studio identity is required before submitting tasks. Please select or create a studio.',
      });
      return;
    }

    const runtime = message.runtime || (readiness?.runtime_instances?.[0]?.implementation || readiness?.available_runtimes?.[0]) || 'opencode';
    if (!runtime || runtime.trim().length === 0) {
      await this.postMessage({
        type: 'error',
        error: 'Execution runtime is required before submitting tasks. Daemon has no active runtimes.',
      });
      return;
    }

    let runtimeInstanceId: string | undefined;
    if (readiness?.runtime_instances) {
      const matchedInst = readiness.runtime_instances.find(
        (inst) => inst.implementation === runtime || inst.instance_id === runtime
      );
      if (matchedInst) {
        runtimeInstanceId = matchedInst.instance_id;
      }
    }

    // Operator confirmation gate using trusted VS Code native modal dialog
    const confirmResult = await OperatorConfirmationModal.requestConfirmation({
      studioId,
      runtime,
      runtimeInstanceId,
      prompt,
      workspacePath: localWorkspace || undefined,
      modelId: message.modelId || this._selectedModelId,
      providerId: message.providerId || this._selectedProviderId,
      actionDescription: 'Mutating Coding Agent Task Execution',
    });

    if (!confirmResult.confirmed) {
      if (confirmResult.cancelledByUser) {
        await this.postMessage({
          type: 'streamChunk',
          chunk: '\n[Operator cancelled task execution confirmation]\n',
        });
      } else {
        await this.postMessage({
          type: 'error',
          error: confirmResult.reason || 'Operator confirmation failed or was rejected.',
        });
      }
      return;
    }

    const store = TaskExecutionStore.instance;
    store.beginSubmission(studioId, prompt);

    try {
      if (typeof this.bridge.submitTask === 'function') {
        const submitResult = await this.bridge.submitTask({
          studioId,
          prompt,
          runtimePreference: runtime,
          runtimeInstanceId,
          workspacePath: localWorkspace || undefined,
          requestedModelId: message.modelId || this._selectedModelId,
          requestedProviderId: message.providerId || this._selectedProviderId,
          confirmedByUser: true,
          confirmationChallenge: confirmResult.challenge,
        });

        store.taskSubmitted(submitResult.taskId, studioId);
        await this.postMessage({
          type: 'taskSubmitted',
          taskId: submitResult.taskId,
        });
      } else {
        throw new Error('Bridge does not support submitTask.');
      }
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      store.completeTask('failed', { message: errorMsg });
      await this.postMessage({
        type: 'error',
        error: `Task submission failed: ${errorMsg}`,
      });
    }
  }

  private async _handleCancelTask(message: {
    taskId?: string;
    studioId?: string;
  }): Promise<void> {
    if (!CapabilityMatrix.isCapabilityEnabled('task.cancel')) {
      await this.postMessage({
        type: 'error',
        error: 'Task cancellation is disabled by the capability matrix (task.cancel capability is not enabled).',
      });
      return;
    }

    const store = TaskExecutionStore.instance;
    const taskId = message.taskId || store.getState().activeTaskId;
    if (!taskId) {
      await this.postMessage({
        type: 'error',
        error: 'Cannot cancel task: No active task ID found.',
      });
      return;
    }

    try {
      if (typeof this.bridge.cancelTask === 'function') {
        const result = await this.bridge.cancelTask(taskId, 'Cancelled by user operator');
        store.completeTask('cancelled', { message: 'Cancelled by user operator' });
        await this.postMessage({
          type: 'taskCancelled',
          taskId,
          result,
        });
      } else {
        await this.bridge.cancelCurrentRun();
        store.completeTask('cancelled', { message: 'Cancelled by user operator' });
        await this.postMessage({
          type: 'taskCancelled',
          taskId,
        });
      }
    } catch (err: unknown) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      await this.postMessage({
        type: 'error',
        error: `Task cancellation failed: ${errorMsg}`,
      });
    }
  }

  private _handleTimelineEvent(event: TimelineEvent): void {
    TaskExecutionStore.instance.consumeDaemonTimelineEvent(event);

    if (event.type === 'approval_requested') {
      const approval = event.details as unknown as ApprovalRequest;
      if (approval) {
        void this.postMessage({
          type: 'streamChunk',
          chunk: {
            type: 'approval_required',
            approval,
          },
        });
      }
    } else if (event.type === 'task_started' || event.type === 'task_completed') {
      void this.postMessage({
        type: 'streamChunk',
        chunk: {
          type: 'task_progress',
          taskSummary: event.summary,
        },
      });
    }
  }

  public async syncState(): Promise<void> {
    if (!this._view) {
      return;
    }

    const session = await this.bridge.getCurrentSession().catch(() => null);
    const sessions = await this.bridge.listSessions().catch(() => []);
    const providers = await this.bridge.listProviderSummaries().catch(() => []);
    const localWorkspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '';

    const readiness = typeof this.bridge.getExecutionReadiness === 'function'
      ? await this.bridge.getExecutionReadiness().catch(() => null)
      : null;

    let runtimeOptions: { id: string; name: string; enabled: boolean; tooltip?: string }[] | undefined;
    let selectedRuntime = '';
    if (readiness) {
      if (readiness.runtime_instances && readiness.runtime_instances.length > 0) {
        runtimeOptions = readiness.runtime_instances.map((inst) => ({
          id: inst.implementation,
          name: `${inst.implementation} (${inst.instance_id})`,
          enabled: inst.healthy,
          tooltip: inst.healthy ? 'Healthy runtime instance' : 'Degraded runtime instance',
        }));
      } else if (readiness.available_runtimes && readiness.available_runtimes.length > 0) {
        runtimeOptions = readiness.available_runtimes.map((r) => ({
          id: r,
          name: r,
          enabled: true,
        }));
      }
      if (readiness.defaults?.configured_instance_id) {
        selectedRuntime = readiness.defaults.configured_instance_id;
      } else if (readiness.available_runtimes && readiness.available_runtimes.length > 0) {
        selectedRuntime = readiness.available_runtimes[0];
      } else if (runtimeOptions && runtimeOptions.length > 0) {
        selectedRuntime = runtimeOptions[0].id;
      }
    }

    let studioOptions: { id: string; name: string }[] | undefined;
    if (sessions && sessions.length > 0) {
      studioOptions = sessions.map((s) => ({
        id: s.id,
        name: s.title ? `${s.title} (${s.id})` : s.id,
      }));
    } else if (session) {
      studioOptions = [{ id: session.id, name: session.goalSummary || session.id }];
    }
    const selectedStudioId = session?.id || (studioOptions && studioOptions[0]?.id) || '';

    const wsVerification = WorkspaceVerification.verify(localWorkspace, readiness);
    const workspaceStatus = wsVerification.status;
    const isWorkspaceMatched = wsVerification.isAuthorized;
    const workspaceMismatchReason = wsVerification.isAuthorized ? '' : wsVerification.reason;

    const isTaskSubmitEnabled = CapabilityMatrix.isCapabilityEnabled('task.submit') && isWorkspaceMatched;

    await this.postMessage({
      type: 'syncState',
      state: {
        session,
        sessions,
        runtimes: runtimeOptions,
        studios: studioOptions,
        selectedRuntime,
        selectedStudioId,
        connectionState: this.bridge.connectionState,
        isPreview: this.isPreview,
        providers,
        mode: this._activeMode,
        composerDraft: this._composerDraft,
        messages: this._messages,
        attachments: this._attachments,
        executionState: TaskExecutionStore.instance.getState(),
        workspaceStatus,
        isWorkspaceMatched,
        workspaceMismatchReason,
        isTaskSubmitEnabled,
      },
    });
  }

  public async postMessage(message: unknown): Promise<boolean> {
    if (!this._view) {
      return false;
    }
    return this._view.webview.postMessage(message);
  }

  public dispose(): void {
    if (this._isDisposed) {
      return;
    }
    this._isDisposed = true;
    while (this._disposables.length) {
      const d = this._disposables.pop();
      d?.dispose();
    }
    this._view = undefined;
  }
}
