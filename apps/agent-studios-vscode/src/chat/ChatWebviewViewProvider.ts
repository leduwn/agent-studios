/**
 * Agent Studios — Chat WebviewView Provider
 * Implements the primary conversation-first Chat view ('agent-studios.chat')
 * with unified Ask/Plan/Agent send routing, real ProviderProfileManager merging,
 * inline recovery actions, grouped tool activity, diff review, and progressive multi-agent disclosure.
 */

import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { BridgeManager } from '../bridge/BridgeManager';
import { CapabilityMatrix } from '../bridge/ipc/CapabilityMatrix';
import { RustExecutionReadinessReport } from '../bridge/ipc/RustIpcProtocolTypes';
import {
  AgentMode,
  ApprovalDecisionResult,
  ApprovalRequest,
  ChatMessage,
  ChatSendRequest,
  ChatStreamChunk,
  ContextAttachment,
  ProviderSummary,
  SessionSummary,
  TimelineEvent,
} from '../bridge/types';
import { ProviderProfileManager } from '../commands/ProviderManagementCommands';
import { ContextEngine } from '../context/ContextEngine';
import { AGENT_STUDIOS_PROVIDER_MODEL_KEY } from '../controls/ProviderModelSelector';
import { OperatorConfirmationModal } from '../security/OperatorConfirmationModal';
import { WorkspaceVerification } from '../security/WorkspaceVerification';
import { SessionStateProjection } from '../state/SessionStateProjection';
import { TaskExecutionStore } from '../state/TaskExecutionStore';
import { isKnownTestOrFixtureRecord } from '../state/UserStateMigration';
import { ChatHtmlBuilder, ChatHtmlInitialState, RuntimeOption } from './ChatHtmlBuilder';
import {
  filterCompatibleRuntimes,
  pickDefaultRuntime,
  resolveSendReadiness,
  resolveUiConnectionStatus,
} from './ChatPresenter';
import { ChatSecurityGuards } from './ChatSecurityGuards';
import { ConversationManager } from './ConversationManager';

export class ChatWebviewViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  public static readonly viewType = 'agent-studios.chat';

  public static buildRuntimeOptions(
    readiness: RustExecutionReadinessReport | null | undefined
  ): { runtimeOptions: RuntimeOption[]; selectedRuntime: string } {
    if (!readiness) {
      return {
        runtimeOptions: [
          { id: '', name: 'No runtimes available (daemon unready)', enabled: false },
        ],
        selectedRuntime: '',
      };
    }

    const instances = readiness.runtime_instances || [];
    const seenIds = new Set<string>();
    const duplicateIds = new Set<string>();
    for (const inst of instances) {
      if (seenIds.has(inst.instance_id)) {
        duplicateIds.add(inst.instance_id);
      }
      seenIds.add(inst.instance_id);
    }

    const runtimeOptions: RuntimeOption[] = [];

    for (const inst of instances) {
      const isDuplicate = duplicateIds.has(inst.instance_id);
      const isNativeUnverified =
        (inst.implementation === 'claude_code' ||
          inst.implementation === 'claude-code' ||
          inst.implementation === 'codex') &&
        (!readiness.available_runtimes || !readiness.available_runtimes.includes(inst.implementation));

      const isEnabled = inst.healthy && !isDuplicate && !isNativeUnverified;
      let tooltip = 'Healthy runtime instance';
      if (isDuplicate) {
        tooltip = 'Duplicate runtime instance ID detected in daemon readiness (disabled)';
      } else if (!inst.healthy) {
        tooltip = 'Degraded/Unhealthy runtime instance';
      } else if (isNativeUnverified) {
        tooltip = `Native ${inst.implementation} adapter is unverified / not implemented in backend`;
      }

      const compositeId = `${inst.instance_id}|${inst.implementation}`;
      const suffix = isDuplicate
        ? ' [Duplicate ID - Disabled]'
        : !inst.healthy
          ? ' [Unhealthy]'
          : isNativeUnverified
            ? ' [Unverified Adapter]'
            : '';
      runtimeOptions.push({
        id: compositeId,
        name: `${inst.implementation} (${inst.instance_id})${suffix}`,
        enabled: isEnabled,
        tooltip,
        instanceId: inst.instance_id,
        implementation: inst.implementation,
      });
    }

    if (readiness.registered_runtimes) {
      for (const reg of readiness.registered_runtimes) {
        if (!instances.some((i) => i.implementation === reg)) {
          runtimeOptions.push({
            id: `unregistered|${reg}`,
            name: `${reg} (Registered, no discovered instance)`,
            enabled: false,
            tooltip: 'Registered adapter has no discovered running instance',
            implementation: reg,
          });
        }
      }
    }

    let selectedRuntime = '';
    const configuredId = readiness.defaults?.configured_instance_id;
    if (configuredId && configuredId.trim().length > 0) {
      const matched = runtimeOptions.find((r) => r.instanceId === configuredId && r.enabled);
      if (matched) {
        selectedRuntime = matched.id;
      }
    }
    if (!selectedRuntime) {
      const healthy = runtimeOptions.filter((r) => r.enabled);
      if (healthy.length === 1) {
        selectedRuntime = healthy[0].id;
      }
    }
    if (runtimeOptions.length === 0) {
      runtimeOptions.push({
        id: '',
        name: 'No runtimes available (daemon unready)',
        enabled: false,
      });
    }

    return { runtimeOptions, selectedRuntime };
  }

  private readonly _extensionUri: vscode.Uri;
  private _bridgeManager?: BridgeManager;
  private _bridge: AgentStudiosBridge;
  private _stateProjection?: SessionStateProjection;
  private _extensionContext?: vscode.ExtensionContext;
  private _view?: vscode.WebviewView;
  private _composerDraft: string = '';
  private _activeMode: AgentMode = 'ask';
  private _reasoningEffort: 'low' | 'medium' | 'high' = 'medium';
  private _permissionMode: 'read-only' | 'auto-edit' | 'full-access' = 'auto-edit';
  private _followUpQueueMode: 'queue' | 'steer' = 'queue';
  private _contextEngine?: ContextEngine;
  private _conversationManager?: ConversationManager;
  private _selectedModelId?: string;
  private _selectedProviderId?: string;
  private _selectedRuntime?: string;
  private _attachments: ContextAttachment[] = [];
  private _messages: ChatMessage[] = [];
  private _hasSeenOnboarding = false;
  private _isRunning = false;
  private readonly _disposables: vscode.Disposable[] = [];
  private _isDisposed = false;

  constructor(
    extensionUri: vscode.Uri,
    bridgeOrManager: AgentStudiosBridge | BridgeManager,
    stateProjection?: SessionStateProjection,
    contextEngine?: ContextEngine,
    conversationManager?: ConversationManager
  ) {
    this._extensionUri = extensionUri;
    this._contextEngine = contextEngine;
    this._conversationManager = conversationManager;

    if (bridgeOrManager instanceof BridgeManager) {
      this._bridgeManager = bridgeOrManager;
      this._bridge = bridgeOrManager.bridge;
      this._stateProjection = bridgeOrManager.stateProjection;

      this._disposables.push(
        bridgeOrManager.onBridgeChanged((newBridge) => {
          this._bridge = newBridge;
          void this.refresh();
        })
      );
    } else {
      this._bridge = bridgeOrManager;
      this._stateProjection = stateProjection;
    }

    if (this._stateProjection) {
      this._disposables.push(
        this._stateProjection.onSessionChanged(() => {
          void this.refresh();
        })
      );
    }

    this._disposables.push(
      TaskExecutionStore.instance.onDidChangeExecutionState((state) => {
        const running =
          state.status === 'submitting' ||
          state.status === 'queued' ||
          state.status === 'running';
        this._isRunning = running;
        void this.postMessage({ type: 'runState', isRunning: running });
        void this.postMessage({ type: 'executionStateChanged', executionState: state });
      })
    );

    this._disposables.push(
      TaskExecutionStore.instance.onDidStreamChunk((chunk) => {
        void this.postMessage({ type: 'streamChunk', chunk });
      })
    );
  }

  public get bridge(): AgentStudiosBridge {
    return this._bridgeManager ? this._bridgeManager.bridge : this._bridge;
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

  public setContextEngine(contextEngine: ContextEngine): void {
    this._contextEngine = contextEngine;
  }

  public setConversationManager(conversationManager: ConversationManager): void {
    this._conversationManager = conversationManager;
  }

  public setExtensionContext(context: vscode.ExtensionContext): void {
    this._extensionContext = context;
    const savedSelection = context.globalState.get<{ instanceId?: string; modelId?: string }>(
      AGENT_STUDIOS_PROVIDER_MODEL_KEY
    );
    if (savedSelection?.instanceId) {
      this._selectedProviderId = savedSelection.instanceId;
    }
    if (savedSelection?.modelId) {
      this._selectedModelId = savedSelection.modelId;
    }
  }

  public async startNewChat(): Promise<void> {
    await this._handleNewSession();
  }

  public async refresh(): Promise<void> {
    if (!this._view || this._isDisposed) {
      return;
    }
    const state = await this._collectState();
    this._view.webview.html = ChatHtmlBuilder.buildHtml(
      this._view.webview,
      this._extensionUri,
      state
    );
  }

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

    const initialState = await this._collectState();
    webviewView.webview.html = ChatHtmlBuilder.buildHtml(
      webviewView.webview,
      this._extensionUri,
      initialState
    );

    webviewView.onDidChangeVisibility(
      () => {
        if (webviewView.visible) {
          void this.refresh();
        }
      },
      null,
      this._disposables
    );

    webviewView.webview.onDidReceiveMessage(
      (message) => void this.handleWebviewMessage(message),
      null,
      this._disposables
    );

    this._disposables.push(
      this.bridge.onEvent((event) => {
        this._handleTimelineEvent(event);
      })
    );
  }

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
    reasoningEffort?: 'low' | 'medium' | 'high';
    permissionMode?: 'read-only' | 'auto-edit' | 'full-access';
    followUpQueueMode?: 'queue' | 'steer';
  }): Promise<void> {
    if (!message || !message.command) {
      return;
    }

    const validation = ChatSecurityGuards.validateWebviewMessage(message);
    if (!validation.valid) {
      await this._appendSystemMessage(`Request rejected: ${validation.error}`);
      return;
    }

    if (!ChatSecurityGuards.verifyWorkspaceTrust(message.command)) {
      await this._appendSystemMessage(
        'This workspace is untrusted. Trust the folder before running editing or terminal tasks.'
      );
      return;
    }

    switch (message.command) {
      case 'sendPrompt':
      case 'sendMessage':
        await this._handleSendPrompt(message);
        break;

      case 'submitTask':
        await this._handleSendPrompt({
          ...message,
          text: message.prompt || message.text,
          mode: 'agent',
        });
        break;

      case 'cancelTask':
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
      case 'viewDiff':
        await this._handleOpenDiff(message.artifactId, message.filePath);
        break;

      case 'applyDiff':
        await this._handleOpenDiff(message.artifactId, message.filePath);
        break;

      case 'revertDiff':
        await vscode.commands.executeCommand('agent-studios.revertCheckpoint');
        break;

      case 'viewTaskDiff':
      case 'openArtifactReview':
        await vscode.commands.executeCommand('agent-studios.openArtifactReview', {
          studioId: message.studioId,
          taskId: message.taskId,
        });
        break;

      case 'attachContext':
        await this._handleAttachContext(message.source);
        break;

      case 'removeAttachment':
        if (message.path) {
          this._attachments = this._attachments.filter(
            (a) => (a.uri || a.relativePath) !== message.path
          );
          await this.refresh();
        }
        break;

      case 'switchMode':
        if (message.mode) {
          this._activeMode = message.mode;
          await this.refresh();
        }
        break;

      case 'switchProviderModel':
        this._selectedModelId = message.modelId;
        this._selectedProviderId = message.providerId;
        if (this._extensionContext && this._selectedProviderId && this._selectedModelId) {
          await this._extensionContext.globalState.update(AGENT_STUDIOS_PROVIDER_MODEL_KEY, {
            instanceId: this._selectedProviderId,
            modelId: this._selectedModelId,
          });
        }
        await this.refresh();
        break;

      case 'switchRuntime':
        if (message.runtime) {
          this._selectedRuntime = message.runtime;
        }
        break;

      case 'switchSession':
      case 'switchConversation':
        if (message.sessionId) {
          await this._handleSwitchSession(message.sessionId);
        }
        break;

      case 'newSession':
      case 'newChat':
        await this._handleNewSession();
        break;

      case 'openSettings':
        await vscode.commands.executeCommand('agent-studios.openSettings');
        break;

      case 'openFolder':
        await vscode.commands.executeCommand('vscode.openFolder');
        break;

      case 'openFile':
        if (message.filePath) {
          await vscode.window.showTextDocument(vscode.Uri.file(message.filePath));
        }
        break;

      case 'copyCode':
        if (typeof message.text === 'string' && vscode.env?.clipboard) {
          await vscode.env.clipboard.writeText(message.text);
        }
        break;

      case 'openTaskGraph':
        await vscode.commands.executeCommand('agent-studios.showTaskGraph');
        break;

      case 'saveDraft':
        this._composerDraft = message.draft ?? '';
        break;

      case 'connectDaemon':
      case 'retryConnection':
        await vscode.commands.executeCommand('agent-studios.connectDaemon');
        await this.refresh();
        break;

      case 'openInIde':
        await vscode.commands.executeCommand('agent-studios.openInIde');
        break;

      case 'openMissionControl':
        await vscode.commands.executeCommand('agent-studios.openMissionControl');
        break;

      case 'continueSession':
        await this._handleContinueSession(message.sessionId);
        break;

      case 'switchReasoningEffort':
        if (message.reasoningEffort) {
          this._reasoningEffort = message.reasoningEffort;
        }
        break;

      case 'switchPermissionMode':
        if (message.permissionMode) {
          this._permissionMode = message.permissionMode;
        }
        break;

      case 'switchFollowUpMode':
        if (message.followUpQueueMode) {
          this._followUpQueueMode = message.followUpQueueMode;
        }
        break;

      case 'forkSession':
        await this._handleForkSession(message.sessionId);
        break;

      case 'dismissOnboarding':
        this._hasSeenOnboarding = true;
        break;
    }
  }

  private async _mergeConfiguredProviders(): Promise<ProviderSummary[]> {
    const bridgeProviders = await this.bridge.listProviderSummaries().catch(() => []);
    const merged: ProviderSummary[] = [...bridgeProviders];
    if (this._extensionContext) {
      const profiles = new ProviderProfileManager(this._extensionContext).listProfiles();
      for (const p of profiles) {
        if (!merged.some((existing) => existing.instanceId === p.instanceId)) {
          const models =
            p.discoveredModels && p.discoveredModels.length > 0
              ? p.discoveredModels
              : p.defaultModel
                ? [{ id: p.defaultModel, name: p.defaultModel, contextWindow: 128000, isDefault: true }]
                : [];
          merged.push({
            instanceId: p.instanceId,
            providerType: p.providerType,
            displayName: p.displayName,
            models,
            status: models.length > 0 ? 'available' : 'unconfigured',
          });
        }
      }
    }
    return merged;
  }

  private async _collectState(): Promise<ChatHtmlInitialState> {
    const rawSession = await this.bridge.getCurrentSession().catch(() => null);
    const session = isKnownTestOrFixtureRecord(rawSession) ? null : rawSession;
    const rawSessions = await this.bridge.listSessions().catch(() => []);
    const sessions: SessionSummary[] = rawSessions.filter((s) => !isKnownTestOrFixtureRecord(s));
    const providers = await this._mergeConfiguredProviders();
    const localWorkspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '';

    if (
      session &&
      this._messages.length === 0 &&
      typeof this.bridge.getSessionTranscript === 'function'
    ) {
      this._messages = await this.bridge.getSessionTranscript(session.id).catch(() => []);
    }

    const readiness =
      typeof this.bridge.getExecutionReadiness === 'function'
        ? await this.bridge.getExecutionReadiness().catch(() => null)
        : null;

    const { runtimeOptions, selectedRuntime } =
      ChatWebviewViewProvider.buildRuntimeOptions(readiness);

    const selectedProviderId =
      this._selectedProviderId || providers[0]?.instanceId || '';
    const activeProvider = providers.find((p) => p.instanceId === selectedProviderId) || providers[0];
    const selectedModelId =
      this._selectedModelId ||
      activeProvider?.models.find((m) => m.isDefault)?.id ||
      activeProvider?.models[0]?.id ||
      '';

    const compatibleRuntimes = filterCompatibleRuntimes(
      runtimeOptions,
      activeProvider?.providerType,
      selectedModelId
    );
    const resolvedRuntime = pickDefaultRuntime(
      compatibleRuntimes,
      this._selectedRuntime || selectedRuntime,
      activeProvider?.providerType,
      selectedModelId
    );

    const wsVerification = WorkspaceVerification.verify(localWorkspace, readiness);
    const tasks = await this.bridge.listTasks().catch(() => []);
    const pendingApprovals = await this.bridge.listPendingApprovals().catch(() => []);

    return {
      session,
      sessions,
      connectionState: this.bridge.connectionState,
      providers,
      activeMode: this._activeMode,
      selectedModelId,
      selectedProviderId,
      selectedRuntime: resolvedRuntime,
      selectedStudioId: session?.id || '',
      reasoningEffort: this._reasoningEffort,
      permissionMode: this._permissionMode,
      followUpQueueMode: this._followUpQueueMode,
      runtimes: compatibleRuntimes,
      workspacePath: localWorkspace,
      workspaceStatus: wsVerification.status,
      isWorkspaceTrusted: vscode.workspace.isTrusted !== false,
      isWorkspaceMatched: wsVerification.isAuthorized,
      workspaceMismatchReason: wsVerification.isAuthorized ? '' : wsVerification.reason,
      isTaskSubmitEnabled: CapabilityMatrix.isCapabilityEnabled('task.submit'),
      composerDraft: this._composerDraft,
      messages: this._messages,
      attachments: this._attachments,
      tasks,
      pendingApprovals,
      hasSeenOnboarding: this._hasSeenOnboarding,
      executionState: TaskExecutionStore.instance.getState(),
    };
  }

  private async _appendSystemMessage(content: string, actionId?: string): Promise<void> {
    this._messages.push({
      id: `msg-sys-${Date.now()}`,
      role: 'system',
      content,
      timestamp: Date.now(),
      approvalId: actionId,
    });
    await this._postRenderedThread();
  }

  private async _postRenderedThread(): Promise<void> {
    const tasks = await this.bridge.listTasks().catch(() => []);
    const approvals = await this.bridge.listPendingApprovals().catch(() => []);
    const html = ChatHtmlBuilder.renderMessagesHtml(this._messages, tasks, approvals);
    await this.postMessage({ type: 'renderedThread', html });
  }

  private async _handleSendPrompt(message: {
    text?: string;
    prompt?: string;
    mode?: AgentMode;
    attachments?: ContextAttachment[];
    modelId?: string;
    providerId?: string;
    runtime?: string;
    studioId?: string;
    reasoningEffort?: 'low' | 'medium' | 'high';
    permissionMode?: 'read-only' | 'auto-edit' | 'full-access';
    followUpQueueMode?: 'queue' | 'steer';
  }): Promise<void> {
    const prompt = (message.text ?? message.prompt ?? '').trim();
    if (!prompt) {
      return;
    }

    const mode: AgentMode = message.mode || this._activeMode;
    this._activeMode = mode;

    // Attempt quiet auto-connect once if currently offline
    if (this.bridge.connectionState === 'disconnected' && this._bridgeManager) {
      try {
        await this._bridgeManager.connectDaemon(undefined, {
          extensionPath: this._extensionUri.fsPath,
          workspacePath: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
          autoStart: true,
        });
        this._bridge = this._bridgeManager.bridge;
      } catch {
        // Handled cleanly below via resolveSendReadiness
      }
    }

    const providers = await this._mergeConfiguredProviders();
    const localWorkspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '';
    const uiConn = resolveUiConnectionStatus({
      connectionState: this.bridge.connectionState,
      providers,
      isRunning: this._isRunning,
    });

    const readiness = resolveSendReadiness({
      uiStatus: uiConn.status,
      mode,
      hasProviders: providers.some((p) => p.status !== 'unconfigured'),
      hasWorkspace: Boolean(localWorkspace),
      isWorkspaceTrusted: vscode.workspace.isTrusted !== false,
      isRunning: this._isRunning,
    });

    if (!readiness.canSendImmediately && readiness.message) {
      await this._appendSystemMessage(readiness.message, readiness.action?.id);
      return;
    }

    const reasoningEffort = message.reasoningEffort || this._reasoningEffort;
    const permissionMode =
      mode === 'ask' || mode === 'plan'
        ? 'read-only'
        : message.permissionMode || this._permissionMode;
    const followUpQueueMode = message.followUpQueueMode || this._followUpQueueMode;
    const attachments: ContextAttachment[] = [...(message.attachments || this._attachments)];

    if (this._contextEngine) {
      const repoMap = await this._contextEngine.collectWorkspaceRepoMap(15);
      const packed = this._contextEngine.packWithinBudget(
        this._contextEngine.collectActiveEditorItems(prompt),
        4096,
        repoMap
      );
      const seenUris = new Set(attachments.map((a) => a.uri || a.relativePath || ''));
      for (const att of packed.attachments) {
        const key = att.uri || att.relativePath || '';
        if (key && !seenUris.has(key)) {
          attachments.push(att);
          seenUris.add(key);
        }
      }
    }

    // Ensure active studio/session exists before Agent mode mutating runs
    let studioId = message.studioId || this._stateProjection?.session?.id;
    if (mode === 'agent' && !studioId) {
      try {
        const created = await this.bridge.createSession(prompt.slice(0, 60));
        studioId = created.id;
        this._stateProjection?.switchSession(created);
      } catch {
        // Continue cleanly to sendMessage fallback
      }
    }

    // High-risk command confirmation gate when full-access is explicitly requested
    let confirmedByUser = false;
    if (mode === 'agent' && permissionMode === 'full-access' && studioId) {
      const confirmResult = await OperatorConfirmationModal.requestConfirmation({
        studioId,
        runtime: message.runtime || this._selectedRuntime || 'internal-codex',
        runtimeInstanceId: message.runtime || 'internal-codex',
        prompt,
        workspacePath: localWorkspace || undefined,
        modelId: message.modelId || this._selectedModelId,
        providerId: message.providerId || this._selectedProviderId,
        actionDescription: 'Full-Access Agent Task Execution',
      });
      if (!confirmResult.confirmed) {
        await this._appendSystemMessage('Action cancelled before running.');
        return;
      }
      confirmedByUser = true;
    }

    const request: ChatSendRequest = {
      message: prompt,
      mode,
      studioId,
      modelId: message.modelId || this._selectedModelId || providers[0]?.models[0]?.id,
      providerInstanceId: message.providerId || this._selectedProviderId || providers[0]?.instanceId,
      reasoningEffort,
      permissionMode,
      followUpQueueMode,
      runtime: message.runtime || this._selectedRuntime,
      workspacePath: localWorkspace || undefined,
      attachments,
      confirmedByUser,
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
    this._isRunning = true;
    await this._postRenderedThread();
    await this.postMessage({ type: 'runState', isRunning: true });

    try {
      const result = await this.bridge.sendMessage(request, (chunk: ChatStreamChunk) => {
        void this.postMessage({ type: 'streamChunk', chunk });
      });

      this._messages.push(result);
      this._isRunning = false;
      await this.postMessage({ type: 'runState', isRunning: false });
      await this._postRenderedThread();
    } catch (err: unknown) {
      this._isRunning = false;
      await this.postMessage({ type: 'runState', isRunning: false });
      await this._appendSystemMessage(err instanceof Error ? err.message : String(err));
    }
  }

  private async _handleCancelRun(): Promise<void> {
    try {
      await this.bridge.cancelCurrentRun();
      this._isRunning = false;
      await this.postMessage({ type: 'runState', isRunning: false });
      await this.postMessage({ type: 'runCancelled' });
    } catch (err: unknown) {
      await this._appendSystemMessage(err instanceof Error ? err.message : String(err));
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
        await this._appendSystemMessage(err instanceof Error ? err.message : String(err));
      }
    }
    await this._postRenderedThread();
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
        await this._appendSystemMessage(err instanceof Error ? err.message : String(err));
      }
    }
    await this._postRenderedThread();
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
        await vscode.window.showTextDocument(vscode.Uri.file(filePath));
      }
    }
  }

  private async _handleAttachContext(source?: 'filePicker' | 'activeEditor'): Promise<void> {
    if (source === 'activeEditor') {
      const editor = vscode.window.activeTextEditor;
      if (editor) {
        const filePath = editor.document.uri.fsPath || editor.document.uri.path;
        const relativePath = vscode.workspace.asRelativePath(editor.document.uri) || filePath;
        this._attachments.push({
          type: 'file',
          uri: editor.document.uri.toString(),
          relativePath,
          languageId: editor.document.languageId,
          contentSnippet:
            editor.selection && !editor.selection.isEmpty
              ? editor.document.getText(editor.selection)
              : undefined,
        });
        await this.refresh();
      } else {
        await vscode.window.showInformationMessage('No active editor open to attach.');
      }
      return;
    }

    const uris = await vscode.window.showOpenDialog({
      canSelectMany: true,
      openLabel: 'Attach to Chat',
    });
    if (uris && uris.length > 0) {
      for (const u of uris) {
        const filePath = u.fsPath || u.path;
        const relativePath = vscode.workspace.asRelativePath(u) || filePath;
        this._attachments.push({
          type: 'file',
          uri: u.toString(),
          relativePath,
        });
      }
      await this.refresh();
    }
  }

  private async _handleContinueSession(targetSessionId?: string): Promise<void> {
    if (targetSessionId) {
      await this._handleSwitchSession(targetSessionId);
      return;
    }
    const sessions = (await this.bridge.listSessions().catch(() => [])).filter(
      (s) => !isKnownTestOrFixtureRecord(s)
    );
    if (sessions.length > 0) {
      const sorted = [...sessions].sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
      await this._handleSwitchSession(sorted[0].id);
    }
  }

  private async _handleSwitchSession(sessionId: string): Promise<void> {
    try {
      const session = await this.bridge.switchSession(sessionId);
      this._stateProjection?.switchSession(session);
      const transcript =
        typeof this.bridge.getSessionTranscript === 'function'
          ? await this.bridge.getSessionTranscript(session.id).catch(() => [])
          : [];
      this._messages = transcript;
      await this.refresh();
    } catch (err: unknown) {
      await this._appendSystemMessage(
        `Failed to switch conversation: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  private async _handleNewSession(): Promise<void> {
    try {
      if (this.bridge.connectionState === 'connected') {
        const session = await this.bridge.createSession();
        this._stateProjection?.switchSession(session);
      }
      this._messages = [];
      this._attachments = [];
      this._composerDraft = '';
      await this.refresh();
    } catch (err: unknown) {
      await this._appendSystemMessage(
        `Failed to create conversation: ${err instanceof Error ? err.message : String(err)}`
      );
    }
  }

  private async _handleForkSession(sourceSessionId?: string): Promise<void> {
    const current = sourceSessionId || this._stateProjection?.session?.id;
    let forkedSession =
      current && this.bridge.forkSession
        ? await this.bridge.forkSession(current).catch(() => undefined)
        : undefined;
    if (!forkedSession && this._conversationManager && current) {
      const forkedRec = await this._conversationManager.forkConversation(current);
      if (forkedRec) {
        forkedSession = {
          id: forkedRec.id,
          workspacePath: this._stateProjection?.session?.workspacePath || '',
          goalSummary: forkedRec.title,
          status: forkedRec.status,
          activeAgentCount: 0,
          totalAgentCount: 0,
          completedTasksCount: 0,
          totalTasksCount: 0,
          startedAt: forkedRec.createdAt,
          elapsedMs: 0,
        };
      }
    }
    if (forkedSession) {
      this._stateProjection?.switchSession(forkedSession);
    }
    await this.refresh();
  }

  private _handleTimelineEvent(event: TimelineEvent): void {
    TaskExecutionStore.instance.consumeDaemonTimelineEvent(event);
    if (event.type === 'approval_requested') {
      const approval = event.details as unknown as ApprovalRequest;
      if (approval) {
        void this.postMessage({
          type: 'streamChunk',
          chunk: { type: 'approval_required', approval },
        });
      }
    } else if (event.type === 'task_started' || event.type === 'task_completed') {
      void this.postMessage({
        type: 'streamChunk',
        chunk: { type: 'task_progress', taskSummary: event.summary },
      });
    }
  }

  public async syncState(): Promise<void> {
    await this.refresh();
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
      this._disposables.pop()?.dispose();
    }
    this._view = undefined;
  }
}
