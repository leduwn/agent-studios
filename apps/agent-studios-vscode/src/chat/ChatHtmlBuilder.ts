/**
 * Agent Studios — Chat View HTML Document Builder
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 *
 * Generates secure, accessible HTML for the Chat WebviewView with:
 * - Strict Content Security Policy with cryptographic nonce
 * - Header with session info, history dropdown, new session button, connection indicator
 * - Smooth-scrolling conversation container for all message types & cards
 * - Sticky bottom composer with auto-grow textarea, context chips, mode/model selectors
 * - Client-side message handling, state synchronization, and keyboard shortcuts
 */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import {
  AgentMode,
  ChatMessage,
  ConnectionState,
  ContextAttachment,
  ProviderSummary,
  SessionInfo,
  SessionSummary,
} from '../bridge/types';
import { CardRenderer } from '../cards/CardRenderer';
import { escapeHtml } from '../webviews/markdownRenderer';
import { CHAT_STYLES } from './styles.css';
import { OnboardingWorkflow } from './OnboardingWorkflow';
import { TaskExecutionState, ExecutionMetrics } from '../state/TaskExecutionStore';
import { WorkspaceAuthorizationStatus } from '../security/WorkspaceVerification';
import { CapabilityMatrix } from '../bridge/ipc/CapabilityMatrix';

export interface RuntimeOption {
  id: string;
  name: string;
  enabled: boolean;
  tooltip?: string;
  instanceId?: string;
  implementation?: string;
}

export interface StudioOption {
  id: string;
  name: string;
}

export interface ChatHtmlInitialState {
  session?: SessionInfo | null;
  sessions?: SessionSummary[];
  connectionState?: ConnectionState;
  isPreview?: boolean;
  providers?: ProviderSummary[];
  activeMode?: AgentMode;
  selectedModelId?: string;
  selectedProviderId?: string;
  selectedRuntime?: string;
  selectedStudioId?: string;
  runtimes?: RuntimeOption[];
  studios?: StudioOption[];
  workspacePath?: string;
  workspaceStatus?: WorkspaceAuthorizationStatus;
  isWorkspaceMatched?: boolean;
  workspaceMismatchReason?: string;
  isTaskSubmitEnabled?: boolean;
  composerDraft?: string;
  messages?: ChatMessage[];
  attachments?: ContextAttachment[];
  hasSeenOnboarding?: boolean;
  executionState?: TaskExecutionState;
}

export class ChatHtmlBuilder {
  /**
   * Generates the complete HTML document for the Agent Studios chat WebviewView.
   */
  public static buildHtml(
    webview: vscode.Webview,
    _extensionUri: vscode.Uri,
    initialState?: ChatHtmlInitialState
  ): string {
    const nonce = crypto.randomBytes(16).toString('base64');

    const session = initialState?.session ?? null;
    const sessions = initialState?.sessions ?? [];
    const connectionState: ConnectionState = initialState?.connectionState ?? 'disconnected';
    const isPreview = Boolean(initialState?.isPreview);
    const providers = initialState?.providers ?? [];
    const activeMode: AgentMode = initialState?.activeMode ?? 'agent';
    const selectedModelId = initialState?.selectedModelId ?? '';
    const selectedProviderId = initialState?.selectedProviderId ?? '';
    const composerDraft = initialState?.composerDraft ?? '';
    const messages = initialState?.messages ?? [];
    const attachments = initialState?.attachments ?? [];

    const sessionTitle = session?.goalSummary || 'Agent Studios Assistant';
    const sessionId = session?.id || '';

    const selectedRuntime = initialState?.selectedRuntime || '';
    const selectedStudioId = initialState?.selectedStudioId || session?.id || '';
    const workspacePath = initialState?.workspacePath || session?.workspacePath || '';
    const workspaceStatus: WorkspaceAuthorizationStatus = initialState?.workspaceStatus
      ?? (initialState?.isWorkspaceMatched === false ? 'MISMATCH' : (initialState?.isWorkspaceMatched ? 'AUTHORIZED' : 'UNAVAILABLE'));
    const isWorkspaceAuthorized = workspaceStatus === 'AUTHORIZED';
    const workspaceMismatchReason = initialState?.workspaceMismatchReason || '';
    const isTaskSubmitEnabled =
      (initialState?.isTaskSubmitEnabled ?? CapabilityMatrix.isCapabilityEnabled('task.submit')) &&
      isWorkspaceAuthorized &&
      CapabilityMatrix.isCapabilityEnabled('task.submit');
    const executionState = initialState?.executionState;

    let badgeLabel = '⚠ Unavailable';
    let badgeClass = 'workspace-badge--unavailable';
    if (workspaceStatus === 'AUTHORIZED') {
      badgeLabel = '✓ Authorized';
      badgeClass = 'workspace-badge--authorized workspace-badge--matched';
    } else if (workspaceStatus === 'MISMATCH') {
      badgeLabel = '⚠ Mismatch';
      badgeClass = 'workspace-badge--mismatch workspace-badge--mismatched';
    } else if (workspaceStatus === 'UNVERIFIED') {
      badgeLabel = '⚠ Unverified';
      badgeClass = 'workspace-badge--unverified';
    }

    const shortWorkspacePath = workspacePath
      ? (workspacePath.length > 28 ? '...' + workspacePath.slice(-25) : workspacePath)
      : 'No workspace';
    const workspaceTooltip = isWorkspaceAuthorized
      ? `Workspace authorized by daemon: ${workspacePath}`
      : (workspaceMismatchReason || `Workspace status ${workspaceStatus}: ${workspacePath}`);

    let selectedModelName = 'Default Model';
    if (providers.length > 0) {
      for (const p of providers) {
        const m = p.models.find(mod => mod.id === selectedModelId && p.instanceId === selectedProviderId);
        if (m) {
          selectedModelName = `${m.name} (${p.displayName})`;
          break;
        }
      }
    }

    const formatElapsed = (sec?: number) => {
      const s = Math.max(0, sec || 0);
      const mins = Math.floor(s / 60).toString().padStart(2, '0');
      const secs = (s % 60).toString().padStart(2, '0');
      return `${mins}:${secs}`;
    };

    const hasSeenOnboarding = Boolean(initialState?.hasSeenOnboarding);
    const onboardingHtml = OnboardingWorkflow.renderOnboardingCard({
      isConnected: connectionState === 'connected',
      isPreview,
      hasSeenOnboarding,
      daemonEndpoint: 'ipc://agent-studios.sock',
      activeSessionId: sessionId,
    });

    // Model options HTML
    const modelOptionsHtml = ChatHtmlBuilder.renderModelOptions(
      providers,
      selectedProviderId,
      selectedModelId
    );

    // Session options HTML
    const sessionOptionsHtml = ChatHtmlBuilder.renderSessionOptions(sessions, sessionId);

    // Runtime options HTML
    const runtimeOptions: RuntimeOption[] = initialState?.runtimes && initialState.runtimes.length > 0
      ? initialState.runtimes
      : [
          { id: '', name: 'No runtimes available (daemon unready)', enabled: false },
        ];

    const runtimeOptionsHtml = ChatHtmlBuilder.renderRuntimeOptions(
      runtimeOptions,
      selectedRuntime
    );

    // Studio options HTML
    const studioOptions: StudioOption[] = (initialState?.studios && initialState.studios.length > 0)
      ? [...initialState.studios]
      : (sessions && sessions.length > 0)
      ? sessions.map((s) => ({ id: s.id, name: s.title || s.id }))
      : [
          { id: '', name: 'No studios found (create studio required)' },
        ];

    if (selectedStudioId && !studioOptions.some((s) => s.id === selectedStudioId)) {
      studioOptions.unshift({ id: selectedStudioId, name: selectedStudioId });
    }

    const studioOptionsHtml = studioOptions.map((s) => {
      const isSelected = s.id === selectedStudioId;
      return `<option value="${escapeHtml(s.id)}" ${isSelected ? 'selected' : ''}>${escapeHtml(s.name)}</option>`;
    }).join('\n');

    // Initial state serialized for in-page script
    const clientStateJson = JSON.stringify({
      session,
      sessions,
      connectionState,
      isPreview,
      providers,
      activeMode,
      selectedModelId,
      selectedProviderId,
      selectedRuntime,
      selectedStudioId,
      workspacePath,
      workspaceStatus,
      isWorkspaceAuthorized,
      isWorkspaceMatched: isWorkspaceAuthorized,
      workspaceMismatchReason,
      isTaskSubmitEnabled,
      composerDraft,
      messages,
      attachments,
      executionState,
    }).replace(/</g, '\\u003c');

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${webview.cspSource};">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios Chat</title>
  <style>
    ${CHAT_STYLES}
    ${CardRenderer.getStyles()}

    /* Custom Chat Extensions */
    .chat-header-top {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .brand-title {
      display: flex;
      align-items: center;
      gap: 6px;
      font-weight: 600;
      font-size: 13px;
      color: var(--fg-primary);
    }

    .brand-icon {
      font-size: 14px;
      line-height: 1;
    }

    .preview-tag {
      background: var(--status-yellow);
      color: #000000;
      font-size: 10px;
      font-weight: bold;
      padding: 1px 5px;
      border-radius: 3px;
      letter-spacing: 0.5px;
      text-transform: uppercase;
    }

    .chat-header-session {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 6px;
      font-size: 11px;
    }

    .session-title-label {
      flex: 1;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      color: var(--fg-subtle);
    }

    .session-actions {
      display: flex;
      align-items: center;
      gap: 4px;
      flex-shrink: 0;
    }

    .session-history-select {
      background: var(--input-bg);
      color: var(--input-fg);
      border: 1px solid var(--border-subtle);
      border-radius: 3px;
      font-size: 11px;
      padding: 2px 4px;
      max-width: 120px;
      cursor: pointer;
    }

    .btn-sm {
      padding: 2px 6px;
      font-size: 11px;
      border-radius: 3px;
      cursor: pointer;
      border: 1px solid var(--border-subtle);
      background: var(--btn-secondary-bg);
      color: var(--btn-secondary-fg);
    }

    .btn-sm:hover {
      background: rgba(255, 255, 255, 0.1);
    }

    /* Context Chips Bar */
    .context-chips-bar {
      display: flex;
      flex-wrap: wrap;
      gap: 4px;
      padding: 4px 0;
      max-height: 80px;
      overflow-y: auto;
    }

    .context-chip {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      background-color: var(--badge-bg);
      color: var(--badge-fg);
      border-radius: 12px;
      padding: 2px 8px;
      font-size: 11px;
      line-height: 1.2;
      max-width: 180px;
    }

    .context-chip-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .chip-remove-btn {
      background: none;
      border: none;
      color: var(--badge-fg);
      cursor: pointer;
      padding: 0 2px;
      font-size: 13px;
      line-height: 1;
      opacity: 0.7;
    }

    .chip-remove-btn:hover {
      opacity: 1;
    }

    .chip-add-btn {
      background: transparent;
      border: 1px dashed var(--border-subtle);
      color: var(--fg-subtle);
      border-radius: 12px;
      padding: 2px 8px;
      font-size: 11px;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 3px;
    }

    .chip-add-btn:hover {
      border-color: var(--border-active);
      color: var(--fg-primary);
    }

    /* Auto-grow Textarea */
    .composer-textarea {
      box-sizing: border-box;
      width: 100%;
      min-height: 40px;
      max-height: 150px;
      resize: none;
      overflow-y: auto;
      border: none;
      background: transparent;
      color: var(--input-fg);
      padding: 6px 8px;
      font-family: inherit;
      font-size: var(--font-size);
      line-height: 1.4;
      outline: none;
    }

    /* Mode & Model Toolbar Selectors */
    .mode-select-dropdown {
      background: var(--input-bg);
      color: var(--input-fg);
      border: 1px solid var(--border-subtle);
      border-radius: 4px;
      padding: 2px 6px;
      font-size: 11px;
      font-weight: 500;
      cursor: pointer;
    }

    .btn-stop {
      background-color: var(--status-red);
      color: #ffffff;
      border: 1px solid transparent;
      border-radius: 4px;
      padding: 4px 10px;
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 4px;
    }

    .btn-stop:hover {
      opacity: 0.9;
    }

    /* Conversation Message Elements */
    .chat-empty-state {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      text-align: center;
      color: var(--fg-subtle);
      padding: 32px 16px;
      gap: 12px;
      height: 100%;
    }

    .chat-empty-state h3 {
      font-size: 14px;
      color: var(--fg-primary);
      margin: 0;
    }

    .chat-empty-state p {
      font-size: 12px;
      max-width: 260px;
      line-height: 1.4;
      margin: 0;
    }

    .streaming-cursor {
      display: inline-block;
      width: 6px;
      height: 14px;
      background-color: var(--fg-primary);
      margin-left: 2px;
      animation: blink 1s step-end infinite;
      vertical-align: text-bottom;
    }

    @keyframes blink {
      0%, 100% { opacity: 1; }
      50% { opacity: 0; }
    }

    /* Onboarding State Cards */
    .onboarding-card {
      border-radius: 4px;
      padding: 10px 12px;
      margin: 8px 12px;
      font-size: 12px;
      line-height: 1.4;
      display: flex;
      flex-direction: column;
      gap: 6px;
    }
    .onboarding-card--preview {
      background: rgba(229, 192, 123, 0.12);
      border: 1px solid var(--status-yellow);
    }
    .onboarding-card--disconnected {
      background: rgba(224, 108, 117, 0.12);
      border: 1px solid var(--status-red);
    }
    .onboarding-card--connected {
      background: rgba(152, 195, 121, 0.12);
      border: 1px solid var(--status-green);
    }
    .onboarding-header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 8px;
    }
    .onboarding-title-group {
      flex: 1;
      display: flex;
      flex-direction: column;
    }
    .onboarding-title {
      font-weight: 600;
      color: var(--fg-primary);
    }
    .onboarding-subtitle {
      font-size: 11px;
      color: var(--fg-subtle);
    }
    .onboarding-dismiss-btn {
      background: none;
      border: none;
      color: var(--fg-subtle);
      cursor: pointer;
      padding: 0 4px;
      font-size: 14px;
      line-height: 1;
    }
    .onboarding-dismiss-btn:hover {
      color: var(--fg-primary);
    }
    .onboarding-security-note {
      display: flex;
      gap: 6px;
      align-items: center;
      font-size: 11px;
      color: var(--fg-subtle);
      background: rgba(0, 0, 0, 0.15);
      padding: 4px 6px;
      border-radius: 3px;
      margin-top: 4px;
    }
    .onboarding-actions {
      display: flex;
      gap: 8px;
      margin-top: 6px;
    }

    /* Agent-First Welcome Surface */
    .agent-welcome-surface {
      display: flex;
      flex-direction: column;
      gap: 16px;
      padding: 16px 8px 24px 8px;
      width: 100%;
      max-width: 540px;
      margin: 0 auto;
      box-sizing: border-box;
    }

    .welcome-hero {
      display: flex;
      flex-direction: column;
      align-items: center;
      text-align: center;
      gap: 4px;
      padding: 8px 0;
    }

    .welcome-icon-box {
      font-size: 28px;
      line-height: 1;
      margin-bottom: 4px;
    }

    .welcome-title {
      font-size: 16px;
      font-weight: 700;
      color: var(--fg-primary);
      margin: 0;
      letter-spacing: -0.3px;
    }

    .welcome-subtitle {
      font-size: 12px;
      color: var(--fg-subtle);
      margin: 0;
      line-height: 1.4;
    }

    .welcome-grid {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }

    .welcome-card {
      background: var(--bg-card);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      padding: 10px 12px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .welcome-card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .welcome-card-title {
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--fg-subtle);
    }

    .status-pill {
      font-size: 9px;
      font-weight: 700;
      text-transform: uppercase;
      padding: 2px 6px;
      border-radius: 10px;
      letter-spacing: 0.4px;
      background: rgba(128, 128, 128, 0.2);
      color: var(--fg-subtle);
    }

    .status-pill--active {
      background: rgba(152, 195, 121, 0.2);
      color: var(--status-green);
    }

    .status-pill--warn {
      background: rgba(229, 192, 123, 0.2);
      color: var(--status-yellow);
    }

    .welcome-card-body {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .welcome-detail-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      font-size: 12px;
    }

    .detail-label {
      color: var(--fg-subtle);
    }

    .detail-val {
      font-weight: 500;
      color: var(--fg-primary);
      text-overflow: ellipsis;
      overflow: hidden;
      white-space: nowrap;
      max-width: 220px;
    }

    .detail-code {
      font-family: var(--font-mono);
      font-size: 11px;
      background: rgba(0, 0, 0, 0.2);
      padding: 2px 5px;
      border-radius: 3px;
      color: var(--fg-primary);
    }

    .welcome-actions {
      display: flex;
      gap: 8px;
      margin-top: 4px;
      flex-wrap: wrap;
    }

    .welcome-notice {
      font-size: 11px;
      line-height: 1.4;
      color: var(--fg-subtle);
      background: rgba(229, 192, 123, 0.1);
      border-left: 2px solid var(--status-yellow);
      padding: 4px 8px;
      border-radius: 0 3px 3px 0;
      margin-top: 2px;
    }

    .welcome-starters-section {
      display: flex;
      flex-direction: column;
      gap: 8px;
      margin-top: 4px;
    }

    .starters-label {
      font-size: 11px;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--fg-subtle);
    }

    .starters-grid {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .starter-chip {
      display: flex;
      align-items: center;
      gap: 8px;
      background: var(--bg-card);
      border: 1px solid var(--border-subtle);
      border-radius: 6px;
      padding: 8px 10px;
      color: var(--fg-primary);
      cursor: pointer;
      font-family: inherit;
      font-size: 12px;
      text-align: left;
      transition: background 0.12s ease, border-color 0.12s ease;
    }

    .starter-chip:hover {
      background: var(--border-subtle);
      border-color: var(--fg-subtle);
    }

    .starter-icon {
      color: var(--fg-subtle);
      font-weight: bold;
      flex-shrink: 0;
    }

    .starter-text {
      flex: 1;
      line-height: 1.3;
    }

    /* 14 Conversation UX States Badges */
    .status-badge.status-connecting,
    .status-badge.status-reconnecting {
      background-color: rgba(229, 192, 123, 0.2);
      color: var(--status-yellow);
    }
    .status-badge.status-connected {
      background-color: rgba(152, 195, 121, 0.2);
      color: var(--status-green);
    }
    .status-badge.status-degraded {
      background-color: rgba(229, 192, 123, 0.25);
      color: var(--status-yellow);
    }
    .status-badge.status-offline {
      background-color: rgba(224, 108, 117, 0.2);
      color: var(--status-red);
    }
    .status-badge.status-session_restored {
      background-color: rgba(97, 175, 239, 0.2);
      color: var(--status-blue);
    }
    .status-badge.status-task_pending {
      background-color: rgba(229, 192, 123, 0.2);
      color: var(--status-yellow);
    }
    .status-badge.status-awaiting_approval {
      background-color: rgba(229, 192, 123, 0.3);
      color: var(--status-yellow);
      border: 1px solid var(--status-yellow);
    }
    .status-badge.status-interrupted {
      background-color: rgba(224, 108, 117, 0.25);
      color: var(--status-red);
    }
    .status-badge.status-completed {
      background-color: rgba(152, 195, 121, 0.2);
      color: var(--status-green);
    }
    .status-badge.status-verified_artifact {
      background-color: rgba(152, 195, 121, 0.25);
      color: var(--status-green);
    }
    .status-badge.status-unverified_artifact {
      background-color: rgba(229, 192, 123, 0.2);
      color: var(--status-yellow);
    }
  </style>
</head>
<body>
  <div class="chat-container">
    <!-- Header -->
    <header class="chat-header" role="banner">
      <div class="chat-header-top">
        <div class="brand-title">
          <span class="brand-icon" aria-hidden="true">⚡</span>
          <span>Agent Studios</span>
          ${isPreview ? '<span class="preview-tag">[Preview Fixture]</span>' : ''}
        </div>
        <div id="connectionStatusIndicator" class="header-status">
          <span class="status-dot status-${connectionState === 'connected' ? 'available' : connectionState === 'connecting' || connectionState === 'reconnecting' ? 'unconfigured' : 'offline'}" aria-hidden="true"></span>
          <span id="connectionStatusText">${escapeHtml(connectionState === 'connected' ? 'Connected' : connectionState === 'connecting' || connectionState === 'reconnecting' ? 'Connecting...' : 'Disconnected')}</span>
        </div>
      </div>
      <div class="chat-header-session">
        <span id="currentSessionName" class="session-title-label" title="${escapeHtml(sessionTitle)}">
          ${escapeHtml(sessionTitle)}
        </span>
        <div class="session-actions">
          <select id="sessionHistorySelect" class="session-history-select" aria-label="Session History" title="Switch session">
            ${sessionOptionsHtml}
          </select>
          <button id="newSessionBtn" class="btn-sm" title="Start a new session" aria-label="New Session">+ New</button>
        </div>
      </div>
    </header>

    <!-- Onboarding / Status Banner -->
    ${onboardingHtml}

    <!-- Conversation Stream Container -->
    <main id="conversationContainer" class="chat-messages" role="log" aria-live="polite">
      ${messages.length === 0 ? ChatHtmlBuilder.renderEmptyState(initialState) : ''}
    </main>

    <!-- Sticky Bottom Composer -->
    <footer class="chat-composer" role="region" aria-label="Message Composer">
      <!-- M15 Workbench Toolbar: Runtime, Studio, Workspace -->
      <div class="workbench-toolbar" role="region" aria-label="Execution Workbench">
        <div class="workbench-field">
          <label for="runtimeSelect" class="workbench-field-label">Runtime</label>
          <select id="runtimeSelect" class="workbench-select" aria-label="Agent Runtime">
            ${runtimeOptionsHtml}
          </select>
        </div>
        <div class="workbench-field">
          <label for="studioSelect" class="workbench-field-label">Studio</label>
          <select id="studioSelect" class="workbench-select" aria-label="Active Studio">
            ${studioOptionsHtml}
          </select>
        </div>
        <div class="workbench-field">
          <span class="workbench-field-label">Workspace</span>
          <div id="workspaceBadge" class="workspace-badge ${badgeClass}" title="${escapeHtml(workspaceTooltip)}">
            <span id="workspaceBadgeLabel">${badgeLabel}</span>
            <span id="workspacePathDisplay">${escapeHtml(workspacePath || shortWorkspacePath)}</span>
            ${!isWorkspaceAuthorized && workspaceMismatchReason ? `<span class="workspace-mismatch-detail" title="${escapeHtml(workspaceMismatchReason)}">(${escapeHtml(workspaceMismatchReason)})</span>` : ''}
          </div>
        </div>
      </div>

      <!-- M15 Execution Configuration Summary Card -->
      <div id="executionSummaryCard" class="execution-summary-card" role="region" aria-label="Execution Configuration Summary">
        <div class="summary-header">
          <span>Execution Target Summary</span>
          <span class="gate-badge" title="Cryptographically signed host modal confirmation">Host Modal Gate (HMAC-SHA256)</span>
        </div>
        <div class="summary-grid">
          <div class="summary-item">
            <span class="summary-label">Runtime</span>
            <span class="summary-value" id="summaryRuntime">Claude Code (Native CLI)</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Model</span>
            <span class="summary-value" id="summaryModel">${escapeHtml(selectedModelName)}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Workspace</span>
            <span class="summary-value" id="summaryWorkspace" title="${escapeHtml(workspacePath)}">${escapeHtml(shortWorkspacePath)}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Approval Policy</span>
            <span class="summary-value">Host-Owned Modal Dialog</span>
          </div>
        </div>
      </div>

      <!-- M15 Live Execution Streaming Pane -->
      <div id="executionStreamingPane" class="execution-streaming-pane" style="${executionState && executionState.status !== 'idle' ? '' : 'display: none;'}" role="region" aria-label="Live Execution Streaming Output" aria-live="polite">
        <div class="execution-pane-header">
          <div class="execution-status-group">
            <span id="executionStatusBadge" class="status-badge status-${executionState?.status || 'idle'}">${(executionState?.status || 'idle').toUpperCase()}</span>
            <span id="executionTimer" class="execution-timer">${formatElapsed(executionState?.elapsedSeconds)}</span>
          </div>
          <div class="execution-actions">
            <button id="cancelExecutionBtn" class="btn-sm" style="${executionState && (executionState.status === 'submitting' || executionState.status === 'running') ? '' : 'display: none;'}" title="Cancel active task execution">Cancel Task</button>
          </div>
        </div>
        <div id="activeToolBanner" class="active-tool-banner" role="status" aria-live="polite" style="${executionState?.activeTool ? '' : 'display: none;'}">
          <div class="active-tool-info">
            <span class="active-tool-spinner" aria-hidden="true"></span>
            <span>Tool: <strong id="activeToolName">${escapeHtml(executionState?.activeTool?.name || '')}</strong> <span id="activeToolStatus">(${escapeHtml(executionState?.activeTool?.status || '')})</span></span>
          </div>
          <span id="activeToolError" class="tool-error" style="color: var(--status-red);">${escapeHtml(executionState?.activeTool?.error || '')}</span>
        </div>
        <div class="streaming-console-box">
          <div class="streaming-console-title">
            <span>Streaming Console</span>
            <div style="display: flex; gap: 6px; align-items: center;">
              <button type="button" class="btn-xs" style="background: none; border: 1px solid var(--border-subtle); color: var(--fg-subtle); cursor: pointer; border-radius: 3px; padding: 1px 6px; font-size: 10px;" data-action="open-mission-control" title="Inspect full DAG, fleet, and diagnostics in Mission Control">Mission Control →</button>
              <button id="clearStreamBtn" class="btn-xs" style="background: none; border: none; color: var(--fg-subtle); cursor: pointer;" title="Clear streaming console">Clear</button>
            </div>
          </div>
          <pre id="streamingConsoleOutput" class="streaming-console-output">${escapeHtml(executionState?.streamOutput || '')}</pre>
        </div>
        <div id="executionMetricsContainer" class="truthful-metrics-container">
          ${ChatHtmlBuilder.renderMetricsHtml(executionState?.metrics)}
        </div>
      </div>

      <!-- Context Chips Bar -->
      <div id="contextChipsBar" class="context-chips-bar" aria-label="Attached Context Files">
        <button id="addContextBtn" class="chip-add-btn" title="Add file context from workspace" aria-label="Add file context">+ Add File</button>
        <button id="addActiveEditorBtn" class="chip-add-btn" title="Attach active editor file" aria-label="Attach active editor">Attach Active File</button>
      </div>

      <!-- Composer Input Box -->
      <div class="composer-box">
        <!-- Prompt Frame Boundary Warning / Error -->
        <div id="composerWarning" class="composer-warning-banner" style="display: none;" role="alert">
          <span>⚠️</span>
          <span>Warning: Prompt exceeds 100 KB wire frame boundary threshold.</span>
        </div>
        <div id="composerError" class="composer-error-banner" style="display: none;" role="alert" aria-live="assertive">
          <span>⛔</span>
          <span>Error: Prompt exceeds 4 MiB wire frame limit. Execution blocked.</span>
        </div>

        <textarea
          id="composerInput"
          class="composer-textarea"
          rows="2"
          placeholder="Describe agent task or instruction in Markdown... (Ctrl+Enter to submit, Shift+Enter for newline)"
          aria-label="Chat prompt input"
        >${escapeHtml(composerDraft)}</textarea>

        <!-- Composer Toolbar -->
        <div class="composer-footer">
          <div class="composer-buttons">
            <!-- Mode Selector -->
            <select id="modeSelect" class="mode-select-dropdown" aria-label="Agent Mode">
              <option value="ask" ${activeMode === 'ask' ? 'selected' : ''}>Ask</option>
              <option value="plan" ${activeMode === 'plan' ? 'selected' : ''}>Plan</option>
              <option value="agent" ${activeMode === 'agent' ? 'selected' : ''}>Agent</option>
            </select>

            <!-- Model Selector -->
            <select id="modelSelect" class="provider-model-dropdown" aria-label="AI Model and Provider">
              ${modelOptionsHtml}
            </select>
          </div>

          <div class="composer-buttons">
            <button id="stopBtn" class="btn-stop" style="display: none;" title="Cancel running task" aria-label="Cancel task">
              ■ Cancel Task
            </button>
            <button id="sendBtn" class="btn-primary" title="Execute task via host modal confirmation (Ctrl+Enter to submit, Shift+Enter for newline)" aria-label="Start Task">
              ▶ Start Task
            </button>
          </div>
        </div>
      </div>
    </footer>
  </div>

  <script nonce="${nonce}">
    (function() {
      var vscode = acquireVsCodeApi();
      var initialState = ${clientStateJson};

      var state = {
        session: initialState.session,
        sessions: initialState.sessions || [],
        connectionState: initialState.connectionState || 'disconnected',
        isPreview: Boolean(initialState.isPreview),
        providers: initialState.providers || [],
        mode: initialState.activeMode || 'agent',
        selectedModelId: initialState.selectedModelId || '',
        selectedProviderId: initialState.selectedProviderId || '',
        composerDraft: initialState.composerDraft || '',
        messages: initialState.messages || [],
        attachments: initialState.attachments || [],
        isExecuting: false,
        runtime: initialState.selectedRuntime || '',
        studioId: initialState.selectedStudioId || '',
        workspacePath: initialState.workspacePath || '',
        isWorkspaceMatched: initialState.isWorkspaceMatched !== false,
        workspaceMismatchReason: initialState.workspaceMismatchReason || '',
        isTaskSubmitEnabled: initialState.isTaskSubmitEnabled !== false,
        executionState: initialState.executionState || { status: 'idle', elapsedSeconds: 0, streamOutput: '' }
      };

      // DOM Elements
      var conversationContainer = document.getElementById('conversationContainer');
      var composerInput = document.getElementById('composerInput');
      var sendBtn = document.getElementById('sendBtn');
      var stopBtn = document.getElementById('stopBtn');
      var modeSelect = document.getElementById('modeSelect');
      var modelSelect = document.getElementById('modelSelect');
      var sessionHistorySelect = document.getElementById('sessionHistorySelect');
      var newSessionBtn = document.getElementById('newSessionBtn');
      var addContextBtn = document.getElementById('addContextBtn');
      var addActiveEditorBtn = document.getElementById('addActiveEditorBtn');
      var contextChipsBar = document.getElementById('contextChipsBar');
      var currentSessionName = document.getElementById('currentSessionName');
      var connectionStatusIndicator = document.getElementById('connectionStatusIndicator');
      var runtimeSelect = document.getElementById('runtimeSelect');
      var studioSelect = document.getElementById('studioSelect');
      var workspaceBadge = document.getElementById('workspaceBadge');
      var workspacePathDisplay = document.getElementById('workspacePathDisplay');
      var summaryRuntime = document.getElementById('summaryRuntime');
      var summaryModel = document.getElementById('summaryModel');
      var summaryWorkspace = document.getElementById('summaryWorkspace');
      var executionStreamingPane = document.getElementById('executionStreamingPane');
      var executionStatusBadge = document.getElementById('executionStatusBadge');
      var executionTimer = document.getElementById('executionTimer');
      var cancelExecutionBtn = document.getElementById('cancelExecutionBtn');
      var activeToolBanner = document.getElementById('activeToolBanner');
      var activeToolName = document.getElementById('activeToolName');
      var activeToolStatus = document.getElementById('activeToolStatus');
      var activeToolError = document.getElementById('activeToolError');
      var clearStreamBtn = document.getElementById('clearStreamBtn');
      var streamingConsoleOutput = document.getElementById('streamingConsoleOutput');
      var executionMetricsContainer = document.getElementById('executionMetricsContainer');
      var composerWarning = document.getElementById('composerWarning');
      var composerError = document.getElementById('composerError');

      function escapeText(str) {
        if (!str) return '';
        return String(str)
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;')
          .replace(/"/g, '&quot;')
          .replace(/'/g, '&#39;');
      }

      function renderMarkdown(md) {
        if (!md) return '';
        var text = escapeText(md);
        // Fenced code blocks
        text = text.replace(/\`\`\`([a-zA-Z0-9_-]*)\\n([\\s\\S]*?)\`\`\`/g, function(_m, lang, code) {
          var cls = lang ? ' class="language-' + lang + '"' : '';
          return '<pre class="code-block"><code' + cls + '>' + code + '</code></pre>';
        });
        // Inline code
        text = text.replace(/\`([^\`]+)\`/g, '<code class="inline-code">$1</code>');
        // Bold & italic
        text = text.replace(/\\*\\*([^*]+)\\*\\*/g, '<strong>$1</strong>');
        text = text.replace(/\\*([^*]+)\\*/g, '<em>$1</em>');
        // Headings
        text = text.replace(/^### (.*$)/gim, '<h4>$1</h4>');
        text = text.replace(/^## (.*$)/gim, '<h3>$1</h3>');
        text = text.replace(/^# (.*$)/gim, '<h2>$1</h2>');
        // Blockquotes
        text = text.replace(/^&gt; (.*$)/gim, '<blockquote>$1</blockquote>');
        // Unordered lists
        text = text.replace(/^\\s*[-*]\\s+(.*$)/gim, '<li>$1</li>');
        // Paragraph line breaks
        text = text.replace(/\\n\\n/g, '<br/><br/>');
        return text;
      }

      function autoGrowTextarea() {
        composerInput.style.height = 'auto';
        var newHeight = Math.min(composerInput.scrollHeight, 150);
        composerInput.style.height = Math.max(newHeight, 40) + 'px';
      }

      function scrollToBottom() {
        conversationContainer.scrollTop = conversationContainer.scrollHeight;
      }

      function renderContextChips() {
        var chipsHtml = state.attachments.map(function(att) {
          var label = att.relativePath || att.name || att.uri || 'file';
          var fullPath = att.uri || att.path || label;
          return '<div class="context-chip" title="' + escapeText(fullPath) + '">' +
            '<span>📄</span>' +
            '<span class="context-chip-label">' + escapeText(label) + '</span>' +
            '<button class="chip-remove-btn" data-remove-path="' + escapeText(fullPath) + '" title="Remove context" aria-label="Remove ' + escapeText(label) + '">&times;</button>' +
            '</div>';
        }).join('');

        contextChipsBar.innerHTML = chipsHtml +
          '<button id="addContextBtn" class="chip-add-btn" title="Add file context" aria-label="Add file context">+ Add File</button>' +
          '<button id="addActiveEditorBtn" class="chip-add-btn" title="Attach active editor file" aria-label="Attach active editor">Attach Active File</button>';

        // Re-bind listeners for newly created buttons
        var newAddBtn = document.getElementById('addContextBtn');
        var newActiveBtn = document.getElementById('addActiveEditorBtn');
        if (newAddBtn) {
          newAddBtn.addEventListener('click', function() {
            vscode.postMessage({ command: 'attachContext', source: 'filePicker' });
          });
        }
        if (newActiveBtn) {
          newActiveBtn.addEventListener('click', function() {
            vscode.postMessage({ command: 'attachContext', source: 'activeEditor' });
          });
        }
      }

      function renderMessages() {
        if (state.messages.length === 0) {
          conversationContainer.innerHTML = '<div class="chat-empty-state">' +
            '<h3>Agent Studios Assistant</h3>' +
            '<p>Ask questions, explore code, propose plans, or delegate multi-agent tasks.</p>' +
            '</div>';
          return;
        }

        var html = '';
        for (var i = 0; i < state.messages.length; i++) {
          var msg = state.messages[i];
          if (msg.role === 'user') {
            html += '<div class="message-bubble user">' +
              '<div class="message-meta"><span>You</span><span>' + (msg.mode ? msg.mode.toUpperCase() : '') + '</span></div>' +
              '<div class="message-body">' + escapeText(msg.content) + '</div>';
            if (msg.attachments && msg.attachments.length > 0) {
              html += '<div class="context-chips-bar" style="margin-top: 4px;">';
              for (var a = 0; a < msg.attachments.length; a++) {
                var att = msg.attachments[a];
                html += '<span class="context-chip">📄 ' + escapeText(att.relativePath || att.name || 'file') + '</span>';
              }
              html += '</div>';
            }
            html += '</div>';
          } else {
            // Assistant message
            html += '<div class="message-bubble assistant" id="' + msg.id + '">' +
              '<div class="message-meta"><span>⚡ Agent Studios</span><span>' + (msg.mode ? msg.mode.toUpperCase() : 'AGENT') + '</span></div>';

            // Tool calls if any
            if (msg.toolCalls && msg.toolCalls.length > 0) {
              for (var t = 0; t < msg.toolCalls.length; t++) {
                var tool = msg.toolCalls[t];
                html += '<details class="collapsible">' +
                  '<summary>🔧 ' + escapeText(tool.name) + ' (' + escapeText(tool.status) + ')' + (tool.target ? ' — ' + escapeText(tool.target) : '') + '</summary>' +
                  '<div class="collapsible-content">' +
                  (tool.inputSummary ? '<div><strong>Input:</strong> ' + escapeText(tool.inputSummary) + '</div>' : '') +
                  (tool.outputSummary ? '<div><strong>Output:</strong> ' + escapeText(tool.outputSummary) + '</div>' : '') +
                  (tool.durationMs ? '<div><small>Duration: ' + tool.durationMs + 'ms</small></div>' : '') +
                  '</div></details>';
              }
            }

            html += '<div class="message-body">' + renderMarkdown(msg.content) +
              (msg.isStreaming ? '<span class="streaming-cursor"></span>' : '') +
              '</div>';

            // Approval card if pending
            if (msg.approval) {
              var app = msg.approval;
              var isPending = app.status === 'pending';
              html += '<div class="agent-studios-card approval-card" style="border: 1px solid var(--vscode-editorWarning-foreground, #cca700); padding: 8px; margin-top: 6px; border-radius: 4px;">' +
                '<div style="font-weight: 600; font-size: 11px; margin-bottom: 4px;">[APPROVAL REQUIRED — ' + escapeText(app.riskLevel.toUpperCase()) + ']</div>' +
                '<div style="font-family: var(--font-mono); font-size: 12px; margin-bottom: 6px;">' + escapeText(app.commandOrTarget || app.actionType) + '</div>' +
                (isPending ?
                  '<div style="display: flex; gap: 6px;">' +
                  '<button class="btn-primary btn-sm" data-action="approve" data-approval-id="' + escapeText(app.id) + '">Approve</button>' +
                  '<button class="btn-secondary btn-sm" data-action="deny" data-approval-id="' + escapeText(app.id) + '">Deny</button>' +
                  '</div>' :
                  '<div>Status: ' + escapeText(app.status) + '</div>') +
                '</div>';
            }

            html += '</div>';
          }
        }
        conversationContainer.innerHTML = html;
        scrollToBottom();
      }

      function getUtf8ByteLength(str) {
        if (!str) return 0;
        if (typeof TextEncoder !== 'undefined') {
          return new TextEncoder().encode(str).length;
        }
        return encodeURIComponent(str).replace(/%[A-F\d]{2}/g, 'U').length;
      }

      function checkPromptLimits() {
        var bytes = getUtf8ByteLength(composerInput.value);
        var isOver100KB = bytes > 100 * 1024;
        var isOver4MB = bytes > 4 * 1024 * 1024;

        if (composerWarning) {
          composerWarning.style.display = isOver100KB && !isOver4MB ? 'flex' : 'none';
        }
        if (composerError) {
          composerError.style.display = isOver4MB ? 'flex' : 'none';
        }
        updateButtonStates();
      }

      function updateButtonStates() {
        var bytes = getUtf8ByteLength(composerInput.value);
        var isOver4MB = bytes > 4 * 1024 * 1024;
        var isPromptEmpty = !composerInput.value.trim();
        var isDisconnected = state.connectionState !== 'connected' && !state.isPreview;
        var isRunning = state.executionState && (state.executionState.status === 'submitting' || state.executionState.status === 'running');

        var disableReason = '';
        if (isDisconnected) {
          disableReason = 'Daemon disconnected';
        } else if (!state.isTaskSubmitEnabled) {
          disableReason = 'Task submission disabled by capability gate';
        } else if (!state.isWorkspaceMatched) {
          disableReason = state.workspaceMismatchReason || 'Workspace mismatch with daemon root';
        } else if (isOver4MB) {
          disableReason = 'Prompt exceeds 4 MiB wire frame limit';
        } else if (isRunning) {
          disableReason = 'Task execution in progress';
        } else if (isPromptEmpty) {
          disableReason = 'Enter a prompt or instruction';
        }

        sendBtn.disabled = Boolean(disableReason);
        sendBtn.title = disableReason || 'Start task via host modal confirmation (Ctrl+Enter)';

        if (stopBtn) {
          stopBtn.style.display = isRunning ? 'inline-flex' : 'none';
        }
        if (cancelExecutionBtn) {
          cancelExecutionBtn.style.display = isRunning ? 'inline-flex' : 'none';
        }
      }

      function renderExecutionState(execState) {
        if (!execState) return;
        state.executionState = execState;

        if (executionStreamingPane) {
          if (execState.status && execState.status !== 'idle') {
            executionStreamingPane.style.display = 'flex';
          } else {
            executionStreamingPane.style.display = 'none';
          }
        }

        if (executionStatusBadge) {
          executionStatusBadge.className = 'status-badge status-' + (execState.status || 'idle');
          executionStatusBadge.textContent = (execState.status || 'idle').toUpperCase();
        }

        if (executionTimer) {
          var mins = Math.floor(execState.elapsedSeconds / 60);
          var secs = execState.elapsedSeconds % 60;
          executionTimer.textContent = (mins < 10 ? '0' : '') + mins + ':' + (secs < 10 ? '0' : '') + secs;
        }

        if (activeToolBanner) {
          if (execState.activeTool) {
            activeToolBanner.style.display = 'flex';
            if (activeToolName) activeToolName.textContent = execState.activeTool.name || 'tool';
            if (activeToolStatus) activeToolStatus.textContent = '(' + (execState.activeTool.status || 'executing') + ')';
            if (activeToolError) {
              activeToolError.textContent = execState.activeTool.error ? '— ' + execState.activeTool.error : '';
            }
          } else {
            activeToolBanner.style.display = 'none';
          }
        }

        if (streamingConsoleOutput && execState.streamOutput !== undefined) {
          if (streamingConsoleOutput.textContent !== execState.streamOutput) {
            streamingConsoleOutput.textContent = execState.streamOutput;
            streamingConsoleOutput.scrollTop = streamingConsoleOutput.scrollHeight;
          }
        }

        if (executionMetricsContainer && execState.metrics) {
          var m = execState.metrics;
          var metricsHtml = '';
          if (m.isReported) {
            metricsHtml = '<span class="metric-item">Tokens: ' + (m.totalTokens != null ? m.totalTokens.toLocaleString('en-US') : 'N/A') + '</span>';
            if (m.costUsd != null) {
              metricsHtml += '<span class="metric-item">Cost: $' + m.costUsd.toFixed(4) + '</span>';
            }
            if (m.latencyMs != null) {
              metricsHtml += '<span class="metric-item">Duration: ' + (m.latencyMs / 1000).toFixed(1) + 's</span>';
            }
          } else {
            metricsHtml = '<span class="metrics-unreported" title="Truthful reporting: daemon has not emitted token or cost telemetry for this run.">' +
              'Metrics: Pending daemon telemetry' +
              '</span>';
          }
          executionMetricsContainer.innerHTML = metricsHtml;
        }
      }

      function submitPrompt() {
        var text = composerInput.value.trim();
        if (!text || state.isExecuting) return;

        var bytes = getUtf8ByteLength(composerInput.value);
        if (bytes > 4 * 1024 * 1024) return;
        if (state.connectionState !== 'connected' && !state.isPreview) return;
        if (!state.isWorkspaceMatched) return;
        if (!state.isTaskSubmitEnabled) return;

        state.isExecuting = true;
        sendBtn.disabled = true;
        stopBtn.style.display = 'inline-flex';

        // Add user message locally
        var userMsg = {
          id: 'user-' + Date.now(),
          role: 'user',
          content: text,
          mode: state.mode,
          attachments: state.attachments.slice()
        };
        state.messages.push(userMsg);

        // Add streaming assistant message skeleton
        var asstId = 'asst-' + Date.now();
        var asstMsg = {
          id: asstId,
          role: 'assistant',
          content: '',
          mode: state.mode,
          isStreaming: true,
          toolCalls: []
        };
        state.messages.push(asstMsg);

        renderMessages();

        // Clear input and draft
        composerInput.value = '';
        state.composerDraft = '';
        autoGrowTextarea();
        checkPromptLimits();
        vscode.postMessage({ command: 'saveDraft', draft: '' });

        // If in agent mode, submit task to daemon with confirmation modal
        if (state.mode === 'agent') {
          vscode.postMessage({
            command: 'submitTask',
            prompt: text,
            runtime: state.runtime,
            studioId: state.studioId,
            modelId: state.selectedModelId,
            providerId: state.selectedProviderId
          });
        } else {
          // Dispatch sendMessage to provider
          vscode.postMessage({
            command: 'sendMessage',
            text: text,
            mode: state.mode,
            modelId: state.selectedModelId,
            providerId: state.selectedProviderId,
            attachments: state.attachments
          });
        }
      }

      function handleCancel() {
        if (state.mode === 'agent' && state.executionState && state.executionState.activeTaskId) {
          vscode.postMessage({
            command: 'cancelTask',
            taskId: state.executionState.activeTaskId,
            studioId: state.studioId
          });
        } else {
          vscode.postMessage({ command: 'cancelRun' });
        }
      }

      // Event Listeners
      composerInput.addEventListener('keydown', function(e) {
        if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
          e.preventDefault();
          submitPrompt();
        } else if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          submitPrompt();
        } else if (e.key === 'Escape' && state.isExecuting) {
          e.preventDefault();
          cancelExecution();
        }
      });

      window.addEventListener('keydown', function(e) {
        if (e.key === 'Escape' && state.isExecuting) {
          e.preventDefault();
          cancelExecution();
        }
      });

      composerInput.addEventListener('input', function() {
        autoGrowTextarea();
        state.composerDraft = composerInput.value;
        checkPromptLimits();
        vscode.postMessage({ command: 'saveDraft', draft: composerInput.value });
      });

      sendBtn.addEventListener('click', function() {
        submitPrompt();
      });

      stopBtn.addEventListener('click', handleCancel);
      if (cancelExecutionBtn) {
        cancelExecutionBtn.addEventListener('click', handleCancel);
      }
      if (clearStreamBtn) {
        clearStreamBtn.addEventListener('click', function() {
          if (streamingConsoleOutput) {
            streamingConsoleOutput.textContent = '';
          }
        });
      }

      if (runtimeSelect) {
        runtimeSelect.addEventListener('change', function() {
          state.runtime = runtimeSelect.value;
          if (summaryRuntime) {
            summaryRuntime.textContent = runtimeSelect.options[runtimeSelect.selectedIndex].text;
          }
        });
      }

      if (studioSelect) {
        studioSelect.addEventListener('change', function() {
          state.studioId = studioSelect.value;
        });
      }

      modeSelect.addEventListener('change', function() {
        state.mode = modeSelect.value;
        vscode.postMessage({ command: 'switchMode', mode: state.mode });
      });

      modelSelect.addEventListener('change', function() {
        var parts = modelSelect.value.split('::');
        state.selectedProviderId = parts[0] || '';
        state.selectedModelId = parts[1] || modelSelect.value;
        vscode.postMessage({
          command: 'switchProviderModel',
          providerId: state.selectedProviderId,
          modelId: state.selectedModelId
        });
      });

      sessionHistorySelect.addEventListener('change', function() {
        if (sessionHistorySelect.value) {
          vscode.postMessage({ command: 'switchSession', sessionId: sessionHistorySelect.value });
        }
      });

      newSessionBtn.addEventListener('click', function() {
        vscode.postMessage({ command: 'newSession' });
      });

      // Context chips remove delegation
      contextChipsBar.addEventListener('click', function(e) {
        var removeBtn = e.target.closest('.chip-remove-btn');
        if (removeBtn) {
          var removePath = removeBtn.getAttribute('data-remove-path');
          state.attachments = state.attachments.filter(function(a) {
            return (a.uri || a.path || a.relativePath) !== removePath;
          });
          renderContextChips();
          vscode.postMessage({ command: 'removeAttachment', path: removePath });
        }
      });

      // Card and welcome surface action clicks delegation
      conversationContainer.addEventListener('click', function(e) {
        var approveBtn = e.target.closest('[data-action="approve"]');
        if (approveBtn) {
          var approvalId = approveBtn.getAttribute('data-approval-id');
          approveBtn.disabled = true;
          vscode.postMessage({ command: 'approveAction', approvalId: approvalId });
          return;
        }

        var denyBtn = e.target.closest('[data-action="deny"]');
        if (denyBtn) {
          var approvalId = denyBtn.getAttribute('data-approval-id');
          denyBtn.disabled = true;
          vscode.postMessage({ command: 'denyAction', approvalId: approvalId });
          return;
        }

        var diffBtn = e.target.closest('[data-action="open-diff"]');
        if (diffBtn) {
          var artifactId = diffBtn.getAttribute('data-artifact-id');
          var filePath = diffBtn.getAttribute('data-file-path');
          vscode.postMessage({ command: 'openDiff', artifactId: artifactId, filePath: filePath });
          return;
        }

        var tgBtn = e.target.closest('[data-action="open-task-graph"]');
        if (tgBtn) {
          vscode.postMessage({ command: 'openTaskGraph' });
          return;
        }

        var openIdeBtn = e.target.closest('[data-action="open-in-ide"]');
        if (openIdeBtn) {
          vscode.postMessage({ command: 'openInIde' });
          return;
        }

        var openMcBtn = e.target.closest('[data-action="open-mission-control"]');
        if (openMcBtn) {
          vscode.postMessage({ command: 'openMissionControl' });
          return;
        }

        var contSessBtn = e.target.closest('[data-action="continue-session"]');
        if (contSessBtn) {
          var sid = contSessBtn.getAttribute('data-session-id') || '';
          vscode.postMessage({ command: 'continueSession', sessionId: sid });
          return;
        }

        var promptSugBtn = e.target.closest('[data-action="prompt-suggestion"]');
        if (promptSugBtn) {
          var sugText = promptSugBtn.getAttribute('data-prompt') || '';
          if (composerInput) {
            composerInput.value = sugText;
            autoGrowTextarea();
            composerInput.focus();
          }
          return;
        }
      });

      if (executionStreamingPane) {
        executionStreamingPane.addEventListener('click', function(e) {
          var openMcBtn = e.target.closest('[data-action="open-mission-control"]');
          if (openMcBtn) {
            vscode.postMessage({ command: 'openMissionControl' });
          }
        });
      }

      // Incoming message listener from VS Code Extension Host
      window.addEventListener('message', function(event) {
        var msg = event.data;
        if (!msg) return;

        switch (msg.type) {
          case 'streamChunk':
            if (typeof msg.chunk === 'string') {
              if (streamingConsoleOutput) {
                streamingConsoleOutput.textContent += msg.chunk;
                streamingConsoleOutput.scrollTop = streamingConsoleOutput.scrollHeight;
              }
              if (executionStreamingPane && executionStreamingPane.style.display === 'none') {
                executionStreamingPane.style.display = 'flex';
              }
            }
            var lastMsg = state.messages[state.messages.length - 1];
            if (lastMsg && lastMsg.role === 'assistant') {
              if (msg.chunk && typeof msg.chunk === 'object') {
                if (msg.chunk.type === 'text_delta' && msg.chunk.text) {
                  lastMsg.content += msg.chunk.text;
                } else if (msg.chunk.type === 'tool_call' && msg.chunk.toolCall) {
                  lastMsg.toolCalls = lastMsg.toolCalls || [];
                  var existingIdx = lastMsg.toolCalls.findIndex(function(t) { return t.id === msg.chunk.toolCall.id; });
                  if (existingIdx >= 0) {
                    lastMsg.toolCalls[existingIdx] = msg.chunk.toolCall;
                  } else {
                    lastMsg.toolCalls.push(msg.chunk.toolCall);
                  }
                } else if (msg.chunk.type === 'approval_required' && msg.chunk.approval) {
                  lastMsg.approval = msg.chunk.approval;
                }
              } else if (msg.text) {
                lastMsg.content += msg.text;
              }
              renderMessages();
            }
            break;

          case 'executionStateChanged':
            if (msg.executionState) {
              renderExecutionState(msg.executionState);
              updateButtonStates();
            }
            break;

          case 'taskSubmitted':
            if (executionStreamingPane) {
              executionStreamingPane.style.display = 'flex';
            }
            if (msg.taskId && state.executionState) {
              state.executionState.activeTaskId = msg.taskId;
            }
            updateButtonStates();
            break;

          case 'taskCancelled':
          case 'executionCancelled':
            state.isExecuting = false;
            updateButtonStates();
            renderMessages();
            break;

          case 'messageComplete':
            var last = state.messages[state.messages.length - 1];
            if (last && last.role === 'assistant') {
              last.isStreaming = false;
              if (msg.message && msg.message.content) {
                last.content = msg.message.content;
              }
            }
            state.isExecuting = false;
            sendBtn.disabled = false;
            stopBtn.style.display = 'none';
            renderMessages();
            break;

          case 'runCancelled':
            var cancellingMsg = state.messages[state.messages.length - 1];
            if (cancellingMsg && cancellingMsg.role === 'assistant') {
              cancellingMsg.isStreaming = false;
              cancellingMsg.content += '\\n\\n[Execution cancelled by user]';
            }
            state.isExecuting = false;
            sendBtn.disabled = false;
            stopBtn.style.display = 'none';
            renderMessages();
            break;

          case 'error':
            var errorMsg = state.messages[state.messages.length - 1];
            if (errorMsg && errorMsg.role === 'assistant') {
              errorMsg.isStreaming = false;
              errorMsg.content += '\\n\\n⚠️ **Error:** ' + (msg.error || 'Operation failed');
            }
            state.isExecuting = false;
            sendBtn.disabled = false;
            stopBtn.style.display = 'none';
            renderMessages();
            break;

          case 'approvalResult':
            for (var m = 0; m < state.messages.length; m++) {
              if (state.messages[m].approval && state.messages[m].approval.id === msg.approvalId) {
                state.messages[m].approval.status = msg.decision;
                break;
              }
            }
            renderMessages();
            break;

          case 'contextAttached':
            if (msg.attachments) {
              state.attachments = msg.attachments;
            } else if (msg.attachment) {
              state.attachments.push(msg.attachment);
            }
            renderContextChips();
            break;

          case 'modeChanged':
            state.mode = msg.mode;
            modeSelect.value = msg.mode;
            break;

          case 'restoreDraft':
            if (msg.draft !== undefined) {
              composerInput.value = msg.draft;
              state.composerDraft = msg.draft;
              autoGrowTextarea();
            }
            break;

          case 'syncState':
            if (msg.state) {
              if (msg.state.session) {
                state.session = msg.state.session;
                if (currentSessionName) {
                  currentSessionName.textContent = state.session.goalSummary || state.session.id;
                  currentSessionName.title = currentSessionName.textContent;
                }
              }
              if (msg.state.connectionState) {
                state.connectionState = msg.state.connectionState;
                var dot = connectionStatusIndicator.querySelector('.status-dot');
                var textSpan = document.getElementById('connectionStatusText');
                if (dot) {
                  dot.className = 'status-dot status-' + (state.connectionState === 'connected' ? 'available' : state.connectionState === 'connecting' ? 'unconfigured' : 'offline');
                }
                if (textSpan) {
                  textSpan.textContent = state.connectionState === 'connected' ? 'Connected' : state.connectionState === 'connecting' ? 'Connecting...' : 'Disconnected';
                }
              }
              if (msg.state.mode) {
                state.mode = msg.state.mode;
                modeSelect.value = state.mode;
              }
              if (msg.state.composerDraft !== undefined) {
                composerInput.value = msg.state.composerDraft;
                state.composerDraft = msg.state.composerDraft;
                autoGrowTextarea();
              }
              if (msg.state.messages) {
                state.messages = msg.state.messages;
                renderMessages();
              }
              if (msg.state.attachments) {
                state.attachments = msg.state.attachments;
                renderContextChips();
              }
              if (msg.state.executionState) {
                renderExecutionState(msg.state.executionState);
              }
              if (msg.state.workspaceStatus !== undefined || msg.state.isWorkspaceMatched !== undefined) {
                var status = msg.state.workspaceStatus || (msg.state.isWorkspaceMatched ? 'AUTHORIZED' : 'MISMATCH');
                state.workspaceStatus = status;
                state.isWorkspaceMatched = status === 'AUTHORIZED';
                var badge = document.getElementById('workspaceBadge');
                var label = document.getElementById('workspaceBadgeLabel');
                if (badge && label) {
                  badge.className = 'workspace-badge workspace-badge--' + status.toLowerCase();
                  label.textContent = status === 'AUTHORIZED' ? '✓ Authorized' : (status === 'MISMATCH' ? '⚠ Mismatch' : (status === 'UNVERIFIED' ? '⚠ Unverified' : '⚠ Unavailable'));
                }
              }
              if (msg.state.workspaceMismatchReason !== undefined) {
                state.workspaceMismatchReason = msg.state.workspaceMismatchReason;
                var badgeEl = document.getElementById('workspaceBadge');
                if (badgeEl && state.workspaceMismatchReason) {
                  badgeEl.title = state.workspaceMismatchReason;
                }
              }
              if (msg.state.runtimeOptions && Array.isArray(msg.state.runtimeOptions) && runtimeSelect) {
                runtimeSelect.innerHTML = msg.state.runtimeOptions.map(function(opt) {
                  var selected = opt.id === state.runtime ? ' selected' : '';
                  var disabled = !opt.enabled ? ' disabled' : '';
                  var title = opt.tooltip ? ' title="' + opt.tooltip.replace(/"/g, '&quot;') + '"' : '';
                  return '<option value="' + opt.id.replace(/"/g, '&quot;') + '"' + selected + disabled + title + '>' + opt.name.replace(/</g, '&lt;').replace(/>/g, '&gt;') + '</option>';
                }).join('');
              }
              if (msg.state.isTaskSubmitEnabled !== undefined) {
                state.isTaskSubmitEnabled = msg.state.isTaskSubmitEnabled;
              }
              updateButtonStates();
            }
            break;
        }
      });

      // Window helper actions for onboarding cards
      window.enablePreviewMode = function() {
        vscode.postMessage({ command: 'enablePreviewMode' });
      };
      window.connectToDaemon = function() {
        vscode.postMessage({ command: 'connectDaemon' });
      };
      window.dismissOnboarding = function() {
        vscode.postMessage({ command: 'dismissOnboarding' });
        var banner = document.getElementById('onboardingBanner');
        if (banner) { banner.remove(); }
      };
      window.openTaskGraph = function() {
        vscode.postMessage({ command: 'openTaskGraph' });
      };

      // Initial layout setup
      renderContextChips();
      renderMessages();
      autoGrowTextarea();
      checkPromptLimits();
      if (state.executionState) {
        renderExecutionState(state.executionState);
      }
      updateButtonStates();
    })();
  </script>
</body>
</html>`;
  }

  public static renderEmptyState(initialState?: ChatHtmlInitialState): string {
    const workspacePath = initialState?.workspacePath || '';
    const workspaceName = workspacePath ? workspacePath.split(/[\\/]/).filter(Boolean).pop() || workspacePath : 'No workspace open';
    const connectionState = initialState?.connectionState || 'disconnected';
    const isConnected = connectionState === 'connected';
    const isPreview = Boolean(initialState?.isPreview);
    const session = initialState?.session;
    const sessions = initialState?.sessions || [];
    const firstSessionId = session?.id || (sessions.length > 0 ? sessions[0].id : undefined);

    const goalStarters = [
      'Audit repository architecture and verify dependency graph',
      'Diagnose failing integration tests and formulate surgical fix',
      'Explore git branch changes and summarize active worktree diff',
      'Prepare release verification report and quality checklist',
    ];

    const startersHtml = goalStarters.map(starter => `
      <button type="button" class="starter-chip" data-action="prompt-suggestion" data-prompt="${escapeHtml(starter)}" aria-label="${escapeHtml(starter)}">
        <span class="starter-icon" aria-hidden="true">→</span>
        <span class="starter-text">${escapeHtml(starter)}</span>
      </button>
    `.trim()).join('');

    const recentSessionsHtml = firstSessionId ? `
      <div class="welcome-card welcome-sessions-card">
        <div class="welcome-card-header">
          <span class="welcome-card-title">Session Continuity</span>
          <span class="status-pill status-pill--active">Saved</span>
        </div>
        <div class="welcome-card-body">
          <div class="welcome-detail-row">
            <span class="detail-label">Active Session:</span>
            <code class="detail-code" title="${escapeHtml(firstSessionId)}">${escapeHtml(firstSessionId.substring(0, 16))}...</code>
          </div>
          <div class="welcome-actions">
            <button type="button" class="btn-primary btn-xs" data-action="continue-session" data-session-id="${escapeHtml(firstSessionId)}" title="Restore session context and history">
              Restore Session
            </button>
          </div>
        </div>
      </div>
    `.trim() : '';

    return `
      <div class="agent-welcome-surface" role="region" aria-label="Agent Studios Welcome">
        <div class="welcome-hero">
          <div class="welcome-icon-box" aria-hidden="true">⚡</div>
          <h2 class="welcome-title">Agent Studios</h2>
          <p class="welcome-subtitle">Agent-First AI Engineering Workbench ${isPreview ? '<span class="preview-tag">[Preview Fixture]</span>' : ''}</p>
        </div>

        <div class="welcome-grid">
          <!-- Active Workspace Card -->
          <div class="welcome-card">
            <div class="welcome-card-header">
              <span class="welcome-card-title">Active Workspace</span>
              <span class="status-pill ${workspacePath ? 'status-pill--active' : 'status-pill--warn'}">
                ${workspacePath ? 'Bound' : 'None'}
              </span>
            </div>
            <div class="welcome-card-body">
              <div class="welcome-detail-row">
                <span class="detail-label">Location:</span>
                <span class="detail-val" title="${escapeHtml(workspacePath)}">${escapeHtml(workspaceName)}</span>
              </div>
              <div class="welcome-actions">
                <button type="button" class="btn-secondary btn-xs" data-action="open-in-ide" title="Open workspace in Code-OSS editor workbench">
                  Open in IDE Mode
                </button>
                <button type="button" class="btn-secondary btn-xs" data-action="open-mission-control" title="Inspect execution DAG and multi-agent fleet in Mission Control">
                  Mission Control
                </button>
              </div>
            </div>
          </div>

          <!-- Daemon & Runtime Status Card -->
          <div class="welcome-card">
            <div class="welcome-card-header">
              <span class="welcome-card-title">Daemon & Runtime</span>
              <span class="status-pill ${isConnected ? 'status-pill--active' : 'status-pill--warn'}">
                ${escapeHtml(connectionState)}
              </span>
            </div>
            <div class="welcome-card-body">
              <div class="welcome-detail-row">
                <span class="detail-label">Runtime Engine:</span>
                <span class="detail-val">${escapeHtml(initialState?.selectedRuntime || 'Native Daemon (agsd)')}</span>
              </div>
              <div class="welcome-notice" title="Provider discovery contract lands in backend M15.1">
                <strong>Model Catalog:</strong> Dynamic Provider Discovery pending M15.1. Static adapters: Claude Code, Internal Codex, Codex.
              </div>
            </div>
          </div>

          <!-- Session Continuity Card (if session exists) -->
          ${recentSessionsHtml}
        </div>

        <!-- Goal Formulation Starters -->
        <div class="welcome-starters-section">
          <div class="starters-label">Goal Formulation Starters</div>
          <div class="starters-grid">
            ${startersHtml}
          </div>
        </div>
      </div>
    `.trim();
  }

  public static renderMetricsHtml(metrics?: ExecutionMetrics): string {
    if (!metrics || !metrics.isReported) {
      return `
        <span class="metrics-unreported" title="Truthful reporting: daemon has not emitted token or cost telemetry for this run.">
          Metrics: Pending daemon telemetry
        </span>`.trim();
    }
    const items: string[] = [];
    if (metrics.totalTokens !== undefined) {
      items.push(`<span class="metric-item">Tokens: ${metrics.totalTokens.toLocaleString('en-US')}</span>`);
    }
    if (metrics.costUsd !== undefined) {
      items.push(`<span class="metric-item">Cost: $${metrics.costUsd.toFixed(4)}</span>`);
    }
    if (metrics.latencyMs !== undefined) {
      items.push(`<span class="metric-item">Duration: ${(metrics.latencyMs / 1000).toFixed(1)}s</span>`);
    }
    return items.join(' ');
  }

  public static renderModelOptions(
    providers: ProviderSummary[],
    selectedProviderId?: string,
    selectedModelId?: string
  ): string {
    if (providers.length === 0) {
      return '<option value="">No AI models available</option>';
    }

    return providers
      .map((p) => {
        const models = p.models
          .map((m) => {
            const key = `${p.instanceId}::${m.id}`;
            const isSelected =
              (p.instanceId === selectedProviderId && m.id === selectedModelId) ||
              (!selectedProviderId && m.isDefault);
            return `<option value="${escapeHtml(key)}" ${isSelected ? 'selected' : ''}>${escapeHtml(m.name)} (${escapeHtml(p.displayName)})</option>`;
          })
          .join('\n');
        return `<optgroup label="${escapeHtml(p.displayName)} (${escapeHtml(p.status)})">${models}</optgroup>`;
      })
      .join('\n');
  }

  public static renderSessionOptions(
    sessions: SessionSummary[],
    currentSessionId?: string
  ): string {
    if (sessions.length === 0) {
      return '<option value="">Session History</option>';
    }

    return sessions
      .map((s) => {
        const isSelected = s.id === currentSessionId;
        const title = s.title.length > 28 ? s.title.substring(0, 25) + '...' : s.title;
        return `<option value="${escapeHtml(s.id)}" ${isSelected ? 'selected' : ''}>${escapeHtml(title)}</option>`;
      })
      .join('\n');
  }

  public static renderRuntimeOptions(
    runtimes?: RuntimeOption[],
    selectedRuntime?: string
  ): string {
    const list = runtimes && runtimes.length > 0
      ? runtimes
      : [
          { id: '', name: 'No runtimes available (daemon unready)', enabled: false, tooltip: 'Daemon execution readiness report is unavailable or degraded.' },
        ];

    return list
      .map((r) => {
        const isSelected = Boolean(
          selectedRuntime &&
          (r.id === selectedRuntime ||
            r.instanceId === selectedRuntime ||
            r.id.startsWith(selectedRuntime + '|') ||
            r.id.endsWith('|' + selectedRuntime))
        );
        const parts = [`value="${escapeHtml(r.id)}"`];
        if (isSelected) {
          parts.push('selected');
        }
        if (!r.enabled) {
          parts.push('disabled');
        }
        if (r.tooltip) {
          parts.push(`title="${escapeHtml(r.tooltip)}"`);
        }
        return `<option ${parts.join(' ')}>${escapeHtml(r.name)}</option>`;
      })
      .join('\n');
  }
}
