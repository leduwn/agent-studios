/**
 * Agent Studios — Overview View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';
import { renderAttentionQueue } from '../components/AttentionQueue';

export function renderOverviewView(state: MissionControlState): string {
  const { overview, activeStudio, connectionState, isPreview } = state;

  const previewBadge = isPreview
    ? `<span class="badge badge-preview" title="Developer Preview Fixture Data"><span class="codicon codicon-beaker"></span> PREVIEW FIXTURE</span>`
    : '';

  const connBadgeClass =
    connectionState === 'connected'
      ? 'status-success'
      : connectionState === 'connecting' || connectionState === 'reconnecting'
      ? 'status-running'
      : 'status-error';

  return `
    <div class="view-content overview-view">
      <!-- Top Studio Banner -->
      <div class="overview-banner">
        <div class="studio-info">
          <div class="studio-title-row">
            <h2>${escapeHtml(activeStudio?.name ?? 'No Active Session')}</h2>
            ${previewBadge}
            <span class="status-pill ${connBadgeClass}">
              <span class="codicon codicon-${connectionState === 'connected' ? 'pass' : 'circle-filled'}"></span>
              ${connectionState.toUpperCase()}
            </span>
          </div>
          <div class="studio-meta">
            ${
              activeStudio?.workspacePath
                ? `<span><span class="codicon codicon-folder"></span> <code>${escapeHtml(activeStudio.workspacePath)}</code></span>`
                : ''
            }
            <span><span class="codicon codicon-symbol-misc"></span> Studio ID: <code>${escapeHtml(activeStudio?.id ?? 'none')}</code></span>
          </div>
        </div>

        <div class="banner-actions">
          <button class="btn btn-secondary" onclick="openChat()">
            <span class="codicon codicon-comment-discussion"></span> Open Chat
          </button>
          <button class="btn btn-secondary" onclick="refreshState()">
            <span class="codicon codicon-refresh"></span> Refresh
          </button>
        </div>
      </div>

      <!-- KPI Summary Cards -->
      <div class="kpi-grid">
        <div class="kpi-card" onclick="switchTab('agents')">
          <div class="kpi-header">
            <span class="codicon codicon-hubot text-info"></span>
            <span class="kpi-label">Active Agents</span>
          </div>
          <div class="kpi-value">${overview.activeAgentCount} <span class="kpi-sub">/ ${overview.totalAgentsCount}</span></div>
        </div>

        <div class="kpi-card" onclick="switchTab('tasks')">
          <div class="kpi-header">
            <span class="codicon codicon-tasklist text-success"></span>
            <span class="kpi-label">Total Tasks</span>
          </div>
          <div class="kpi-value">${state.tasks.length}</div>
        </div>

        <div class="kpi-card" onclick="switchTab('runs')">
          <div class="kpi-header">
            <span class="codicon codicon-play text-warning"></span>
            <span class="kpi-label">Active Runs</span>
          </div>
          <div class="kpi-value">${overview.activeRunCount}</div>
        </div>

        <div class="kpi-card" onclick="switchTab('approvals')">
          <div class="kpi-header">
            <span class="codicon codicon-shield text-error"></span>
            <span class="kpi-label">Pending Approvals</span>
          </div>
          <div class="kpi-value">${state.approvals.filter((a) => a.status === 'pending').length}</div>
        </div>
      </div>

      <!-- Lifecycle Distribution Chips -->
      <div class="lifecycle-bar-card">
        <div class="section-title">Task Lifecycle Distribution</div>
        <div class="lifecycle-chips">
          <span class="chip chip-pending" onclick="switchTab('tasks')">Pending: ${overview.taskCounts.pending}</span>
          <span class="chip chip-ready" onclick="switchTab('tasks')">Ready: ${overview.taskCounts.ready}</span>
          <span class="chip chip-running" onclick="switchTab('tasks')">Running: ${overview.taskCounts.running}</span>
          <span class="chip chip-blocked" onclick="switchTab('tasks')">Blocked: ${overview.taskCounts.blocked}</span>
          <span class="chip chip-review" onclick="switchTab('tasks')">Review: ${overview.taskCounts.review}</span>
          <span class="chip chip-succeeded" onclick="switchTab('tasks')">Succeeded: ${overview.taskCounts.succeeded}</span>
          <span class="chip chip-failed" onclick="switchTab('tasks')">Failed: ${overview.taskCounts.failed}</span>
          <span class="chip chip-cancelled" onclick="switchTab('tasks')">Cancelled: ${overview.taskCounts.cancelled}</span>
        </div>
      </div>

      <!-- Attention Queue -->
      ${renderAttentionQueue(overview.attentionItems)}

      <!-- Recent Activity Events -->
      <div class="recent-events-card">
        <div class="recent-events-header">
          <span class="section-title">Latest Activity</span>
          <a href="#" onclick="switchTab('timeline'); return false;" class="text-sm">View all ${state.timeline.length} events &rarr;</a>
        </div>
        <div class="recent-events-list">
          ${
            overview.recentEvents.length > 0
              ? overview.recentEvents
                  .map(
                    (evt) => `
                <div class="recent-event-row">
                  <span class="event-seq">#${evt.sequence}</span>
                  <span class="event-type"><code>${escapeHtml(evt.type)}</code></span>
                  <span class="event-summary">${escapeHtml(evt.summary)}</span>
                  <span class="event-time">${formatTime(evt.timestamp)}</span>
                </div>
              `
                  )
                  .join('')
              : '<div class="text-secondary text-sm">No recent activity events recorded.</div>'
          }
        </div>
      </div>
    </div>
  `;
}

function formatTime(isoString: string): string {
  try {
    const d = new Date(isoString);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return isoString;
  }
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
