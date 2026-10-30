/**
 * Agent Studios — Run Inspector View Component
 * Milestone M14 Mission Control Workbench
 */

import { MissionControlState, RunRecord } from '../models/types';

export function renderRunsView(state: MissionControlState): string {
  const { runs, selectedRunId } = state;
  const selectedRun = runs.find((r) => r.runId === selectedRunId) ?? runs[0];

  const runRowsHtml =
    runs.length > 0
      ? runs
          .map((run) => {
            const isSelected = run.runId === selectedRun?.runId;
            const statusClass = `status-${run.status}`;
            const duration = run.completedAt
              ? `${Math.round((run.completedAt - run.startedAt) / 1000)}s`
              : 'Running...';

            return `
            <tr class="table-row ${isSelected ? 'selected' : ''}" onclick="selectRun('${run.runId}')">
              <td><code>${escapeHtml(run.runId)}</code></td>
              <td><a href="#" onclick="selectTask('${run.taskId}'); switchTab('tasks'); return false;">${escapeHtml(run.taskId)}</a></td>
              <td><a href="#" onclick="selectAgent('${run.agentId}'); switchTab('agents'); return false;">${escapeHtml(run.agentId)}</a></td>
              <td><span class="status-pill ${statusClass}">${run.status.toUpperCase()}</span></td>
              <td><code>${escapeHtml(run.runtime)}</code></td>
              <td>${duration}</td>
              <td>${formatDate(run.startedAt)}</td>
              <td>
                <button class="btn btn-secondary btn-sm" onclick="selectRun('${run.runId}'); event.stopPropagation();">
                  Inspect
                </button>
              </td>
            </tr>
          `;
          })
          .join('')
      : `<tr><td colspan="8" class="text-center text-secondary">No execution runs recorded.</td></tr>`;

  const runDetailsPanel = selectedRun
    ? renderSelectedRunDetails(selectedRun)
    : '<div class="inspector-empty">Select a run to inspect its execution trace and terminal evidence.</div>';

  return `
    <div class="view-content runs-view">
      <div class="view-header">
        <div>
          <h2>Execution Runs & Tool Telemetry</h2>
          <div class="view-subtitle text-secondary">Detailed execution evidence, subagent runtime traces, and tool outcomes (${runs.length} runs)</div>
        </div>
      </div>

      <div class="runs-layout-split">
        <div class="runs-table-container">
          <table class="data-table">
            <thead>
              <tr>
                <th>Run ID</th>
                <th>Task ID</th>
                <th>Agent ID</th>
                <th>Status</th>
                <th>Runtime</th>
                <th>Duration</th>
                <th>Started</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              ${runRowsHtml}
            </tbody>
          </table>
        </div>

        <div class="run-inspector-container">
          ${runDetailsPanel}
        </div>
      </div>
    </div>
  `;
}

function renderSelectedRunDetails(run: RunRecord): string {
  const statusClass = `status-${run.status}`;

  const toolCallsHtml =
    run.toolCalls.length > 0
      ? run.toolCalls
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
      : '<div class="text-secondary text-sm">No tool invocations recorded.</div>';

  return `
    <div class="run-details-panel">
      <div class="inspector-header">
        <div class="inspector-title-group">
          <span class="codicon codicon-play"></span>
          <h3>Run <code>${escapeHtml(run.runId)}</code></h3>
          <span class="status-pill ${statusClass}">${run.status.toUpperCase()}</span>
        </div>
      </div>

      <div class="inspector-meta-grid">
        <div class="meta-item">
          <span class="meta-label">Task</span>
          <span class="meta-value"><code>${escapeHtml(run.taskId)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Agent</span>
          <span class="meta-value"><code>${escapeHtml(run.agentId)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Runtime</span>
          <span class="meta-value"><code>${escapeHtml(run.runtime)}</code></span>
        </div>
        <div class="meta-item">
          <span class="meta-label">Worktree</span>
          <span class="meta-value">
            ${run.worktreePath ? `<code>${escapeHtml(run.worktreePath)}</code>` : '<span class="text-secondary">Default</span>'}
          </span>
        </div>
      </div>

      ${
        run.error
          ? `
        <div class="inspector-error-box">
          <span class="codicon codicon-error"></span>
          <span>${escapeHtml(run.error)}</span>
        </div>
      `
          : ''
      }

      ${
        run.terminalEvidence
          ? `
        <div class="inspector-section">
          <h4>Terminal Outcome Evidence</h4>
          <div class="terminal-box">
            ${
              run.terminalEvidence.command
                ? `<div class="terminal-cmd">$ ${escapeHtml(run.terminalEvidence.command)}</div>`
                : ''
            }
            ${
              run.terminalEvidence.exitCode !== undefined
                ? `<div class="terminal-exit">Exit Code: <span class="${run.terminalEvidence.exitCode === 0 ? 'text-success' : 'text-error'}">${run.terminalEvidence.exitCode}</span></div>`
                : ''
            }
            ${
              run.terminalEvidence.stdoutSnippet
                ? `<pre class="terminal-stdout">${escapeHtml(run.terminalEvidence.stdoutSnippet)}</pre>`
                : ''
            }
            ${
              run.terminalEvidence.stderrSnippet
                ? `<pre class="terminal-stderr text-error">${escapeHtml(run.terminalEvidence.stderrSnippet)}</pre>`
                : ''
            }
          </div>
        </div>
      `
          : ''
      }

      <div class="inspector-section">
        <h4>Tool Execution Trace (${run.toolCalls.length})</h4>
        <div class="tool-calls-stream">
          ${toolCallsHtml}
        </div>
      </div>
    </div>
  `;
}

function formatDate(timestamp: number): string {
  try {
    return new Date(timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return String(timestamp);
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
