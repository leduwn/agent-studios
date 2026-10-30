/**
 * Agent Studios — Diagnostics View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';

export function renderDiagnosticsView(state: MissionControlState): string {
  const { diagnostics, connectionState, isPreview, version } = state;

  const connBadgeClass =
    connectionState === 'connected'
      ? 'status-success'
      : connectionState === 'connecting' || connectionState === 'reconnecting'
      ? 'status-running'
      : 'status-error';

  return `
    <div class="view-content diagnostics-view">
      <div class="view-header">
        <div>
          <h2>System Diagnostics & IPC Health</h2>
          <div class="view-subtitle text-secondary">Transport telemetry, sequence watermarks, and daemon connectivity state</div>
        </div>

        <button class="btn btn-secondary btn-sm" onclick="refreshState()">
          <span class="codicon codicon-refresh"></span> Refresh Diagnostics
        </button>
      </div>

      <div class="diagnostics-grid">
        <div class="diag-card">
          <div class="diag-card-title">Daemon Connection</div>
          <div class="diag-prop">
            <span class="diag-label">Status:</span>
            <span class="status-pill ${connBadgeClass}">${connectionState.toUpperCase()}</span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Execution Mode:</span>
            <span>${isPreview ? 'Developer Preview (Fixtures)' : 'Live Daemon Connected'}</span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">IPC Endpoint:</span>
            <code>${escapeHtml(diagnostics.endpoint)}</code>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Session ID:</span>
            <code>${escapeHtml(diagnostics.sessionId ?? 'None')}</code>
          </div>
        </div>

        <div class="diag-card">
          <div class="diag-card-title">Wire Protocol & Sequence</div>
          <div class="diag-prop">
            <span class="diag-label">Protocol Version:</span>
            <code>${escapeHtml(diagnostics.protocolVersion)}</code>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Event Watermark:</span>
            <span>seq# <strong>${diagnostics.watermark}</strong></span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Active Clients:</span>
            <span>${diagnostics.activeClients}</span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Daemon Uptime:</span>
            <span>${formatUptime(diagnostics.uptimeSecs)}</span>
          </div>
        </div>

        <div class="diag-card">
          <div class="diag-card-title">Agreed Capabilities</div>
          <div class="capabilities-list">
            ${
              diagnostics.agreedCapabilities.length > 0
                ? diagnostics.agreedCapabilities.map((c) => `<span class="badge badge-secondary">${escapeHtml(c)}</span>`).join(' ')
                : '<span class="text-secondary">None negotiated</span>'
            }
          </div>
        </div>

        <div class="diag-card">
          <div class="diag-card-title">Extension Host Metadata</div>
          <div class="diag-prop">
            <span class="diag-label">Extension Version:</span>
            <span>${escapeHtml(version)}</span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Platform:</span>
            <span>${escapeHtml(process.platform)} (${escapeHtml(process.arch)})</span>
          </div>
          <div class="diag-prop">
            <span class="diag-label">Node Runtime:</span>
            <span>${escapeHtml(process.version)}</span>
          </div>
        </div>
      </div>

      ${
        diagnostics.lastError
          ? `
        <div class="diag-error-section">
          <div class="diag-error-header">
            <span class="codicon codicon-error text-error"></span>
            <strong>Last Connection Error</strong>
          </div>
          <pre class="diag-error-trace">${escapeHtml(diagnostics.lastError)}</pre>
        </div>
      `
          : ''
      }
    </div>
  `;
}

function formatUptime(seconds: number): string {
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const min = Math.floor(seconds / 60);
  if (min < 60) {
    return `${min}m ${seconds % 60}s`;
  }
  const hr = Math.floor(min / 60);
  return `${hr}h ${min % 60}m`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
