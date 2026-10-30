/**
 * Agent Studios — Mission Control HTML Builder
 * Milestone M14 Mission Control Workbench
 *
 * Assembles the complete WebviewPanel HTML document with:
 * - Cryptographic nonce-based Content Security Policy
 * - Persistent top navigation bar
 * - 9 tab view containers with active tab state
 * - Full client-side message routing, DAG pan/zoom, timeline filtering, and keyboard navigation
 */

import * as crypto from 'crypto';
import * as vscode from 'vscode';
import { MissionControlState, MissionControlTabId } from './models/types';
import { getMissionControlCss } from './styles/missionControlStyles';
import { renderOverviewView } from './views/OverviewView';
import { renderAgentsView } from './views/AgentsView';
import { renderTasksView } from './views/TasksView';
import { renderRunsView } from './views/RunsView';
import { renderTimelineView } from './views/TimelineView';
import { renderReviewView } from './views/ReviewView';
import { renderApprovalsView } from './views/ApprovalsView';
import { renderProvidersView } from './views/ProvidersView';
import { renderDiagnosticsView } from './views/DiagnosticsView';

export class MissionControlHtmlBuilder {
  /**
   * Generates individual view fragment HTML strings for all 9 tabs.
   * Used for initial render and incremental webview DOM patching.
   */
  public static buildViewFragments(
    state: MissionControlState
  ): Record<MissionControlTabId, string> {
    return {
      overview: renderOverviewView(state),
      agents: renderAgentsView(state),
      tasks: renderTasksView(state),
      runs: renderRunsView(state),
      timeline: renderTimelineView(state),
      review: renderReviewView(state),
      approvals: renderApprovalsView(state),
      providers: renderProvidersView(state),
      diagnostics: renderDiagnosticsView(state),
    };
  }

  /**
   * Generates complete HTML string for the Mission Control WebviewPanel.
   */
  public static buildHtml(
    webview: vscode.Webview,
    _extensionUri: vscode.Uri,
    state: MissionControlState
  ): string {
    const nonce = crypto.randomBytes(16).toString('base64');
    const activeTab = state.activeTab || 'overview';
    const pendingApprovalsCount = state.approvals.filter((a) => a.status === 'pending').length;

    // Build navigation tabs
    const tabs: Array<{ id: MissionControlTabId; label: string; icon: string; badge?: number }> = [
      { id: 'overview', label: 'Overview', icon: 'dashboard' },
      { id: 'agents', label: 'Agents', icon: 'hubot', badge: state.fleet.length },
      { id: 'tasks', label: 'Tasks', icon: 'checklist', badge: state.tasks.length },
      { id: 'runs', label: 'Runs', icon: 'history' },
      { id: 'timeline', label: 'Timeline', icon: 'pulse' },
      { id: 'review', label: 'Review', icon: 'diff', badge: state.artifacts.length },
      {
        id: 'approvals',
        label: 'Approvals',
        icon: 'shield',
        badge: pendingApprovalsCount > 0 ? pendingApprovalsCount : undefined,
      },
      { id: 'providers', label: 'Providers', icon: 'server' },
      { id: 'diagnostics', label: 'Diagnostics', icon: 'tools' },
    ];

    const tabsHtml = tabs
      .map((tab) => {
        const isActive = tab.id === activeTab;
        const badgeHtml =
          tab.badge !== undefined && tab.badge > 0
            ? `<span class="badge ${tab.id === 'approvals' ? 'badge-warning' : 'badge-secondary'}">${tab.badge}</span>`
            : '';
        return `
          <button class="nav-tab-btn ${isActive ? 'active' : ''}" data-tab="${tab.id}" onclick="switchTab('${tab.id}')">
            <span class="codicon codicon-${tab.icon}"></span>
            <span>${tab.label}</span>
            ${badgeHtml}
          </button>
        `;
      })
      .join('');

    // Pre-render all 9 views
    const fragments = MissionControlHtmlBuilder.buildViewFragments(state);

    const clientStateJson = JSON.stringify({
      activeTab: state.activeTab,
      selectedAgentId: state.selectedAgentId,
      selectedTaskId: state.selectedTaskId,
      selectedRunId: state.selectedRunId,
      isPreview: state.isPreview,
    }).replace(/</g, '\\u003c');

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; font-src ${webview.cspSource}; img-src ${webview.cspSource} data: https:;">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Studios — Mission Control</title>
  <style>
    ${getMissionControlCss()}

    /* Codicon fallback glyphs */
    .codicon {
      font-family: inherit;
      font-style: normal;
      display: inline-block;
      text-align: center;
      line-height: 1;
    }
    .codicon-dashboard::before { content: "📊"; }
    .codicon-hubot::before { content: "🤖"; }
    .codicon-checklist::before { content: "📋"; }
    .codicon-history::before { content: "⏱"; }
    .codicon-pulse::before { content: "📈"; }
    .codicon-diff::before { content: "🔀"; }
    .codicon-shield::before { content: "🛡"; }
    .codicon-server::before { content: "⚡"; }
    .codicon-tools::before { content: "🔧"; }
    .codicon-refresh::before { content: "🔄"; }
    .codicon-comment-discussion::before { content: "💬"; }
    .codicon-plus::before { content: "+"; }
    .codicon-close::before { content: "✕"; }
    .codicon-check::before { content: "✓"; }
    .codicon-zoom-in::before { content: "🔍+"; }
    .codicon-zoom-out::before { content: "🔍-"; }
    .codicon-screen-normal::before { content: "⛶"; }
    .codicon-info::before { content: "ℹ"; }
    .codicon-warning::before { content: "⚠"; }
    .codicon-error::before { content: "✖"; }
    .codicon-lock::before { content: "🔒"; }
  </style>
</head>
<body>
  <!-- Persistent Top Navigation Bar -->
  <header class="mc-navbar">
    <div class="mc-brand">
      <span class="codicon codicon-hubot"></span>
      <span>Mission Control</span>
      ${state.isPreview ? '<span class="badge badge-preview">PREVIEW</span>' : ''}
    </div>

    <nav class="mc-tabs-container" role="tablist">
      ${tabsHtml}
    </nav>

    <div class="mc-actions">
      <button class="btn btn-secondary btn-sm" onclick="refreshState()" title="Refresh Mission Control data">
        <span class="codicon codicon-refresh"></span> Refresh
      </button>
    </div>
  </header>

  <!-- Viewport Area containing all tabs -->
  <main class="mc-viewport">
    ${fragments.overview}
    ${fragments.agents}
    ${fragments.tasks}
    ${fragments.runs}
    ${fragments.timeline}
    ${fragments.review}
    ${fragments.approvals}
    ${fragments.providers}
    ${fragments.diagnostics}
  </main>

  <script nonce="${nonce}">
    (function() {
      const vscode = acquireVsCodeApi();
      let currentState = ${clientStateJson};

      // Helper to update navigation badges dynamically
      function updateBadge(tabId, count) {
        const btn = document.querySelector('.nav-tab-btn[data-tab="' + tabId + '"]');
        if (!btn) return;
        let badge = btn.querySelector('.badge');
        if (count !== undefined && count > 0) {
          if (!badge) {
            badge = document.createElement('span');
            badge.className = 'badge ' + (tabId === 'approvals' ? 'badge-warning' : 'badge-secondary');
            btn.appendChild(badge);
          }
          badge.textContent = String(count);
        } else if (badge) {
          badge.remove();
        }
      }

      // Set initial tab visibility
      function applyActiveTab(tabId) {
        document.querySelectorAll('.nav-tab-btn').forEach(btn => {
          btn.classList.toggle('active', btn.getAttribute('data-tab') === tabId);
        });

        document.querySelectorAll('.view-content').forEach(view => {
          view.classList.toggle('active', view.classList.contains(tabId + '-view'));
        });
      }

      applyActiveTab(currentState.activeTab || 'overview');

      // Tab switching
      window.switchTab = function(tabId) {
        currentState.activeTab = tabId;
        applyActiveTab(tabId);
        vscode.postMessage({
          type: 'switchTab',
          payload: { tab: tabId }
        });
      };

      // Sub-tab switching in Tasks View
      window.switchTasksSubView = function(subView) {
        document.querySelectorAll('.sub-tab-btn').forEach(btn => {
          btn.classList.toggle('active', btn.getAttribute('data-subview') === subView);
        });
        document.querySelectorAll('.tasks-sub-view').forEach(view => {
          view.classList.toggle('active', view.getAttribute('data-subview') === subView);
        });
      };

      // Agent selection
      window.selectAgent = function(agentId) {
        currentState.selectedAgentId = agentId;
        vscode.postMessage({
          type: 'selectAgent',
          payload: { agentId: agentId }
        });
      };

      // Task selection
      window.selectTask = function(taskId) {
        currentState.selectedTaskId = taskId;
        vscode.postMessage({
          type: 'selectTask',
          payload: { taskId: taskId }
        });
      };

      // Run selection
      window.selectRun = function(runId) {
        currentState.selectedRunId = runId;
        vscode.postMessage({
          type: 'selectRun',
          payload: { runId: runId }
        });
      };

      // Open Diff in native editor
      window.openDiff = function(artifactId) {
        vscode.postMessage({
          type: 'openDiff',
          payload: { artifactId: artifactId }
        });
      };

      // Open Chat sidebar
      window.openChat = function(sessionId) {
        vscode.postMessage({
          type: 'openChat',
          payload: { sessionId: sessionId }
        });
      };

      // Refresh state
      window.refreshState = function() {
        vscode.postMessage({
          type: 'refresh'
        });
      };

      // Timeline filtering
      window.filterTimeline = function(eventType) {
        document.querySelectorAll('.pill-btn').forEach(btn => {
          btn.classList.toggle('active', btn.getAttribute('data-filter') === eventType);
        });

        document.querySelectorAll('.timeline-entry').forEach(entry => {
          if (eventType === 'all' || entry.getAttribute('data-type') === eventType) {
            entry.style.display = 'flex';
          } else {
            entry.style.display = 'none';
          }
        });
      };

      // SVG DAG Pan and Zoom
      let zoomLevel = 1.0;
      let panX = 0;
      let panY = 0;
      let isPanning = false;
      let startX = 0;
      let startY = 0;

      function updateDagTransform() {
        const group = document.getElementById('dag-viewport-group');
        if (group) {
          group.setAttribute('transform', 'translate(' + panX + ',' + panY + ') scale(' + zoomLevel + ')');
        }
      }

      window.zoomDag = function(delta) {
        zoomLevel = Math.max(0.4, Math.min(2.5, zoomLevel + delta));
        updateDagTransform();
      };

      window.resetDagZoom = function() {
        zoomLevel = 1.0;
        panX = 0;
        panY = 0;
        updateDagTransform();
      };

      function initDagSvg() {
        const dagSvg = document.getElementById('dag-svg');
        if (dagSvg && !dagSvg.dataset.initialized) {
          dagSvg.dataset.initialized = 'true';
          dagSvg.addEventListener('mousedown', function(e) {
            if (e.target.closest('.dag-node')) {
              return;
            }
            isPanning = true;
            startX = e.clientX - panX;
            startY = e.clientY - panY;
            dagSvg.style.cursor = 'grabbing';
          });

          dagSvg.addEventListener('wheel', function(e) {
            e.preventDefault();
            const delta = e.deltaY < 0 ? 0.1 : -0.1;
            window.zoomDag(delta);
          }, { passive: false });
        }
      }

      initDagSvg();

      window.addEventListener('mousemove', function(e) {
        if (!isPanning) return;
        panX = e.clientX - startX;
        panY = e.clientY - startY;
        updateDagTransform();
      });

      window.addEventListener('mouseup', function() {
        if (isPanning) {
          isPanning = false;
          const dagSvg = document.getElementById('dag-svg');
          if (dagSvg) {
            dagSvg.style.cursor = 'grab';
          }
        }
      });

      // Incremental state updates from extension host via postMessage
      window.addEventListener('message', function(event) {
        const message = event.data;
        if (!message) return;

        if (message.type === 'stateUpdate') {
          if (message.state) {
            currentState = message.state;
          }

          // Update badges
          if (message.badges) {
            updateBadge('agents', message.badges.agents);
            updateBadge('tasks', message.badges.tasks);
            updateBadge('review', message.badges.review);
            updateBadge('approvals', message.badges.approvals);
          }

          // Incremental DOM update preserving scroll positions
          if (message.fragments) {
            for (const tabId in message.fragments) {
              const viewEl = document.querySelector('.' + tabId + '-view');
              if (viewEl) {
                const prevScrollTop = viewEl.scrollTop;
                const prevScrollLeft = viewEl.scrollLeft;
                viewEl.outerHTML = message.fragments[tabId];
                const newViewEl = document.querySelector('.' + tabId + '-view');
                if (newViewEl) {
                  newViewEl.scrollTop = prevScrollTop;
                  newViewEl.scrollLeft = prevScrollLeft;
                  newViewEl.classList.toggle('active', tabId === (currentState.activeTab || 'overview'));
                }
              }
            }
            initDagSvg();
          }

          if (currentState.activeTab) {
            applyActiveTab(currentState.activeTab);
          }
        }
      });

      // Keyboard shortcuts
      window.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') {
          // Deselect inspector if any
          const activeInspector = document.querySelector('.agent-inspector-container, .task-drawer-panel, .run-inspector-container');
          if (activeInspector) {
            // Can trigger a deselect
          }
        }
      });
    })();
  </script>
</body>
</html>`;
  }
}
