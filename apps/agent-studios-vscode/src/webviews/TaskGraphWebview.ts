/**
 * Agent Studios — Interactive Task Graph (DAG) Webview Panel
 * Milestone M14-UI-00: Code-OSS Native Agents Workbench
 */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import { AgentStudiosBridge } from '../bridge/AgentStudiosBridge';
import { TaskGraph, TaskId, TaskStatus } from '../bridge/types';
import { computeTopologicalLayout, LayoutResult } from './graphLayout';
import { escapeHtml } from './markdownRenderer';

export class TaskGraphWebview implements vscode.Disposable {
  public static readonly viewType = 'agent-studios.taskGraph';
  public static currentPanel: TaskGraphWebview | undefined;

  private readonly _panel: vscode.WebviewPanel;
  private readonly _extensionUri: vscode.Uri;
  private _disposables: vscode.Disposable[] = [];
  private _graph: TaskGraph | null = null;
  private _bridge?: AgentStudiosBridge;
  private _selectedTaskId: TaskId | null = null;
  private _isDisposing = false;

  public get extensionUri(): vscode.Uri {
    return this._extensionUri;
  }

  public get selectedTaskId(): TaskId | null {
    return this._selectedTaskId;
  }

  public get currentGraph(): TaskGraph | null {
    return this._graph;
  }

  private readonly _onDidSelectTask = new vscode.EventEmitter<TaskId>();
  public readonly onDidSelectTask: vscode.Event<TaskId> = this._onDidSelectTask.event;

  /**
   * Creates or reveals the Task Graph webview panel.
   */
  public static createOrShow(
    extensionUri: vscode.Uri,
    graph?: TaskGraph | null,
    bridge?: AgentStudiosBridge
  ): TaskGraphWebview {
    const column = vscode.window.activeTextEditor
      ? vscode.window.activeTextEditor.viewColumn
      : undefined;

    if (TaskGraphWebview.currentPanel) {
      TaskGraphWebview.currentPanel._panel.reveal(column || vscode.ViewColumn.One);
      if (graph !== undefined) {
        TaskGraphWebview.currentPanel.setGraph(graph);
      }
      return TaskGraphWebview.currentPanel;
    }

    const panel = vscode.window.createWebviewPanel(
      TaskGraphWebview.viewType,
      'Agent Studios: Task Graph',
      column || vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [extensionUri],
      }
    );

    TaskGraphWebview.currentPanel = new TaskGraphWebview(panel, extensionUri, graph, bridge);
    return TaskGraphWebview.currentPanel;
  }

  private constructor(
    panel: vscode.WebviewPanel,
    extensionUri: vscode.Uri,
    graph?: TaskGraph | null,
    bridge?: AgentStudiosBridge
  ) {
    this._panel = panel;
    this._extensionUri = extensionUri;
    this._graph = graph || null;
    this._bridge = bridge;

    this._panel.onDidDispose(() => this.dispose(), null, this._disposables);

    this._panel.webview.onDidReceiveMessage(
      (message) => this._handleWebviewMessage(message),
      null,
      this._disposables
    );

    // If bridge is provided, subscribe to timeline events to auto-refresh graph
    if (this._bridge) {
      this._bridge.onEvent(
        (event) => {
          if (
            event.type.startsWith('task_') ||
            event.type.startsWith('run_') ||
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
   * Sets graph data and re-renders the webview.
   */
  public setGraph(graph: TaskGraph | null): void {
    this._graph = graph;
    this._update();
  }

  /**
   * Selects a task node in the webview.
   */
  public selectTask(taskId: TaskId): void {
    this._selectedTaskId = taskId;
    this._panel.webview.postMessage({ command: 'selectTask', taskId });
  }

  /**
   * Refreshes the task graph directly from the connected bridge.
   */
  public async refreshFromBridge(): Promise<void> {
    if (!this._bridge) {
      return;
    }
    try {
      const graph = await this._bridge.getTaskGraph(this._graph?.runId);
      if (graph) {
        this.setGraph(graph);
      }
    } catch {
      // Bridge disconnected or failed; maintain current view
    }
  }

  public dispose(): void {
    if (this._isDisposing) {
      return;
    }
    this._isDisposing = true;
    TaskGraphWebview.currentPanel = undefined;
    this._panel.dispose();
    this._onDidSelectTask.dispose();

    while (this._disposables.length) {
      const d = this._disposables.pop();
      if (d) {
        d.dispose();
      }
    }
  }

  private _handleWebviewMessage(message: { command: string; taskId?: string }): void {
    switch (message.command) {
      case 'selectTask':
        if (typeof message.taskId === 'string') {
          this._selectedTaskId = message.taskId;
          this._onDidSelectTask.fire(message.taskId);
        }
        break;
      case 'refresh':
        this.refreshFromBridge();
        break;
      case 'showError':
        if (typeof message.taskId === 'string') {
          const node = this._graph?.nodes.find((n) => n.id === message.taskId);
          if (node?.error) {
            vscode.window.showErrorMessage(`Task "${node.title}" failed: ${node.error}`);
          }
        }
        break;
    }
  }

  private _update(): void {
    const webview = this._panel.webview;
    const layout = this._computeCurrentLayout();
    this._panel.webview.html = this._getHtmlForWebview(webview, layout);
  }

  private _computeCurrentLayout(): LayoutResult {
    if (!this._graph || !this._graph.nodes || this._graph.nodes.length === 0) {
      return {
        hasCycle: false,
        nodes: [],
        edges: [],
        width: 800,
        height: 600,
        levelCount: 0,
      };
    }
    return computeTopologicalLayout(this._graph.nodes, this._graph.edges);
  }

  private _getHtmlForWebview(webview: vscode.Webview, layout: LayoutResult): string {
    const nonce = crypto.randomBytes(16).toString('base64');
    const nodesJson = JSON.stringify(layout.nodes);
    const edgesJson = JSON.stringify(layout.edges);
    const hasCycle = layout.hasCycle;
    const cyclePathStr = layout.cyclePath ? layout.cyclePath.join(' &rarr; ') : '';

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${webview.cspSource};">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios: Task Graph</title>
  <style>
    :root {
      --bg: var(--vscode-editor-background);
      --fg: var(--vscode-editor-foreground);
      --card-bg: var(--vscode-sideBar-background, #252526);
      --border-color: var(--vscode-widget-border, var(--vscode-panel-border, #3c3c3c));
      --focus-border: var(--vscode-focusBorder, #007fd4);
      --badge-bg: var(--vscode-badge-background, #4d4d4d);
      --badge-fg: var(--vscode-badge-foreground, #ffffff);

      /* VS Code theme tokens */
      --status-succeeded: var(--vscode-charts-green, #89d185);
      --status-failed: var(--vscode-charts-red, #f14c4c);
      --status-running: var(--vscode-charts-yellow, #cca700);
      --status-blocked: var(--vscode-charts-orange, #d18616);
      --status-ready: var(--vscode-charts-blue, #3794ff);
      --status-pending: var(--vscode-charts-purple, #b180d7);
      --status-cancelled: var(--vscode-disabledForeground, #888888);
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
      width: 100vw;
      height: 100vh;
      overflow: hidden;
      display: flex;
      flex-direction: column;
    }

    /* Top Toolbar */
    .toolbar {
      height: 40px;
      background: var(--card-bg);
      border-bottom: 1px solid var(--border-color);
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 16px;
      z-index: 10;
      user-select: none;
    }

    .toolbar-left, .toolbar-right {
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .toolbar-title {
      font-weight: 600;
      font-size: 13px;
      display: flex;
      align-items: center;
      gap: 6px;
    }

    .tool-btn {
      background: transparent;
      border: 1px solid var(--border-color);
      color: var(--fg);
      padding: 4px 10px;
      border-radius: 4px;
      cursor: pointer;
      font-size: 12px;
      display: inline-flex;
      align-items: center;
      gap: 4px;
      transition: background 0.15s, border-color 0.15s;
    }

    .tool-btn:hover {
      background: var(--vscode-toolbar-hoverBackground, rgba(90, 93, 94, 0.31));
      border-color: var(--focus-border);
    }

    .tool-btn:focus-visible {
      outline: 1px solid var(--focus-border);
    }

    .zoom-display {
      font-size: 11px;
      color: var(--vscode-descriptionForeground, #999);
      min-width: 44px;
      text-align: center;
    }

    /* Cycle Alert Banner */
    .cycle-banner {
      background: rgba(241, 76, 76, 0.18);
      border-bottom: 1px solid var(--status-failed);
      color: var(--fg);
      padding: 10px 16px;
      display: flex;
      align-items: center;
      gap: 12px;
      z-index: 9;
    }

    .cycle-icon {
      font-size: 20px;
      color: var(--status-failed);
    }

    .cycle-text strong {
      color: var(--status-failed);
      font-weight: 600;
    }

    .cycle-path {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 12px;
      margin-top: 2px;
      color: var(--fg);
    }

    /* Main Container: Viewport & Inspector */
    .main-container {
      flex: 1;
      display: flex;
      position: relative;
      overflow: hidden;
    }

    #viewport {
      flex: 1;
      height: 100%;
      position: relative;
      overflow: hidden;
      cursor: grab;
      user-select: none;
      background: var(--bg);
      background-image: radial-gradient(var(--border-color) 1px, transparent 1px);
      background-size: 24px 24px;
    }

    #viewport.dragging {
      cursor: grabbing;
    }

    #stage {
      position: absolute;
      top: 0;
      left: 0;
      transform-origin: 0 0;
      pointer-events: auto;
    }

    /* SVG Edges */
    .graph-edges {
      position: absolute;
      top: 0;
      left: 0;
      pointer-events: none;
      overflow: visible;
    }

    .graph-edge {
      fill: none;
      stroke: var(--border-color);
      stroke-width: 1.5;
      transition: stroke 0.2s, stroke-width 0.2s, opacity 0.2s;
    }

    .graph-edge.highlight-in {
      stroke: var(--status-ready);
      stroke-width: 2.5;
    }

    .graph-edge.highlight-out {
      stroke: var(--status-succeeded);
      stroke-width: 2.5;
    }

    .graph-edge.dimmed {
      opacity: 0.15;
    }

    .graph-edge.cycle-edge {
      stroke: var(--status-failed);
      stroke-width: 2;
      stroke-dasharray: 4;
    }

    /* Task Nodes Container */
    .nodes-container {
      position: absolute;
      top: 0;
      left: 0;
      pointer-events: none;
    }

    /* Individual Task Node Card */
    .task-node {
      position: absolute;
      pointer-events: auto;
      background: var(--card-bg);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 10px 12px;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
      cursor: pointer;
      user-select: none;
      box-shadow: 0 2px 6px rgba(0, 0, 0, 0.2);
      transition: border-color 0.15s, box-shadow 0.15s, transform 0.1s, opacity 0.2s;
    }

    .task-node:hover {
      border-color: var(--focus-border);
      box-shadow: 0 4px 10px rgba(0, 0, 0, 0.3);
    }

    .task-node:focus-visible {
      outline: 2px solid var(--focus-border);
      outline-offset: 1px;
    }

    .task-node.selected {
      border-color: var(--focus-border);
      box-shadow: 0 0 0 2px var(--focus-border);
    }

    .task-node.dimmed {
      opacity: 0.35;
    }

    .node-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 6px;
      margin-bottom: 6px;
    }

    .status-badge {
      display: inline-block;
      font-size: 10px;
      font-weight: 600;
      padding: 2px 6px;
      border-radius: 3px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      border: 1px solid transparent;
    }

    .badge-running {
      color: var(--status-running);
      background: rgba(204, 167, 0, 0.15);
      border-color: var(--status-running);
    }

    .badge-succeeded {
      color: var(--status-succeeded);
      background: rgba(137, 209, 133, 0.15);
      border-color: var(--status-succeeded);
    }

    .badge-failed {
      color: var(--status-failed);
      background: rgba(241, 76, 76, 0.15);
      border-color: var(--status-failed);
    }

    .badge-blocked {
      color: var(--status-blocked);
      background: rgba(209, 134, 22, 0.15);
      border-color: var(--status-blocked);
    }

    .badge-ready {
      color: var(--status-ready);
      background: rgba(55, 148, 255, 0.15);
      border-color: var(--status-ready);
    }

    .badge-pending {
      color: var(--status-pending);
      background: rgba(177, 128, 215, 0.15);
      border-color: var(--status-pending);
    }

    .badge-cancelled {
      color: var(--status-cancelled);
      background: rgba(128, 128, 128, 0.15);
      border-color: var(--status-cancelled);
    }

    .node-level-tag {
      font-size: 10px;
      color: var(--vscode-descriptionForeground, #888);
    }

    .node-title {
      font-weight: 600;
      font-size: 12px;
      color: var(--fg);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      margin-bottom: 6px;
    }

    .node-footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      font-size: 11px;
      color: var(--vscode-descriptionForeground, #999);
    }

    .node-agent {
      display: flex;
      align-items: center;
      gap: 4px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      max-width: 150px;
    }

    .node-progress-mini {
      font-weight: 500;
      font-size: 10px;
    }

    .node-progress-bar {
      position: absolute;
      bottom: 0;
      left: 0;
      right: 0;
      height: 3px;
      background: rgba(255, 255, 255, 0.08);
      border-bottom-left-radius: 6px;
      border-bottom-right-radius: 6px;
      overflow: hidden;
    }

    .node-progress-fill {
      height: 100%;
      background: var(--status-succeeded);
    }

    /* Side Inspector Panel */
    .inspector-panel {
      width: 320px;
      height: 100%;
      background: var(--card-bg);
      border-left: 1px solid var(--border-color);
      display: flex;
      flex-direction: column;
      transform: translateX(100%);
      transition: transform 0.2s ease-in-out;
      position: absolute;
      right: 0;
      top: 0;
      z-index: 20;
      box-shadow: -4px 0 12px rgba(0, 0, 0, 0.25);
    }

    .inspector-panel.open {
      transform: translateX(0);
    }

    .inspector-header {
      padding: 12px 16px;
      border-bottom: 1px solid var(--border-color);
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .inspector-title {
      font-weight: 600;
      font-size: 13px;
    }

    .close-btn {
      background: transparent;
      border: none;
      color: var(--fg);
      cursor: pointer;
      font-size: 16px;
      padding: 2px 6px;
      border-radius: 3px;
    }

    .close-btn:hover {
      background: var(--vscode-toolbar-hoverBackground, rgba(90, 93, 94, 0.31));
    }

    .inspector-body {
      flex: 1;
      overflow-y: auto;
      padding: 16px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }

    .inspector-section {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .inspector-label {
      font-size: 11px;
      text-transform: uppercase;
      font-weight: 600;
      color: var(--vscode-descriptionForeground, #999);
      letter-spacing: 0.5px;
    }

    .inspector-value {
      font-size: 12px;
    }

    .id-code {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 11px;
      background: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.2));
      padding: 2px 6px;
      border-radius: 3px;
      word-break: break-all;
    }

    .error-card {
      background: rgba(241, 76, 76, 0.15);
      border: 1px solid var(--status-failed);
      border-radius: 4px;
      padding: 8px 10px;
      color: var(--status-failed);
      font-size: 12px;
      word-break: break-word;
    }

    .chips-container {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }

    .task-chip {
      background: var(--bg);
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 4px 8px;
      font-size: 11px;
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 4px;
      transition: border-color 0.15s, background 0.15s;
    }

    .task-chip:hover {
      border-color: var(--focus-border);
      background: var(--vscode-toolbar-hoverBackground, rgba(90, 93, 94, 0.31));
    }

    /* Empty state */
    .empty-state {
      position: absolute;
      top: 50%;
      left: 50%;
      transform: translate(-50%, -50%);
      text-align: center;
      color: var(--vscode-descriptionForeground, #888);
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 12px;
    }

    .empty-state-icon {
      font-size: 40px;
      opacity: 0.6;
    }
  </style>
</head>
<body>
  <!-- Toolbar -->
  <div class="toolbar">
    <div class="toolbar-left">
      <span class="toolbar-title">
        <span>⚡</span> Task Graph (DAG)
      </span>
      <span class="zoom-display" id="zoom-text">100%</span>
      <button class="tool-btn" id="btn-zoom-in" title="Zoom In (+)">+</button>
      <button class="tool-btn" id="btn-zoom-out" title="Zoom Out (-)">-</button>
      <button class="tool-btn" id="btn-fit" title="Fit to View">Fit</button>
      <button class="tool-btn" id="btn-reset" title="Reset Zoom (1:1)">1:1</button>
    </div>
    <div class="toolbar-right">
      <button class="tool-btn" id="btn-refresh" title="Refresh Graph">⟳ Refresh</button>
    </div>
  </div>

  ${
    hasCycle
      ? `<div class="cycle-banner">
    <div class="cycle-icon">⚠️</div>
    <div class="cycle-text">
      <strong>Cycle Detected in Task Graph!</strong>
      <div class="cycle-path">Path: ${cyclePathStr}</div>
      <div style="font-size: 11px; margin-top: 2px;">Circular dependencies violate DAG execution. Fallback layout displayed.</div>
    </div>
  </div>`
      : ''
  }

  <div class="main-container">
    <div id="viewport">
      <div id="stage" style="width: ${layout.width}px; height: ${layout.height}px;">
        <!-- SVG Edges -->
        <svg class="graph-edges" width="${layout.width}" height="${layout.height}">
          <defs>
            <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
              <path d="M 0 1 L 9 5 L 0 9 z" fill="var(--vscode-widget-border, #666)" />
            </marker>
            <marker id="arrow-in" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
              <path d="M 0 1 L 9 5 L 0 9 z" fill="var(--vscode-charts-blue, #3794ff)" />
            </marker>
            <marker id="arrow-out" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
              <path d="M 0 1 L 9 5 L 0 9 z" fill="var(--vscode-charts-green, #89d185)" />
            </marker>
          </defs>
          ${layout.edges
            .map(
              (e) =>
                `<path class="graph-edge ${hasCycle ? 'cycle-edge' : ''}" data-from="${escapeHtml(
                  e.from
                )}" data-to="${escapeHtml(e.to)}" d="${e.pathD}" marker-end="url(#arrow)" />`
            )
            .join('')}
        </svg>

        <!-- Nodes -->
        <div class="nodes-container" style="width: ${layout.width}px; height: ${layout.height}px;">
          ${layout.nodes
            .map(
              (n) => `
            <div class="task-node status-${n.status}"
                 tabindex="0"
                 role="button"
                 id="node-${escapeHtml(n.id)}"
                 data-task-id="${escapeHtml(n.id)}"
                 aria-label="Task: ${escapeHtml(n.title)}, Status: ${n.status}"
                 style="left: ${n.x}px; top: ${n.y}px; width: ${n.width}px; height: ${n.height}px;">
              <div class="node-header">
                <span class="status-badge badge-${n.status}">${formatStatusLabel(n.status)}</span>
                <span class="node-level-tag">L${n.level}</span>
              </div>
              <div class="node-title" title="${escapeHtml(n.title)}">${escapeHtml(n.title)}</div>
              <div class="node-footer">
                <span class="node-agent" title="${escapeHtml(n.assignedAgentName || 'Unassigned')}">
                  👤 ${escapeHtml(n.assignedAgentName || 'Unassigned')}
                </span>
                ${
                  n.progress > 0
                    ? `<span class="node-progress-mini">${n.progress}%</span>`
                    : ''
                }
              </div>
              ${
                n.progress > 0
                  ? `<div class="node-progress-bar"><div class="node-progress-fill" style="width: ${n.progress}%"></div></div>`
                  : ''
              }
            </div>
          `
            )
            .join('')}
        </div>
      </div>

      ${
        layout.nodes.length === 0
          ? `<div class="empty-state">
        <div class="empty-state-icon">📋</div>
        <div><strong>No Tasks in Graph</strong></div>
        <div>No active or queued tasks currently available for this run.</div>
      </div>`
          : ''
      }
    </div>

    <!-- Side Inspector Panel -->
    <div class="inspector-panel" id="inspector">
      <div class="inspector-header">
        <span class="inspector-title" id="insp-header-title">Task Details</span>
        <button class="close-btn" id="btn-close-inspector" title="Close Inspector (Esc)">&times;</button>
      </div>
      <div class="inspector-body" id="insp-body">
        <!-- Rendered dynamically via JS -->
      </div>
    </div>
  </div>

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const nodes = ${nodesJson};
    const edges = ${edgesJson};

    const viewport = document.getElementById('viewport');
    const stage = document.getElementById('stage');
    const zoomText = document.getElementById('zoom-text');
    const inspector = document.getElementById('inspector');
    const inspBody = document.getElementById('insp-body');
    const btnCloseInsp = document.getElementById('btn-close-inspector');

    let scale = 1.0;
    let translateX = 30;
    let translateY = 30;
    let isDragging = false;
    let startX = 0;
    let startY = 0;
    let selectedTaskId = null;

    function updateTransform() {
      stage.style.transform = \`translate(\${translateX}px, \${translateY}px) scale(\${scale})\`;
      zoomText.textContent = \`\${Math.round(scale * 100)}%\`;
    }

    // Drag / Pan Canvas
    viewport.addEventListener('mousedown', (e) => {
      if (e.target.closest('.task-node') || e.target.closest('.tool-btn') || e.target.closest('.inspector-panel')) {
        return;
      }
      isDragging = true;
      startX = e.clientX - translateX;
      startY = e.clientY - translateY;
      viewport.classList.add('dragging');
    });

    window.addEventListener('mousemove', (e) => {
      if (!isDragging) return;
      translateX = e.clientX - startX;
      translateY = e.clientY - startY;
      updateTransform();
    });

    window.addEventListener('mouseup', () => {
      if (isDragging) {
        isDragging = false;
        viewport.classList.remove('dragging');
      }
    });

    // Zoom Controls
    function setZoom(newScale, focalX, focalY) {
      const minScale = 0.2;
      const maxScale = 2.5;
      const clamped = Math.max(minScale, Math.min(maxScale, newScale));
      if (focalX !== undefined && focalY !== undefined) {
        const factor = clamped / scale;
        translateX = focalX - (focalX - translateX) * factor;
        translateY = focalY - (focalY - translateY) * factor;
      }
      scale = clamped;
      updateTransform();
    }

    document.getElementById('btn-zoom-in').addEventListener('click', () => {
      setZoom(scale * 1.25);
    });

    document.getElementById('btn-zoom-out').addEventListener('click', () => {
      setZoom(scale / 1.25);
    });

    document.getElementById('btn-reset').addEventListener('click', () => {
      scale = 1.0;
      translateX = 40;
      translateY = 40;
      updateTransform();
    });

    document.getElementById('btn-fit').addEventListener('click', fitToView);

    function fitToView() {
      const vRect = viewport.getBoundingClientRect();
      const sWidth = ${layout.width};
      const sHeight = ${layout.height};
      if (sWidth <= 0 || sHeight <= 0) return;

      const pad = 60;
      const scaleX = (vRect.width - pad) / sWidth;
      const scaleY = (vRect.height - pad) / sHeight;
      scale = Math.min(1.0, Math.max(0.25, Math.min(scaleX, scaleY)));
      translateX = Math.max(20, (vRect.width - sWidth * scale) / 2);
      translateY = Math.max(20, (vRect.height - sHeight * scale) / 2);
      updateTransform();
    }

    viewport.addEventListener('wheel', (e) => {
      e.preventDefault();
      const rect = viewport.getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;
      const delta = e.deltaY < 0 ? 1.15 : 0.87;
      setZoom(scale * delta, mouseX, mouseY);
    }, { passive: false });

    // Node Selection & Inspector
    function selectTaskNode(taskId, panIntoView = false) {
      selectedTaskId = taskId;
      const task = nodes.find(n => n.id === taskId);

      // Notify Extension Host
      vscode.postMessage({ command: 'selectTask', taskId });

      // Update Node Highlight
      document.querySelectorAll('.task-node').forEach(el => {
        const id = el.getAttribute('data-task-id');
        if (id === taskId) {
          el.classList.add('selected');
          el.classList.remove('dimmed');
        } else if (taskId) {
          el.classList.remove('selected');
          el.classList.add('dimmed');
        } else {
          el.classList.remove('selected', 'dimmed');
        }
      });

      // Update Edge Highlight
      document.querySelectorAll('.graph-edge').forEach(edge => {
        const from = edge.getAttribute('data-from');
        const to = edge.getAttribute('data-to');
        if (!taskId) {
          edge.classList.remove('highlight-in', 'highlight-out', 'dimmed');
          edge.setAttribute('marker-end', 'url(#arrow)');
        } else if (to === taskId) {
          edge.classList.add('highlight-in');
          edge.classList.remove('dimmed', 'highlight-out');
          edge.setAttribute('marker-end', 'url(#arrow-in)');
        } else if (from === taskId) {
          edge.classList.add('highlight-out');
          edge.classList.remove('dimmed', 'highlight-in');
          edge.setAttribute('marker-end', 'url(#arrow-out)');
        } else {
          edge.classList.add('dimmed');
          edge.classList.remove('highlight-in', 'highlight-out');
          edge.setAttribute('marker-end', 'url(#arrow)');
        }
      });

      if (!task) {
        inspector.classList.remove('open');
        return;
      }

      // Render Inspector Content
      renderInspector(task);
      inspector.classList.add('open');

      if (panIntoView) {
        centerNode(task);
      }
    }

    function centerNode(task) {
      const vRect = viewport.getBoundingClientRect();
      translateX = vRect.width / 2 - (task.x + task.width / 2) * scale;
      translateY = vRect.height / 2 - (task.y + task.height / 2) * scale;
      updateTransform();
    }

    function renderInspector(task) {
      const incoming = edges.filter(e => e.to === task.id).map(e => e.from);
      const outgoing = edges.filter(e => e.from === task.id).map(e => e.to);

      let html = \`
        <div class="inspector-section">
          <div class="inspector-label">Title</div>
          <div class="inspector-value" style="font-weight: 600; font-size: 13px;">\${escapeText(task.title)}</div>
        </div>

        <div class="inspector-section">
          <div class="inspector-label">Status</div>
          <div><span class="status-badge badge-\${task.status}">\${task.status.toUpperCase()}</span></div>
        </div>

        <div class="inspector-section">
          <div class="inspector-label">Task ID</div>
          <div class="id-code">\${escapeText(task.id)}</div>
        </div>

        <div class="inspector-section">
          <div class="inspector-label">Assigned Agent</div>
          <div class="inspector-value">👤 \${escapeText(task.assignedAgentName || 'Unassigned')}</div>
        </div>

        <div class="inspector-section">
          <div class="inspector-label">Progress</div>
          <div class="inspector-value">\${task.progress}%</div>
        </div>
      \`;

      if (task.error) {
        html += \`
          <div class="inspector-section">
            <div class="inspector-label" style="color: var(--status-failed);">Failure Error</div>
            <div class="error-card">\${escapeText(task.error)}</div>
          </div>
        \`;
      }

      if (task.description) {
        html += \`
          <div class="inspector-section">
            <div class="inspector-label">Description</div>
            <div class="inspector-value" style="color: var(--vscode-descriptionForeground, #aaa); font-size: 12px; line-height: 1.4;">
              \${escapeText(task.description)}
            </div>
          </div>
        \`;
      }

      // Dependencies (Incoming)
      html += \`
        <div class="inspector-section">
          <div class="inspector-label">Dependencies (\${incoming.length})</div>
          <div class="chips-container">
      \`;
      if (incoming.length === 0) {
        html += '<span style="font-size: 11px; color: var(--vscode-descriptionForeground);">None (Root Task)</span>';
      } else {
        incoming.forEach(depId => {
          const depTask = nodes.find(n => n.id === depId);
          const name = depTask ? depTask.title : depId;
          html += \`<button class="task-chip" data-goto-id="\${depId}" title="Select dependency">← \${escapeText(name)}</button>\`;
        });
      }
      html += \`</div></div>\`;

      // Dependents (Outgoing)
      html += \`
        <div class="inspector-section">
          <div class="inspector-label">Dependent Tasks (\${outgoing.length})</div>
          <div class="chips-container">
      \`;
      if (outgoing.length === 0) {
        html += '<span style="font-size: 11px; color: var(--vscode-descriptionForeground);">None (Leaf Task)</span>';
      } else {
        outgoing.forEach(outId => {
          const outTask = nodes.find(n => n.id === outId);
          const name = outTask ? outTask.title : outId;
          html += \`<button class="task-chip" data-goto-id="\${outId}" title="Select dependent">→ \${escapeText(name)}</button>\`;
        });
      }
      html += \`</div></div>\`;

      inspBody.innerHTML = html;

      // Attach click events on chips
      inspBody.querySelectorAll('.task-chip').forEach(chip => {
        chip.addEventListener('click', () => {
          const targetId = chip.getAttribute('data-goto-id');
          if (targetId) {
            selectTaskNode(targetId, true);
          }
        });
      });
    }

    btnCloseInsp.addEventListener('click', () => {
      selectTaskNode(null);
    });

    // Node click handlers
    document.querySelectorAll('.task-node').forEach(el => {
      el.addEventListener('click', (e) => {
        e.stopPropagation();
        const id = el.getAttribute('data-task-id');
        selectTaskNode(id);
      });

      // Keyboard navigation: Enter or Space selects
      el.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          const id = el.getAttribute('data-task-id');
          selectTaskNode(id);
        }
      });
    });

    // Global Key Listener (Esc to close inspector)
    window.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && selectedTaskId) {
        selectTaskNode(null);
      }
    });

    // Refresh Button
    document.getElementById('btn-refresh').addEventListener('click', () => {
      vscode.postMessage({ command: 'refresh' });
    });

    // Host Message Listener
    window.addEventListener('message', (event) => {
      const msg = event.data;
      if (msg.command === 'selectTask' && msg.taskId) {
        selectTaskNode(msg.taskId, true);
      }
    });

    function escapeText(str) {
      if (!str) return '';
      return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
    }

    // Auto-fit on initial render
    fitToView();
  </script>
</body>
</html>`;
  }
}

function formatStatusLabel(status: TaskStatus): string {
  switch (status) {
    case 'running':
      return 'Running';
    case 'succeeded':
      return 'Succeeded';
    case 'failed':
      return 'Failed';
    case 'blocked':
      return 'Blocked';
    case 'ready':
      return 'Ready';
    case 'pending':
      return 'Pending';
    case 'cancelled':
      return 'Cancelled';
    default:
      return status;
  }
}
