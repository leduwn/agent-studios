/**
 * Agent Studios — Mission Control Styles
 * Milestone M14 Mission Control Workbench
 *
 * Implements native VS Code CSS variable tokens, high-contrast support,
 * responsive layouts, and SVG DAG styling.
 */

export function getMissionControlCss(): string {
  return `
    :root {
      --bg-primary: var(--vscode-editor-background);
      --bg-secondary: var(--vscode-sideBar-background);
      --bg-card: var(--vscode-editorWidget-background, #252526);
      --bg-hover: var(--vscode-list-hoverBackground, rgba(255, 255, 255, 0.08));
      --border-color: var(--vscode-widget-border, var(--vscode-editorGroup-border, #333333));
      --text-primary: var(--vscode-foreground);
      --text-secondary: var(--vscode-descriptionForeground, #8c8c8c);
      --focus-border: var(--vscode-focusBorder, #007acc);

      --status-running: var(--vscode-progressBar-background, #3794ff);
      --status-success: var(--vscode-testing-iconPassed, #4ec9b0);
      --status-warning: var(--vscode-warningForeground, #cca700);
      --status-error: var(--vscode-errorForeground, #f14c4c);
      --status-neutral: var(--vscode-descriptionForeground, #8c8c8c);

      --font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif);
      --font-mono: var(--vscode-editor-font-family, Consolas, "Courier New", monospace);
    }

    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
    }

    body {
      background-color: var(--bg-primary);
      color: var(--text-primary);
      font-family: var(--font-family);
      font-size: 13px;
      line-height: 1.4;
      display: flex;
      flex-direction: column;
      height: 100vh;
      overflow: hidden;
    }

    code, pre {
      font-family: var(--font-mono);
    }

    a {
      color: var(--vscode-textLink-foreground, #3794ff);
      text-decoration: none;
    }
    a:hover {
      text-decoration: underline;
    }

    /* Top Persistent Navigation Bar */
    .mc-navbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      background-color: var(--bg-secondary);
      border-bottom: 1px solid var(--border-color);
      padding: 0 16px;
      height: 48px;
      flex-shrink: 0;
    }

    .mc-brand {
      display: flex;
      align-items: center;
      gap: 8px;
      font-weight: 600;
      font-size: 14px;
    }

    .mc-tabs-container {
      display: flex;
      gap: 2px;
      height: 100%;
    }

    .nav-tab-btn {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 0 12px;
      height: 100%;
      background: transparent;
      border: none;
      border-bottom: 2px solid transparent;
      color: var(--text-secondary);
      font-size: 12px;
      font-family: inherit;
      cursor: pointer;
      transition: color 0.15s, border-color 0.15s;
    }

    .nav-tab-btn:hover {
      color: var(--text-primary);
    }

    .nav-tab-btn.active {
      color: var(--text-primary);
      border-bottom-color: var(--status-running);
      font-weight: 600;
    }

    .nav-tab-btn:focus-visible {
      outline: 1px solid var(--focus-border);
    }

    /* Main Viewport */
    .mc-viewport {
      flex: 1;
      overflow-y: auto;
      padding: 20px;
    }

    .view-content {
      display: none;
      max-width: 1440px;
      margin: 0 auto;
    }

    .view-content.active {
      display: block;
    }

    .view-header {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      margin-bottom: 20px;
    }

    .view-header h2 {
      font-size: 18px;
      font-weight: 600;
      margin-bottom: 4px;
    }

    .view-subtitle {
      font-size: 12px;
    }

    /* Buttons & Controls */
    .btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 10px;
      font-size: 12px;
      border-radius: 3px;
      cursor: pointer;
      border: 1px solid transparent;
      font-family: inherit;
      white-space: nowrap;
    }

    .btn-sm {
      padding: 2px 8px;
      font-size: 11px;
    }

    .btn-primary {
      background-color: var(--vscode-button-background, #0e639c);
      color: var(--vscode-button-foreground, #ffffff);
    }
    .btn-primary:hover:not(:disabled) {
      background-color: var(--vscode-button-hoverBackground, #1177bb);
    }

    .btn-secondary {
      background-color: var(--vscode-button-secondaryBackground, #3a3d41);
      color: var(--vscode-button-secondaryForeground, #ffffff);
      border-color: var(--border-color);
    }
    .btn-secondary:hover:not(:disabled) {
      background-color: var(--vscode-button-secondaryHoverBackground, #45494e);
    }

    .btn:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    .btn-icon {
      background: transparent;
      border: none;
      color: var(--text-secondary);
      padding: 4px;
      cursor: pointer;
    }
    .btn-icon:hover {
      color: var(--text-primary);
    }

    /* Badges & Status Pills */
    .status-pill {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      padding: 2px 8px;
      border-radius: 12px;
      font-size: 11px;
      font-weight: 600;
      border: 1px solid transparent;
    }

    .status-pill.status-running {
      background: rgba(55, 148, 255, 0.15);
      color: var(--status-running);
      border-color: rgba(55, 148, 255, 0.3);
    }
    .status-pill.status-success {
      background: rgba(78, 201, 176, 0.15);
      color: var(--status-success);
      border-color: rgba(78, 201, 176, 0.3);
    }
    .status-pill.status-warning, .status-pill.status-blocked {
      background: rgba(204, 167, 0, 0.15);
      color: var(--status-warning);
      border-color: rgba(204, 167, 0, 0.3);
    }
    .status-pill.status-error, .status-pill.status-failed {
      background: rgba(241, 76, 76, 0.15);
      color: var(--status-error);
      border-color: rgba(241, 76, 76, 0.3);
    }
    .status-pill.status-idle, .status-pill.status-pending {
      background: rgba(140, 140, 140, 0.15);
      color: var(--status-neutral);
      border-color: rgba(140, 140, 140, 0.3);
    }

    .badge {
      display: inline-block;
      padding: 2px 6px;
      font-size: 11px;
      border-radius: 3px;
    }
    .badge-preview {
      background-color: #d29922;
      color: #000;
      font-weight: 700;
      font-size: 10px;
    }
    .badge-secondary {
      background-color: var(--bg-hover);
      color: var(--text-primary);
      border: 1px solid var(--border-color);
    }
    .badge-success {
      background-color: rgba(78, 201, 176, 0.2);
      color: var(--status-success);
    }
    .badge-error {
      background-color: rgba(241, 76, 76, 0.2);
      color: var(--status-error);
    }
    .badge-warning {
      background-color: rgba(204, 167, 0, 0.2);
      color: var(--status-warning);
    }

    .status-indicator {
      display: inline-block;
      width: 8px;
      height: 8px;
      border-radius: 50%;
      margin-right: 6px;
    }
    .status-indicator.status-running { background-color: var(--status-running); }
    .status-indicator.status-success, .status-indicator.status-completed { background-color: var(--status-success); }
    .status-indicator.status-failed { background-color: var(--status-error); }
    .status-indicator.status-blocked { background-color: var(--status-warning); }
    .status-indicator.status-idle { background-color: var(--status-neutral); }

    /* Overview Dashboard */
    .overview-banner {
      display: flex;
      justify-content: space-between;
      align-items: center;
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px 20px;
      margin-bottom: 20px;
    }

    .studio-title-row {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 6px;
    }
    .studio-title-row h2 {
      font-size: 18px;
      font-weight: 600;
    }

    .studio-meta {
      display: flex;
      gap: 16px;
      font-size: 12px;
      color: var(--text-secondary);
    }

    .banner-actions {
      display: flex;
      gap: 8px;
    }

    /* KPI Grid */
    .kpi-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 16px;
      margin-bottom: 20px;
    }

    .kpi-card {
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px;
      cursor: pointer;
      transition: background-color 0.15s, border-color 0.15s;
    }
    .kpi-card:hover {
      background-color: var(--bg-hover);
      border-color: var(--focus-border);
    }

    .kpi-header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 8px;
      font-size: 12px;
      color: var(--text-secondary);
    }
    .kpi-value {
      font-size: 24px;
      font-weight: 700;
    }
    .kpi-sub {
      font-size: 14px;
      font-weight: 400;
      color: var(--text-secondary);
    }

    /* Lifecycle Bar Card */
    .lifecycle-bar-card {
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px;
      margin-bottom: 20px;
    }
    .section-title {
      font-weight: 600;
      font-size: 13px;
      margin-bottom: 12px;
    }

    .lifecycle-chips {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }

    .chip {
      padding: 4px 10px;
      border-radius: 4px;
      font-size: 12px;
      cursor: pointer;
      background: var(--bg-hover);
      border: 1px solid var(--border-color);
    }
    .chip:hover {
      border-color: var(--focus-border);
    }

    /* Attention Queue */
    .attention-queue-container {
      margin-bottom: 20px;
    }
    .attention-header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 12px;
    }
    .attention-grid {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .attention-card {
      background-color: var(--bg-card);
      border-left: 4px solid var(--status-neutral);
      border-radius: 0 4px 4px 0;
      border-top: 1px solid var(--border-color);
      border-right: 1px solid var(--border-color);
      border-bottom: 1px solid var(--border-color);
      padding: 12px 16px;
    }
    .attention-card.severity-error, .attention-card.severity-critical {
      border-left-color: var(--status-error);
    }
    .attention-card.severity-warning {
      border-left-color: var(--status-warning);
    }
    .attention-card.severity-info {
      border-left-color: var(--status-running);
    }

    .attention-card-header {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 4px;
    }
    .attention-title {
      font-weight: 600;
      flex: 1;
    }
    .attention-desc {
      font-size: 12px;
      color: var(--text-secondary);
      margin-left: 24px;
    }

    /* Recent Events */
    .recent-events-card {
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px;
    }
    .recent-events-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 12px;
    }
    .recent-events-list {
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .recent-event-row {
      display: flex;
      align-items: center;
      gap: 12px;
      font-size: 12px;
      padding: 4px 0;
      border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    }
    .event-seq {
      font-family: var(--font-mono);
      color: var(--text-secondary);
      width: 40px;
    }
    .event-summary {
      flex: 1;
    }
    .event-time {
      color: var(--text-secondary);
    }

    /* Split Views (Agents & Tasks & Runs) */
    .agents-layout-split, .tasks-layout-split, .runs-layout-split {
      display: flex;
      gap: 20px;
      align-items: flex-start;
    }

    .agents-table-container, .runs-table-container {
      flex: 1;
      overflow-x: auto;
    }

    .agent-inspector-container, .run-inspector-container, .task-drawer-panel {
      width: 420px;
      flex-shrink: 0;
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px;
    }

    /* Data Tables */
    .data-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 12px;
      text-align: left;
    }
    .data-table th {
      padding: 8px 12px;
      border-bottom: 1px solid var(--border-color);
      color: var(--text-secondary);
      font-weight: 600;
    }
    .data-table td {
      padding: 8px 12px;
      border-bottom: 1px solid rgba(255, 255, 255, 0.05);
      vertical-align: middle;
    }
    .table-row {
      cursor: pointer;
    }
    .table-row:hover {
      background-color: var(--bg-hover);
    }
    .table-row.selected {
      background-color: rgba(55, 148, 255, 0.15);
      outline: 1px solid var(--focus-border);
    }

    /* Inspector Panes */
    .inspector-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 16px;
      border-bottom: 1px solid var(--border-color);
      padding-bottom: 8px;
    }
    .inspector-title-group {
      display: flex;
      align-items: center;
      gap: 8px;
    }
    .inspector-title-group h3 {
      font-size: 14px;
      font-weight: 600;
    }

    .inspector-meta-grid {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 12px;
      margin-bottom: 16px;
    }
    .meta-item {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .meta-label {
      font-size: 11px;
      color: var(--text-secondary);
    }
    .meta-value {
      font-size: 12px;
    }

    .inspector-section {
      margin-top: 16px;
      border-top: 1px solid var(--border-color);
      padding-top: 12px;
    }
    .inspector-section h4 {
      font-size: 12px;
      font-weight: 600;
      margin-bottom: 8px;
    }

    .inspector-error-box {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      background: rgba(241, 76, 76, 0.15);
      border: 1px solid var(--status-error);
      border-radius: 4px;
      color: var(--status-error);
      font-size: 12px;
      margin-bottom: 16px;
    }

    .tool-call-row {
      background-color: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 8px 10px;
      margin-bottom: 8px;
      font-size: 11px;
    }
    .tool-call-header {
      display: flex;
      justify-content: space-between;
      margin-bottom: 4px;
    }

    /* Sub Tabs (Tasks View) */
    .sub-nav-tabs {
      display: flex;
      gap: 6px;
    }
    .sub-tab-btn {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 10px;
      font-size: 12px;
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      color: var(--text-secondary);
      border-radius: 4px;
      cursor: pointer;
    }
    .sub-tab-btn.active {
      background-color: var(--bg-hover);
      color: var(--text-primary);
      border-color: var(--focus-border);
      font-weight: 600;
    }

    .tasks-content-area {
      flex: 1;
      min-width: 0;
    }

    /* Kanban Board */
    .kanban-board-container {
      display: flex;
      gap: 12px;
      overflow-x: auto;
      padding-bottom: 12px;
    }
    .kanban-column {
      width: 240px;
      flex-shrink: 0;
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      display: flex;
      flex-direction: column;
      max-height: calc(100vh - 220px);
    }
    .kanban-column-header {
      padding: 10px 12px;
      border-bottom: 1px solid var(--border-color);
      display: flex;
      align-items: center;
      gap: 6px;
      font-weight: 600;
      font-size: 12px;
    }
    .col-count {
      margin-left: auto;
      background-color: var(--bg-hover);
      padding: 1px 6px;
      border-radius: 10px;
      font-size: 10px;
    }
    .kanban-column-body {
      padding: 8px;
      overflow-y: auto;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .kanban-card {
      background-color: var(--bg-primary);
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 8px 10px;
      cursor: pointer;
      font-size: 12px;
    }
    .kanban-card:hover {
      border-color: var(--focus-border);
    }
    .kanban-card.selected {
      border-color: var(--focus-border);
      background-color: rgba(55, 148, 255, 0.1);
    }
    .kanban-card-title {
      font-weight: 600;
      margin-bottom: 6px;
    }
    .kanban-card-meta {
      display: flex;
      justify-content: space-between;
      font-size: 11px;
      color: var(--text-secondary);
    }

    /* DAG Graph SVG */
    .dag-graph-wrapper {
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      display: flex;
      flex-direction: column;
      height: 600px;
    }
    .dag-toolbar {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border-bottom: 1px solid var(--border-color);
    }
    .dag-svg-container {
      flex: 1;
      overflow: hidden;
      cursor: grab;
      position: relative;
    }
    .dag-edge {
      fill: none;
      stroke: var(--text-secondary);
      stroke-width: 1.5;
      opacity: 0.6;
    }
    .dag-arrow-head {
      fill: var(--text-secondary);
    }
    .dag-node {
      cursor: pointer;
    }
    .dag-node-bg {
      fill: var(--bg-primary);
      stroke: var(--border-color);
      stroke-width: 1.5;
    }
    .dag-node.status-running .dag-node-bg { stroke: var(--status-running); stroke-width: 2; }
    .dag-node.status-succeeded .dag-node-bg { stroke: var(--status-success); }
    .dag-node.status-failed .dag-node-bg { stroke: var(--status-error); }
    .dag-node.status-blocked .dag-node-bg { stroke: var(--status-warning); }
    .dag-node.selected .dag-node-bg {
      stroke: var(--focus-border);
      stroke-width: 2.5;
    }

    .dag-node-title {
      fill: var(--text-primary);
      font-size: 12px;
      font-weight: 600;
    }
    .dag-node-meta {
      fill: var(--text-secondary);
      font-size: 10px;
    }
    .dag-node-prog-bg {
      fill: rgba(255, 255, 255, 0.1);
    }
    .dag-node-prog-fill {
      fill: var(--status-running);
    }
    .dag-node-status {
      fill: var(--text-secondary);
      font-size: 9px;
      font-weight: 600;
    }

    /* Terminal Box */
    .terminal-box {
      background-color: #000000;
      color: #cccccc;
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 10px;
      font-family: var(--font-mono);
      font-size: 11px;
    }
    .terminal-cmd {
      color: #4ec9b0;
      margin-bottom: 6px;
    }
    .terminal-stdout, .terminal-stderr {
      max-height: 180px;
      overflow-y: auto;
      white-space: pre-wrap;
    }

    /* Timeline Stream */
    .timeline-stream-container {
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    .timeline-entry {
      display: flex;
      gap: 16px;
      align-items: flex-start;
    }
    .timeline-seq-col {
      width: 50px;
      text-align: right;
    }
    .seq-badge {
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--text-secondary);
    }
    .timeline-icon-col {
      display: flex;
      flex-direction: column;
      align-items: center;
      width: 20px;
    }
    .timeline-line {
      width: 2px;
      flex: 1;
      min-height: 24px;
      background-color: var(--border-color);
      margin-top: 4px;
    }
    .timeline-content-col {
      flex: 1;
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 4px;
      padding: 8px 12px;
    }
    .timeline-header-row {
      display: flex;
      align-items: center;
      gap: 8px;
      margin-bottom: 4px;
    }

    /* Diagnostics Grid */
    .diagnostics-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
      gap: 16px;
      margin-bottom: 20px;
    }
    .diag-card {
      background-color: var(--bg-card);
      border: 1px solid var(--border-color);
      border-radius: 6px;
      padding: 16px;
    }
    .diag-card-title {
      font-weight: 600;
      font-size: 13px;
      margin-bottom: 12px;
      border-bottom: 1px solid var(--border-color);
      padding-bottom: 6px;
    }
    .diag-prop {
      display: flex;
      justify-content: space-between;
      margin-bottom: 8px;
      font-size: 12px;
    }
    .diag-label {
      color: var(--text-secondary);
    }

    /* Progress bar */
    .progress-bar-container {
      background-color: rgba(255, 255, 255, 0.1);
      height: 4px;
      border-radius: 2px;
      overflow: hidden;
      margin: 4px 0;
    }
    .progress-bar-fill {
      background-color: var(--status-running);
      height: 100%;
    }

    /* Utilities */
    .text-success { color: var(--status-success); }
    .text-error { color: var(--status-error); }
    .text-warning { color: var(--status-warning); }
    .text-info { color: var(--status-running); }
    .text-secondary { color: var(--text-secondary); }
    .text-sm { font-size: 11px; }
    .text-xs { font-size: 10px; }

    /* Search inputs */
    .input-search {
      width: 100%;
      max-width: 320px;
      padding: 6px 10px;
      border-radius: 4px;
      background-color: var(--vscode-input-background, #3c3c3c);
      color: var(--vscode-input-foreground, #cccccc);
      border: 1px solid var(--vscode-input-border, var(--border-color));
      font-size: 12px;
      font-family: inherit;
    }
    .input-search:focus {
      outline: 1px solid var(--focus-border);
    }

    .timeline-filter-pills {
      display: flex;
      gap: 6px;
      margin-bottom: 16px;
    }
    .pill-btn {
      padding: 3px 8px;
      border-radius: 12px;
      background: var(--bg-hover);
      border: 1px solid var(--border-color);
      color: var(--text-secondary);
      font-size: 11px;
      cursor: pointer;
    }
    .pill-btn.active {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border-color: var(--vscode-button-background);
    }
  `;
}
