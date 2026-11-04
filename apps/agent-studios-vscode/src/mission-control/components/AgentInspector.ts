/**
 * Agent Studios — Agent Inspector Component
 * Milestone M14 Mission Control Workbench
 */

import { AgentFleetMember, RunRecord } from '../models/types';

export function renderAgentInspector(
  agent: AgentFleetMember | undefined,
  runs: RunRecord[]
): string {
  if (!agent) {
    return `
      <div class="inspector-empty">
        <span class="codicon codicon-info"></span>
        <span>Select an agent from the fleet to inspect its state, runtime parameters, and tool execution history.</span>
      </div>
    `;
  }

  const linkedRun = runs.find((r) => r.agentId === agent.agentId || r.runId === agent.activeRunId);

  const statusBadge = `
    <span class="status-pill status-${agent.status}">
      <span class="codicon codicon-${agent.status === 'running' ? 'sync~spin' : agent.status === 'blocked' ? 'lock' : agent.status === 'failed' ? 'error' : 'circle-filled'}"></span>
      ${agent.status.toUpperCase()}
    </span>
  `;

  const toolCallsHtml =
    linkedRun && linkedRun.toolCalls.length > 0
      ? linkedRun.toolCalls
          .map(
            (tc) => `
        <div class="tool-call-row">
          <div class="tool-call-header">
            <span class="tool-name"><code>${escapeHtml(tc.toolName)}</code></span>
            <span class="tool-status status-${tc.status}">${tc.status}</span>
            <span class="tool-duration">${tc.durationMs ? `${tc.durationMs}ms` : ''}</span>
          </div>
          <div class="tool-input"><code>${escapeHtml(tc.inputSummary)}</code></div>
          ${tc.outputSummary ? `<div class="tool-output">${escapeHtml(tc.outputSummary)}</div>` : ''}
        </div>
      `
          )
          .join('')
      : '<div class="text-secondary text-sm">No tool invocations recorded for current run.</div>';

  return `
    <div class="agent-inspector-panel">
      <div class="inspector-header">
        <div class="inspector-title-group">
          <span class="codicon codicon-hubot"></span>
          <h3>${escapeHtml(agent.name)}</h3>
          <span class="agent-role-pill">${escapeHtml(agent.role)}</span>
        </div>
        <button class="btn btn-icon" onclick="selectAgent(null)" title="Close Inspector" aria-label="Close Inspector">
          <span class="codicon codicon-close"></span>
        </button>
      </div>

      <div class="inspector-meta-grid">
        <div class="meta-item">
          <span class="meta-label">Status</span>
          <span class="meta-value">${statusBadge}</span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Agent ID</span>
          <span class="meta-value"><code>${escapeHtml(agent.agentId)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Runtime Adapter</span>
          <span class="meta-value"><code>${escapeHtml(agent.runtime)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Provider & Model</span>
          <span class="meta-value"><code>${escapeHtml(agent.providerModel)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Assigned Task</span>
          <span class="meta-value">
            ${
              agent.assignedTaskId
                ? `<a href="#" onclick="selectTask('${agent.assignedTaskId}'); return false;">${escapeHtml(agent.assignedTaskTitle ?? agent.assignedTaskId)}</a>`
                : '<span class="text-secondary">None</span>'
            }
          </span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Git Worktree</span>
          <span class="meta-value">
            ${agent.worktreePath ? `<code>${escapeHtml(agent.worktreePath)}</code>` : '<span class="text-secondary">Default Workspace</span>'}
          </span>
        </div>
      </div>

      ${
        agent.error
          ? `
        <div class="inspector-error-box">
          <span class="codicon codicon-error"></span>
          <span>${escapeHtml(agent.error)}</span>
        </div>
      `
          : ''
      }

      <div class="inspector-section">
        <h4>Recent Tool Invocations (${linkedRun ? linkedRun.toolCalls.length : 0})</h4>
        <div class="tool-calls-stream">
          ${toolCallsHtml}
        </div>
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
