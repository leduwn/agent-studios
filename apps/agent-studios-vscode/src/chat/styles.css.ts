/**
 * Agent Studios — Clean Monochrome Chat Webview Styles
 * Built strictly with native VS Code theme variables (--vscode-*)
 * for Dark+, Light+, and High Contrast themes.
 */

export const CHAT_STYLES = `
:root {
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;
  --border-subtle: var(--vscode-widget-border, rgba(128,128,128,0.22));
}

*, *::before, *::after {
  box-sizing: border-box;
}

html, body {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif);
  font-size: var(--vscode-font-size, 13px);
  line-height: 1.45;
  color: var(--vscode-foreground, var(--vscode-editor-foreground));
  background: var(--vscode-sideBar-background, var(--vscode-editor-background));
}

.chat-container {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100vh;
  min-width: 0;
  overflow: hidden;
}

/* 1. Compact 1-Row Header */
.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 36px;
  min-height: 36px;
  padding: 0 10px;
  border-bottom: 1px solid var(--vscode-panel-border, var(--vscode-widget-border, rgba(128,128,128,0.18)));
  background: var(--vscode-sideBar-background, var(--vscode-editor-background));
}

.header-brand {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
  font-weight: 600;
  color: var(--vscode-foreground);
}

.header-title {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--vscode-descriptionForeground, var(--vscode-foreground));
  cursor: pointer;
  font-size: 13px;
}

.icon-btn:hover {
  background: var(--vscode-toolbar-hoverBackground, var(--vscode-list-hoverBackground));
  color: var(--vscode-foreground);
}

.icon-btn:focus-visible,
button:focus-visible,
select:focus-visible,
textarea:focus-visible {
  outline: 1px solid var(--vscode-focusBorder);
  outline-offset: 1px;
}

/* Collapsible History Drawer */
.history-drawer {
  border-bottom: 1px solid var(--vscode-panel-border, var(--vscode-widget-border, rgba(128,128,128,0.18)));
  background: var(--vscode-editorWidget-background, var(--vscode-sideBar-background));
  max-height: 190px;
  overflow-y: auto;
  padding: 6px 10px;
}

.history-drawer[hidden] {
  display: none;
}

.history-drawer-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--vscode-descriptionForeground);
  margin-bottom: 4px;
}

.history-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 5px 8px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--vscode-foreground);
  text-align: left;
  cursor: pointer;
  font-size: 12px;
}

.history-item:hover,
.history-item.active {
  background: var(--vscode-list-hoverBackground);
}

.history-item-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 2. Quiet 1-Line Workspace & Status Strip */
.context-strip {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 28px;
  padding: 4px 10px;
  font-size: 11px;
  color: var(--vscode-descriptionForeground);
  border-bottom: 1px solid var(--vscode-panel-border, var(--vscode-widget-border, rgba(128,128,128,0.14)));
}

.context-workspace {
  display: flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.workspace-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.context-status {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  display: inline-block;
  flex-shrink: 0;
}

.tone-ok .status-dot { background: var(--vscode-charts-green, #388a34); }
.tone-active .status-dot { background: var(--vscode-charts-blue, #3794ff); }
.tone-warn .status-dot { background: var(--vscode-charts-yellow, #cca700); }
.tone-error .status-dot { background: var(--vscode-charts-red, #f14c4c); }
.tone-neutral .status-dot { background: var(--vscode-descriptionForeground); }

.link-btn {
  border: none;
  background: transparent;
  padding: 0;
  font: inherit;
  color: var(--vscode-textLink-foreground);
  cursor: pointer;
}

.link-btn:hover {
  text-decoration: underline;
}

/* 3. Scrollable Conversation Thread */
.chat-messages {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  scroll-behavior: smooth;
  padding: 12px 12px 16px;
}

.chat-thread-inner {
  width: 100%;
  max-width: 760px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* Empty State & Onboarding */
.empty-state {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px 2px;
}

.empty-headline {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--vscode-foreground);
}

.empty-subtext {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--vscode-descriptionForeground);
}

.starter-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.starter-btn {
  text-align: left;
  width: 100%;
  padding: 8px 10px;
  border-radius: var(--radius-md);
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.22));
  background: var(--vscode-editorWidget-background, transparent);
  color: var(--vscode-foreground);
  font-size: 12px;
  cursor: pointer;
}

.starter-btn:hover {
  background: var(--vscode-list-hoverBackground);
  border-color: var(--vscode-focusBorder);
}

.onboarding-card {
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.25));
  border-radius: var(--radius-md);
  background: var(--vscode-editorWidget-background, rgba(128,128,128,0.06));
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.onboarding-card-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 8px;
}

.onboarding-title {
  font-size: 13px;
  font-weight: 600;
}

.onboarding-subtitle {
  margin-top: 2px;
  font-size: 12px;
  color: var(--vscode-descriptionForeground);
}

.onboarding-steps {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.onboarding-step {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--vscode-descriptionForeground);
}

.onboarding-step.done {
  color: var(--vscode-foreground);
}

.onboarding-step.current {
  color: var(--vscode-foreground);
  font-weight: 500;
}

.step-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  font-size: 10px;
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.35));
  flex-shrink: 0;
}

.onboarding-step.done .step-mark {
  border-color: var(--vscode-charts-green, #388a34);
  color: var(--vscode-charts-green, #388a34);
}

.onboarding-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

/* Conversation Messages */
.msg-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.msg-user {
  align-self: flex-end;
  max-width: 92%;
  padding: 8px 11px;
  border-radius: var(--radius-md);
  background: var(--vscode-input-background, var(--vscode-editorWidget-background));
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.2));
  color: var(--vscode-foreground);
  word-break: break-word;
  white-space: pre-wrap;
}

.msg-assistant {
  width: 100%;
  min-width: 0;
  color: var(--vscode-foreground);
  word-break: break-word;
}

.msg-system {
  font-size: 12px;
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  background: var(--vscode-editorWidget-background);
  border-left: 2px solid var(--vscode-charts-yellow, #cca700);
  color: var(--vscode-foreground);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}

.msg-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--vscode-descriptionForeground);
}

.markdown-body p { margin: 0 0 8px; }
.markdown-body p:last-child { margin-bottom: 0; }
.markdown-body ul, .markdown-body ol { margin: 4px 0 8px; padding-left: 18px; }
.markdown-body code {
  font-family: var(--vscode-editor-font-family, monospace);
  font-size: 12px;
  padding: 1px 4px;
  border-radius: 3px;
  background: var(--vscode-textCodeBlock-background, rgba(128,128,128,0.14));
}

.code-block-wrap {
  margin: 8px 0;
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.22));
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--vscode-editor-background);
}

.code-block-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px;
  font-size: 11px;
  color: var(--vscode-descriptionForeground);
  background: var(--vscode-editorWidget-background);
  border-bottom: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.16));
}

.code-block-wrap pre {
  margin: 0;
  padding: 8px 10px;
  overflow-x: auto;
  font-family: var(--vscode-editor-font-family, monospace);
  font-size: 12px;
  line-height: 1.45;
}

.code-block-wrap pre code {
  padding: 0;
  background: transparent;
}

.copy-code-btn {
  border: none;
  background: transparent;
  color: var(--vscode-descriptionForeground);
  font-size: 11px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 3px;
}

.copy-code-btn:hover {
  background: var(--vscode-list-hoverBackground);
  color: var(--vscode-foreground);
}

.streaming-dot {
  display: inline-block;
  width: 6px;
  height: 12px;
  background: var(--vscode-foreground);
  opacity: 0.7;
  vertical-align: middle;
  margin-left: 3px;
}

/* Collapsible Grouped Tool Activity & Multi-Agent Disclosure */
.tool-group,
.multi-agent-card,
details.collapsible {
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-md);
  background: var(--vscode-editorWidget-background, rgba(128,128,128,0.05));
  font-size: 12px;
}

.tool-group summary,
.multi-agent-card summary,
details.collapsible summary {
  cursor: pointer;
  padding: 6px 10px;
  list-style: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  color: var(--vscode-foreground);
  user-select: none;
}

.tool-group summary::-webkit-details-marker,
.multi-agent-card summary::-webkit-details-marker {
  display: none;
}

.tool-group-list,
.multi-agent-list {
  border-top: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.16));
  padding: 6px 10px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}

.tool-row,
.agent-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  font-size: 11px;
  min-width: 0;
}

.tool-row-name,
.agent-row-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tool-row-status {
  color: var(--vscode-descriptionForeground);
  flex-shrink: 0;
}

/* Diff & Approval Cards */
.diff-card,
.approval-card {
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.25));
  border-radius: var(--radius-md);
  background: var(--vscode-editorWidget-background);
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
}

.diff-card-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}

.diff-file-path {
  font-family: var(--vscode-editor-font-family, monospace);
  font-size: 11px;
  word-break: break-all;
}

.card-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

/* Buttons */
.btn-primary,
.btn-secondary,
.btn-ghost {
  font-family: inherit;
  font-size: 12px;
  border-radius: var(--radius-sm);
  padding: 5px 10px;
  cursor: pointer;
  border: 1px solid transparent;
}

.btn-sm {
  font-size: 11px;
  padding: 4px 8px;
}

.btn-primary {
  background: var(--vscode-button-background);
  color: var(--vscode-button-foreground);
}

.btn-primary:hover {
  background: var(--vscode-button-hoverBackground);
}

.btn-secondary {
  background: var(--vscode-button-secondaryBackground);
  color: var(--vscode-button-secondaryForeground);
}

.btn-secondary:hover {
  background: var(--vscode-button-secondaryHoverBackground);
}

.btn-ghost {
  background: transparent;
  color: var(--vscode-foreground);
  border-color: var(--vscode-widget-border, rgba(128,128,128,0.25));
}

.btn-ghost:hover {
  background: var(--vscode-list-hoverBackground);
}

/* 4. Compact Docked Composer */
.chat-composer {
  position: sticky;
  bottom: 0;
  z-index: 10;
  padding: 8px 10px 10px;
  border-top: 1px solid var(--vscode-panel-border, var(--border-subtle));
  background: var(--vscode-sideBar-background, var(--vscode-editor-background));
}

.composer-inner {
  width: 100%;
  max-width: 760px;
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.composer-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--radius-sm);
  background: var(--vscode-editorWidget-background);
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.25));
  font-size: 11px;
}

.composer-banner[hidden] {
  display: none;
}

.attachment-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.attachment-chips:empty {
  display: none;
}

.attachment-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 6px;
  border-radius: var(--radius-sm);
  font-size: 11px;
  background: var(--vscode-editorWidget-background);
  border: 1px solid var(--vscode-widget-border);
}

.composer-box {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 7px 8px;
  border-radius: var(--radius-md);
  background: var(--vscode-input-background);
  border: 1px solid var(--vscode-input-border, var(--vscode-widget-border, rgba(128,128,128,0.3)));
}

.composer-box:focus-within {
  border-color: var(--vscode-focusBorder);
}

#messageInput {
  width: 100%;
  min-height: 36px;
  max-height: 160px;
  resize: none;
  border: none;
  background: transparent;
  color: var(--vscode-input-foreground);
  font-family: inherit;
  font-size: inherit;
  line-height: 1.4;
  padding: 0;
}

#messageInput:focus {
  outline: none;
}

.composer-controls {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  flex-wrap: wrap;
}

.composer-selects {
  display: flex;
  align-items: center;
  gap: 4px;
  flex: 1;
  min-width: 0;
  flex-wrap: wrap;
}

.compact-select {
  height: 24px;
  padding: 0 6px;
  font-size: 11px;
  font-family: inherit;
  color: var(--vscode-dropdown-foreground, var(--vscode-foreground));
  background: var(--vscode-dropdown-background, transparent);
  border: 1px solid var(--vscode-dropdown-border, transparent);
  border-radius: var(--radius-sm);
  max-width: 100%;
  min-width: 0;
}

.model-select {
  flex: 1;
  min-width: 96px;
  max-width: 210px;
}

.composer-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.send-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: var(--radius-sm);
  background: var(--vscode-button-background);
  color: var(--vscode-button-foreground);
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}

.send-btn:hover {
  background: var(--vscode-button-hoverBackground);
}

.send-btn.is-stop {
  background: var(--vscode-charts-red, #c72e0f);
  color: #ffffff;
}

.advanced-popover {
  border: 1px solid var(--vscode-widget-border, rgba(128,128,128,0.28));
  border-radius: var(--radius-md);
  background: var(--vscode-editorWidget-background);
  padding: 10px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  font-size: 11px;
}

.advanced-popover[hidden] {
  display: none;
}

.advanced-field {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.advanced-field label {
  color: var(--vscode-descriptionForeground);
  font-size: 11px;
}

.advanced-field-full {
  grid-column: 1 / -1;
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

@media (max-width: 300px) {
  .chat-header { padding: 0 8px; }
  .chat-messages { padding: 10px 8px 12px; }
  .chat-composer { padding: 6px 8px 8px; }
  .advanced-popover { grid-template-columns: 1fr; }
  .model-select { max-width: 120px; }
}

@media (min-width: 301px) and (max-width: 599px) {
  .model-select { max-width: 190px; }
}

@media (min-width: 600px) {
  .chat-messages { padding: 16px 24px 20px; }
  .chat-composer { padding: 10px 24px 12px; }
}

body.vscode-high-contrast .composer-box,
body.vscode-high-contrast .tool-group,
body.vscode-high-contrast-light .composer-box,
body.vscode-high-contrast-light .tool-group {
  border: 1px solid var(--vscode-contrastBorder, var(--vscode-focusBorder));
}

@media (forced-colors: active) {
  .composer-box,
  .onboarding-card,
  .tool-group,
  .diff-card,
  .approval-card,
  .multi-agent-card {
    border: 1px solid CanvasText;
  }
}

@media (prefers-reduced-motion: reduce) {
  .chat-messages {
    scroll-behavior: auto;
  }
  * {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
`;

export function getChatStyles(): string {
  return CHAT_STYLES;
}

