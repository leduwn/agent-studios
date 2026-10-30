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

export interface ChatHtmlInitialState {
  session?: SessionInfo | null;
  sessions?: SessionSummary[];
  connectionState?: ConnectionState;
  isPreview?: boolean;
  providers?: ProviderSummary[];
  activeMode?: AgentMode;
  selectedModelId?: string;
  selectedProviderId?: string;
  composerDraft?: string;
  messages?: ChatMessage[];
  attachments?: ContextAttachment[];
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

    // Model options HTML
    const modelOptionsHtml = ChatHtmlBuilder.renderModelOptions(
      providers,
      selectedProviderId,
      selectedModelId
    );

    // Session options HTML
    const sessionOptionsHtml = ChatHtmlBuilder.renderSessionOptions(sessions, sessionId);

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
      composerDraft,
      messages,
      attachments,
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

    <!-- Conversation Stream Container -->
    <main id="conversationContainer" class="chat-messages" role="log" aria-live="polite">
      ${messages.length === 0 ? ChatHtmlBuilder.renderEmptyState() : ''}
    </main>

    <!-- Sticky Bottom Composer -->
    <footer class="chat-composer" role="region" aria-label="Message Composer">
      <!-- Context Chips Bar -->
      <div id="contextChipsBar" class="context-chips-bar" aria-label="Attached Context Files">
        <button id="addContextBtn" class="chip-add-btn" title="Add file context from workspace" aria-label="Add file context">+ Add File</button>
        <button id="addActiveEditorBtn" class="chip-add-btn" title="Attach active editor file" aria-label="Attach active editor">Attach Active File</button>
      </div>

      <!-- Composer Input Box -->
      <div class="composer-box">
        <textarea
          id="composerInput"
          class="composer-textarea"
          rows="1"
          placeholder="Ask Agent Studios, plan architecture, or instruct agents... (Enter to send, Shift+Enter for newline)"
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
            <button id="stopBtn" class="btn-stop" style="display: none;" title="Stop execution" aria-label="Stop execution">
              ■ Stop
            </button>
            <button id="sendBtn" class="btn-primary" title="Send prompt (Enter)" aria-label="Send message">
              Send
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
        isExecuting: false
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

      function submitPrompt() {
        var text = composerInput.value.trim();
        if (!text || state.isExecuting) return;

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
        vscode.postMessage({ command: 'saveDraft', draft: '' });

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

      // Event Listeners
      composerInput.addEventListener('keydown', function(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          submitPrompt();
        }
      });

      composerInput.addEventListener('input', function() {
        autoGrowTextarea();
        state.composerDraft = composerInput.value;
        vscode.postMessage({ command: 'saveDraft', draft: composerInput.value });
      });

      sendBtn.addEventListener('click', function() {
        submitPrompt();
      });

      stopBtn.addEventListener('click', function() {
        vscode.postMessage({ command: 'cancelRun' });
      });

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

      // Card action clicks delegation (approve, deny, open-diff, open-task-graph)
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
      });

      // Incoming message listener from VS Code Extension Host
      window.addEventListener('message', function(event) {
        var msg = event.data;
        if (!msg) return;

        switch (msg.type) {
          case 'streamChunk':
            var lastMsg = state.messages[state.messages.length - 1];
            if (lastMsg && lastMsg.role === 'assistant') {
              if (msg.chunk) {
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
            }
            break;
        }
      });

      // Initial layout setup
      renderContextChips();
      renderMessages();
      autoGrowTextarea();
    })();
  </script>
</body>
</html>`;
  }

  public static renderEmptyState(): string {
    return `
      <div class="chat-empty-state">
        <span style="font-size: 28px; line-height: 1;">⚡</span>
        <h3>Agent Studios Assistant</h3>
        <p>Ask questions, explore code, propose plans, or delegate multi-agent tasks.</p>
      </div>`.trim();
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
}
