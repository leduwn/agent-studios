/**
 * Agent Studios — Model Providers View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';

export function renderProvidersView(state: MissionControlState): string {
  const { providers } = state;

  const rowsHtml =
    providers.length > 0
      ? providers
          .map((prov) => {
            const statusClass = prov.status === 'available' ? 'status-success' : 'status-warning';

            return `
            <tr class="table-row">
              <td><strong>${escapeHtml(prov.displayName)}</strong></td>
              <td><code>${escapeHtml(prov.id)}</code></td>
              <td><code>${escapeHtml(prov.modelId)}</code></td>
              <td><span class="status-pill ${statusClass}">${prov.status.toUpperCase()}</span></td>
              <td>${prov.contextWindow ? `${prov.contextWindow.toLocaleString()} tokens` : '<span class="text-secondary">Not Available</span>'}</td>
              <td>${prov.latencyMs ? `${prov.latencyMs}ms` : '<span class="text-secondary">Not Available</span>'}</td>
              <td>${prov.tokensUsed ? prov.tokensUsed.toLocaleString() : '<span class="text-secondary">Not Available</span>'}</td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="7" class="text-center text-secondary">No AI model providers configured.</td></tr>`;

  return `
    <div class="view-content providers-view">
      <div class="view-header">
        <div>
          <h2>Model Providers & Telemetry</h2>
          <div class="view-subtitle text-secondary">Configured AI runtime backends and measured latency (${providers.length} providers)</div>
        </div>
      </div>

      <div class="providers-security-banner">
        <span class="codicon codicon-lock text-info"></span>
        <span><strong>Secret Scrubbing:</strong> API credentials and signing keys are scrubbed by the Extension Host boundary and are never transmitted to this webview.</span>
      </div>

      <div class="providers-table-container">
        <table class="data-table">
          <thead>
            <tr>
              <th>Provider Name</th>
              <th>Instance ID</th>
              <th>Active Model</th>
              <th>Status</th>
              <th>Context Window</th>
              <th>Measured Latency</th>
              <th>Tokens Recorded</th>
            </tr>
          </thead>
          <tbody>
            ${rowsHtml}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
