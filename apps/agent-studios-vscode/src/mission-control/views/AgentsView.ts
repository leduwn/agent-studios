/**
 * Agent Studios — Agents Fleet View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState } from '../models/types';
import { renderAgentInspector } from '../components/AgentInspector';

export function renderAgentsView(state: MissionControlState): string {
  const { fleet, selectedAgentId, runs } = state;
  const selectedAgent = fleet.find((a) => a.agentId === selectedAgentId);

  const rowsHtml =
    fleet.length > 0
      ? fleet
          .map((agent) => {
            const isSelected = agent.agentId === selectedAgentId;
            const statusClass = `status-${agent.status}`;

            return `
            <tr class="table-row ${isSelected ? 'selected' : ''}" onclick="selectAgent('${agent.agentId}')">
              <td>
                <span class="status-indicator ${statusClass}"></span>
                <strong>${escapeHtml(agent.name)}</strong>
              </td>
              <td><code>${escapeHtml(agent.agentId)}</code></td>
              <td><span class="badge badge-secondary">${escapeHtml(agent.role)}</span></td>
              <td><span class="status-pill ${statusClass}">${agent.status.toUpperCase()}</span></td>
              <td>
                ${
                  agent.assignedTaskId
                    ? `<a href="#" onclick="selectTask('${agent.assignedTaskId}'); return false;">${escapeHtml(agent.assignedTaskTitle ?? agent.assignedTaskId)}</a>`
                    : '<span class="text-secondary">—</span>'
                }
              </td>
              <td><code>${escapeHtml(agent.runtime)}</code></td>
              <td><span class="text-sm">${escapeHtml(agent.providerModel)}</span></td>
              <td>${formatRelativeTime(agent.lastActivityAt)}</td>
              <td>
                <button class="btn btn-secondary btn-sm" onclick="selectAgent('${agent.agentId}'); event.stopPropagation();">
                  Inspect
                </button>
              </td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="9" class="text-center text-secondary">No agents currently registered in fleet.</td></tr>`;

  return `
    <div class="view-content agents-view">
      <div class="view-header">
        <div>
          <h2>Agents Fleet Supervision</h2>
          <div class="view-subtitle text-secondary">Observability and telemetry for concurrent autonomous agent threads (${fleet.length} total)</div>
        </div>
      </div>

      <div class="agents-layout-split">
        <div class="agents-table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th>Agent</th>
                <th>ID</th>
                <th>Role</th>
                <th>Status</th>
                <th>Assigned Task</th>
                <th>Runtime</th>
                <th>Model</th>
                <th>Last Active</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              ${rowsHtml}
            </tbody>
          </table>
        </div>

        <div class="agent-inspector-container">
          ${renderAgentInspector(selectedAgent, runs)}
        </div>
      </div>
    </div>
  `;
}

function formatRelativeTime(timestampMs: number): string {
  const diffSec = Math.floor((Date.now() - timestampMs) / 1000);
  if (diffSec < 10) {
    return 'Just now';
  }
  if (diffSec < 60) {
    return `${diffSec}s ago`;
  }
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) {
    return `${diffMin}m ago`;
  }
  const diffHr = Math.floor(diffMin / 60);
  return `${diffHr}h ago`;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
