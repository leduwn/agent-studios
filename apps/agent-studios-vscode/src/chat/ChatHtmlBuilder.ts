/**
 * Agent Studios — Clean Conversation-First Chat HTML Builder
 * Focused 4-region layout:
 * 1. Compact header (+, History, Settings, Mission Control)
 * 2. 1-line workspace & 9-state connection strip
 * 3. Scrollable conversation stream (Markdown + Copy + File links + Grouped tools + Diffs + Approvals + Progressive multi-agent card)
 * 4. Compact docked composer (Ask/Plan/Agent, Model, Reasoning, Advanced popover, Send/Stop)
 */

import * as crypto from 'crypto';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  AgentMode,
  ApprovalRequest,
  ChatMessage,
  ConnectionState,
  ContextAttachment,
  ProviderSummary,
  SessionInfo,
  SessionSummary,
  TaskNode,
} from '../bridge/types';
import { WorkspaceAuthorizationStatus } from '../security/WorkspaceVerification';
import { TaskExecutionState } from '../state/TaskExecutionStore';
import { escapeHtml, renderSafeMarkdown } from '../webviews/markdownRenderer';
import {
  filterCompatibleRuntimes,
  pickDefaultRuntime,
  resolveSendReadiness,
  resolveUiConnectionStatus,
  RuntimeDescriptor,
} from './ChatPresenter';
import { OnboardingWorkflow } from './OnboardingWorkflow';
import { CHAT_STYLES } from './styles.css';

export type RuntimeOption = RuntimeDescriptor;

export interface StudioOption {
  id: string;
  name: string;
}

export interface ChatHtmlInitialState {
  session?: SessionInfo | null;
  sessions?: SessionSummary[];
  connectionState?: ConnectionState;
  providers?: ProviderSummary[];
  activeMode?: AgentMode;
  selectedModelId?: string;
  selectedProviderId?: string;
  selectedRuntime?: string;
  selectedStudioId?: string;
  reasoningEffort?: 'low' | 'medium' | 'high';
  permissionMode?: 'read-only' | 'auto-edit' | 'full-access';
  followUpQueueMode?: 'queue' | 'steer';
  runtimes?: RuntimeOption[];
  studios?: StudioOption[];
  workspacePath?: string;
  workspaceStatus?: WorkspaceAuthorizationStatus;
  isWorkspaceTrusted?: boolean;
  isWorkspaceMatched?: boolean;
  workspaceMismatchReason?: string;
  isTaskSubmitEnabled?: boolean;
  composerDraft?: string;
  messages?: ChatMessage[];
  attachments?: ContextAttachment[];
  tasks?: TaskNode[];
  pendingApprovals?: ApprovalRequest[];
  hasSeenOnboarding?: boolean;
  isStartingEngine?: boolean;
  lastError?: string | null;
  executionState?: TaskExecutionState;
}

export class ChatHtmlBuilder {
  public static renderEmptyState(initialState?: ChatHtmlInitialState): string {
    const connectionState: ConnectionState = initialState?.connectionState ?? 'disconnected';
    const providers = initialState?.providers ?? [];
    const workspacePath = initialState?.workspacePath || initialState?.session?.workspacePath || '';
    const hasProviders = OnboardingWorkflow.hasConfiguredProviders(providers);
    const isConnected = connectionState === 'connected';

    const onboardingHtml = OnboardingWorkflow.renderBanner({
      isConnected,
      connectionState,
      hasProviders,
      hasWorkspace: Boolean(workspacePath),
      hasDismissed: Boolean(initialState?.hasSeenOnboarding),
    });

    const starters = [
      'Explain how this workspace is structured and where core logic lives.',
      'Review recent changes and identify edge cases or missing tests.',
      'Implement the requested fix cleanly and run verification tests.',
    ];

    const startersHtml =
      isConnected && hasProviders
        ? `
          <div class="starter-list" aria-label="Suggested prompts">
            ${starters
              .map(
                (prompt) =>
                  `<button type="button" class="starter-btn" data-starter="${escapeHtml(prompt)}">${escapeHtml(prompt)}</button>`
              )
              .join('')}
          </div>
        `
        : '';

    return `
      <div class="empty-state">
        <div>
          <h2 class="empty-headline">What would you like to work on?</h2>
          <p class="empty-subtext">Ask questions, plan changes, or let Agent mode edit and test your workspace.</p>
        </div>
        ${onboardingHtml}
        ${startersHtml}
      </div>
    `;
  }

  public static buildHtml(
    webview: vscode.Webview,
    _extensionUri: vscode.Uri,
    initialState?: ChatHtmlInitialState
  ): string {
    const nonce = crypto.randomBytes(16).toString('base64');
    const session = initialState?.session ?? null;
    const sessions = initialState?.sessions ?? [];
    const connectionState: ConnectionState = initialState?.connectionState ?? 'disconnected';
    const providers = initialState?.providers ?? [];
    const activeMode: AgentMode = initialState?.activeMode ?? 'ask';
    const reasoningEffort = initialState?.reasoningEffort ?? 'medium';
    const permissionMode = initialState?.permissionMode ?? 'auto-edit';
    const followUpQueueMode = initialState?.followUpQueueMode ?? 'queue';
    const selectedProviderId = initialState?.selectedProviderId ?? providers[0]?.instanceId ?? '';
    const selectedModelId =
      initialState?.selectedModelId ??
      providers.find((p) => p.instanceId === selectedProviderId)?.models[0]?.id ??
      providers[0]?.models[0]?.id ??
      '';
    const composerDraft = initialState?.composerDraft ?? '';
    const messages = initialState?.messages ?? [];
    const attachments = initialState?.attachments ?? [];
    const tasks = initialState?.tasks ?? [];
    const pendingApprovals = initialState?.pendingApprovals ?? [];
    const workspacePath = initialState?.workspacePath || session?.workspacePath || '';
    const isRunning =
      initialState?.executionState?.status === 'submitting' ||
      initialState?.executionState?.status === 'queued' ||
      initialState?.executionState?.status === 'running';

    const selectedProvider = providers.find((p) => p.instanceId === selectedProviderId);
    const compatibleRuntimes = filterCompatibleRuntimes(
      initialState?.runtimes ?? [],
      selectedProvider?.providerType,
      selectedModelId
    );
    const selectedRuntime = pickDefaultRuntime(
      compatibleRuntimes,
      initialState?.selectedRuntime,
      selectedProvider?.providerType,
      selectedModelId
    );

    const uiConn = resolveUiConnectionStatus({
      connectionState,
      providers,
      isRunning,
      pendingApprovalCount: pendingApprovals.length,
      isStartingEngine: initialState?.isStartingEngine,
      lastError: initialState?.lastError,
    });

    const readiness = resolveSendReadiness({
      uiStatus: uiConn.status,
      mode: activeMode,
      hasProviders: OnboardingWorkflow.hasConfiguredProviders(providers),
      hasWorkspace: Boolean(workspacePath),
      isWorkspaceTrusted: initialState?.isWorkspaceTrusted !== false,
      workspaceStatus: initialState?.workspaceStatus,
      isRunning,
    });

    const workspaceLabel = workspacePath
      ? path.basename(workspacePath) || workspacePath
      : 'No folder open';

    const historyItemsHtml =
      sessions.length > 0
        ? sessions
            .map((s) => {
              const isActive = s.id === session?.id;
              return `<button type="button" class="history-item ${isActive ? 'active' : ''}" data-session-id="${escapeHtml(s.id)}">
                <span class="history-item-title">${escapeHtml(s.title || 'Untitled conversation')}</span>
              </button>`;
            })
            .join('')
        : '<div class="empty-subtext">No previous conversations yet.</div>';

    const modelOptionsHtml = ChatHtmlBuilder.renderModelOptions(
      providers,
      selectedProviderId,
      selectedModelId
    );

    const runtimeOptionsHtml = compatibleRuntimes
      .map(
        (rt) =>
          `<option value="${escapeHtml(rt.id)}" ${rt.id === selectedRuntime ? 'selected' : ''} ${rt.enabled ? '' : 'disabled'}>${escapeHtml(rt.name)}</option>`
      )
      .join('');

    const attachmentsHtml = attachments
      .map(
        (att) => `
          <span class="attachment-chip">
            <span>${escapeHtml(att.relativePath)}</span>
            <button type="button" class="link-btn" data-remove-attachment="${escapeHtml(att.uri || att.relativePath)}" aria-label="Remove attachment">✕</button>
          </span>
        `
      )
      .join('');

    const messagesHtml =
      messages.length > 0
        ? ChatHtmlBuilder.renderMessagesHtml(messages, tasks, pendingApprovals)
        : ChatHtmlBuilder.renderEmptyState(initialState);

    const bannerHidden = readiness.canSendImmediately || readiness.blockingReason === 'running';
    const bannerActionHtml = readiness.action
      ? `<button type="button" class="btn-primary btn-sm" data-ui-action="${escapeHtml(readiness.action.id)}">${escapeHtml(readiness.action.label)}</button>`
      : '';

    const statusActionHtml = uiConn.action
      ? `<button type="button" class="link-btn" data-ui-action="${escapeHtml(uiConn.action.id)}">${escapeHtml(uiConn.action.label)}</button>`
      : '';

    const clientBootstrap = JSON.stringify({
      activeMode,
      selectedModelId,
      selectedProviderId,
      selectedRuntime,
      reasoningEffort,
      permissionMode,
      followUpQueueMode,
      uiStatus: uiConn.status,
      isRunning,
      providers: providers.map((p) => ({
        instanceId: p.instanceId,
        providerType: p.providerType,
        displayName: p.displayName,
        status: p.status,
        models: p.models.map((m) => ({ id: m.id, name: m.name })),
      })),
    }).replace(/</g, '\\u003c');

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios</title>
  <style>${CHAT_STYLES}</style>
</head>
<body>
  <div class="chat-container">
    <header class="chat-header" role="banner">
      <div class="header-brand">
        <span class="header-title">Agent Studios</span>
      </div>
      <div class="header-actions">
        <button id="newSessionBtn" type="button" class="icon-btn" title="New Chat" aria-label="New Chat">+</button>
        <button id="historyToggleBtn" type="button" class="icon-btn" title="History" aria-label="Toggle History" aria-expanded="false">◷</button>
        <button id="openSettingsBtn" type="button" class="icon-btn" title="Providers & Models Settings" aria-label="Providers & Models Settings">⚙</button>
        <button id="openMissionControlBtn" type="button" class="icon-btn" title="Open Mission Control" aria-label="Open Mission Control">⊞</button>
      </div>
    </header>

    <div id="historyDrawer" class="history-drawer" hidden aria-label="Conversation History">
      <div class="history-drawer-title">Recent Conversations</div>
      <div id="historyList">${historyItemsHtml}</div>
    </div>

    <div class="context-strip">
      <div class="context-workspace" title="${escapeHtml(workspacePath || 'No folder open')}">
        <span class="workspace-name">${escapeHtml(workspaceLabel)}</span>
        ${!workspacePath ? '<button type="button" class="link-btn" data-ui-action="open_folder">Open</button>' : ''}
      </div>
      <div id="connectionStatusIndicator" class="context-status tone-${uiConn.tone}" title="${escapeHtml(uiConn.detail)}">
        <span class="status-dot" aria-hidden="true"></span>
        <span id="connectionStatusText">${escapeHtml(uiConn.label)}</span>
        ${statusActionHtml}
      </div>
    </div>

    <main id="conversationContainer" class="chat-messages" role="log" aria-live="polite">
      <div id="threadInner" class="chat-thread-inner">
        ${messagesHtml}
      </div>
    </main>

    <footer class="chat-composer" role="region" aria-label="Message Composer">
      <div class="composer-inner">
        <div id="advancedPopover" class="advanced-popover" hidden aria-label="Advanced Options">
          <div class="advanced-field">
            <label for="permissionModeSelect">Permissions</label>
            <select id="permissionModeSelect" class="compact-select">
              <option value="read-only" ${permissionMode === 'read-only' ? 'selected' : ''}>Read-only</option>
              <option value="auto-edit" ${permissionMode === 'auto-edit' ? 'selected' : ''}>Workspace Write</option>
              <option value="full-access" ${permissionMode === 'full-access' ? 'selected' : ''}>Ask Before Commands</option>
            </select>
          </div>
          <div class="advanced-field">
            <label for="runtimeSelect">Engine Runtime</label>
            <select id="runtimeSelect" class="compact-select">
              ${runtimeOptionsHtml}
            </select>
          </div>
          <div class="advanced-field">
            <label for="followUpModeSelect">Follow-up Messages</label>
            <select id="followUpModeSelect" class="compact-select">
              <option value="queue" ${followUpQueueMode === 'queue' ? 'selected' : ''}>Queue</option>
              <option value="steer" ${followUpQueueMode === 'steer' ? 'selected' : ''}>Steer Current</option>
            </select>
          </div>
          <div class="advanced-field-full">
            <button id="attachFileBtn" type="button" class="btn-ghost btn-sm">+ Attach File</button>
            <button id="attachEditorBtn" type="button" class="btn-ghost btn-sm">+ Active Selection</button>
          </div>
        </div>

        <div id="composerBanner" class="composer-banner" ${bannerHidden ? 'hidden' : ''}>
          <span id="composerBannerText">${escapeHtml(readiness.message || '')}</span>
          <span id="composerBannerAction">${bannerActionHtml}</span>
        </div>

        <div id="attachmentChips" class="attachment-chips">${attachmentsHtml}</div>

        <div class="composer-box">
          <textarea
            id="messageInput"
            rows="2"
            placeholder="Ask a question, plan changes, or describe a task…"
            aria-label="Prompt input"
          >${escapeHtml(composerDraft)}</textarea>

          <div class="composer-controls">
            <div class="composer-selects">
              <select id="modeSelect" class="compact-select" aria-label="Mode">
                <option value="ask" ${activeMode === 'ask' ? 'selected' : ''}>Ask</option>
                <option value="plan" ${activeMode === 'plan' ? 'selected' : ''}>Plan</option>
                <option value="agent" ${activeMode === 'agent' ? 'selected' : ''}>Agent</option>
              </select>

              <select id="modelSelect" class="compact-select model-select" aria-label="Model">
                ${modelOptionsHtml}
              </select>

              <select id="reasoningEffortSelect" class="compact-select" aria-label="Reasoning effort">
                <option value="low" ${reasoningEffort === 'low' ? 'selected' : ''}>Low</option>
                <option value="medium" ${reasoningEffort === 'medium' ? 'selected' : ''}>Medium</option>
                <option value="high" ${reasoningEffort === 'high' ? 'selected' : ''}>High</option>
              </select>
            </div>

            <div class="composer-actions">
              <button id="advancedToggleBtn" type="button" class="icon-btn" title="More options" aria-label="More options" aria-expanded="false">⋯</button>
              <button id="sendBtn" type="button" class="send-btn ${isRunning ? 'is-stop' : ''}" title="${isRunning ? 'Stop generation' : 'Send message'}" aria-label="${isRunning ? 'Stop generation' : 'Send message'}">
                ${isRunning ? '■' : '↑'}
              </button>
            </div>
          </div>
        </div>
      </div>
    </footer>
  </div>

  <script nonce="${nonce}">
    (function() {
      const vscodeApi = typeof acquireVsCodeApi === 'function' ? acquireVsCodeApi() : null;
      const state = ${clientBootstrap};
      const input = document.getElementById('messageInput');
      const sendBtn = document.getElementById('sendBtn');
      const modeSelect = document.getElementById('modeSelect');
      const modelSelect = document.getElementById('modelSelect');
      const reasoningSelect = document.getElementById('reasoningEffortSelect');
      const permissionSelect = document.getElementById('permissionModeSelect');
      const runtimeSelect = document.getElementById('runtimeSelect');
      const followUpSelect = document.getElementById('followUpModeSelect');
      const historyDrawer = document.getElementById('historyDrawer');
      const advancedPopover = document.getElementById('advancedPopover');
      const conversationContainer = document.getElementById('conversationContainer');
      const threadInner = document.getElementById('threadInner');

      function post(msg) {
        if (vscodeApi) { vscodeApi.postMessage(msg); }
      }

      function scrollToBottom() {
        if (conversationContainer) {
          conversationContainer.scrollTop = conversationContainer.scrollHeight;
        }
      }

      function autoResizeInput() {
        if (!input) return;
        input.style.height = 'auto';
        input.style.height = Math.min(160, Math.max(36, input.scrollHeight)) + 'px';
      }

      function dispatchUiAction(actionId) {
        switch (actionId) {
          case 'configure_provider':
          case 'openSettings':
            post({ command: 'openSettings' });
            break;
          case 'connect':
          case 'connectDaemon':
            post({ command: 'connectDaemon' });
            break;
          case 'retry':
            post({ command: 'retryConnection' });
            break;
          case 'open_folder':
          case 'openFolder':
            post({ command: 'openFolder' });
            break;
          case 'review_approval':
          case 'open_mission_control':
            post({ command: 'openMissionControl' });
            break;
        }
      }

      function sendCurrentPrompt() {
        if (state.isRunning) {
          post({ command: 'cancelRun' });
          return;
        }
        const text = (input?.value || '').trim();
        if (!text) return;
        post({
          command: 'sendPrompt',
          text,
          mode: state.activeMode,
          modelId: state.selectedModelId,
          providerId: state.selectedProviderId,
          reasoningEffort: state.reasoningEffort,
          permissionMode: state.permissionMode,
          followUpQueueMode: state.followUpQueueMode,
          runtime: state.selectedRuntime,
        });
        if (input) {
          input.value = '';
          autoResizeInput();
        }
      }

      document.getElementById('newSessionBtn')?.addEventListener('click', () => post({ command: 'newChat' }));
      document.getElementById('openSettingsBtn')?.addEventListener('click', () => post({ command: 'openSettings' }));
      document.getElementById('openMissionControlBtn')?.addEventListener('click', () => post({ command: 'openMissionControl' }));

      document.getElementById('historyToggleBtn')?.addEventListener('click', (e) => {
        if (!historyDrawer) return;
        const willOpen = historyDrawer.hasAttribute('hidden');
        if (willOpen) historyDrawer.removeAttribute('hidden');
        else historyDrawer.setAttribute('hidden', '');
        e.currentTarget.setAttribute('aria-expanded', String(willOpen));
      });

      document.getElementById('advancedToggleBtn')?.addEventListener('click', (e) => {
        if (!advancedPopover) return;
        const willOpen = advancedPopover.hasAttribute('hidden');
        if (willOpen) advancedPopover.removeAttribute('hidden');
        else advancedPopover.setAttribute('hidden', '');
        e.currentTarget.setAttribute('aria-expanded', String(willOpen));
      });

      document.getElementById('attachFileBtn')?.addEventListener('click', () => post({ command: 'attachContext', source: 'filePicker' }));
      document.getElementById('attachEditorBtn')?.addEventListener('click', () => post({ command: 'attachContext', source: 'activeEditor' }));

      modeSelect?.addEventListener('change', (e) => {
        state.activeMode = e.target.value;
        post({ command: 'switchMode', mode: state.activeMode });
      });

      modelSelect?.addEventListener('change', (e) => {
        const val = e.target.value;
        if (val === '__manage_providers__') {
          post({ command: 'openSettings' });
          e.target.value = state.selectedProviderId && state.selectedModelId ? state.selectedProviderId + '::' + state.selectedModelId : '';
          return;
        }
        const parts = val.split('::');
        if (parts.length === 2) {
          state.selectedProviderId = parts[0];
          state.selectedModelId = parts[1];
          post({ command: 'switchProviderModel', providerId: parts[0], modelId: parts[1] });
        }
      });

      reasoningSelect?.addEventListener('change', (e) => {
        state.reasoningEffort = e.target.value;
        post({ command: 'switchReasoningEffort', reasoningEffort: state.reasoningEffort });
      });

      permissionSelect?.addEventListener('change', (e) => {
        state.permissionMode = e.target.value;
        post({ command: 'switchPermissionMode', permissionMode: state.permissionMode });
      });

      runtimeSelect?.addEventListener('change', (e) => {
        state.selectedRuntime = e.target.value;
        post({ command: 'switchRuntime', runtime: state.selectedRuntime });
      });

      followUpSelect?.addEventListener('change', (e) => {
        state.followUpQueueMode = e.target.value;
        post({ command: 'switchFollowUpMode', followUpQueueMode: state.followUpQueueMode });
      });

      input?.addEventListener('input', () => {
        autoResizeInput();
        post({ command: 'saveDraft', draft: input.value });
      });

      input?.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && state.isRunning) {
          e.preventDefault();
          post({ command: 'cancelRun' });
          return;
        }
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          sendCurrentPrompt();
        }
      });

      sendBtn?.addEventListener('click', sendCurrentPrompt);

      document.body.addEventListener('click', (e) => {
        const target = e.target instanceof HTMLElement ? e.target.closest('button, a') : null;
        if (!target) return;

        const uiAction = target.getAttribute('data-ui-action') || target.getAttribute('data-onboarding-action');
        if (uiAction) {
          e.preventDefault();
          e.stopPropagation();
          dispatchUiAction(uiAction);
          return;
        }

        const starter = target.getAttribute('data-starter');
        if (starter && input) {
          input.value = starter;
          autoResizeInput();
          input.focus();
          return;
        }

        const sessionId = target.getAttribute('data-session-id');
        if (sessionId) {
          post({ command: 'switchConversation', sessionId });
          historyDrawer?.setAttribute('hidden', '');
          return;
        }

        const approveId = target.getAttribute('data-approve-id');
        if (approveId) {
          post({ command: 'approveAction', approvalId: approveId });
          return;
        }

        const denyId = target.getAttribute('data-deny-id');
        if (denyId) {
          post({ command: 'denyAction', approvalId: denyId });
          return;
        }

        const diffPath = target.getAttribute('data-view-diff');
        if (diffPath) {
          post({ command: 'viewDiff', filePath: diffPath, artifactId: target.getAttribute('data-artifact-id') || undefined });
          return;
        }

        const applyPath = target.getAttribute('data-apply-diff');
        if (applyPath) {
          post({ command: 'applyDiff', filePath: applyPath, artifactId: target.getAttribute('data-artifact-id') || undefined });
          return;
        }

        const revertPath = target.getAttribute('data-revert-diff');
        if (revertPath) {
          post({ command: 'revertDiff', filePath: revertPath, artifactId: target.getAttribute('data-artifact-id') || undefined });
          return;
        }

        const openFilePath = target.getAttribute('data-open-file');
        if (openFilePath) {
          e.preventDefault();
          post({ command: 'openFile', filePath: openFilePath });
          return;
        }

        const removeAtt = target.getAttribute('data-remove-attachment');
        if (removeAtt) {
          post({ command: 'removeAttachment', path: removeAtt });
          return;
        }

        if (target.id === 'dismissOnboardingBtn') {
          post({ command: 'dismissOnboarding' });
          target.closest('.onboarding-card')?.remove();
          return;
        }

        if (target.classList.contains('copy-code-btn')) {
          const codeEl = target.closest('.code-block-wrap')?.querySelector('code');
          const code = codeEl ? codeEl.textContent || '' : '';
          post({ command: 'copyCode', text: code });
          target.textContent = 'Copied';
          setTimeout(() => { target.textContent = 'Copy'; }, 1500);
        }
      });

      let streamingEl = null;
      window.addEventListener('message', (event) => {
        const msg = event.data;
        if (!msg || typeof msg !== 'object') return;
        if (msg.type === 'renderedThread' && threadInner) {
          threadInner.innerHTML = msg.html || '';
          streamingEl = null;
          scrollToBottom();
        } else if (msg.type === 'streamChunk' && msg.chunk && msg.chunk.type === 'text_delta' && msg.chunk.text && threadInner) {
          if (!streamingEl) {
            streamingEl = document.createElement('div');
            streamingEl.className = 'msg-row msg-assistant markdown-body';
            threadInner.appendChild(streamingEl);
          }
          streamingEl.textContent += msg.chunk.text;
          scrollToBottom();
        } else if (msg.type === 'runState') {
          state.isRunning = Boolean(msg.isRunning);
          if (sendBtn) {
            sendBtn.className = 'send-btn' + (state.isRunning ? ' is-stop' : '');
            sendBtn.textContent = state.isRunning ? '■' : '↑';
            sendBtn.title = state.isRunning ? 'Stop generation' : 'Send message';
            sendBtn.setAttribute('aria-label', sendBtn.title);
          }
        }
      });

      autoResizeInput();
      scrollToBottom();
    })();
  </script>
</body>
</html>`;
  }

  public static renderModelOptions(
    providers: readonly ProviderSummary[],
    selectedProviderId?: string,
    selectedModelId?: string
  ): string {
    const options: string[] = [];
    for (const p of providers) {
      for (const m of p.models) {
        const val = `${p.instanceId}::${m.id}`;
        const selected =
          (p.instanceId === selectedProviderId && m.id === selectedModelId) ||
          (!selectedModelId && m.isDefault);
        options.push(
          `<option value="${escapeHtml(val)}" ${selected ? 'selected' : ''}>${escapeHtml(m.name)} (${escapeHtml(p.displayName)})</option>`
        );
      }
    }
    if (options.length === 0) {
      options.push('<option value="" selected>No model configured</option>');
    }
    options.push('<option value="__manage_providers__">Manage Providers…</option>');
    return options.join('');
  }

  public static renderMessagesHtml(
    messages: readonly ChatMessage[],
    tasks: readonly TaskNode[] = [],
    pendingApprovals: readonly ApprovalRequest[] = []
  ): string {
    const blocks: string[] = [];

    for (let idx = 0; idx < messages.length; idx++) {
      const m = messages[idx];
      if (m.role === 'user') {
        blocks.push(`<div class="msg-row"><div class="msg-user">${escapeHtml(m.content)}</div></div>`);
        continue;
      }

      if (m.role === 'system') {
        const extraCta =
          m.approvalId === 'configure_provider' || m.approvalId === 'open_folder' || m.approvalId === 'connect' || m.approvalId === 'retry'
            ? `<button type="button" class="btn-primary btn-sm" data-ui-action="${escapeHtml(m.approvalId)}">${escapeHtml(m.approvalId === 'configure_provider' ? 'Configure Provider' : m.approvalId === 'open_folder' ? 'Open Folder' : 'Connect')}</button>`
            : '';
        blocks.push(`<div class="msg-system"><span>${escapeHtml(m.content)}</span>${extraCta}</div>`);
        continue;
      }

      // Assistant message with optional grouped tool calls, diffs, and streaming Markdown
      const parts: string[] = [];

      if (m.toolCalls && m.toolCalls.length > 0) {
        const completedCount = m.toolCalls.filter((t) => t.status === 'completed').length;
        const summaryText =
          m.toolCalls.length === 1
            ? `${m.toolCalls[0].name}${m.toolCalls[0].target ? ` · ${m.toolCalls[0].target}` : ''}`
            : `${m.toolCalls.length} tool calls · ${completedCount} completed`;
        const rows = m.toolCalls
          .map(
            (tc) => `
              <div class="tool-row">
                <span class="tool-row-name">${escapeHtml(tc.name)}${tc.target ? ` — <code>${escapeHtml(tc.target)}</code>` : ''}</span>
                <span class="tool-row-status">${escapeHtml(tc.status)}</span>
              </div>
            `
          )
          .join('');
        parts.push(`
          <details class="tool-group">
            <summary>
              <span>${escapeHtml(summaryText)}</span>
              <span class="tool-row-status">Details</span>
            </summary>
            <div class="tool-group-list">${rows}</div>
          </details>
        `);
      }

      const mdHtml = ChatHtmlBuilder.enhanceRenderedMarkdown(renderSafeMarkdown(m.content));
      parts.push(
        `<div class="msg-assistant markdown-body">${mdHtml}${m.isStreaming ? '<span class="streaming-dot" aria-label="Streaming"></span>' : ''}</div>`
      );

      if (m.artifacts && m.artifacts.length > 0) {
        for (const art of m.artifacts) {
          const targetPath = art.path || art.id || 'Modified file';
          parts.push(`
            <div class="diff-card">
              <div class="diff-card-row">
                <span class="diff-file-path">${escapeHtml(targetPath)}</span>
                <div class="card-actions">
                  <button type="button" class="btn-secondary btn-sm" data-view-diff="${escapeHtml(targetPath)}" data-artifact-id="${escapeHtml(art.id || '')}">View Diff</button>
                  <button type="button" class="btn-primary btn-sm" data-apply-diff="${escapeHtml(targetPath)}" data-artifact-id="${escapeHtml(art.id || '')}">Apply</button>
                  <button type="button" class="btn-ghost btn-sm" data-revert-diff="${escapeHtml(targetPath)}" data-artifact-id="${escapeHtml(art.id || '')}">Revert</button>
                </div>
              </div>
            </div>
          `);
        }
      }

      blocks.push(`<div class="msg-row">${parts.join('')}</div>`);
    }

    // Inline approval cards for pending approvals
    for (const req of pendingApprovals.filter((a) => a.status === 'pending')) {
      blocks.push(`
        <div class="approval-card" role="region" aria-label="Approval Request">
          <div class="diff-card-row">
            <strong>Action approval needed (${escapeHtml(req.riskLevel)})</strong>
          </div>
          <div class="diff-file-path">${escapeHtml(req.commandOrTarget)}</div>
          <div class="card-actions">
            <button type="button" class="btn-primary btn-sm" data-approve-id="${escapeHtml(req.id)}">Approve</button>
            <button type="button" class="btn-ghost btn-sm" data-deny-id="${escapeHtml(req.id)}">Deny</button>
          </div>
        </div>
      `);
    }

    // Progressive multi-agent disclosure: rendered ONLY when >= 2 subtasks/agents exist
    if (tasks.length >= 2) {
      const done = tasks.filter((t) => t.status === 'succeeded').length;
      const rows = tasks
        .map(
          (t) => `
            <div class="agent-row">
              <span class="agent-row-title">${escapeHtml(t.assignedAgentName ? `${t.assignedAgentName}: ` : '')}${escapeHtml(t.title)}</span>
              <span class="tool-row-status">${escapeHtml(t.status)}</span>
            </div>
          `
        )
        .join('');
      blocks.push(`
        <details class="multi-agent-card" open>
          <summary>
            <span>Plan &amp; Agents (${done}/${tasks.length} done)</span>
            <button type="button" class="link-btn" data-ui-action="open_mission_control">Mission Control</button>
          </summary>
          <div class="multi-agent-list">${rows}</div>
        </details>
      `);
    }

    return blocks.join('');
  }

  /**
   * Adds language header + working Copy button around fenced code blocks produced by renderSafeMarkdown.
   */
  public static enhanceRenderedMarkdown(html: string): string {
    return html.replace(
      /<pre class="code-block"><code(?: class="language-([^"]*)")?>([\s\S]*?)<\/code><\/pre>/g,
      (_m, lang, codeBody) => {
        const label = lang ? escapeHtml(lang) : 'code';
        return `<div class="code-block-wrap"><div class="code-block-header"><span>${label}</span><button type="button" class="copy-code-btn">Copy</button></div><pre><code${lang ? ` class="language-${escapeHtml(lang)}"` : ''}>${codeBody}</code></pre></div>`;
      }
    );
  }
}
