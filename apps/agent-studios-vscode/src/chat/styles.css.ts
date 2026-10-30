/**
 * Agent Studios — Chat WebviewView Full Stylesheet
 * Milestone M14-UI-01: Professional AI Coding Assistant Experience
 *
 * Monochrome, clean developer aesthetic matching native VS Code tokens.
 * Responsive, accessible, sticky prompt composer, subtle 1px borders.
 */

export const CHAT_STYLES = `
/* ==========================================================================
   CSS Custom Properties & Native VS Code Token Mapping
   ========================================================================== */
:root {
  --bg-primary: var(--vscode-editor-background, #1e1e1e);
  --fg-primary: var(--vscode-editor-foreground, #cccccc);
  --bg-sidebar: var(--vscode-sideBar-background, #252526);
  --fg-sidebar: var(--vscode-sideBar-foreground, #cccccc);
  --fg-subtle: var(--vscode-descriptionForeground, #858585);

  --border-subtle: var(--vscode-widget-border, var(--vscode-panel-border, rgba(128, 128, 128, 0.2)));
  --border-active: var(--vscode-focusBorder, #007fd4);

  --input-bg: var(--vscode-input-background, #3c3c3c);
  --input-fg: var(--vscode-input-foreground, #cccccc);
  --input-border: var(--vscode-input-border, transparent);
  --input-placeholder: var(--vscode-input-placeholderForeground, #767676);

  --btn-bg: var(--vscode-button-background, #0e639c);
  --btn-fg: var(--vscode-button-foreground, #ffffff);
  --btn-hover: var(--vscode-button-hoverBackground, #1177bb);
  --btn-secondary-bg: var(--vscode-button-secondaryBackground, #3a3d41);
  --btn-secondary-fg: var(--vscode-button-secondaryForeground, #ffffff);

  --code-bg: var(--vscode-textCodeBlock-background, rgba(0, 0, 0, 0.25));
  --badge-bg: var(--vscode-badge-background, #4d4d4d);
  --badge-fg: var(--vscode-badge-foreground, #ffffff);

  /* Status Colors */
  --status-green: var(--vscode-charts-green, #89d185);
  --status-yellow: var(--vscode-charts-yellow, #cca700);
  --status-red: var(--vscode-charts-red, #f14c4c);

  --font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif);
  --font-size: var(--vscode-font-size, 13px);
  --font-mono: var(--vscode-editor-font-family, Menlo, Monaco, "Courier New", monospace);
}

/* ==========================================================================
   Base Reset & Layout Structure
   ========================================================================== */
*, *::before, *::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

html, body {
  width: 100%;
  height: 100%;
  overflow: hidden;
  background-color: var(--bg-primary);
  color: var(--fg-primary);
  font-family: var(--font-family);
  font-size: var(--font-size);
  line-height: 1.5;
  -webkit-font-smoothing: antialiased;
}

.chat-container {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100%;
  background-color: var(--bg-primary);
  overflow: hidden;
  position: relative;
}

/* ==========================================================================
   Header Bar (Controls Bar)
   ========================================================================== */
.chat-header {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 12px;
  background-color: var(--bg-sidebar);
  border-bottom: 1px solid var(--border-subtle);
  z-index: 10;
}

.chat-controls-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
}

/* ==========================================================================
   ModeSelector Styles (Ask, Plan, Agent)
   ========================================================================== */
.mode-selector-container {
  display: inline-flex;
  align-items: center;
  background-color: var(--bg-primary);
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
  padding: 2px;
  gap: 2px;
  user-select: none;
}

.mode-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: transparent;
  color: var(--fg-subtle);
  border: 1px solid transparent;
  border-radius: 3px;
  padding: 3px 8px;
  font-family: inherit;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.15s ease-in-out;
}

.mode-pill:hover:not(:disabled):not(.unsupported) {
  background-color: rgba(255, 255, 255, 0.06);
  color: var(--fg-primary);
}

.mode-pill.active {
  background-color: var(--btn-secondary-bg);
  color: var(--btn-secondary-fg);
  border-color: var(--border-subtle);
  font-weight: 600;
}

.mode-pill.unsupported {
  opacity: 0.5;
  cursor: not-allowed;
  text-decoration: line-through;
  color: var(--status-red);
}

.mode-unsupported-badge {
  font-size: 9px;
  text-transform: uppercase;
  color: var(--status-red);
  margin-left: 2px;
}

/* ==========================================================================
   ProviderModelSelector Styles (Read-Only)
   ========================================================================== */
.provider-model-selector {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background-color: var(--input-bg);
  border: 1px solid var(--input-border);
  border-radius: 4px;
  padding: 2px 6px;
  max-width: 100%;
  flex-shrink: 1;
  min-width: 0;
}

.provider-model-selector.empty {
  color: var(--fg-subtle);
  font-size: 11px;
  padding: 4px 8px;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
  background-color: var(--fg-subtle);
}

.status-dot.status-available {
  background-color: var(--status-green);
  box-shadow: 0 0 4px var(--status-green);
}

.status-dot.status-unconfigured {
  background-color: var(--status-yellow);
}

.status-dot.status-offline {
  background-color: var(--status-red);
}

.provider-model-dropdown {
  background: transparent;
  color: var(--input-fg);
  border: none;
  font-family: inherit;
  font-size: 11px;
  outline: none;
  cursor: pointer;
  flex: 1;
  min-width: 0;
  text-overflow: ellipsis;
  overflow: hidden;
  white-space: nowrap;
}

.provider-model-dropdown option,
.provider-model-dropdown optgroup {
  background-color: var(--bg-sidebar);
  color: var(--fg-primary);
}

.context-window-badge {
  display: inline-block;
  background-color: var(--badge-bg);
  color: var(--badge-fg);
  font-size: 10px;
  font-family: var(--font-mono);
  padding: 1px 4px;
  border-radius: 3px;
  flex-shrink: 0;
}

/* ==========================================================================
   Conversation Message Stream
   ========================================================================== */
.chat-messages {
  flex: 1 1 auto;
  overflow-y: auto;
  overflow-x: hidden;
  scroll-behavior: smooth;
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.chat-messages::-webkit-scrollbar {
  width: 10px;
}

.chat-messages::-webkit-scrollbar-thumb {
  background-color: rgba(128, 128, 128, 0.2);
  border-radius: 5px;
}

.chat-messages::-webkit-scrollbar-thumb:hover {
  background-color: rgba(128, 128, 128, 0.35);
}

/* Message Bubble */
.message-bubble {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-width: 100%;
  animation: fadeIn 0.15s ease-in;
}

.message-bubble.user {
  align-self: flex-end;
  background-color: var(--btn-secondary-bg);
  border: 1px solid var(--border-subtle);
  border-radius: 8px 8px 2px 8px;
  padding: 10px 14px;
  max-width: 90%;
}

.message-bubble.assistant {
  align-self: flex-start;
  background-color: transparent;
  border: 1px solid var(--border-subtle);
  border-radius: 8px 8px 8px 2px;
  padding: 10px 14px;
  width: 100%;
}

.message-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--fg-subtle);
  margin-bottom: 2px;
}

.message-body {
  font-size: 13px;
  line-height: 1.55;
  word-break: break-word;
}

.message-body pre {
  background-color: var(--code-bg);
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
  padding: 8px 10px;
  overflow-x: auto;
  font-family: var(--font-mono);
  font-size: 12px;
  margin: 8px 0;
}

.message-body code {
  font-family: var(--font-mono);
  font-size: 12px;
  background-color: var(--code-bg);
  padding: 2px 4px;
  border-radius: 3px;
}

/* ==========================================================================
   Collapsible Sections (Details & Summary)
   ========================================================================== */
details.collapsible {
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
  background-color: rgba(0, 0, 0, 0.1);
  overflow: hidden;
  margin: 6px 0;
}

details.collapsible summary {
  padding: 6px 10px;
  cursor: pointer;
  font-weight: 500;
  font-size: 12px;
  color: var(--fg-subtle);
  user-select: none;
  display: flex;
  align-items: center;
  gap: 6px;
  background-color: rgba(255, 255, 255, 0.02);
  transition: background-color 0.15s ease;
}

details.collapsible summary:hover {
  background-color: rgba(255, 255, 255, 0.05);
  color: var(--fg-primary);
}

details.collapsible[open] summary {
  border-bottom: 1px solid var(--border-subtle);
}

.collapsible-content {
  padding: 8px 10px;
  font-size: 12px;
}

/* ==========================================================================
   Sticky Bottom Prompt Composer
   ========================================================================== */
.chat-composer {
  position: sticky;
  bottom: 0;
  flex-shrink: 0;
  background-color: var(--bg-sidebar);
  border-top: 1px solid var(--border-subtle);
  padding: 10px 12px;
  z-index: 100;
  box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.15);
}

.composer-box {
  display: flex;
  flex-direction: column;
  background-color: var(--input-bg);
  border: 1px solid var(--input-border);
  border-radius: 6px;
  transition: border-color 0.15s ease;
}

.composer-box:focus-within {
  border-color: var(--border-active);
}

.composer-textarea {
  width: 100%;
  min-height: 48px;
  max-height: 200px;
  background: transparent;
  color: var(--input-fg);
  border: none;
  outline: none;
  resize: vertical;
  padding: 8px 10px;
  font-family: inherit;
  font-size: var(--font-size);
  line-height: 1.45;
}

.composer-textarea::placeholder {
  color: var(--input-placeholder);
}

.composer-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 10px;
  border-top: 1px solid var(--border-subtle);
}

.composer-hints {
  font-size: 11px;
  color: var(--fg-subtle);
  display: flex;
  align-items: center;
  gap: 8px;
}

.composer-buttons {
  display: flex;
  align-items: center;
  gap: 6px;
}

.btn-primary {
  background-color: var(--btn-bg);
  color: var(--btn-fg);
  border: 1px solid transparent;
  border-radius: 4px;
  padding: 4px 12px;
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.btn-primary:hover:not(:disabled) {
  background-color: var(--btn-hover);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* ==========================================================================
   Accessibility & Focus Rings
   ========================================================================== */
:focus-visible {
  outline: 1px solid var(--vscode-focusBorder);
  outline-offset: 2px;
}

.sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(3px); }
  to { opacity: 1; transform: translateY(0); }
}

/* Reduced Motion */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}

/* High-Contrast Themes */
@media (forced-colors: active) {
  .mode-pill,
  .mode-selector-container,
  .provider-model-selector,
  .message-bubble,
  details.collapsible,
  .chat-composer,
  .composer-box,
  .btn-primary {
    border: 1px solid ButtonText !important;
  }

  .mode-pill.active {
    background-color: Highlight !important;
    color: HighlightText !important;
  }

  :focus-visible {
    outline: 2px solid Highlight !important;
  }
}

body.vscode-high-contrast,
body.vscode-high-contrast-light {
  --border-subtle: var(--vscode-widget-border, #ffffff);
}

body.vscode-high-contrast .mode-pill.active,
body.vscode-high-contrast-light .mode-pill.active {
  border: 1px solid var(--vscode-focusBorder, #ffffff);
}

body.vscode-high-contrast :focus-visible,
body.vscode-high-contrast-light :focus-visible {
  outline: 2px solid var(--vscode-focusBorder, #ffffff);
  outline-offset: 2px;
}

/* ==========================================================================
   Responsive Breakpoints
   - Narrow sidebar: <= 300px
   - Standard sidebar: 301px - 599px (~400px)
   - Wide secondary sidebar / panel: >= 600px
   ========================================================================== */
@media (max-width: 300px) {
  .chat-header {
    padding: 6px 8px;
    gap: 6px;
  }

  .chat-controls-row {
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
  }

  .mode-selector-container {
    width: 100%;
    display: flex;
    justify-content: stretch;
  }

  .mode-pill {
    flex: 1;
    justify-content: center;
    padding: 3px 4px;
    font-size: 10px;
  }

  .provider-model-selector {
    width: 100%;
  }

  .chat-messages {
    padding: 10px 8px;
    gap: 12px;
  }

  .chat-composer {
    padding: 8px 8px;
  }

  .composer-hints {
    display: none;
  }
}

@media (min-width: 301px) and (max-width: 599px) {
  .chat-header {
    padding: 8px 12px;
  }

  .chat-controls-row {
    flex-wrap: wrap;
  }

  .provider-model-selector {
    flex-grow: 1;
  }
}

@media (min-width: 600px) {
  .chat-header {
    padding: 10px 16px;
    gap: 10px;
  }

  .chat-controls-row {
    flex-direction: row;
    justify-content: space-between;
  }

  .chat-messages {
    padding: 20px 24px;
    max-width: 900px;
    margin: 0 auto;
    width: 100%;
  }

  .chat-composer {
    padding: 12px 16px;
  }

  .composer-box {
    max-width: 900px;
    margin: 0 auto;
  }

  .mode-pill {
    padding: 4px 12px;
    font-size: 12px;
  }
}

/* ==========================================================================
   Milestone M15 UI-01: Interactive Agent Execution Workbench Styles
   ========================================================================== */
.execution-workbench {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 8px 0;
  width: 100%;
}

.workbench-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  background-color: rgba(255, 255, 255, 0.02);
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  padding: 6px 10px;
}

.workbench-field {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  min-width: 130px;
}

.workbench-field-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--fg-subtle);
}

.workbench-select {
  background-color: var(--input-bg);
  color: var(--input-fg);
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
  padding: 4px 6px;
  font-size: 11px;
  font-family: inherit;
  outline: none;
  cursor: pointer;
}

.workbench-select:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.workbench-select option:disabled {
  color: var(--fg-subtle);
}

.workspace-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  padding: 3px 8px;
  border-radius: 4px;
  border: 1px solid var(--border-subtle);
  background-color: rgba(0, 0, 0, 0.2);
}

.workspace-badge--matched {
  color: var(--status-green);
  border-color: rgba(152, 195, 121, 0.4);
}

.workspace-badge--mismatched {
  color: var(--status-red);
  border-color: rgba(224, 108, 117, 0.4);
}

/* Execution Summary Card */
.execution-summary-card {
  background-color: rgba(0, 0, 0, 0.15);
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  padding: 8px 10px;
  font-size: 11px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.summary-header {
  font-weight: 600;
  color: var(--fg-subtle);
  text-transform: uppercase;
  font-size: 10px;
  letter-spacing: 0.5px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
  gap: 6px;
}

.summary-item {
  display: flex;
  flex-direction: column;
}

.summary-label {
  font-size: 10px;
  color: var(--fg-subtle);
}

.summary-value {
  font-weight: 500;
  color: var(--fg-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* Prompt Boundary Warnings & Errors */
.composer-warning-banner {
  background-color: rgba(229, 192, 123, 0.15);
  border: 1px solid var(--status-yellow);
  color: var(--status-yellow);
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

.composer-error-banner {
  background-color: rgba(224, 108, 117, 0.15);
  border: 1px solid var(--status-red);
  color: var(--status-red);
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 4px;
}

/* Execution Streaming & Live Output Pane */
.execution-streaming-pane {
  background-color: rgba(0, 0, 0, 0.2);
  border: 1px solid var(--border-subtle);
  border-radius: 6px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 6px;
}

.execution-pane-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.execution-status-group {
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-badge {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  padding: 2px 6px;
  border-radius: 3px;
  letter-spacing: 0.5px;
}

.status-badge.status-idle {
  background-color: rgba(128, 128, 128, 0.2);
  color: var(--fg-subtle);
}

.status-badge.status-submitting {
  background-color: rgba(229, 192, 123, 0.2);
  color: var(--status-yellow);
}

.status-badge.status-running {
  background-color: rgba(97, 175, 239, 0.2);
  color: var(--status-blue);
}

.status-badge.status-succeeded {
  background-color: rgba(152, 195, 121, 0.2);
  color: var(--status-green);
}

.status-badge.status-failed {
  background-color: rgba(224, 108, 117, 0.2);
  color: var(--status-red);
}

.status-badge.status-cancelled {
  background-color: rgba(128, 128, 128, 0.3);
  color: var(--fg-subtle);
}

.execution-timer {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--fg-subtle);
}

/* Active Tool Activity */
.active-tool-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background-color: rgba(97, 175, 239, 0.1);
  border: 1px solid rgba(97, 175, 239, 0.3);
  border-radius: 4px;
  padding: 4px 8px;
  font-size: 11px;
}

.active-tool-info {
  display: flex;
  align-items: center;
  gap: 6px;
}

.active-tool-spinner {
  display: inline-block;
  width: 10px;
  height: 10px;
  border: 2px solid rgba(97, 175, 239, 0.3);
  border-radius: 50%;
  border-top-color: var(--status-blue);
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

/* Streaming Console */
.streaming-console-box {
  display: flex;
  flex-direction: column;
  background-color: #121212;
  border: 1px solid var(--border-subtle);
  border-radius: 4px;
  overflow: hidden;
}

.streaming-console-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px;
  font-size: 10px;
  text-transform: uppercase;
  color: var(--fg-subtle);
  background-color: rgba(255, 255, 255, 0.03);
  border-bottom: 1px solid var(--border-subtle);
}

.streaming-console-output {
  box-sizing: border-box;
  margin: 0;
  padding: 8px 10px;
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.4;
  color: #abb2bf;
  background: transparent;
  max-height: 180px;
  min-height: 50px;
  overflow-y: auto;
  white-space: pre-wrap;
  word-break: break-all;
}

/* Truthful Metrics Display */
.truthful-metrics-container {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 11px;
  padding: 4px 0;
  color: var(--fg-subtle);
}

.metrics-unreported-badge {
  font-style: italic;
  font-size: 10px;
  color: var(--fg-subtle);
}

.metrics-value {
  color: var(--fg-primary);
  font-weight: 500;
}
}
`.trim();

/**
 * Returns full chat webview stylesheet string.
 */
export function getChatStyles(): string {
  return CHAT_STYLES;
}

export default CHAT_STYLES;
