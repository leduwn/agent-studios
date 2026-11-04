/**
 * Agent Studios — Deep Session Detail Webview Panel
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { SessionInfo, SessionStatus, TaskNode } from '../bridge/types';
import { escapeHtml, renderSafeMarkdown } from './markdownRenderer';
import { TaskGraphWebview } from './TaskGraphWebview';

export interface AgentRosterMember {
  id: string;
  name: string;
  role?: string;
  currentTaskId?: string;
  currentTaskTitle?: string;
  status: 'active' | 'busy' | 'idle' | 'offline';
}

export class SessionDetailWebview implements vscode.Disposable {
  public static readonly viewType = 'agent-studios.sessionDetail';
  public static currentPanel: SessionDetailWebview | undefined;

  private readonly _panel: vscode.WebviewPanel;
  private readonly _extensionUri: vscode.Uri;
  private _disposables: vscode.Disposable[] = [];
  private _sessionInfo: SessionInfo | null = null;
  private _tasks: TaskNode[] = [];
  private _agents: AgentRosterMember[] = [];
  private _bridge?: AgentStudiosBridge;
  private _isDisposing = false;

  public get extensionUri(): vscode.Uri {
    return this._extensionUri;
  }

  public get currentSession(): SessionInfo | null {
    return this._sessionInfo;
  }

  /**
   * Creates or reveals the Session Detail webview panel.
   */
  public static createOrShow(
    extensionUri: vscode.Uri,
    sessionInfo?: SessionInfo | null,
    tasks?: TaskNode[],
    agents?: AgentRosterMember[],
    bridge?: AgentStudiosBridge
  ): SessionDetailWebview {
    const column = vscode.window.activeTextEditor
      ? vscode.window.activeTextEditor.viewColumn
      : undefined;

    if (SessionDetailWebview.currentPanel) {
      SessionDetailWebview.currentPanel._panel.reveal(column || vscode.ViewColumn.One);
      if (sessionInfo !== undefined) {
        SessionDetailWebview.currentPanel.setData(sessionInfo, tasks, agents);
      }
      return SessionDetailWebview.currentPanel;
    }

    const panel = vscode.window.createWebviewPanel(
      SessionDetailWebview.viewType,
      'Agent Studios: Session Overview',
      column || vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [extensionUri],
      }
    );

    SessionDetailWebview.currentPanel = new SessionDetailWebview(
      panel,
      extensionUri,
      sessionInfo,
      tasks,
      agents,
      bridge
    );
    return SessionDetailWebview.currentPanel;
  }

  private constructor(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    sessionInfo?: SessionInfo | null,
    tasks?: TaskNode[],
    agents?: AgentRosterMember[],
    bridge?: AgentStudiosBridge
  ) {
    this._panel = panel;
    this._extensionUri = extensionUri;
    this._sessionInfo = sessionInfo || null;
    this._tasks = tasks || [];
    this._agents = agents || [];
    this._bridge = bridge;

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    this._panel.webview.onDidReceiveMessage(
      (message) => this._handleWebviewMessage(message),
      null,
      this._disposables
    );

    if (this._bridge) {
      this._bridge.onEvent(
        (event) => {
          if (
            event.type.startsWith('session_') ||
            event.type.startsWith('run_') ||
            event.type.startsWith('task_') ||
            event.type === 'agent_assigned'
          ) {
            this.refreshFromBridge();
          }
        },
        null,
        this._disposables
      );
    }

    this._update();
  }

  /**
   * Updates the session and task data displayed in the webview.
   */
  public setData(
    sessionInfo: SessionInfo | null,
    tasks?: TaskNode[],
    agents?: AgentRosterMember[]
  ): void {
    this._sessionInfo = sessionInfo;
    if (tasks !== undefined) {
      this._tasks = tasks;
    }
    if (agents !== undefined) {
      this._agents = agents;
    }
    this._update();
  }

  /**
   * Refreshes the session data directly from the connected bridge.
   */
  public async refreshFromBridge(): Promise<void> {
    if (!this._bridge) {
      return;
    }
    try {
      const session = await this._bridge.getCurrentSession();
      const tasks = await this._bridge.listTasks();
      this.setData(session, tasks);
    } catch {
      // Bridge disconnected; maintain current view
    }
  }

  public dispose(): void {
    if (this._isDisposing) {
      return;
    }
    this._isDisposing = true;
    SessionDetailWebview.currentPanel = undefined;
    this._panel.dispose();

    while (this._disposables.length) {
      const d = this._disposables.pop();
      if (d) {
        d.dispose();
      }
    }
  }

  private _handleWebviewMessage(message: { command: string; taskId?: string }): void {
    switch (message.command) {
      case 'refresh':
        this.refreshFromBridge();
        break;
      case 'openTaskGraph':
        TaskGraphWebview.createOrShow(this._extensionUri, null, this._bridge);
        break;
      case 'selectTask':
        if (message.taskId) {
          const taskGraph = TaskGraphWebview.createOrShow(this._extensionUri, null, this._bridge);
          taskGraph.selectTask(message.taskId);
        }
        break;
    }
  }

  private _update(): void {
    const webview = this._panel.webview;
    this._panel.webview.html = this._getHtmlForWebview(webview);
  }

  private _getHtmlForWebview(webview: vscode.Webview): string {
    const nonce = crypto.randomBytes(16).toString('base64');
    const session = this._sessionInfo;

    // Derive metrics
    const totalTasks = session?.totalTasksCount ?? this._tasks.length;
    const completedTasks =
      session?.completedTasksCount ??
      this._tasks.filter((t) => t.status === 'succeeded').length;
    const runningTasks = this._tasks.filter((t) => t.status === 'running').length;
    const blockedTasks = this._tasks.filter((t) => t.status === 'blocked').length;
    const failedTasks = this._tasks.filter((t) => t.status === 'failed').length;

    const progressPct =
      totalTasks > 0 ? Math.round((completedTasks / totalTasks) * 100) : 0;

    // Derive agent roster if not explicitly supplied
    const agentRoster = this._resolveAgentRoster();

    // Goal markdown
    const goalHtml = renderSafeMarkdown(
      session?.goalSummary || 'No session goal summary specified.'
    );

    const isPreview = Boolean(session?.isPreview);
    const elapsedMs = session?.elapsedMs || 0;
    const sessionStatus: SessionStatus = session?.status || 'idle';

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${webview.cspSource};">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios: Session Overview</title>
  <style>
    :root {
      --bg: var(--vscode-editor-background);
      --fg: var(--vscode-editor-foreground);
      --card-bg: var(--vscode-sideBar-background, #252526);
      --border-color: var(--vscode-widget-border, var(--vscode-panel-border, #3c3c3c));
      --focus-border: var(--vscode-focusBorder, #007fd4);
      --code-bg: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.25));

      /* VS Code theme tokens */
      --status-succeeded: var(--vscode-charts-green, #89d185);
      --status-failed: var(--vscode-charts-red, #f14c4c);
      --status-running: var(--vscode-charts-yellow, #cca700);
      --status-blocked: var(--vscode-charts-orange, #d18616);
      --status-planning: var(--vscode-charts-blue, #3794ff);
      --status-idle: var(--vscode-descriptionForeground, #888888);
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      background-color: var(--bg);
      color: var(--fg);
      font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif);
      font-size: var(--vscode-font-size, 13px);
      line-height: 1.5;
      padding: 0;
      margin: 0;
      overflow-y: auto;
    }

    /* Watermark / Preview mode banner */
    .preview-banner {
      background: rgba(209, 134, 22, 0.15);
      border-bottom: 1px solid var(--status-blocked);
      color: var(--fg);
      padding: 8px 24px;
      display: flex;
      align-items: center;
      gap: 12px;
      font-size: 12px;
      position: sticky;
      top: 0;
      z-index: 100;
      backdrop-filter: blur(4px);
    }

    .preview-badge {
      background: var(--status-blocked);
      color: #000;
      font-weight: 700;
      font-size: 10px;
      padding: 2px 6px;
      border-radius: 3px;
      letter-spacing: 0.5px;
    }

    .container {
      max-width: 1080px;
      margin: 0 auto;
      padding: 24px 32px 48px 32px;
      display: flex;
      flex-direction: column;
      gap: 24px;
    }

    /* Header Section */
    .header-bar {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      border-bottom: 1px solid var(--border-color);
      padding-bottom: 16px;
    }

    .header-title-area {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .header-title-row {
      display: flex;
      align-items: center;
      gap: 12px;
    }

    .title-text {
      font-size: 18px;
      font-weight: 600;
      letter-spacing: -0.2px;
    }

    .session-badge {
      font-size: 11px;
      font-weight: 600;
      padding: 3px 8px;
      border-radius: 4px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      border: 1px solid transparent;
    }

    .badge-status-running {
      color: var(--status-running);
      background: rgba(204, 167, 0, 0.15);
      border-color: var(--status-running);
    }

    .badge-status-completed {
      color: var(--status-succeeded);
      background: rgba(137, 209, 133, 0.15);
      border-color: var(--status-succeeded);
    }

    .badge-status-failed {
      color: var(--status-failed);
      background: rgba(241, 76, 76, 0.15);
      border-color: var(--status-failed);
    }

    .badge-status-planning {
      color: var(--status-planning);
      background: rgba(55, 148, 255, 0.15);
      border-color: var(--status-planning);
    }

    .badge-status-paused {
      color: var(--status-blocked);
      background: rgba(209, 134, 22, 0.15);
      border-color: var(--status-blocked);
    }

    .badge-status-idle {
      color: var(--status-idle);
      background: rgba(128, 128, 128, 0.15);
      border-color: var(--status-idle);
    }

    .meta-row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 16px;
      color: var(--vscode-descriptionForeground, #999);
      font-size: 12px;
    }

    .meta-item {
      display: inline-flex;
      align-items: center;
      gap: 5px;
    }

    .mono-code {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 11px;
      background: var(--code-bg);
      padding: 2px 6px;
      border-radius: 3px;
    }

    .header-actions {
      display: flex;
      gap: 8px;
    }

    .btn-action {
      background: var(--vscode-button-background, #0e639c);
      color: var(--vscode-button-foreground, #fff);
      border: 1px solid transparent;
      padding: 6px 14px;
      border-radius: 4px;
      font-size: 12px;
      font-weight: 500;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      transition: background 0.15s;
    }

    .btn-action:hover {
      background: var(--vscode-button-hoverBackground, #1177bb);
    }

    .btn-secondary {
      background: transparent;
      border: 1px solid var(--border-color);
      color: var(--fg);
    }

    .btn-secondary:hover {
      background: var(--vscode-toolbar-hoverBackground, rgba(90, 93, 94, 0.31));
      border-color: var(--focus-border);
    }

    /* Metric Cards Grid */
    .metrics-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 14px;
    }

    .metric-card {
      background: var(--card-bg);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 14px 16px;
      display: flex;
      flex-direction: column;
      gap: 4px;
    }

    .metric-label {
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      color: var(--vscode-descriptionForeground, #999);
      font-weight: 600;
    }

    .metric-value {
      font-size: 24px;
      font-weight: 600;
      letter-spacing: -0.5px;
      color: var(--fg);
    }

    .metric-sub {
      font-size: 11px;
      color: var(--vscode-descriptionForeground, #888);
    }

    .text-green { color: var(--status-succeeded); }
    .text-yellow { color: var(--status-running); }
    .text-orange { color: var(--status-blocked); }
    .text-red { color: var(--status-failed); }

    /* Progress Bar */
    .progress-section {
      background: var(--card-bg);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 14px 16px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }

    .progress-bar-track {
      width: 100%;
      height: 6px;
      background: rgba(255, 255, 255, 0.08);
      border-radius: 3px;
      overflow: hidden;
    }

    .progress-bar-fill {
      height: 100%;
      background: var(--status-succeeded);
      transition: width 0.3s ease;
    }

    /* Card Panels */
    .panel-card {
      background: var(--card-bg);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 14px;
    }

    .panel-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      border-bottom: 1px solid rgba(255, 255, 255, 0.06);
      padding-bottom: 10px;
    }

    .panel-title {
      font-size: 14px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    /* Goal Markdown Body */
    .markdown-content {
      font-size: 13px;
      line-height: 1.6;
    }

    .markdown-content h1,
    .markdown-content h2,
    .markdown-content h3 {
      margin-top: 14px;
      margin-bottom: 8px;
      font-weight: 600;
    }

    .markdown-content h1 { font-size: 16px; }
    .markdown-content h2 { font-size: 14px; }
    .markdown-content h3 { font-size: 13px; }

    .markdown-content p {
      margin-bottom: 10px;
    }

    .markdown-content ul,
    .markdown-content ol {
      margin-left: 20px;
      margin-bottom: 10px;
    }

    .markdown-content li {
      margin-bottom: 4px;
    }

    .markdown-content blockquote {
      border-left: 3px solid var(--focus-border);
      padding-left: 12px;
      color: var(--vscode-descriptionForeground, #aaa);
      margin: 10px 0;
      font-style: italic;
    }

    .markdown-content pre.code-block {
      background: var(--code-bg);
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 10px 12px;
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 12px;
      overflow-x: auto;
      margin: 10px 0;
    }

    .markdown-content code.inline-code {
      background: var(--code-bg);
      padding: 2px 4px;
      border-radius: 3px;
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 12px;
    }

    .markdown-content a.external-link {
      color: var(--vscode-textLink-foreground, #3794ff);
      text-decoration: none;
    }

    .markdown-content a.external-link:hover {
      text-decoration: underline;
    }

    /* Agent Roster Table/Cards */
    .roster-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
      gap: 12px;
    }

    .agent-card {
      background: var(--bg);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 12px 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
      transition: border-color 0.15s;
    }

    .agent-card:hover {
      border-color: var(--focus-border);
    }

    .agent-card-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .agent-identity {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .agent-avatar {
      width: 24px;
      height: 24px;
      border-radius: 50%;
      background: var(--vscode-badge-background, #4d4d4d);
      color: var(--vscode-badge-foreground, #fff);
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 12px;
    }

    .agent-name {
      font-weight: 600;
      font-size: 12px;
    }

    .agent-status-indicator {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      font-size: 11px;
    }

    .status-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
    }

    .dot-busy { background: var(--status-running); }
    .dot-active { background: var(--status-succeeded); }
    .dot-idle { background: var(--status-idle); }
    .dot-offline { background: var(--status-failed); }

    .agent-task-info {
      font-size: 11px;
      color: var(--vscode-descriptionForeground, #999);
      display: flex;
      flex-direction: column;
      gap: 2px;
    }

    .agent-task-link {
      color: var(--vscode-textLink-foreground, #3794ff);
      cursor: pointer;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .agent-task-link:hover {
      text-decoration: underline;
    }

    .role-badge {
      background: rgba(255, 255, 255, 0.08);
      font-size: 10px;
      padding: 1px 6px;
      border-radius: 3px;
      color: var(--vscode-descriptionForeground, #ccc);
    }
  </style>
</head>
<body>
  ${
    isPreview
      ? `<div class="preview-banner">
    <span class="preview-badge">DEVELOPER FIXTURE PREVIEW</span>
    <span>Connected to mock fixture bridge. Real daemon IPC disabled; showing simulated state.</span>
  </div>`
      : ''
  }

  <div class="container">
    <!-- Header -->
    <div class="header-bar">
      <div class="header-title-area">
        <div class="header-title-row">
          <span class="title-text">Session Overview</span>
          <span class="session-badge badge-status-${sessionStatus}">
            ● ${sessionStatus.toUpperCase()}
          </span>
        </div>
        <div class="meta-row">
          <span class="meta-item">
            ID: <span class="mono-code">${escapeHtml(session?.id || 'none')}</span>
          </span>
          <span class="meta-item">
            Workspace: <span class="mono-code">${escapeHtml(
              session?.workspacePath || '(no workspace)'
            )}</span>
          </span>
          ${
            session?.activeRunId
              ? `<span class="meta-item">Run: <span class="mono-code">${escapeHtml(
                  session.activeRunId
                )}</span></span>`
              : ''
          }
          ${
            session?.providerSummary
              ? `<span class="meta-item">Provider: ${escapeHtml(
                  session.providerSummary
                )}</span>`
              : ''
          }
        </div>
      </div>
      <div class="header-actions">
        <button class="btn-action btn-secondary" id="btn-open-graph" title="Open Interactive DAG View">
          ⚡ Task Graph
        </button>
        <button class="btn-action btn-secondary" id="btn-refresh" title="Refresh Session Details">
          ⟳ Refresh
        </button>
      </div>
    </div>

    <!-- Metric Cards -->
    <div class="metrics-grid">
      <div class="metric-card">
        <span class="metric-label">Total Tasks</span>
        <span class="metric-value">${totalTasks}</span>
        <span class="metric-sub">${completedTasks} completed</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">In-Flight</span>
        <span class="metric-value text-yellow">${runningTasks}</span>
        <span class="metric-sub">currently running</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">Blocked</span>
        <span class="metric-value text-orange">${blockedTasks}</span>
        <span class="metric-sub">awaiting dependencies</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">Failed</span>
        <span class="metric-value ${failedTasks > 0 ? 'text-red' : ''}">${failedTasks}</span>
        <span class="metric-sub">errors encountered</span>
      </div>
      <div class="metric-card">
        <span class="metric-label">Elapsed Time</span>
        <span class="metric-value" id="elapsed-counter">${formatElapsed(elapsedMs)}</span>
        <span class="metric-sub">session duration</span>
      </div>
    </div>

    <!-- Progress Card -->
    <div class="progress-section">
      <div style="display: flex; justify-content: space-between; align-items: center; font-size: 12px;">
        <span style="font-weight: 600;">Overall Session Completion</span>
        <span style="color: var(--vscode-descriptionForeground);">${progressPct}% (${completedTasks}/${totalTasks})</span>
      </div>
      <div class="progress-bar-track">
        <div class="progress-bar-fill" style="width: ${progressPct}%;"></div>
      </div>
    </div>

    <!-- Goal & Scope Section -->
    <div class="panel-card">
      <div class="panel-header">
        <span class="panel-title">🎯 Goal &amp; Scope Overview</span>
      </div>
      <div class="markdown-content">
        ${goalHtml}
      </div>
    </div>

    <!-- Active Agent Roster Section -->
    <div class="panel-card">
      <div class="panel-header">
        <span class="panel-title">🤖 Active Agent Roster (${agentRoster.length})</span>
      </div>
      ${
        agentRoster.length === 0
          ? `<div style="color: var(--vscode-descriptionForeground); font-size: 12px; padding: 8px 0;">No active agents currently assigned to this session.</div>`
          : `<div class="roster-grid">
          ${agentRoster
            .map(
              (agent) => `
            <div class="agent-card">
              <div class="agent-card-header">
                <div class="agent-identity">
                  <div class="agent-avatar">${escapeHtml(
                    agent.name.charAt(0).toUpperCase()
                  )}</div>
                  <span class="agent-name">${escapeHtml(agent.name)}</span>
                </div>
                <span class="role-badge">${escapeHtml(agent.role || 'Agent')}</span>
              </div>
              <div class="agent-card-header">
                <div class="agent-status-indicator">
                  <span class="status-dot dot-${agent.status}"></span>
                  <span style="text-transform: capitalize;">${agent.status}</span>
                </div>
              </div>
              <div class="agent-task-info">
                <span>Current Task:</span>
                ${
                  agent.currentTaskId
                    ? `<span class="agent-task-link" data-task-id="${escapeHtml(
                        agent.currentTaskId
                      )}" title="Inspect Task in Graph">⚡ ${escapeHtml(
                        agent.currentTaskTitle || agent.currentTaskId
                      )}</span>`
                    : '<span style="font-style: italic;">Idle (waiting for assignment)</span>'
                }
              </div>
            </div>
          `
            )
            .join('')}
        </div>`
      }
    </div>
  </div>

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();

    // Elapsed timer live updating
    let elapsedMs = ${elapsedMs};
    const isRunning = ${sessionStatus === 'running'};
    const elapsedEl = document.getElementById('elapsed-counter');

    function formatTime(ms) {
      if (ms < 0) ms = 0;
      const totalSec = Math.floor(ms / 1000);
      const hours = Math.floor(totalSec / 3600);
      const minutes = Math.floor((totalSec % 3600) / 60);
      const seconds = totalSec % 60;

      const pad = (n) => String(n).padStart(2, '0');
      if (hours > 0) {
        return \`\${hours}h \${pad(minutes)}m \${pad(seconds)}s\`;
      }
      return \`\${pad(minutes)}m \${pad(seconds)}s\`;
    }

    if (isRunning) {
      setInterval(() => {
        elapsedMs += 1000;
        if (elapsedEl) {
          elapsedEl.textContent = formatTime(elapsedMs);
        }
      }, 1000);
    }

    // Button Listeners
    document.getElementById('btn-refresh').addEventListener('click', () => {
      vscode.postMessage({ command: 'refresh' });
    });

    document.getElementById('btn-open-graph').addEventListener('click', () => {
      vscode.postMessage({ command: 'openTaskGraph' });
    });

    // Task links
    document.querySelectorAll('.agent-task-link').forEach(link => {
      link.addEventListener('click', () => {
        const taskId = link.getAttribute('data-task-id');
        if (taskId) {
          vscode.postMessage({ command: 'selectTask', taskId });
        }
      });
    });
  </script>
</body>
</html>`;
  }

  private _resolveAgentRoster(): AgentRosterMember[] {
    if (this._agents && this._agents.length > 0) {
      return this._agents;
    }

    // Derive from tasks
    const agentMap = new Map<string, AgentRosterMember>();

    for (const task of this._tasks) {
      if (!task.assignedAgentName && !task.assignedAgentId) {
        continue;
      }
      const agentId = task.assignedAgentId || task.assignedAgentName || 'agent';
      const agentName = task.assignedAgentName || task.assignedAgentId || 'Agent';

      let member = agentMap.get(agentId);
      if (!member) {
        member = {
          id: agentId,
          name: agentName,
          role: deriveRoleFromName(agentName),
          status: 'idle',
        };
        agentMap.set(agentId, member);
      }

      if (task.status === 'running') {
        member.status = 'busy';
        member.currentTaskId = task.id;
        member.currentTaskTitle = task.title;
      } else if (!member.currentTaskId && (task.status === 'ready' || task.status === 'blocked')) {
        member.currentTaskId = task.id;
        member.currentTaskTitle = task.title;
        if (member.status === 'idle') {
          member.status = 'active';
        }
      }
    }

    return Array.from(agentMap.values());
  }
}

function deriveRoleFromName(name: string): string {
  const lower = name.toLowerCase();
  if (lower.includes('architect') || lower.includes('planner')) {
    return 'Architect / Planner';
  }
  if (lower.includes('reviewer') || lower.includes('audit')) {
    return 'Security Reviewer';
  }
  if (lower.includes('test') || lower.includes('qa')) {
    return 'Test Engineer';
  }
  if (lower.includes('coordinator') || lower.includes('orchestrator')) {
    return 'Orchestrator';
  }
  return 'Specialist Agent';
}

function formatElapsed(ms: number): string {
  if (!ms || ms < 0) {
    return '00m 00s';
  }
  const totalSec = Math.floor(ms / 1000);
  const hours = Math.floor(totalSec / 3600);
  const minutes = Math.floor((totalSec % 3600) / 60);
  const seconds = totalSec % 60;

  const pad = (n: number) => String(n).padStart(2, '0');
  if (hours > 0) {
    return `${hours}h ${pad(minutes)}m ${pad(seconds)}s`;
  }
  return `${pad(minutes)}m ${pad(seconds)}s`;
}
